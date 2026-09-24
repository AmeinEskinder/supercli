#!/usr/bin/env python3
"""Driver helpers for scripts/e2e-scenario.sh (Phase 8 S2).

Each subcommand does one fiddly thing the shell script orchestrates:
pinned-HTTPS mobile calls, MCP stdio sidecar flows, review-log polling,
hash-chain verification, and the record-for-record comparison.

Environment required:
  UNPEEL_HOME       private home for this run
  UNPEEL_HOST_BIN   path to the unpeel-host binary
  E2E_TOKEN         the seeded controller bearer token
"""
import hashlib
import http.client
import json
import os
import socket
import sqlite3
import ssl
import subprocess
import sys
import threading
import time

HOME = os.environ["UNPEEL_HOME"]
TOKEN = os.environ["E2E_TOKEN"]
UNPEEL_HOST = os.environ["UNPEEL_HOST_BIN"]


# ---------------------------------------------------------------- mobile ---

def mobile_port():
    with open(os.path.join(HOME, "mobile", "server-port")) as handle:
        return int(handle.read().strip())


def cert_pin():
    with open(os.path.join(HOME, "remote", "tls", "cert.pem")) as handle:
        pem = handle.read()
    return hashlib.sha256(ssl.PEM_cert_to_DER_cert(pem)).hexdigest()


def mobile(method, path, body=None):
    """Pinned-HTTPS mobile request, like the harness's mobile_request."""
    port = mobile_port()
    pin = cert_pin()
    payload = json.dumps(body).encode() if body is not None else None
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE  # the fingerprint pin is the trust decision
    conn = http.client.HTTPSConnection("127.0.0.1", port, timeout=20, context=context)
    try:
        conn.connect()
        served = hashlib.sha256(conn.sock.getpeercert(binary_form=True)).hexdigest()
        if served != pin:
            return 0, {"error": "certificate pin mismatch"}
        conn.request(
            method,
            path,
            body=payload,
            headers={
                "Authorization": "Bearer " + TOKEN,
                "Content-Type": "application/json",
            },
        )
        resp = conn.getresponse()
        raw = resp.read()
        try:
            data = json.loads(raw or b"{}")
        except ValueError:
            data = {}
        return resp.status, data
    except Exception as error:  # noqa: BLE001 - surfaced as a failed check
        return 0, {"error": str(error)}
    finally:
        conn.close()


def cmd_mobile(args):
    method, path = args[0], args[1]
    body = json.loads(args[2]) if len(args) > 2 else None
    status, data = mobile(method, path, body)
    print(status)
    print(json.dumps(data))
    return 0 if status == 200 else 1


def cmd_wait_approval(args):
    timeout = float(args[0])
    end = time.time() + timeout
    while time.time() < end:
        status, boot = mobile("GET", "/mobile/bootstrap")
        if status == 200:
            for approval in boot.get("pendingApprovals", []):
                if approval.get("kind") == "connector":
                    print(json.dumps(approval))
                    return 0
        time.sleep(0.3)
    print("no connector approval appeared", file=sys.stderr)
    return 1


# ------------------------------------------------------------- MCP sidecar ---

class McpSidecar:
    """A real `unpeel-host __mcp__` stdio peer for one session."""

    def __init__(self, session_id):
        env = dict(os.environ, UNPEEL_SESSION_ID=session_id)
        self.proc = subprocess.Popen(
            [UNPEEL_HOST, "__mcp__"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            env=env,
            text=True,
            bufsize=1,
        )
        self._next_id = 0
        self._lock = threading.Lock()
        self._responses = {}
        self._reader = threading.Thread(target=self._read_loop, daemon=True)
        self._reader.start()

    def _read_loop(self):
        for line in self.proc.stdout:
            try:
                msg = json.loads(line)
            except ValueError:
                continue
            if "id" in msg and ("result" in msg or "error" in msg):
                with self._lock:
                    self._responses[msg["id"]] = msg

    def _send(self, payload):
        self.proc.stdin.write(json.dumps(payload) + "\n")
        self.proc.stdin.flush()

    def call(self, method, params=None):
        with self._lock:
            self._next_id += 1
            mid = self._next_id
        msg = {"jsonrpc": "2.0", "id": mid, "method": method}
        if params is not None:
            msg["params"] = params
        self._send(msg)
        return mid

    def notify(self, method, params=None):
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        self._send(msg)

    def wait(self, mid, timeout):
        end = time.time() + timeout
        while time.time() < end:
            with self._lock:
                if mid in self._responses:
                    return self._responses.pop(mid)
            time.sleep(0.1)
        return None

    def kill(self):
        try:
            self.proc.kill()
        except Exception:  # noqa: BLE001
            pass


def sidecar_ready(car):
    init = car.call(
        "initialize",
        {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "e2e-scenario", "version": "0"},
        },
    )
    if car.wait(init, 15) is None:
        raise RuntimeError("sidecar did not answer initialize")
    car.notify("notifications/initialized")


def cmd_mcp_ask(args):
    """Call asky.echo (Ask policy); answer the mobile approval; print result."""
    session_id = args[0]
    car = McpSidecar(session_id)
    try:
        sidecar_ready(car)
        mid = car.call("tools/call", {"name": "asky.echo", "arguments": {"msg": "ask-hello"}})

        def answerer():
            end = time.time() + 60
            while time.time() < end:
                status, boot = mobile("GET", "/mobile/bootstrap")
                if status == 200:
                    for approval in boot.get("pendingApprovals", []):
                        if (
                            approval.get("kind") == "connector"
                            and approval.get("callerSessionID") == session_id
                        ):
                            astatus, _ = mobile(
                                "POST",
                                "/mobile/approvals/answer",
                                {"id": approval["id"], "approved": True},
                            )
                            print("answered approval %s -> %s" % (approval["id"], astatus),
                                  file=sys.stderr)
                            return
                time.sleep(0.3)
            print("approval never appeared", file=sys.stderr)

        watcher = threading.Thread(target=answerer, daemon=True)
        watcher.start()
        resp = car.wait(mid, 120)
        watcher.join(timeout=5)
        if resp is None:
            print("tools/call timed out", file=sys.stderr)
            return 1
        print(json.dumps(resp))
        if "error" in resp:
            return 1
        texts = [
            item.get("text", "")
            for item in resp.get("result", {}).get("content", [])
            if isinstance(item, dict)
        ]
        return 0 if any("ask-hello" in t for t in texts) else 1
    finally:
        car.kill()


def cmd_mcp_allow(args):
    """Call allowy.echo (Allow policy); print result."""
    session_id = args[0]
    car = McpSidecar(session_id)
    try:
        sidecar_ready(car)
        mid = car.call(
            "tools/call", {"name": "allowy.echo", "arguments": {"msg": "allow-hello"}}
        )
        resp = car.wait(mid, 60)
        if resp is None:
            print("tools/call timed out", file=sys.stderr)
            return 1
        print(json.dumps(resp))
        if "error" in resp:
            return 1
        texts = [
            item.get("text", "")
            for item in resp.get("result", {}).get("content", [])
            if isinstance(item, dict)
        ]
        return 0 if any("allow-hello" in t for t in texts) else 1
    finally:
        car.kill()


def session_dir(session_id):
    return os.path.join(HOME, "app-sessions", session_id)


def review_lines(session_id):
    path = os.path.join(session_dir(session_id), "action-reviews.jsonl")
    try:
        with open(path) as handle:
            return [json.loads(line) for line in handle if line.strip()]
    except FileNotFoundError:
        return []


def inflight_review_for_tool(session_id, tool, actor=None):
    """Approved review with no outcome yet, for one tool name (and actor)."""
    entries = review_lines(session_id)
    resolved = {
        e.get("review_id") for e in entries if e.get("type") == "attempt_outcome"
    }
    for entry in entries:
        if (
            entry.get("type") != "attempt_outcome"
            and entry.get("decision") == "approved"
            and entry.get("tool") == tool
            and entry.get("review_id") not in resolved
            and (actor is None or entry.get("actor") == actor)
        ):
            return entry["review_id"]
    return None


def cmd_mcp_slow(args):
    """Call slowy.sleep (Allow); print INFLIGHT <review_id> once the review
    is durably in-flight, then wait for the call to settle (the orchestrator
    cancels and kills us first)."""
    session_id, seconds = args[0], args[1]
    car = McpSidecar(session_id)
    try:
        sidecar_ready(car)
        mid = car.call(
            "tools/call",
            {"name": "slowy.sleep", "arguments": {"seconds": seconds}},
        )
        end = time.time() + 30
        rid = None
        while time.time() < end and rid is None:
            rid = inflight_review_for_tool(session_id, "slowy.sleep")
            if rid is None:
                time.sleep(0.2)
        if rid is None:
            print("review never went in-flight", file=sys.stderr)
            return 1
        print("INFLIGHT %s" % rid, flush=True)
        resp = car.wait(mid, 60)
        print("call settled: %s" % json.dumps(resp), file=sys.stderr)
        return 0
    finally:
        car.kill()


def cmd_wait_inflight(args):
    """Wait for an in-flight review for TOOL on SESSION_ID; print review_id.
    Optional 4th arg restricts to one actor (e.g. scheduled:e2e-takeover)."""
    session_id, tool, timeout = args[0], args[1], float(args[2])
    actor = args[3] if len(args) > 3 else None
    end = time.time() + timeout
    while time.time() < end:
        rid = inflight_review_for_tool(session_id, tool, actor)
        if rid:
            print(rid)
            return 0
        time.sleep(0.3)
    print("no in-flight review for %s" % tool, file=sys.stderr)
    return 1


# ------------------------------------------------------- hash chain verify ---

def cmd_verify_chain(args):
    """Reimplement verify_review_chain faithfully: each stored line is a
    serde_json `json!` value, and serde_json is built WITHOUT preserve_order,
    so keys serialize in alphabetical (BTreeMap) order. The canonical bytes
    are the stored object minus `entry_hash`, dumped compact with sorted
    keys; sha256 over those bytes must equal `entry_hash`, and each
    `prev_hash` must link to the previous entry's hash from 'genesis'."""
    sdir = args[0]
    path = os.path.join(sdir, "action-reviews.jsonl")
    prev = "genesis"
    count = 0
    with open(path) as handle:
        for lineno, line in enumerate(handle, 1):
            if not line.strip():
                continue
            entry = json.loads(line)
            if entry.get("prev_hash") != prev:
                print("line %d: broken link" % lineno, file=sys.stderr)
                return 1
            canon = {k: v for k, v in entry.items() if k != "entry_hash"}
            blob = json.dumps(
                canon, sort_keys=True, separators=(",", ":"), ensure_ascii=False
            ).encode()
            if hashlib.sha256(blob).hexdigest() != entry.get("entry_hash"):
                print("line %d: hash mismatch" % lineno, file=sys.stderr)
                return 1
            prev = entry["entry_hash"]
            count += 1
    print(count)
    return 0


# ------------------------------------------------- record-for-record compare ---

def cmd_compare(args):
    """Correlate action-reviews.jsonl, connectors-audit.jsonl and the
    /mobile/events stream by review_id; report per-attempt agreement."""
    session_id = args[0]
    sdir = session_dir(session_id)
    entries = review_lines(session_id)

    audits = {}
    audit_path = os.path.join(sdir, "connectors-audit.jsonl")
    try:
        with open(audit_path) as handle:
            for line in handle:
                if not line.strip():
                    continue
                audit = json.loads(line)
                audits.setdefault(audit.get("review_id"), []).append(audit)
    except FileNotFoundError:
        pass

    status, events_body = mobile(
        "GET", "/mobile/events?session_id=%s&after_seq=0&limit=1024" % session_id
    )
    if status != 200:
        print("events poll failed: %s" % events_body, file=sys.stderr)
        return 1
    events_by_review = {}
    for event in events_body.get("events", []):
        rid = event.get("review_id")
        if rid:
            events_by_review.setdefault(rid, []).append(event.get("kind"))

    reviews = {}
    outcomes = {}
    for entry in entries:
        rid = entry.get("review_id")
        if entry.get("type") == "attempt_outcome":
            outcomes[rid] = entry
        else:
            reviews[rid] = entry

    problems = []
    rows = []
    for rid, review in sorted(reviews.items()):
        decision = review.get("decision")
        tool = review.get("tool")
        actor = review.get("actor")
        outcome = outcomes.get(rid)
        outcome_kind = outcome.get("outcome") if outcome else None
        audit_outcomes = sorted(
            {a.get("outcome") for a in audits.get(rid, []) if a.get("outcome")}
        )
        kinds = sorted(set(events_by_review.get(rid, [])))
        rows.append(
            "review %s tool=%s actor=%s decision=%s outcome=%s audit=%s events=%s"
            % (rid[:8], tool, actor, decision, outcome_kind, audit_outcomes, kinds)
        )
        if decision != "approved":
            continue
        if outcome is None:
            rows.append("  -> INFO: still in-flight (no outcome recorded)")
            continue
        if outcome_kind == "executed":
            if "ok" not in audit_outcomes and "definite_failed" not in audit_outcomes:
                problems.append("%s: executed but no audit line" % rid[:8])
            if "tool.executed" not in kinds:
                problems.append("%s: executed but no tool.executed event" % rid[:8])
        elif outcome_kind == "ambiguous":
            if "tool.ambiguous" not in kinds or "needs_review" not in kinds:
                problems.append(
                    "%s: ambiguous but missing tool.ambiguous/needs_review events" % rid[:8]
                )
            # A cancelled-then-killed attempt legitimately has no audit line:
            # the attempt never completed, so nothing was audited.
            if not audit_outcomes:
                rows.append("  -> INFO: ambiguous with no audit line (attempt never completed)")
    for row in rows:
        print(row)
    if problems:
        print("MISMATCHES:", file=sys.stderr)
        for problem in problems:
            print("  " + problem, file=sys.stderr)
        return 1
    print("all %d approved reviews agree across the three records" % len(reviews))
    return 0


# ----------------------------------------------------------------- leases ---

def cmd_expire_lease(args):
    """Force a schedule lease to lapse (simulates a crashed worker's expiry)."""
    schedule_id = args[0]
    db = os.path.join(HOME, "schedule-leases.db")
    conn = sqlite3.connect(db)
    try:
        past = int(time.time() * 1000) - 60_000
        cur = conn.execute(
            "UPDATE schedule_leases SET expires_at_ms = ? "
            "WHERE tenant = 'default' AND schedule_id = ?",
            (past, schedule_id),
        )
        conn.commit()
        print("rows updated: %d" % cur.rowcount)
        return 0 if cur.rowcount == 1 else 1
    finally:
        conn.close()


def cmd_wait_scheduled_record(args):
    """Wait for a scheduled-runs.jsonl record with outcome needs_review."""
    sdir, timeout = args[0], float(args[1])
    path = os.path.join(sdir, "scheduled-runs.jsonl")
    end = time.time() + timeout
    seen = 0
    while time.time() < end:
        try:
            with open(path) as handle:
                lines = [line for line in handle if line.strip()]
        except FileNotFoundError:
            lines = []
        for line in lines[seen:]:
            record = json.loads(line)
            if record.get("outcome") == "needs_review":
                print(json.dumps(record))
                return 0
        seen = len(lines)
        time.sleep(0.5)
    print("no needs_review scheduled record appeared", file=sys.stderr)
    return 1


COMMANDS = {
    "mobile": cmd_mobile,
    "wait-approval": cmd_wait_approval,
    "mcp-ask": cmd_mcp_ask,
    "mcp-allow": cmd_mcp_allow,
    "mcp-slow": cmd_mcp_slow,
    "wait-inflight": cmd_wait_inflight,
    "verify-chain": cmd_verify_chain,
    "compare": cmd_compare,
    "expire-lease": cmd_expire_lease,
    "wait-scheduled-record": cmd_wait_scheduled_record,
}


def main(argv):
    if len(argv) < 2 or argv[1] not in COMMANDS:
        print("usage: %s <%s> ..." % (argv[0], "|".join(sorted(COMMANDS))), file=sys.stderr)
        return 2
    try:
        return COMMANDS[argv[1]](argv[2:])
    except Exception as error:  # noqa: BLE001 - fail loudly, never silently
        print("helper %s failed: %s" % (argv[1], error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
