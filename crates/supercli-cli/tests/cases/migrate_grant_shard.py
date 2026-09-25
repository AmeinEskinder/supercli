"""`unpeel migrate` grant shard: v0.9-pre (grants in app-state.json) -> v0.9 (grants.json).

Builds a home with grants in app-state.json (pre-S2 layout), runs
`unpeel migrate --apply`, then asserts:
- grants.json exists and contains the grants
- app-state.json no longer has the grant keys (non-grant keys preserved)
- migration is idempotent (second run is a no-op)
"""

import sys, os, json

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run_cli, run  # noqa: E402


def migrate_grant_shard(case):
    home = case.home

    # Pre-S2 layout: grants live in app-state.json
    app_state = {
        "mcp_write_approvals": {
            "session-1": ["target-a", "target-b"],
            "session-2": ["target-c"],
        },
        "browser_approvals": ["session-1", "session-3"],
        "some_other_key": "should_remain",
    }
    with open(home.path("app-state.json"), "w") as f:
        json.dump(app_state, f)

    grants_path = home.path("grants.json")
    case.check("grants.json absent pre-migration", not os.path.exists(grants_path))

    # Dry-run first: should report but not change
    r = run_cli(home, ["migrate"], expect_ok=True)
    case.check("dry-run exits 0", r.returncode == 0)
    case.check("dry-run creates no grants.json", not os.path.exists(grants_path))

    # Apply
    r = run_cli(home, ["migrate", "--apply"], expect_ok=True)
    case.check("migrate --apply exits 0", r.returncode == 0)

    # grants.json now has the grants
    case.check("grants.json created", os.path.exists(grants_path))
    with open(grants_path) as f:
        grants = json.load(f)
    case.check(
        "mcp_write_approvals migrated",
        grants.get("mcp_write_approvals", {}).get("session-1") == ["target-a", "target-b"],
    )
    case.check(
        "browser_approvals migrated",
        "session-1" in grants.get("browser_approvals", []),
    )

    # app-state.json no longer has grant keys; other keys preserved
    with open(home.path("app-state.json")) as f:
        new_state = json.load(f)
    case.check("mcp_write_approvals removed from app-state.json",
               "mcp_write_approvals" not in new_state)
    case.check("browser_approvals removed from app-state.json",
               "browser_approvals" not in new_state)
    case.check("non-grant key preserved",
               new_state.get("some_other_key") == "should_remain")

    # Idempotent: second apply is a no-op
    with open(grants_path) as f:
        before = f.read()
    r = run_cli(home, ["migrate", "--apply"], expect_ok=True)
    case.check("second migrate --apply exits 0", r.returncode == 0)
    with open(grants_path) as f:
        after = f.read()
    case.check("grants.json unchanged on second apply", before == after)


run("migrate-grant-shard", migrate_grant_shard)
