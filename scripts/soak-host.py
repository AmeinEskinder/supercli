#!/usr/bin/env python3
"""Phase 9 H3 — Host soak and load test.

Runs the real Host (`supercli serve`) for 30+ minutes under sustained load:
many concurrent sessions, continuous event polling, Ask approvals answered
from the paired-controller path, and turn cancels mid-flight. Samples
heap/RSS, file-descriptor counts, and tool-call round-trip latency (the
review-log lock contention proxy) throughout, then empirically checks the
event ring-buffer bound.

Real binaries, private short-path SUPERCLI_HOME (never the real ~/.supercli).
No mocks. Usage:

    SOAK_SECS=2100 python3 scripts/soak-host.py

Report lands in out/soak/soak-report-<ts>.md (+ metrics.jsonl).
"""
import hashlib
import http.client
import json
import os
import socket
import ssl
import subprocess
import sys
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SUPERCLI = os.path.join(ROOT, "crates", "target", "debug", "supercli")
SUPERCLI_HOST = os.path.join(ROOT, "crates", "target", "debug", "supercli-host")
PAIR_CLIENT = os.path.join(ROOT, "crates", "target", "debug", "examples", "pair_client")
OUT_DIR = os.path.join(ROOT, "out", "soak")

SOAK_SECS = int(os.environ.get("SOAK_SECS", "2100"))
N_SESSIONS = int(os.environ.get("SOAK_SESSIONS", "6"))
N_WORKERS = int(os.environ.get("SOAK_WORKERS", "3"))
METRIC_EVERY = 20

TS = time.strftime("%Y%m%d-%H%M%S")
HOME = "/home/hatch/soak-supercli-%d" % os.getpid()
CONN_DIR = os.path.join(HOME, "connectors")
LOG_DIR = os.path.join(HOME, "logs")
TOKEN = {"value": None}

os.makedirs(OUT_DIR, exist_ok=True)
os.makedirs(LOG_DIR, exist_ok=True)
report_path = os.path.join(OUT_DIR, "soak-report-%s.md" % TS)
metrics_path = os.path.join(OUT_DIR, "metrics-%s.jsonl" % TS)

env = dict(
    os.environ,
    SUPERCLI_HOME=HOME,
    SUPERCLI_CONNECTORS_DIR=CONN_DIR,
    SUPERCLI_CONNECTORS_KEYCHAIN="memory",
    SUPERCLI_TEST="1",
    SUPERCLI_HOST_BIN=SUPERCLI_HOST,
)

stop_flag = threading.Event()
stats_lock = threading.Lock()


def record_error(worker_id, kind, detail=""):
    with stats_lock:
        counters["errors"] += 1
        if len(error_log) < 50:
            error_log.append({"t": time.time(), "worker": worker_id,
                              "kind": kind, "detail": str(detail)[:300]})
    # Live visibility: print immediately, not just at report time.
    print("[soak-error %s] worker=%s kind=%s detail=%s"
          % (time.strftime("%H:%M:%S"), worker_id, kind, str(detail)[:200]),
          flush=True)
latencies = []  # allow-call round-trip ms
counters = {"allow": 0, "ask": 0, "cancel": 0, "events": 0, "errors": 0,
            "rate_limited": 0}  # 429s are expected, not errors
error_log = []  # first 50: {t, worker, kind, detail}


def record_rate_limited(worker_id):
    """R5: 429 is an expected outcome (client should back off + retry),
    not an error. Counted separately. Must never become a 500 or lost review."""
    with stats_lock:
        counters["rate_limited"] += 1


def log(msg):
    print("[soak %s] %s" % (time.strftime("%H:%M:%S"), msg), flush=True)


def sh(cmd, **kw):
    return subprocess.run(cmd, env=env, capture_output=True, text=True, **kw)


# ------------------------------------------- process tracking / teardown ---
# Every subprocess this driver spawns is registered here. Teardown kills
# only these (plus their descendants) — never a global pkill.
children = []
children_lock = threading.Lock()
sidecars = []
sidecars_lock = threading.Lock()


def spawn(cmd, **kw):
    p = subprocess.Popen(cmd, **kw)
    with children_lock:
        children.append(p)
    return p


def reap(proc, timeout=10):
    """Wait for a process that should have exited on its own; terminate
    it if it is stuck, then SIGKILL as a last resort."""
    try:
        proc.wait(timeout=timeout)
        return
    except subprocess.TimeoutExpired:
        pass
    try:
        proc.terminate()
        proc.wait(timeout=5)
    except Exception:
        try:
            proc.kill()
        except Exception:
            pass


def kill_process_tree(root_pid):
    """SIGTERM then SIGKILL every descendant of root_pid (children first).
    Scoped to this driver's process tree — never touches other processes."""

    def children_of(pid):
        kids = []
        for p in os.listdir("/proc"):
            if not p.isdigit():
                continue
            try:
                with open("/proc/%s/stat" % p) as h:
                    ppid = int(h.read().rsplit(")", 1)[1].split()[1])
                if ppid == pid:
                    kids.append(int(p))
            except Exception:
                continue
        return kids

    def descendants(pid):
        out = []
        for k in children_of(pid):
            out.append(k)
            out.extend(descendants(k))
        return out

    # (pid, starttime) pairs — every kill below re-verifies identity so a
    # recycled pid is never signalled (repo invariant).
    targets = [(p, proc_starttime(p)) for p in descendants(root_pid)]
    targets = [(p, st) for p, st in targets if st is not None]

    for sig in (15, 9):
        for pid, st in reversed(targets):
            if proc_starttime(pid) != st:
                continue
            try:
                os.kill(pid, sig)
            except Exception:
                pass
        time.sleep(3 if sig == 15 else 1)


def kill_all_sidecars():
    with sidecars_lock:
        cars = list(sidecars)
    for car in cars:
        car.kill()


def proc_starttime(pid):
    """Kernel start time of a pid (jiffies since boot), or None."""
    try:
        with open("/proc/%s/stat" % pid) as h:
            return h.read().rsplit(")", 1)[1].split()[20]
    except Exception:
        return None


def kill_home_processes(home):
    """Terminate processes whose SUPERCLI_HOME environ equals this run's
    home (e.g. the PTY core serve leaves running on purpose). Scoped by
    environ match — other runs' and other users' processes are untouched.
    Every kill verifies the pid's kernel start time first: under load the
    pid counter wraps fast and an unverified kill can take out an
    innocent process (repo invariant: process identity before any
    signal). Ambiguous ownership fails closed."""
    hits = []
    for pid in os.listdir("/proc"):
        if not pid.isdigit() or int(pid) == os.getpid():
            continue
        try:
            with open("/proc/%s/environ" % pid, "rb") as h:
                envb = h.read().split(b"\0")
            if ("SUPERCLI_HOME=%s" % home).encode() not in envb:
                continue
            with open("/proc/%s/cmdline" % pid, "rb") as h:
                cmd = h.read().replace(b"\0", b" ").decode("utf8", "replace")
            if "supercli" in cmd:
                st = proc_starttime(pid)
                if st is not None:
                    hits.append((int(pid), st))
        except Exception:
            continue
    for sig in (15, 9):
        for pid, st in hits:
            # Re-verify identity: skip if the pid was recycled.
            if proc_starttime(pid) != st:
                continue
            try:
                os.kill(pid, sig)
            except Exception:
                pass
        time.sleep(2 if sig == 15 else 1)
    return [pid for pid, _ in hits]


# ------------------------------------------------------------ connectors ---
def write_connector(name, tool, policy, sleep_default=0):
    d = os.path.join(CONN_DIR, name)
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, "connector.toml"), "w") as h:
        h.write(
            '[connector]\nname = "%s"\nversion = "1.0.0"\n'
            'display_name = "%s soak"\ndescription = "soak fixture"\n'
            'kind = "mcp-stdio"\n\n[auth]\nflow = "none"\n\n'
            '[tools]\nprovides = ["%s"]\n' % (name, name, tool)
        )
        if policy == "allow":
            h.write('\n[policy]\n"%s" = "allow"\n' % tool)
    with open(os.path.join(d, "stub.py"), "w") as h:
        h.write(
            "import json, sys, time\n"
            'TOOLS = {"%s": %d}\n' % (tool, sleep_default)
            + "def respond(rid, result):\n"
            '    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": rid, "result": result}) + "\\n")\n'
            "    sys.stdout.flush()\n"
            "for line in sys.stdin:\n"
            "    line = line.strip()\n"
            "    if not line:\n"
            "        continue\n"
            "    msg = json.loads(line)\n"
            "    method = msg.get('method')\n"
            "    rid = msg.get('id')\n"
            "    if method == 'initialize':\n"
            '        respond(rid, {"protocolVersion": "2024-11-05", "serverInfo": {"name": "%s"}})\n'
            % name
            + "    elif method == 'tools/list':\n"
            '        respond(rid, {"tools": [{"name": t, "inputSchema": {"type": "object"}} for t in TOOLS]})\n'
            "    elif method == 'tools/call':\n"
            "        name = msg['params']['name']\n"
            "        args = msg['params'].get('arguments', {}) or {}\n"
            "        if name in TOOLS:\n"
            "            secs = args.get('seconds', TOOLS[name])\n"
            "            time.sleep(max(0, min(float(secs), 120)))\n"
            '            respond(rid, {"content": [{"type": "text", "text": "echo:" + json.dumps(args)}]})\n'
            "        else:\n"
            '            sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": rid, "error": {"code": -32601, "message": "unknown tool"}}) + "\\n")\n'
            "            sys.stdout.flush()\n"
            "    elif rid is not None:\n"
            "        respond(rid, {})\n"
        )
    with open(os.path.join(d, "connector"), "w") as h:
        h.write('#!/usr/bin/env bash\nexec python3 "$(dirname "$0")/stub.py"\n')
    os.chmod(os.path.join(d, "connector"), 0o755)


# ---------------------------------------------------------------- mobile ---
_mobile_port_cache = None
_cert_pin_cache = None

def mobile_port():
    global _mobile_port_cache
    if _mobile_port_cache is None:
        with open(os.path.join(HOME, "mobile", "server-port")) as h:
            _mobile_port_cache = int(h.read().strip())
    return _mobile_port_cache


def cert_pin():
    global _cert_pin_cache
    if _cert_pin_cache is None:
        with open(os.path.join(HOME, "remote", "tls", "cert.pem")) as h:
            pem = h.read()
        _cert_pin_cache = hashlib.sha256(ssl.PEM_cert_to_DER_cert(pem)).hexdigest()
    return _cert_pin_cache


def mobile(method, path, body=None, timeout=20):
    port = mobile_port()
    pin = cert_pin()
    payload = json.dumps(body).encode() if body is not None else None
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    conn = http.client.HTTPSConnection("127.0.0.1", port, timeout=timeout, context=context)
    try:
        conn.connect()
        served = hashlib.sha256(conn.sock.getpeercert(binary_form=True)).hexdigest()
        if served != pin:
            return 0, {"error": "certificate pin mismatch"}
        conn.request(
            method, path, body=payload,
            headers={"Authorization": "Bearer " + TOKEN["value"],
                      "Content-Type": "application/json"},
        )
        resp = conn.getresponse()
        raw = resp.read()
        try:
            data = json.loads(raw or b"{}")
        except ValueError:
            data = {}
        return resp.status, data
    except Exception as error:
        return 0, {"error": str(error)}
    finally:
        conn.close()


# ------------------------------------------------------------ MCP sidecar ---
class McpSidecar:
    def __init__(self, session_id):
        e = dict(env, SUPERCLI_SESSION_ID=session_id)
        self.proc = spawn(
            [SUPERCLI_HOST, "__mcp__"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, env=e, text=True, bufsize=1,
        )
        with sidecars_lock:
            sidecars.append(self)
        self._next_id = 0
        self._lock = threading.Lock()
        self._responses = {}
        self._reader = threading.Thread(target=self._read_loop, daemon=True)
        self._reader.start()
        init = self.call("initialize", {"protocolVersion": "2024-11-05",
                                        "capabilities": {},
                                        "clientInfo": {"name": "soak", "version": "0"}})
        if self.wait(init, 15) is None:
            raise RuntimeError("sidecar did not answer initialize")
        self.notify("notifications/initialized")

    def _read_loop(self):
        for line in self.proc.stdout:
            try:
                msg = json.loads(line)
            except ValueError:
                continue
            if "id" in msg and ("result" in msg or "error" in msg):
                with self._lock:
                    self._responses[msg["id"]] = msg

    def call(self, method, params=None):
        with self._lock:
            self._next_id += 1
            mid = self._next_id
        msg = {"jsonrpc": "2.0", "id": mid, "method": method}
        if params is not None:
            msg["params"] = params
        self.proc.stdin.write(json.dumps(msg) + "\n")
        self.proc.stdin.flush()
        return mid

    def notify(self, method, params=None):
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        self.proc.stdin.write(json.dumps(msg) + "\n")
        self.proc.stdin.flush()

    def wait(self, mid, timeout):
        end = time.time() + timeout
        while time.time() < end:
            with self._lock:
                if mid in self._responses:
                    return self._responses.pop(mid)
            time.sleep(0.05)
        return None

    def kill(self):
        try:
            self.proc.kill()
        except Exception:
            pass


def session_dir(sid):
    return os.path.join(HOME, "app-sessions", sid)


def inflight_review_for_tool(sid, tool):
    """Return the review_id of the latest in-flight approved review for tool.

    Reads only the file tail (O(1) in log size) and tolerates partially
    written trailing lines from concurrent Host appends.
    """
    path = os.path.join(session_dir(sid), "action-reviews.jsonl")
    try:
        with open(path, "rb") as h:
            h.seek(0, 2)
            size = h.tell()
            h.seek(max(0, size - 65536))
            tail = h.read().decode("utf-8", "replace")
    except FileNotFoundError:
        return None
    entries = []
    for line in tail.splitlines()[-300:]:
        line = line.strip()
        if not line:
            continue
        try:
            entries.append(json.loads(line))
        except ValueError:
            continue  # partially written line from a concurrent append
    resolved = {e.get("review_id") for e in entries if e.get("type") == "attempt_outcome"}
    for e in reversed(entries):
        if (e.get("type") != "attempt_outcome" and e.get("decision") == "approved"
                and e.get("tool") == tool and e.get("review_id") not in resolved):
            return e["review_id"]
    return None


# ----------------------------------------------------------------- setup ---
def setup():
    for f in (SUPERCLI, SUPERCLI_HOST, PAIR_CLIENT):
        if not (os.path.isfile(f) and os.access(f, os.X_OK)):
            log("FATAL: binary missing: %s (build first)" % f)
            sys.exit(2)
    os.makedirs(os.path.join(HOME, "pids"), exist_ok=True)
    os.makedirs(os.path.join(HOME, "mobile"), exist_ok=True)
    os.makedirs(os.path.join(HOME, "mcp"), exist_ok=True)
    os.chmod(HOME, 0o700)
    write_connector("asky", "asky.echo", "ask")
    write_connector("allowy", "allowy.echo", "allow")
    write_connector("slowy", "slowy.sleep", "allow")

    port = int(sh(["python3", "-c",
                   "import socket;s=socket.socket();s.bind(('127.0.0.1',0));"
                   "print(s.getsockname()[1])"]).stdout.strip())
    with open(os.path.join(HOME, "mobile", "server-port"), "w") as h:
        h.write(str(port))
    with open(os.path.join(HOME, "mcp", "auth-token"), "w") as h:
        h.write(sh(["python3", "-c", "import secrets;print(secrets.token_hex(32))"]).stdout)
    os.chmod(os.path.join(HOME, "mcp", "auth-token"), 0o600)

    serve_log = open(os.path.join(LOG_DIR, "serve.log"), "w")
    serve = spawn([SUPERCLI, "serve"], env=env, stdout=serve_log, stderr=subprocess.STDOUT)
    with open(os.path.join(HOME, "pids", "serve.pid"), "w") as h:
        h.write(str(serve.pid))
    for _ in range(40):
        try:
            d = json.load(open(os.path.join(HOME, "serve.json")))
            if d.get("pid") and d.get("hookPort"):
                break
        except Exception:
            pass
        time.sleep(0.5)
    else:
        log("FATAL: serve did not become ready")
        sys.exit(2)
    log("Host started (pid %d)" % serve.pid)

    # Pairing ceremony (same as e2e-scenario.sh step 2).
    pair_log = open(os.path.join(LOG_DIR, "pair.log"), "w")
    pair = spawn(
        [SUPERCLI, "pair", "--advertise-host", "127.0.0.1", "--advertise-port", str(port)],
        env=env, stdout=pair_log, stderr=subprocess.STDOUT)
    qr = None
    for _ in range(40):
        try:
            with open(os.path.join(LOG_DIR, "pair.log")) as h:
                for line in h:
                    if line.startswith("SUPERCLI:"):
                        qr = line.strip()
                        break
            if qr:
                break
        except FileNotFoundError:
            pass
        time.sleep(0.5)
    if not qr:
        log("FATAL: no QR code from supercli pair")
        sys.exit(2)
    for _ in range(40):
        if os.path.exists(os.path.join(HOME, "remote", "tls", "cert.pem")):
            try:
                ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
                ctx.check_hostname = False
                ctx.verify_mode = ssl.CERT_NONE
                c = ctx.wrap_socket(socket.create_connection(("127.0.0.1", port), timeout=3))
                c.close()
                break
            except Exception:
                pass
        time.sleep(0.5)
    out = sh([PAIR_CLIENT, qr])
    if out.returncode != 0:
        log("FATAL: sealed pairing exchange failed: %s" % out.stderr[-500:])
        sys.exit(2)
    # pair_client prints a JSON object; the token is its auth_token field
    # (same extraction as scripts/e2e-scenario.sh).
    try:
        TOKEN["value"] = json.loads(out.stdout.strip())["auth_token"]
    except (ValueError, KeyError) as e:
        log("FATAL: could not parse auth_token from pair_client output: %s" % e)
        sys.exit(2)
    if not TOKEN["value"]:
        log("FATAL: pairing returned no auth token")
        sys.exit(2)
    log("paired (genuine sealed /mobile/pair exchange)")
    # The `supercli pair` process exits on its own once pairing completes;
    # reap it here so it never lingers past setup.
    reap(pair, timeout=10)

    sessions = []
    for i in range(N_SESSIONS):
        r = sh([SUPERCLI, "new", "--command", "sleep 3600", "--json"])
        sid = json.loads(r.stdout)["id"]
        for c in ("asky", "allowy", "slowy"):
            rr = sh([SUPERCLI, "connector", "enable", c, "--session", sid])
            if rr.returncode != 0:
                log("FATAL: connector enable %s: %s" % (c, rr.stderr[-300:]))
                sys.exit(2)
        sessions.append(sid)
    log("%d sessions created, connectors attached" % len(sessions))
    return serve, sessions


# ------------------------------------------------------------------ load ---
def approval_answerer():
    """Answer Ask approvals from the paired-controller path."""
    answered = set()
    while not stop_flag.is_set():
        try:
            status, boot = mobile("GET", "/mobile/bootstrap", timeout=10)
            if status == 200:
                for ap in boot.get("pendingApprovals", []):
                    if ap.get("kind") == "connector":
                        aid = ap.get("id")
                        if aid and aid not in answered:
                            answered.add(aid)
                            mobile("POST", "/mobile/approvals/answer",
                                   {"id": aid, "approved": True}, timeout=10)
        except Exception:
            pass
        time.sleep(0.4)


def event_poller(sids):
    cursors = {s: 0 for s in sids}
    while not stop_flag.is_set():
        for sid in sids:
            try:
                status, data = mobile(
                    "GET", "/mobile/events?session_id=%s&after_seq=%d&limit=256"
                    % (sid, cursors[sid]), timeout=15)
                if status == 200:
                    evs = data.get("events", [])
                    cursors[sid] = data.get("next_seq", cursors[sid])
                    with stats_lock:
                        counters["events"] += len(evs)
            except Exception:
                pass
            if stop_flag.is_set():
                break
        time.sleep(0.2)


def load_worker(sid, worker_id, do_cancels=False):
    car = McpSidecar(sid)
    i = 0
    try:
        while not stop_flag.is_set():
            i += 1
            try:
                if i % 40 == 0 and do_cancels:
                    # Slow call, then cancel it mid-flight. Runs on the SAME
                    # session as the concurrent fast load (R1): the
                    # turn-cancel TOCTOU (list in-flight, then mark Ambiguous)
                    # is now benign — a call that completes in between lands
                    # in `already_resolved` with HTTP 200, never 500.
                    mid = car.call("tools/call", {"name": "slowy.sleep",
                                                  "arguments": {"seconds": 25}})
                    rid = None
                    end = time.time() + 20
                    while time.time() < end and rid is None and not stop_flag.is_set():
                        rid = inflight_review_for_tool(sid, "slowy.sleep")
                        time.sleep(0.2)
                    if rid:
                        time.sleep(1.5)
                        st, body = mobile("POST", "/mobile/turn-cancel",
                                          {"sessionID": sid, "reason": "soak cancel"},
                                          timeout=15)
                        with stats_lock:
                            counters["cancel"] += 1
                        if st == 429:
                            # R5: rate-limited cancel is expected, not an error.
                            # The cancel was NOT recorded; client backs off.
                            record_rate_limited(worker_id)
                        elif st != 200:
                            record_error(worker_id, "cancel-not-200",
                                         "status=%s body=%s rid=%s"
                                         % (st, str(body)[:200], rid[:8]))
                    car.wait(mid, 30)
                elif i % 12 == 0:
                    mid = car.call("tools/call", {"name": "asky.echo",
                                                  "arguments": {"msg": "ask-%d" % i}})
                    resp = car.wait(mid, 60)
                    with stats_lock:
                        counters["ask"] += 1
                    if resp is None or "error" in resp:
                        # R5: rate-limited tool call is expected, not an error.
                        err_str = json.dumps(resp)[:200] if resp else "timeout"
                        if "rate limit exceeded" in err_str:
                            record_rate_limited(worker_id)
                        else:
                            record_error(worker_id, "ask-failed", err_str)
                else:
                    t0 = time.time()
                    mid = car.call("tools/call", {"name": "allowy.echo",
                                                  "arguments": {"msg": "allow-%d" % i}})
                    resp = car.wait(mid, 60)
                    dt = (time.time() - t0) * 1000
                    with stats_lock:
                        counters["allow"] += 1
                        latencies.append(dt)
                    if resp is None or "error" in resp:
                        # R5: rate-limited tool call is expected, not an error.
                        err_str = json.dumps(resp)[:200] if resp else "timeout"
                        if "rate limit exceeded" in err_str:
                            record_rate_limited(worker_id)
                        else:
                            record_error(worker_id, "allow-failed", err_str)
            except Exception as e:
                import traceback as _tb
                record_error(worker_id, "worker-exception",
                             "%s | %s" % (repr(e)[:150], _tb.format_exc(limit=3)[-250:]))
            time.sleep(0.15)
    finally:
        car.kill()


def proc_snapshot():
    """RSS (kB) and fd counts for supercli processes."""
    procs = []
    for pid in os.listdir("/proc"):
        if not pid.isdigit():
            continue
        try:
            with open("/proc/%s/cmdline" % pid, "rb") as h:
                cmd = h.read().replace(b"\0", b" ").decode("utf8", "replace")
        except Exception:
            continue
        if "target/debug/supercli-host" in cmd or ("target/debug/supercli" in cmd and " serve" in cmd):
            try:
                rss = vmdata = rssanon = 0
                with open("/proc/%s/status" % pid) as h:
                    for line in h:
                        if line.startswith("VmRSS:"):
                            rss = int(line.split()[1])
                        elif line.startswith("VmData:"):
                            vmdata = int(line.split()[1])
                        elif line.startswith("RssAnon:"):
                            rssanon = int(line.split()[1])
                fds = len(os.listdir("/proc/%s/fd" % pid))
                procs.append({"pid": int(pid), "rss_kb": rss, "fds": fds,
                              "vmdata_kb": vmdata, "rssanon_kb": rssanon,
                              "cmd": cmd[:80]})
            except Exception:
                pass
    return procs


def metrics_sampler():
    while not stop_flag.is_set():
        snap = {"t": time.time(), "procs": proc_snapshot()}
        with stats_lock:
            snap["counters"] = dict(counters)
        with open(metrics_path, "a") as h:
            h.write(json.dumps(snap) + "\n")
        time.sleep(METRIC_EVERY)


# --------------------------------------------------------------- ring buf ---
def ring_buffer_check(sid):
    """Empirically verify the per-session event ring-buffer bound (512)."""
    car = McpSidecar(sid)
    try:
        for i in range(700):
            mid = car.call("tools/call", {"name": "allowy.echo",
                                           "arguments": {"msg": "rb-%d" % i}})
            car.wait(mid, 30)
    finally:
        car.kill()
    time.sleep(2)
    status, data = mobile("GET", "/mobile/events?session_id=%s&after_seq=0&limit=4096" % sid,
                          timeout=30)
    if status != 200:
        return {"ok": False, "error": "poll failed: %s" % status}
    evs = data.get("events", [])
    first_seq = evs[0].get("seq") if evs else None
    return {"ok": len(evs) <= 512, "retained": len(evs),
            "first_seq": first_seq,
            "evicted": (first_seq or 0) > 0, "bound": 512}


# ------------------------------------------------------------------- main ---
def percentile(xs, p):
    if not xs:
        return 0.0
    s = sorted(xs)
    return s[min(len(s) - 1, int(p / 100.0 * len(s)))]


def main():
    t_start = time.time()
    # Stale-binary guard: rebuild before running. A soak with an old binary
    # (e.g., built before the fix under test) produces misleading results.
    # This ensures the binary matches the current source.
    log("rebuilding binaries (stale-binary guard)...")
    cargo = os.path.expanduser("~/.cargo/bin/cargo")
    if not os.path.exists(cargo):
        cargo = "cargo"  # fall back to PATH
    build = subprocess.run(
        [cargo, "build", "--bin", "supercli"],
        cwd=os.path.join(ROOT, "crates"),
        capture_output=True, text=True, timeout=600,
    )
    if build.returncode != 0:
        print("FATAL: cargo build --bin supercli failed:\n%s" % build.stderr[-2000:],
              flush=True)
        sys.exit(1)
    build_host = subprocess.run(
        [cargo, "build", "-p", "supercli-host"],
        cwd=os.path.join(ROOT, "crates"),
        capture_output=True, text=True, timeout=600,
    )
    if build_host.returncode != 0:
        print("FATAL: cargo build -p supercli-host failed:\n%s" % build_host.stderr[-2000:],
              flush=True)
        sys.exit(1)
    log("binaries rebuilt OK")
    serve, sessions = setup()
    threads = []
    ans = threading.Thread(target=approval_answerer, daemon=True)
    ans.start()
    threads.append(ans)
    # R1: cancels run on the SAME sessions as the concurrent fast load —
    # no dedicated cancel session. The turn-cancel TOCTOU is fixed, so a
    # cancel racing a completing call is benign (200 + already_resolved).
    for sid in sessions:
        for wid in range(N_WORKERS):
            t = threading.Thread(target=load_worker, args=(sid, wid, True),
                                 daemon=True)
            t.start()
            threads.append(t)
    for i in range(0, len(sessions), 3):
        chunk = sessions[i:i+3]
        t = threading.Thread(target=event_poller, args=(chunk,), daemon=True)
        t.start()
        threads.append(t)
    samp = threading.Thread(target=metrics_sampler, daemon=True)
    samp.start()
    threads.append(samp)

    log("load running for %d s (%d sessions, cancels on same sessions)"
        % (SOAK_SECS, len(sessions)))
    end = time.time() + SOAK_SECS
    while time.time() < end:
        time.sleep(10)
        with stats_lock:
            c = dict(counters)
        log("t+%ds allow=%d ask=%d cancel=%d events=%d errors=%d" % (
            time.time() - t_start, c["allow"], c["ask"], c["cancel"], c["events"], c["errors"]))

    log("load done; 120 s idle settle, then ring-buffer check")
    stop_flag.set()
    for t in threads:
        t.join(timeout=30)
    time.sleep(120)
    with open(metrics_path, "a") as h:
        h.write(json.dumps({"t": time.time(), "procs": proc_snapshot(),
                            "counters": dict(counters), "idle": True}) + "\n")

    t_end = time.time()
    rb = ring_buffer_check(sessions[0])
    log("ring-buffer check: %s" % json.dumps(rb))

    lock_timeouts = 0
    try:
        with open(os.path.join(LOG_DIR, "serve.log")) as h:
            for line in h:
                if "timed out acquiring review-log lock" in line:
                    lock_timeouts += 1
    except FileNotFoundError:
        pass

    snaps = []
    with open(metrics_path) as h:
        for line in h:
            try:
                snaps.append(json.loads(line))
            except ValueError:
                pass
    # Guarded indexing: a short or failed run may leave < 2 snapshots.
    if len(snaps) >= 2:
        first, mid = snaps[0], snaps[-2]
    elif snaps:
        first = mid = snaps[0]
    else:
        first = mid = {"procs": []}
    last = snaps[-1] if snaps else {"procs": []}

    def rss_total(s):
        return sum(p.get("rss_kb", 0) for p in s["procs"])

    def fds_total(s):
        return sum(p.get("fds", 0) for p in s["procs"])

    def vmdata_total(s):
        return sum(p.get("vmdata_kb", 0) for p in s["procs"])

    lat = sorted(latencies)
    actual_secs = t_end - t_start
    load_mins = SOAK_SECS / 60.0
    rss_growth_mib = (rss_total(mid) - rss_total(first)) / 1024
    # RSS leak test: after the idle settle, RSS must return to near the start
    # baseline. Growth during load is normal (event buffers, caches); a leak
    # would keep RSS elevated after idle.
    rss_after_idle_mib = rss_total(last) / 1024
    rss_leak_mib = rss_after_idle_mib - rss_total(first) / 1024
    fd_growth = fds_total(mid) - fds_total(first)
    # FD leak test: after the idle settle, FDs must return to near the start
    # baseline. Transient growth during load is normal (sockets/pipes for
    # active calls); a leak would keep FDs elevated after idle.
    fd_after_idle = fds_total(last)
    fd_leak = fd_after_idle - fds_total(first)

    checks = [
        ("tool-call errors == 0", counters["errors"] == 0),
        ("ring buffer within 512-event bound", bool(rb.get("ok"))),
        ("ring buffer actually evicted (bound exercised)", bool(rb.get("evicted"))),
        ("RSS growth < 200 MiB over load (transient)", rss_growth_mib < 200),
        ("no RSS leak: after-idle RSS <= start + 50 MiB", rss_leak_mib <= 50),
        ("fd growth < 150 over load (transient)", fd_growth < 150),
        ("no fd leak: after-idle FDs <= start + 20", fd_leak <= 20),
        ("no review-log lock 30 s timeouts", lock_timeouts == 0),
    ]
    verdict = "PASS" if all(ok for _, ok in checks) else "FAIL"

    with open(report_path, "w") as h:
        h.write("# Host soak report — %s\n\n" % TS)
        h.write("Soak: %d s requested load + 120 s idle; actual wall-clock %.0f s "
                "(setup + load + idle + ring check). %d sessions x %d workers, "
                "debug binaries.\n\n"
                % (SOAK_SECS, actual_secs, N_SESSIONS, N_WORKERS))
        h.write("## Load\n\n")
        h.write("- tool calls: allow=%d ask=%d cancels=%d events_polled=%d errors=%d\n"
                % (counters["allow"], counters["ask"], counters["cancel"],
                   counters["events"], counters["errors"]))
        h.write("## Memory (all supercli processes)\n\n")
        h.write("- RSS: start %d MiB, end of load %d MiB, after idle %d MiB\n"
                % (rss_total(first) // 1024, rss_total(mid) // 1024,
                   rss_total(last) // 1024))
        h.write("- RSS growth over load: %+.1f MiB (%+.2f MiB/min)\n"
                % (rss_growth_mib, rss_growth_mib / load_mins if load_mins else 0))
        h.write("- VmData: start %d MiB, end of load %d MiB\n"
                % (vmdata_total(first) // 1024, vmdata_total(mid) // 1024))
        h.write("- allocator heap: not instrumented in this build; RSS/VmData "
                "are the proxies\n")
        h.write("## File descriptors (all supercli processes)\n\n")
        h.write("- start: %d, end of load: %d, after idle: %d (growth %+.0f)\n"
                % (fds_total(first), fds_total(mid), fds_total(last), fd_growth))
        h.write("## Event ring buffer\n\n")
        h.write("- bound (code): EVENT_BUFFER_CAPACITY = 512 per session\n")
        h.write("- empirical: retained=%s first_seq=%s evicted=%s ok=%s\n"
                % (rb.get("retained"), rb.get("first_seq"), rb.get("evicted"), rb.get("ok")))
        h.write("## Lock contention (tool-call round-trip latency, includes review-log writes)\n\n")
        h.write("- n=%d p50=%.1f ms p99=%.1f ms max=%.1f ms\n"
                % (len(lat), percentile(lat, 50), percentile(lat, 99), max(lat) if lat else 0))
        h.write("- review-log lock 30 s timeouts in serve.log: %d\n" % lock_timeouts)
        if error_log:
            h.write("## Errors (first %d)\n\n" % len(error_log))
            for e in error_log:
                h.write("- t+%.0fs worker=%s %s %s\n"
                        % (e["t"] - t_start, e["worker"], e["kind"], e["detail"]))
        h.write("## Verdict: %s\n\n" % verdict)
        for name, ok in checks:
            h.write("- [%s] %s\n" % ("x" if ok else " ", name))
    log("report: %s verdict=%s" % (report_path, verdict))

    # Teardown: stop every session, kill sidecars, terminate serve, then
    # sweep this driver's process tree. Scoped — never a global pkill.
    for sid in sessions:
        sh([SUPERCLI, "stop", sid])
    kill_all_sidecars()
    serve.terminate()
    try:
        serve.wait(timeout=15)
    except subprocess.TimeoutExpired:
        serve.kill()
        serve.wait(timeout=10)
    kill_process_tree(os.getpid())
    owned = kill_home_processes(HOME)
    if owned:
        log("cleaned %d run-owned processes by SUPERCLI_HOME match: %s" % (len(owned), owned))
    # Remove this run's private home; the report + metrics stay in out/soak.
    import shutil
    shutil.rmtree(HOME, ignore_errors=True)
    log("done")


if __name__ == "__main__":
    main()
