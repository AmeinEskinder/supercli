"""`supercli init` on a truly fresh HOME, end to end.

Fresh HOME -> `init` (creates the home with 0700, seeds builtin presets,
writes a default valid config, starts the Host, issues a pairing code,
ends with a green doctor) -> pair a device -> one approved action through
the running Host -> `doctor` green again.

The crypto pairing handshake itself is covered by `pairing.py`; here the
"pair" step registers the device record the handshake produces, proving
the init-created home pairs and serves. The "one approved action" drives
the real MCP approval flow: a write approval is requested, the paired
Controller sees it pending, answers approve, and the blocked call is
released.
"""

import sys, os, json, stat, shutil, time, threading, hashlib

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run_cli, run, mobile_request, mcp_post, mobile_port  # noqa: E402


class _Fresh:
    """Minimal home shim: a root that does not exist until `init` creates it."""

    def __init__(self, root):
        self.root = root

    def path(self, *parts):
        return os.path.join(self.root, *parts)


def _pair_device(fresh, token, name="e2e-phone"):
    """Write the device record a completed pairing handshake produces."""
    os.makedirs(fresh.path("mobile"), exist_ok=True)
    with open(fresh.path("mobile", "devices.json"), "w") as handle:
        json.dump(
            {
                "version": 1,
                "devices": [
                    {
                        "id": "dev-e2e",
                        "name": name,
                        "platform": "iOS",
                        "tokenHash": hashlib.sha256(token.encode()).hexdigest(),
                        "pairedAtUnixMs": int(time.time() * 1000),
                        "relayTokenHash": "x",
                    }
                ],
            },
            handle,
        )


def _serve_status(fresh, timeout=25):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        try:
            with open(fresh.path("serve.json")) as handle:
                status = json.load(handle)
            if status.get("hookPort") and status.get("pid"):
                return status
        except (FileNotFoundError, ValueError, OSError):
            pass
        time.sleep(0.3)
    return {}


def body(case):
    home = case.home
    fresh = _Fresh(home.root + "-fresh")
    shutil.rmtree(fresh.root, ignore_errors=True)

    # 1. `supercli init --json` on a truly fresh HOME.
    inited = run_cli(fresh, ["init", "--json"], timeout=90)
    case.check("init exits 0", inited.returncode == 0, inited.stderr[:500])
    try:
        report = json.loads(inited.stdout)
    except ValueError:
        report = {}
    case.check(
        "init reports a single JSON object",
        report.get("ok") is True and report.get("seeded") is True,
        inited.stdout[:300],
    )
    case.check(
        "init issues a pairing code",
        isinstance(report.get("pairing_code"), str) and len(report["pairing_code"]) > 8,
        inited.stdout[:200],
    )
    doctor = report.get("doctor", {})
    case.check(
        "init ends with a green doctor",
        doctor.get("failed") == 0 and doctor.get("passed", 0) > 0,
        inited.stdout[:300],
    )

    # 2. The fresh home is private.
    mode = stat.S_IMODE(os.stat(fresh.root).st_mode)
    case.check("fresh home is 0700", mode == 0o700, oct(mode))

    # 3. The default config validates.
    checked = run_cli(fresh, ["config", "check"], timeout=30)
    case.check(
        "init writes a default valid config",
        checked.returncode == 0,
        (checked.stdout + checked.stderr)[:300],
    )

    # 4. Pair a device on the init-created home.
    token = "init-e2e-phone-token"
    _pair_device(fresh, token)
    listed = run_cli(fresh, ["pair", "list", "--json"], timeout=30)
    case.check(
        "the paired device is listed",
        listed.returncode == 0 and "dev-e2e" in listed.stdout,
        (listed.stdout + listed.stderr)[:300],
    )

    # 5. One approved action through the Host init started.
    status = _serve_status(fresh)
    case.check(
        "init started the Host",
        bool(status.get("hookPort")),
        str(status)[:200],
    )
    if not status.get("hookPort"):
        return
    port = mobile_port(fresh, timeout=25)
    case.check("the phone endpoint is up", isinstance(port, int), str(port))
    if not isinstance(port, int):
        return
    boot_status, _ = mobile_request(port, "/mobile/bootstrap", token, home=fresh)
    case.check(
        "the paired device authenticates",
        boot_status == 200,
        str(boot_status),
    )

    results = {}

    # The MCP hook listener authenticates with the Host's auth token
    # (not the device token).
    with open(fresh.path("mcp", "auth-token")) as handle:
        mcp_token = handle.read().strip()
    case.check("init provisioned the MCP auth token", bool(mcp_token))

    def ask():
        results["call"] = mcp_post(
            status["hookPort"],
            "/mcp/approve-write",
            {"caller_session_id": "e2e-caller", "target_session_id": "e2e-target"},
            token=mcp_token,
        )

    thread = threading.Thread(target=ask)
    thread.start()
    pending = {}
    end = time.monotonic() + 10.0
    while time.monotonic() < end and not pending:
        boot_status, boot = mobile_request(port, "/mobile/bootstrap", token, home=fresh)
        if boot_status == 200:
            pending = next(
                (
                    approval
                    for approval in boot.get("pendingApprovals", [])
                    if approval.get("callerSessionID") == "e2e-caller"
                ),
                {},
            )
        time.sleep(0.3)
    case.check(
        "the Host publishes the approval to the paired Controller",
        bool(pending) and pending.get("kind") == "write",
        str(pending)[:200],
    )
    if pending:
        answer_status, _ = mobile_request(
            port,
            "/mobile/approvals/answer",
            token,
            method="POST",
            body={"id": pending["id"], "approved": True},
            home=fresh,
        )
        case.check("the Controller approves", answer_status == 200, str(answer_status))
    thread.join(timeout=15)
    case.check(
        "the approved action is released",
        results.get("call", (0, {}))[1].get("approved") is True,
        str(results.get("call"))[:200],
    )

    # 6. Doctor is green after the whole journey.
    doctored = run_cli(fresh, ["doctor"], timeout=30)
    case.check(
        "doctor is green after pair + approved action",
        doctored.returncode == 0,
        (doctored.stdout + doctored.stderr)[:500],
    )

    # 7. Stop the Host init started so the case leaves no stray process.
    # Verify the kernel start time immediately before signaling, so a
    # recycled PID can never be killed.
    def _starttime(pid):
        try:
            with open(f"/proc/{pid}/stat") as handle:
                return handle.read().rsplit(")", 1)[1].split()[20]
        except (FileNotFoundError, ValueError, IndexError):
            return None

    try:
        with open(fresh.path("serve.json")) as handle:
            host_pid = json.load(handle).get("pid")
        recorded = _starttime(host_pid) if host_pid else None
        if host_pid and recorded and _starttime(host_pid) == recorded:
            os.kill(host_pid, 15)
            end = time.monotonic() + 10.0
            while time.monotonic() < end:
                try:
                    os.kill(host_pid, 0)
                except OSError:
                    break
                time.sleep(0.2)
    except (FileNotFoundError, ValueError, OSError, PermissionError, TypeError):
        pass


run("init", body)
