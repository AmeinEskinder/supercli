"""`supercli migrate` on a real v0.8 home fixture, end to end.

Builds a v0.8 home with v0.8-isms (legacy bare-string grants, pre-chain
review log, old-schema lease DB, invalid config values), runs
`supercli migrate --apply`, then asserts `supercli doctor` is green and the
review chain verifies.
"""

import sys, os, json, sqlite3, shutil

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run_cli, run  # noqa: E402


class _Home:
    def __init__(self, root):
        self.root = root

    def path(self, *parts):
        return os.path.join(self.root, *parts)


def _write_prechain_log(path):
    """v0.8 review log: entries without prev_hash/entry_hash (pre-chain)."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        for i in range(3):
            f.write(json.dumps({
                "review_id": f"r{i}",
                "ts_ms": 1700000000000 + i,
                "actor": "human:phone-1",
                "connector": "test-connector",
                "tool": "test-tool",
                "args_hash": f"hash{i:04d}",
                "decision": "approved",
            }) + "\n")


def _write_old_lease_db(path):
    """v0.8 lease DB: schema without the lease_generation column."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    conn = sqlite3.connect(path)
    conn.execute("""
        CREATE TABLE IF NOT EXISTS schedule_leases (
            tenant TEXT NOT NULL,
            schedule_id TEXT NOT NULL,
            owner TEXT NOT NULL,
            expires_at_ms INTEGER NOT NULL,
            claimed_at_ms INTEGER NOT NULL,
            PRIMARY KEY (tenant, schedule_id)
        )
    """)
    conn.execute(
        "INSERT INTO schedule_leases VALUES (?, ?, ?, ?, ?)",
        ("test-tenant", "sched-1", "owner-1", 9999999999999, 1000),
    )
    conn.commit()
    conn.close()


def build_v08_fixture(home):
    os.makedirs(home.root, exist_ok=True)
    # app-state.json with v0.8-isms: legacy bare-string grant, invalid
    # config values, an unknown key (warning only).
    state = {
        "mcp_connector_approvals": {
            "legacy-caller": ["legacy-tool"],  # bare string in array: v0.8 grant
            "namespaced": {"connector": "real-conn", "tool": "real-tool"},
        },
        "theme": "blue",  # invalid: must be system/light/dark
        "auto_stop_archive_minutes": 999,  # invalid: not in allowed set
        "mystery_setting": True,  # unknown: warning only, left alone
    }
    with open(home.path("app-state.json"), "w") as f:
        json.dump(state, f)
    # Pre-chain review log.
    _write_prechain_log(home.path("app-sessions", "s1", "action-reviews.jsonl"))
    with open(home.path("app-sessions", "s1", "manifest.json"), "w") as f:
        json.dump({"id": "s1"}, f)
    # Old-schema lease DB + schedules registry.
    _write_old_lease_db(home.path("schedule-leases.db"))
    with open(home.path("schedules.json"), "w") as f:
        json.dump({"schedules": []}, f)


def migrate_v08(case):
    home = case.home
    build_v08_fixture(home)

    # Dry-run first: reports, changes nothing.
    r = run_cli(home, ["migrate"], expect_ok=True)
    case.check("migrate dry-run exits 0", r.returncode == 0)
    out = r.stdout + r.stderr
    case.check("dry-run reports legacy grants", "legacy" in out.lower())
    case.check("dry-run reports invalid config", "invalid" in out.lower())

    # Apply.
    r = run_cli(home, ["migrate", "--apply"], expect_ok=True)
    case.check("migrate --apply exits 0", r.returncode == 0)

    # Config check passes now.
    r = run_cli(home, ["config", "check"], expect_ok=True)
    case.check("config check passes after migrate", r.returncode == 0)

    # Doctor is green.
    r = run_cli(home, ["doctor", "--json"], expect_ok=True)
    try:
        doc = json.loads(r.stdout)
    except Exception:
        doc = {}
    failed = doc.get("failed", 1)
    case.check("doctor green after migrate", failed == 0, f"doctor: {r.stdout[:500]}")

    # Chain verifies: the re-chained log must verify via doctor's chain check
    # (covered by doctor green above); assert the log now has chain metadata.
    log_path = home.path("app-sessions", "s1", "action-reviews.jsonl")
    with open(log_path) as f:
        lines = [json.loads(l) for l in f if l.strip()]
    case.check(
        "review log re-chained with entry_hash",
        all("entry_hash" in e and "prev_hash" in e for e in lines),
    )

    # Legacy grant quarantined, invalid config reset, unknown key kept.
    with open(home.path("app-state.json")) as f:
        state = json.load(f)
    case.check(
        "legacy grant quarantined",
        "legacy-caller" in state.get("mcp_connector_approvals_quarantined", {}),
    )
    case.check("invalid theme reset", "theme" not in state)
    case.check("unknown key preserved", state.get("mystery_setting") is True)

    # Idempotent: second apply is a no-op.
    r = run_cli(home, ["migrate", "--apply"], expect_ok=True)
    case.check("second migrate --apply exits 0", r.returncode == 0)


run("migrate-v08", migrate_v08)
