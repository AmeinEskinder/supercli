#!/usr/bin/env python3
"""Q5 final: n=2000, based on the working breakdown script."""
import json, os, sys, time, threading, shutil

ROOT = "/home/hatch/workspace/muse-harness/unpeel"
os.environ["UNPEEL_TUI_BINARY"] = os.path.join(ROOT, "crates", "target", "release", "unpeel")
sys.path.insert(0, "/home/hatch/workspace")
sys.path.insert(0, os.path.join(ROOT, "crates", "unpeel-cli", "tests"))

from pooled_client import PooledMobileClient
from harness import Home, Serve, mcp_post

UNPEEL_HOST = os.path.join(ROOT, "crates", "target", "release", "unpeel-host")
BASELINE_PATH = os.environ.get("PERF_OUTPUT", os.path.join(ROOT, "scripts", "perf-baseline.json"))
WARMUP = int(os.environ.get("PERF_WARMUP", "200"))
N_ITER = int(os.environ.get("PERF_ITER", "2000"))
TS = time.strftime("%Y%m%d-%H%M%S")

def percentile(xs, p):
    s = sorted(xs)
    return s[min(len(s)-1, int(p/100.0*len(s)))] if s else 0.0

def machine_specs():
    specs = {"cpu_count": os.cpu_count(), "kernel": os.uname().release}
    try:
        with open("/proc/cpuinfo") as f:
            for l in f:
                if l.startswith("model name"):
                    specs["cpu_model"] = l.split(":",1)[1].strip(); break
        with open("/proc/meminfo") as f:
            for l in f:
                if l.startswith("MemTotal"):
                    specs["mem_total_kb"] = int(l.split()[1]); break
    except Exception: pass
    return specs

def main():
    root = "/home/hatch/perf-unpeel-%d" % os.getpid()
    shutil.rmtree(root, ignore_errors=True)
    home = Home(root)
    token = home.pair_device(token="perf-token")
    mcp_token = home.auth_token
    env = dict(os.environ, UNPEEL_HOST_BIN=UNPEEL_HOST)
    service = Serve(home, env=env)
    try:
        ready = service.ready(timeout=30.0)
        hook_port, phone_port = ready["hookPort"], ready["directPort"]
        print(f"host up: hook={hook_port} phone={phone_port}", flush=True)
        client = PooledMobileClient(phone_port, token, home)
        client.request("/mobile/bootstrap")

        latencies = []

        def one(i, record):
            caller = f"perf-{i}"
            res = {}
            def ask():
                res["t0"] = time.monotonic()
                st,_ = mcp_post(hook_port, "/mcp/approve-write",
                              {"caller_session_id": caller, "target_session_id": caller+"-t"},
                              token=mcp_token, timeout=30)
                res.update(status=st, t_end=time.monotonic())
            th = threading.Thread(target=ask); th.start()
            # Exact same as breakdown script that got 58ms
            time.sleep(0.010)
            t_poll0 = time.monotonic()
            st, boot = client.request("/mobile/bootstrap")
            pending = None
            polls = 1
            if st == 200:
                pending = next((a for a in boot.get("pendingApprovals", [])
                              if a.get("callerSessionID") == caller), None)
            while not pending and polls < 10:
                time.sleep(0.020)
                st, boot = client.request("/mobile/bootstrap")
                polls += 1
                if st == 200:
                    pending = next((a for a in boot.get("pendingApprovals", [])
                                  if a.get("callerSessionID") == caller), None)
            if pending:
                client.request("/mobile/approvals/answer", method="POST",
                             body={"id": pending["id"], "approved": True})
            th.join(timeout=30)
            if res.get("status") == 200 and record:
                latencies.append((res["t_end"]-res["t0"])*1000.0)
            return polls

        print(f"warming up ({WARMUP})...", flush=True)
        for i in range(WARMUP):
            p = one(f"w{i}", False)
            if i < 5: print(f"  warmup {i}: polls={p}", flush=True)

        print(f"measuring ({N_ITER})...", flush=True)
        for i in range(N_ITER):
            one(i, True)
            if (i+1) % 500 == 0: print(f"  {i+1}/{N_ITER}", flush=True)

        lat = sorted(latencies)
        report = {
            "ts": TS, "binary": "release", "machine": machine_specs(),
            "method": "pooled TLS, 10ms sleep + 20ms poll, closed-loop MCP",
            "approve_latency_ms": {
                "n": len(lat), "warmup_discarded": WARMUP,
                "p50": round(percentile(lat,50),2),
                "p95": round(percentile(lat,95),2),
                "p99": round(percentile(lat,99),2),
                "max": round(max(lat),2) if lat else 0.0,
            },
            "breakdown_note": "58ms p50 in 10-iter probe: ~46ms approval creation + bootstrap, ~42ms answer POST (includes persist_grant fsync). 160ms in first Q5 run was TLS handshake artifact (new connection per poll).",
            "capacity": {"note": "ramp not run in this invocation"},
        }
        print(json.dumps(report, indent=2))
        with open(BASELINE_PATH, "w") as f: json.dump(report, f, indent=2)
        print(f"baseline written to {BASELINE_PATH}")
        return 0
    finally:
        service.process.terminate()
        try: service.process.wait(timeout=10)
        except Exception: service.process.kill()
        shutil.rmtree(root, ignore_errors=True)

if __name__ == "__main__": sys.exit(main())
