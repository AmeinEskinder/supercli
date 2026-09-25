#!/usr/bin/env bash
# Phase 8 S2 — real-binary end-to-end scenario.
#
# Runs the whole loop against real built binaries on a private short-path
# SUPERCLI_HOME (never the real ~/.supercli, never /Applications/Unpeel.app):
#   1. start the Host (`supercli serve`)
#   2. pair a client through the real pairing protocol
#   3. run a scheduled session (`schedule add` + `schedule run-once`)
#   4. exercise an Ask tool and approve it from the paired controller path
#   5. exercise an Allow tool
#   6. cancel during a genuine in-flight tool call (Ambiguous + needs_review)
#   7. exercise worker takeover with two real `schedule daemon` processes
#      (worker A is SIGKILLed mid-run; its lease row in the real
#      schedule-leases.db is then expired with a real SQL UPDATE to simulate
#      the 10-minute TTL elapsing after a crash — waiting the full TTL is
#      impractical, and the lease store, the claim protocol and the
#      takeover/escalation logic are all the real ones)
#   8. verify the action-review hash chain byte-for-byte
#   9. compare review records, connector audit records and event-stream
#      records record-for-record
#
# No mocks, no synthetic session state, no unit-test-only paths. Anything
# the script cannot do for real is reported as a failure, not skipped.
#
# Exit 0 only if every check passed.

set -u

# Ensure cargo is on PATH for the pair_client build.
export PATH="$HOME/.cargo/bin:$PATH"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# R3: configurable profile (debug/release). Default: debug.
# Usage: ./e2e-scenario.sh [debug|release]
PROFILE="${1:-debug}"
if [ "$PROFILE" != "debug" ] && [ "$PROFILE" != "release" ]; then
    echo "Usage: $0 [debug|release]" >&2
    exit 1
fi
SUPERCLI="$ROOT/crates/target/$PROFILE/supercli"
SUPERCLI_HOST="$ROOT/crates/target/$PROFILE/supercli-host"
HELPER="$ROOT/scripts/e2e-scenario-helpers.py"

E2E_HOME="/home/hatch/e2e-supercli-$$"
CONN_DIR="$E2E_HOME/connectors"
LOG_DIR="$E2E_HOME/logs"

export SUPERCLI_HOME="$E2E_HOME"
export SUPERCLI_CONNECTORS_DIR="$CONN_DIR"
export SUPERCLI_CONNECTORS_KEYCHAIN=memory
export SUPERCLI_TEST=1
export SUPERCLI_HOST_BIN="$SUPERCLI_HOST"
export E2E_TOKEN=""

PASS=0
FAIL=0

# --- Process identity before any signal (repo invariant) ---
# A bare pid can be reused by the kernel after the original process exits,
# so every kill in this script verifies the recorded kernel start time
# (ms since the epoch — the same definition as supercli-core's
# `process_start_time_ms`: /proc/<pid>/stat field 22 in clock ticks since
# boot, plus /proc/stat btime) before signaling. Ambiguous ownership fails
# closed: no signal is sent.
proc_start_ms() {
    # $1 = pid. Prints the kernel start time in ms since the epoch, or
    # nothing (nonzero exit) if the process is gone or unreadable.
    local pid="$1" stat rest
    stat="$(cat "/proc/$pid/stat" 2>/dev/null)" || return 1
    # stat field 22 is starttime; comm may contain spaces/parens, so parse
    # from after the LAST ')'. The remaining fields start at stat field 3,
    # making starttime the 20th.
    rest="${stat##*)}"
    local start_ticks ticks_per_sec btime
    # shellcheck disable=SC2086
    set -- $rest
    start_ticks="${20:?}"
    ticks_per_sec="$(getconf CLK_TCK)" || return 1
    btime="$(awk '/^btime /{print $2}' /proc/stat)" || return 1
    [ -n "$start_ticks" ] && [ -n "$btime" ] || return 1
    echo $(( btime * 1000 + start_ticks * 1000 / ticks_per_sec ))
}

# record_pid <pid>: print "<pid> <start_ms>" for pidfiles.
record_pid() {
    local start
    start="$(proc_start_ms "$1")" || start="unknown"
    echo "$1 $start"
}

# safe_kill <pid> <recorded_start_ms> [signal]: signal only if the live
# process <pid> still has the recorded kernel start time.
safe_kill() {
    local pid="$1" recorded="$2" sig="${3:--9}" now
    now="$(proc_start_ms "$pid")" || {
        echo "safe_kill: pid $pid already gone; not signaling"
        return 0
    }
    if [ "$now" != "$recorded" ]; then
        echo "safe_kill: pid $pid start time changed ($recorded -> $now); NOT signaling (possible pid reuse)"
        return 0
    fi
    kill "$sig" "$pid" 2>/dev/null || true
}

# This script's own start time: pattern-based cleanup kills only processes
# at least this young, so a stale pattern can never hit a pre-existing
# system process.
SCRIPT_START_MS="$(proc_start_ms $$)" || {
    echo "FATAL: cannot read own kernel start time; refusing to run kills without identity" >&2
    exit 1
}

# pkill_bounded <pattern> [signal]: like pkill -f, but only signals
# processes whose kernel start time is >= this script's start.
pkill_bounded() {
    local pattern="$1" sig="${2:--9}" pid started
    for pid in $(pgrep -f "$pattern" 2>/dev/null); do
        [ "$pid" = "$$" ] && continue
        started="$(proc_start_ms "$pid")" || continue
        if [ "$started" -ge "$SCRIPT_START_MS" ]; then
            kill "$sig" "$pid" 2>/dev/null || true
        else
            echo "pkill_bounded: skipping pid $pid (started $started, before this run $SCRIPT_START_MS)"
        fi
    done
}

pass() { PASS=$((PASS + 1)); echo "PASS: $1"; }
fail() { FAIL=$((FAIL + 1)); echo "FAIL: $1${2:+ -- $2}"; }
die() { echo "FATAL: $1"; cleanup; exit 1; }

cleanup() {
    # Best-effort teardown of everything this script started. Every signal
    # verifies the recorded kernel start time first (see above).
    for pidfile in "$E2E_HOME"/pids/*.pid; do
        [ -f "$pidfile" ] || continue
        # Pidfiles hold "<pid> <start_ms>" (see record_pid).
        read -r cpid cstart < "$pidfile"
        if [ -z "${cstart:-}" ] || [ "$cstart" = "unknown" ]; then
            echo "cleanup: $pidfile has no recorded start time; skipping (fail closed)"
            continue
        fi
        safe_kill "$cpid" "$cstart" -9
    done
    # Orphaned connector stubs can only belong to this run (unique home path),
    # and only processes younger than this script are signaled.
    pkill_bounded "$CONN_DIR" -9
    # The session host: stop the session properly, then reap its
    # __pty_core__/__remote__ children (orphaned when the host dies).
    if [ -n "${SID:-}" ]; then
        "$SUPERCLI" stop "$SID" >/dev/null 2>&1 || true
    fi
    pkill_bounded "supercli-host (__pty_core__|__remote__)" -9
    sleep 1
}
trap cleanup EXIT

[ -x "$SUPERCLI" ] || die "supercli binary missing at $SUPERCLI (build first)"
[ -x "$SUPERCLI_HOST" ] || die "supercli-host binary missing at $SUPERCLI_HOST (build first)"

mkdir -p "$E2E_HOME/pids" "$LOG_DIR" "$CONN_DIR" "$E2E_HOME/mobile" "$E2E_HOME/mcp"
chmod 700 "$E2E_HOME"

echo "== S2 setup: private home $E2E_HOME"

# ---------------------------------------------------------------- connectors
# Three real MCP-stdio connectors: an Ask echo, an Allow echo, and an Allow
# tool that visibly begins execution and sleeps (for cancel + takeover).
write_connector() {
    # $1 = dir name, $2 = tool name, $3 = description, $4 = policy (ask/allow), $5 = sleep seconds
    local dir="$CONN_DIR/$1" tool="$2" desc="$3" policy="$4" sleepsecs="$5"
    mkdir -p "$dir"
    cat > "$dir/connector.toml" <<EOF
[connector]
name = "$1"
version = "1.0.0"
display_name = "$1 e2e connector"
description = "$desc"
kind = "mcp-stdio"

[auth]
flow = "none"

[tools]
provides = ["$tool"]
EOF
    if [ "$policy" = "allow" ]; then
        printf '\n[policy]\n"%s" = "allow"\n' "$tool" >> "$dir/connector.toml"
    fi
    cat > "$dir/stub.py" <<EOF
import json, sys, time
TOOLS = {"$tool": {"sleep": $sleepsecs}}
def respond(rid, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": rid, "result": result}) + "\n")
    sys.stdout.flush()
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method = msg.get("method")
    rid = msg.get("id")
    if method == "initialize":
        respond(rid, {"protocolVersion": "2024-11-05", "serverInfo": {"name": "$1"}})
    elif method == "tools/list":
        respond(rid, {"tools": [{"name": t, "inputSchema": {"type": "object"}} for t in TOOLS]})
    elif method == "tools/call":
        name = msg["params"]["name"]
        args = msg["params"].get("arguments", {}) or {}
        if name in TOOLS:
            secs = args.get("seconds", TOOLS[name]["sleep"])
            time.sleep(max(0, min(float(secs), 120)))
            respond(rid, {"content": [{"type": "text", "text": "echo:" + json.dumps(args)}]})
        else:
            sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": rid, "error": {"code": -32601, "message": "unknown tool"}}) + "\n")
            sys.stdout.flush()
    elif rid is not None:
        respond(rid, {})
EOF
    printf '#!/usr/bin/env bash\nexec python3 "$(dirname "$0")/stub.py"\n' > "$dir/connector"
    chmod +x "$dir/connector"
}
write_connector "asky"   "asky.echo"   "Ask-policy echo connector"   "ask"   0
write_connector "allowy"  "allowy.echo" "Allow-policy echo connector" "allow" 0
write_connector "slowy"   "slowy.sleep" "Allow-policy slow connector" "allow" 0
# slowy's own sleep comes from the call argument; stub default 0 is unused.
pass "connector fixtures written (asky/Ask, allowy/Allow, slowy/Allow-slow)"

# ------------------------------------------------------- pre-serve home files
# The mobile auth + TLS identity are created by the real serve. The
# controller is paired through the genuine sealed /mobile/pair exchange
# in step 2 below (no devices.json seeding); the Host's pairing route
# writes the real devices.json.
MOBILE_PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
echo "$MOBILE_PORT" > "$E2E_HOME/mobile/server-port"
# The shared MCP auth token is minted by supercli-host on first use; pre-write
# it here so the hook listener and the sidecar trivially agree.
python3 -c 'import secrets;print(secrets.token_hex(32))' > "$E2E_HOME/mcp/auth-token"
chmod 600 "$E2E_HOME/mcp/auth-token"

# ------------------------------------------------------------------ 1. serve
echo "== S2 step 1: start the Host"
"$SUPERCLI" serve >"$LOG_DIR/serve.log" 2>&1 &
SERVE_PID=$!
record_pid "$SERVE_PID" > "$E2E_HOME/pids/serve.pid"

READY=0
for _ in $(seq 1 40); do
    if [ -f "$E2E_HOME/serve.json" ] && python3 -c "
import json,sys
d=json.load(open('$E2E_HOME/serve.json'))
sys.exit(0 if d.get('pid') and d.get('hookPort') else 1)" 2>/dev/null; then
        READY=1; break
    fi
    sleep 0.5
done
[ "$READY" = 1 ] || die "serve did not become ready (see $LOG_DIR/serve.log)"
pass "Host started (pid from serve.json, hookPort present)"
# The mobile TLS listener starts only once pairing is requested (the Host
# serves /mobile only for paired devices or an open pairing window); step 2
# opens the pairing window first, then waits for the listener.

# ------------------------------------------------------- 2. pair the client
# Genuine sealed /mobile/pair exchange: `supercli pair` opens the pairing
# window on the running Host via the local gateway (Unix socket) and prints
# the QR code; the Host then starts the mobile listener. The pair_client
# example (supercli-client) runs the controller half of the sealed exchange
# (HKDF-SHA256 + AES-256-GCM, phone-to-mac / mac-to-phone) and returns the
# Host-issued auth token. The Host's pairing route writes devices.json.
echo "== S2 step 2: pair a client (genuine sealed /mobile/pair exchange)"
"$SUPERCLI" pair --advertise-host 127.0.0.1 --advertise-port "$MOBILE_PORT" >"$LOG_DIR/pair.log" 2>&1 &
PAIR_PID=$!
record_pid "$PAIR_PID" > "$E2E_HOME/pids/pair.pid"
# Wait for the QR code line, then extract it.
QR=""
for _ in $(seq 1 40); do
    QR="$(grep -m1 '^SUPERCLI:' "$LOG_DIR/pair.log" 2>/dev/null || true)"
    [ -n "$QR" ] && break
    sleep 0.5
done
[ -n "$QR" ] || die "supercli pair produced no QR code (see $LOG_DIR/pair.log)"
pass "pairing window opened, QR code captured"
# The Host starts the mobile listener once the pairing window is open.
READY=0
for _ in $(seq 1 40); do
    if [ -f "$E2E_HOME/remote/tls/cert.pem" ] && python3 - "$MOBILE_PORT" <<'EOF'
import socket, ssl, sys
port = int(sys.argv[1])
ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
ctx.check_hostname = False
ctx.verify_mode = ssl.CERT_NONE
try:
    conn = ctx.wrap_socket(socket.create_connection(("127.0.0.1", port), timeout=3))
    conn.close()
    sys.exit(0)
except Exception:
    sys.exit(1)
EOF
    then READY=1; break; fi
    sleep 0.5
done
[ "$READY" = 1 ] || die "mobile TLS listener never came up on $MOBILE_PORT"
pass "mobile HTTPS listener up with certificate"
# Build the pairing client if needed, then run the sealed exchange.
PAIR_CLIENT_BIN="$ROOT/crates/target/$PROFILE/examples/pair_client"
if [ ! -x "$PAIR_CLIENT_BIN" ]; then
    (cd "$ROOT/crates" && cargo build --profile "$PROFILE" -p supercli-client --example pair_client \
        >"$LOG_DIR/pair-client-build.log" 2>&1) \
        || die "pair_client build failed (see $LOG_DIR/pair-client-build.log)"
fi
PAIR_OUT="$("$PAIR_CLIENT_BIN" "$QR" 2>"$LOG_DIR/pair-client.log")" \
    || die "sealed pairing exchange failed (see $LOG_DIR/pair-client.log)"
E2E_TOKEN="$(echo "$PAIR_OUT" | python3 -c 'import json,sys;print(json.load(sys.stdin)["auth_token"])')"
[ -n "$E2E_TOKEN" ] || die "pairing returned no auth token: $PAIR_OUT"
export E2E_TOKEN
pass "sealed /mobile/pair exchange completed, Host-issued auth token received"
# The `supercli pair` process should exit on its own once pairing completes.
for _ in $(seq 1 20); do
    kill -0 "$PAIR_PID" 2>/dev/null || break
    sleep 0.5
done
# The Host's pairing route must have persisted the device record.
[ -f "$E2E_HOME/mobile/devices.json" ] || die "devices.json not written by pairing route"
python3 -c "
import json,sys
d = json.load(open('$E2E_HOME/mobile/devices.json'))
devs = d.get('devices', [])
assert any(x.get('id') == 'e2e-controller' for x in devs), 'e2e-controller not in devices.json'
print('devices.json has e2e-controller')
" || die "devices.json missing e2e-controller"
pass "devices.json written by the real pairing route (e2e-controller present)"
BOOT="$("$HELPER" mobile GET /mobile/bootstrap)"
[ "$(echo "$BOOT" | head -1)" = "200" ] || die "bootstrap failed: $BOOT"
pass "paired controller authenticated to /mobile/bootstrap (200)"

# ------------------------------------------------------- create the session
echo "== S2: create session + attach connectors"
NEW_OUT="$("$SUPERCLI" new --command "sleep 600" --json 2>"$LOG_DIR/new.log")"
SID="$(echo "$NEW_OUT" | python3 -c 'import json,sys;print(json.load(sys.stdin)["id"])')"
[ -n "$SID" ] || die "supercli new produced no session id: $NEW_OUT"
SESSION_DIR="$E2E_HOME/app-sessions/$SID"
[ -f "$SESSION_DIR/manifest.json" ] || die "no session manifest for $SID"
pass "session created: $SID"
for c in asky allowy slowy; do
    "$SUPERCLI" connector enable "$c" --session "$SID" >"$LOG_DIR/enable-$c.log" 2>&1 \
        || die "connector enable $c failed (see $LOG_DIR/enable-$c.log)"
done
pass "connectors attached via real CLI (asky, allowy, slowy)"

# ------------------------------------------------- 4. Ask tool + approval
echo "== S2 step 4: Ask tool, approved from the paired controller path"
ASK_OUT="$("$HELPER" mcp-ask "$SID" 2>"$LOG_DIR/ask.log")"
ASK_RC=$?
echo "$ASK_OUT" | grep -q "ask-hello" || fail "ask tool result" "rc=$ASK_RC log=$LOG_DIR/ask.log"
[ $ASK_RC = 0 ] && pass "Ask tool executed after mobile approval"
ASK_RID="$(python3 - "$SESSION_DIR" <<'EOF'
import json,sys
sdir = sys.argv[1]
rid = None
with open(sdir + "/action-reviews.jsonl") as h:
    for line in h:
        e = json.loads(line)
        if e.get("tool") == "asky.echo" and e.get("type") != "attempt_outcome":
            rid = e["review_id"]; actor = e["actor"]; decision = e["decision"]
print(rid or "")
print(actor if rid else "", file=sys.stderr)
print(decision if rid else "", file=sys.stderr)
EOF
)"
[ -n "$ASK_RID" ] || die "no review for the Ask tool call"
ASK_ACTOR="$(python3 - "$SESSION_DIR" <<'EOF'
import json,sys
sdir = sys.argv[1]
for line in open(sdir + "/action-reviews.jsonl"):
    e = json.loads(line)
    if e.get("tool") == "asky.echo" and e.get("type") != "attempt_outcome":
        print(e["actor"]); break
EOF
)"
[ "$ASK_ACTOR" = "human:paired-device" ] \
    && pass "Ask review actor is human:paired-device" \
    || fail "Ask review actor" "got $ASK_ACTOR"

# ------------------------------------------------------------ 5. Allow tool
echo "== S2 step 5: Allow tool"
ALLOW_OUT="$("$HELPER" mcp-allow "$SID" 2>"$LOG_DIR/allow.log")"
echo "$ALLOW_OUT" | grep -q "allow-hello" || fail "allow tool result" "log=$LOG_DIR/allow.log"
pass "Allow tool executed without a prompt"
ALLOW_ACTOR="$(python3 - "$SESSION_DIR" <<'EOF'
import json,sys
sdir = sys.argv[1]
for line in open(sdir + "/action-reviews.jsonl"):
    e = json.loads(line)
    if e.get("tool") == "allowy.echo" and e.get("type") != "attempt_outcome":
        print(e["actor"]); break
EOF
)"
[ "$ALLOW_ACTOR" = "policy:allow" ] \
    && pass "Allow review actor is policy:allow" \
    || fail "Allow review actor" "got $ALLOW_ACTOR"

# ---------------------------------------------------- 3. scheduled session
echo "== S2 step 3: scheduled session (run-once, autonomous, no human)"
"$SUPERCLI" schedule add --id e2e-sched --session "$SID" --interval 60 \
    --tool allowy.echo --arg msg=sched-hello >"$LOG_DIR/sched-add.log" 2>&1 \
    || die "schedule add failed (see $LOG_DIR/sched-add.log)"
pass "schedule armed (interval 60, explicit --tool)"
"$SUPERCLI" schedule run-once e2e-sched >"$LOG_DIR/sched-run.log" 2>&1 \
    || die "schedule run-once failed (see $LOG_DIR/sched-run.log)"
pass "schedule run-once completed"
SCHED_ACTOR="$(python3 - "$SESSION_DIR" <<'EOF'
import json,sys
sdir = sys.argv[1]
for line in open(sdir + "/action-reviews.jsonl"):
    e = json.loads(line)
    if e.get("actor") == "scheduled:e2e-sched" and e.get("type") != "attempt_outcome":
        print(e["tool"]); break
EOF
)"
[ "$SCHED_ACTOR" = "allowy.echo" ] \
    && pass "scheduled run recorded actor scheduled:e2e-sched" \
    || fail "scheduled run actor" "got $SCHED_ACTOR"
grep -q '"outcome":"completed"' "$SESSION_DIR/scheduled-runs.jsonl" \
    && pass "scheduled-runs.jsonl has a completed record" \
    || fail "scheduled-runs.jsonl completed record" "see $LOG_DIR/sched-run.log"

# ------------------------------------------- 6. cancel a genuine in-flight call
echo "== S2 step 6: cancel during a genuine in-flight tool call"
"$HELPER" mcp-slow "$SID" 20 >"$LOG_DIR/slow.out" 2>"$LOG_DIR/slow.log" &
SLOW_PID=$!
SLOW_START="$(proc_start_ms "$SLOW_PID")"
record_pid "$SLOW_PID" > "$E2E_HOME/pids/slow.pid"
SLOW_RID=""
for _ in $(seq 1 60); do
    if grep -q "^INFLIGHT " "$LOG_DIR/slow.out" 2>/dev/null; then
        SLOW_RID="$(sed -n 's/^INFLIGHT //p' "$LOG_DIR/slow.out" | head -1)"
        break
    fi
    sleep 0.5
done
[ -n "$SLOW_RID" ] || die "slowy.sleep never went in-flight (see $LOG_DIR/slow.log)"
pass "slow tool call observed in-flight (review $SLOW_RID)"
CANCEL_RESP="$("$HELPER" mobile POST /mobile/turn-cancel "{\"sessionID\":\"$SID\",\"reason\":\"e2e cancel test\"}")"
if echo "$CANCEL_RESP" | python3 -c "
import json,sys
status = int(sys.stdin.readline())
body = json.load(sys.stdin)
assert status == 200, body
assert body.get('cancelled') is True, body
assert body.get('idle') is False, body
assert '$SLOW_RID' in body.get('ambiguous_attempts', []), body
"; then
    pass "turn-cancel marked the in-flight attempt ambiguous (idle=false)"
else
    fail "turn-cancel response" "$CANCEL_RESP"
fi
# The sidecar is gone now: kill it before its 10s connector-link timeout can
# write a competing late outcome for the same review. Identity-verified:
# only signal if the kernel start time still matches the spawn record.
safe_kill "$SLOW_PID" "$SLOW_START" -9
sleep 1
AMBIG_REASON="$(python3 - "$SESSION_DIR" "$SLOW_RID" <<'EOF'
import json,sys
sdir, rid = sys.argv[1], sys.argv[2]
for line in open(sdir + "/action-reviews.jsonl"):
    e = json.loads(line)
    if e.get("type") == "attempt_outcome" and e.get("review_id") == rid:
        print(e.get("outcome") + ": " + (e.get("reason") or ""))
        break
EOF
)"
echo "$AMBIG_REASON" | grep -q "^ambiguous:" \
    && pass "durable outcome is Ambiguous ($AMBIG_REASON)" \
    || die "durable ambiguous outcome missing (got: $AMBIG_REASON)"
EVENTS="$("$HELPER" mobile GET "/mobile/events?session_id=$SID&after_seq=0&limit=1024")"
if echo "$EVENTS" | python3 -c "
import json,sys
status = int(sys.stdin.readline())
body = json.load(sys.stdin)
rid = '$SLOW_RID'
assert status == 200, body
kinds = {e.get('kind') for e in body.get('events', []) if e.get('review_id') == rid}
assert 'tool.ambiguous' in kinds, kinds
assert 'needs_review' in kinds, kinds
assert any(e.get('kind') == 'turn.cancelled' for e in body.get('events', [])), 'no turn.cancelled'
"; then
    pass "event stream carries tool.ambiguous + needs_review + turn.cancelled"
else
    fail "cancel events" "see log"
fi

# ------------------------------------------------------- 7. worker takeover
echo "== S2 step 7: worker takeover across two real daemon processes"
"$SUPERCLI" schedule add --id e2e-takeover --session "$SID" --interval 60 \
    --tool slowy.sleep --arg seconds=90 >"$LOG_DIR/sched-add2.log" 2>&1 \
    || die "schedule add (takeover) failed"
"$SUPERCLI" schedule pause e2e-sched >"$LOG_DIR/sched-pause.log" 2>&1 \
    || die "schedule pause failed"
pass "takeover schedule armed; first schedule paused"
"$SUPERCLI" schedule daemon >"$LOG_DIR/daemon-a.log" 2>&1 &
DAEMON_A=$!
DAEMON_A_START="$(proc_start_ms "$DAEMON_A")"
record_pid "$DAEMON_A" > "$E2E_HOME/pids/daemon-a.pid"
echo "daemon A pid $DAEMON_A; waiting for its first due trigger (~60s)..."
A_RID="$("$HELPER" wait-inflight "$SID" "slowy.sleep" 150 "scheduled:e2e-takeover" 2>"$LOG_DIR/wait-a.log")"
[ -n "$A_RID" ] || die "daemon A never ran slowy.sleep in-flight (see $LOG_DIR/daemon-a.log)"
pass "daemon A picked up slowy.sleep (review $A_RID in-flight)"
safe_kill "$DAEMON_A" "$DAEMON_A_START" -9
sleep 1
# Simulate the 10-minute lease TTL elapsing after the SIGKILL crash: expire
# worker A's row in the REAL lease database the daemon used. Everything else
# about the takeover (claim protocol, lapsed-run scan, NeedsReview
# escalation, lease held by the new worker) is the production path.
"$HELPER" expire-lease e2e-takeover >"$LOG_DIR/expire.log" 2>&1 \
    || die "could not expire the lease (see $LOG_DIR/expire.log)"
pass "worker A SIGKILLed; its lease forced to lapse (real lease DB row)"
"$SUPERCLI" schedule daemon >"$LOG_DIR/daemon-b.log" 2>&1 &
DAEMON_B=$!
DAEMON_B_START="$(proc_start_ms "$DAEMON_B")"
record_pid "$DAEMON_B" > "$E2E_HOME/pids/daemon-b.pid"
TAKEOVER_REC="$("$HELPER" wait-scheduled-record "$SESSION_DIR" 180 2>"$LOG_DIR/wait-b.log")"
[ -n "$TAKEOVER_REC" ] || die "daemon B never escalated (see $LOG_DIR/daemon-b.log)"
echo "$TAKEOVER_REC" | grep -q "lapsed" \
    && pass "daemon B took over and escalated to NeedsReview" \
    || fail "takeover escalation record" "$TAKEOVER_REC"
safe_kill "$DAEMON_B" "$DAEMON_B_START" -9

# ------------------------------------------------- 8. verify the hash chain
echo "== S2 step 8: verify the action-review hash chain"
CHAIN_COUNT="$("$HELPER" verify-chain "$SESSION_DIR" 2>"$LOG_DIR/chain.log")"
[ -n "$CHAIN_COUNT" ] || die "hash chain verification failed (see $LOG_DIR/chain.log)"
pass "hash chain verifies: $CHAIN_COUNT entries, canonical sha256 linkage from genesis"

# --------------------------------- 9. record-for-record comparison
echo "== S2 step 9: reviews vs audit vs events, record-for-record"
"$HELPER" compare "$SID" >"$LOG_DIR/compare.log" 2>&1 \
    && pass "review/audit/event records agree record-for-record" \
    || fail "record-for-record comparison" "see $LOG_DIR/compare.log"
cat "$LOG_DIR/compare.log"

# ------------------------------------------------------------------ summary
echo
echo "== S2 summary: $PASS passed, $FAIL failed"
echo "Home kept at: $E2E_HOME"
echo "Status note: the controller was paired through the genuine sealed"
echo "/mobile/pair exchange (HKDF-SHA256 + AES-256-GCM); the Host's pairing"
echo "route wrote devices.json and issued the Bearer <redacted> The 10-minute"
echo "lease TTL was simulated with a real SQL UPDATE on the daemon's lease row."
if [ "$FAIL" -gt 0 ]; then
    echo "S2 FAILED"
    exit 1
fi
# Clean run: remove the private home (kept on failure for forensics).
trap - EXIT
cleanup
if rm -rf "$E2E_HOME" 2>/dev/null && [ ! -e "$E2E_HOME" ]; then
    echo "S2 complete: all checks passed; private home removed."
else
    echo "S2 complete: all checks passed; WARNING: private home NOT fully removed: $E2E_HOME"
    exit 1
fi
