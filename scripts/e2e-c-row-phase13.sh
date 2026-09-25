#!/usr/bin/env bash
# Phase 13 v3 (C): E2E single row, n=2001 (to get >=2000 completed).
# Usage: e2e-c-row-phase13.sh <concurrency> <label>
# Each run uses its own UNPEEL_HOME so rows can run in parallel.
set -u

CONC="$1"
LABEL="$2"

ROOT="$HOME/workspace/muse-harness/unpeel"
CRATES="$ROOT/crates"
UNPEEL="$CRATES/target/release/unpeel"
LOAD="$CRATES/target/release/unpeel-load"
PAIR_CLIENT="$CRATES/target/debug/examples/pair_client"

E2E_HOME="/tmp/e2e-${LABEL}-phase13"
LOG_DIR="$E2E_HOME/logs"
mkdir -p "$E2E_HOME/pids" "$LOG_DIR" "$E2E_HOME/mcp" "$E2E_HOME/mobile"

die() { echo "FATAL [$LABEL]: $*" >&2; exit 1; }
pass() { echo "PASS [$LABEL]: $*"; }

export UNPEEL_HOME="$E2E_HOME"
python3 -c 'import secrets;print(secrets.token_hex(32))' > "$E2E_HOME/mcp/auth-token"
chmod 600 "$E2E_HOME/mcp/auth-token"
MCP_TOKEN=$(cat "$E2E_HOME/mcp/auth-token")
MOBILE_PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
echo "$MOBILE_PORT" > "$E2E_HOME/mobile/server-port"

echo "== [$LABEL] starting Host"
"$UNPEEL" serve >"$LOG_DIR/serve.log" 2>&1 &
echo $! > "$E2E_HOME/pids/serve.pid"
READY=0
for _ in $(seq 1 40); do
    if python3 -c "
import json,sys
d=json.load(open('$E2E_HOME/serve.json'))
sys.exit(0 if d.get('pid') and d.get('hookPort') else 1)" 2>/dev/null; then
        READY=1; break; fi
    sleep 0.5
done
[ "$READY" = 1 ] || die "serve not ready"
HOOK_PORT=$(python3 -c "import json;d=json.load(open('$E2E_HOME/serve.json'));print(d['hookPort'])")
pass "Host started (hookPort $HOOK_PORT)"

echo "== [$LABEL] pairing load controller (sealed /mobile/pair)"
"$UNPEEL" pair --advertise-host 127.0.0.1 --advertise-port "$MOBILE_PORT" >"$LOG_DIR/pair.log" 2>&1 &
PAIR_PID=$!
QR=""
for _ in $(seq 1 40); do
    QR="$(grep -m1 '^UNPEEL:' "$LOG_DIR/pair.log" 2>/dev/null || true)"
    [ -n "$QR" ] && break
    sleep 0.5
done
[ -n "$QR" ] || die "no QR"
for _ in $(seq 1 40); do
    if [ -f "$E2E_HOME/remote/tls/cert.pem" ] && python3 - "$MOBILE_PORT" <<'EOF'
import socket, ssl, sys
port = int(sys.argv[1])
ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
ctx.check_hostname = False; ctx.verify_mode = ssl.CERT_NONE
try:
    conn = ctx.wrap_socket(socket.create_connection(("127.0.0.1", port), timeout=3))
    conn.close(); sys.exit(0)
except Exception: sys.exit(1)
EOF
    then break; fi
    sleep 0.5
done
[ -f "$E2E_HOME/remote/tls/cert.pem" ] || die "mobile TLS cert not created"
MOBILE_PORT=$(cat "$E2E_HOME/mobile/server-port")
[ -n "$MOBILE_PORT" ] || die "no mobile port"
pass "mobile listener on port $MOBILE_PORT"

PAIR_OUT="$("$PAIR_CLIENT" "$QR" 2>"$LOG_DIR/pair-client.log")" || die "pairing failed"
MOBILE_TOKEN=$(python3 -c "import json,sys;d=json.load(sys.stdin);print(d['auth_token'])" <<<"$PAIR_OUT")
[ -n "$MOBILE_TOKEN" ] || die "no token"
pass "controller paired, token in memory"
for _ in $(seq 1 20); do kill -0 "$PAIR_PID" 2>/dev/null || break; sleep 0.5; done

echo "== [$LABEL] c$CONC: n=2001 concurrency=$CONC"
"$LOAD" \
    --hook-port "$HOOK_PORT" \
    --phone-port "$MOBILE_PORT" \
    --mcp-token "$MCP_TOKEN" \
    --mobile-token "$MOBILE_TOKEN" \
    --concurrency "$CONC" \
    --rate 2 \
    --n 2001 \
    --duration 1500 \
    --json-out "$LOG_DIR/${LABEL}.json" \
    >"$LOG_DIR/${LABEL}.log" 2>&1
ec=$?
echo "[$LABEL] exit: $ec"
python3 <<EOF
import json
d = json.load(open('/tmp/e2e-${LABEL}-phase13/logs/${LABEL}.json'))
print(f"[$LABEL] c$CONC: offered={d['offered']} completed={d['completed']} errors={d['errors']}")
print(f"[$LABEL] c$CONC: p50={d['p50_ms']:.1f}ms p95={d['p95_ms']:.1f}ms p99={d['p99_ms']:.1f}ms max={d['max_ms']:.1f}ms")
print(f"[$LABEL] c$CONC: throughput={d['throughput_per_s']:.2f}/s")
EOF
echo "== [$LABEL] done"
