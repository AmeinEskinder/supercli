//! Fixture-home tests for `unpeel migrate` (included as `migrate_cli::tests`).

use super::*;
use unpeel_core::action_reviews::{record_review, verify_review_chain, Actor, ReviewDecision};

/// Build a fixture home with every legacy format. Never touches the
/// real UNPEEL_HOME: all paths are explicit.
fn fixture_home(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("unpeel-migrate-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();

    // Legacy connector grants: one bare string (legacy), one namespaced
    // object (current). The connector for "search" must never be guessed.
    std::fs::write(
        home.join("app-state.json"),
        serde_json::json!({
            "mcp_connector_approvals": {
                "sess-1": ["search", {"connector": "github", "tool": "fetch"}],
            },
        })
        .to_string(),
    )
    .unwrap();

    // s1: pre-chain review log (no prev_hash/entry_hash anywhere).
    let s1 = home.join("app-sessions").join("s1");
    std::fs::create_dir_all(&s1).unwrap();
    std::fs::write(
        s1.join(REVIEWS_FILE),
        "{\"review_id\":\"r-1\",\"ts_ms\":1700000000000,\
             \"actor\":\"human:phone-1\",\"connector\":\"c1\",\"tool\":\"t.search\",\
             \"args_hash\":\"aa\",\"decision\":\"approved\",\"replaces_attempt\":null}\n\
             {\"type\":\"attempt_outcome\",\"review_id\":\"r-1\",\"ts_ms\":1700000000001,\
             \"actor\":\"human:phone-1\",\"outcome\":\"executed\",\"success\":true,\
             \"reason\":null}\n",
    )
    .unwrap();

    // s2: chained log, then one byte flipped — tamper evidence.
    let s2 = home.join("app-sessions").join("s2");
    std::fs::create_dir_all(&s2).unwrap();
    record_review(
        &s2,
        Actor::Human {
            device_id: "phone-1".into(),
        },
        "c1",
        "t.search",
        "aa",
        ReviewDecision::Approved,
        None,
    )
    .unwrap();
    let p2 = s2.join(REVIEWS_FILE);
    let raw = std::fs::read_to_string(&p2).unwrap();
    let tampered = raw.replacen("\"decision\":\"approved\"", "\"decision\":\"denied\"", 1);
    assert_ne!(raw, tampered);
    std::fs::write(&p2, tampered).unwrap();

    // s3: current chained log — the migrator must not touch it.
    let s3 = home.join("app-sessions").join("s3");
    std::fs::create_dir_all(&s3).unwrap();
    record_review(
        &s3,
        Actor::PolicyAllow,
        "c1",
        "t.search",
        "aa",
        ReviewDecision::Approved,
        None,
    )
    .unwrap();

    // Schedules registry + a lease database with a live row.
    std::fs::write(
        home.join("schedules.json"),
        "[{\"id\":\"sched-1\",\"session_id\":\"s3\"}]",
    )
    .unwrap();
    let store = ScheduleLeases::open(&home, "fixture").unwrap();
    let claimed = store.claim("sched-1").unwrap();
    assert!(claimed.is_some(), "fixture must hold a live lease");
    let owner_before = store.lease_row("sched-1").unwrap().unwrap().owner;
    std::fs::write(home.join("lease-owner.txt"), owner_before).unwrap();
    drop(store);

    home
}

fn bak_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if e.file_name().to_str().is_some_and(is_bak_file) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn dry_run_reports_and_changes_nothing() {
    let home = fixture_home("dry-run");
    let state_path = home.join("app-state.json");
    let state_before = std::fs::read_to_string(&state_path).unwrap();
    let s1_before =
        std::fs::read_to_string(home.join("app-sessions").join("s1").join(REVIEWS_FILE)).unwrap();
    let s2_before =
        std::fs::read_to_string(home.join("app-sessions").join("s2").join(REVIEWS_FILE)).unwrap();

    // Grants: one legacy bare string found, namespaced grant ignored.
    let g = step_grants_at(&state_path, false);
    assert!(g.error.is_none());
    assert!(g.lines.iter().any(|l| l.contains("\"search\"")));
    assert!(!g.changed);

    // Review logs: s1 would re-chain, s2 broken (left alone), s3 current.
    let r = step_review_logs(&home, false);
    assert!(r.error.is_none());
    assert!(r
        .lines
        .iter()
        .any(|l| l.contains("s1") && l.contains("would back up and re-chain")));
    assert!(r
        .lines
        .iter()
        .any(|l| l.contains("s2") && l.contains("left untouched")));
    assert!(r
        .lines
        .iter()
        .any(|l| l.contains("s3") && l.contains("current")));
    assert!(!r.changed);

    // Schedules: registry + lease db found; schema upgrade only described.
    let s = step_schedules(&home, false);
    assert!(s.error.is_none());
    assert!(s.lines.iter().any(|l| l.contains("1 schedule")));
    assert!(!s.changed);

    // Nothing changed anywhere: no backups, bytes identical.
    assert!(bak_files(&home).is_empty(), "dry-run must not back up");
    assert_eq!(std::fs::read_to_string(&state_path).unwrap(), state_before);
    assert_eq!(
        std::fs::read_to_string(home.join("app-sessions").join("s1").join(REVIEWS_FILE)).unwrap(),
        s1_before
    );
    assert_eq!(
        std::fs::read_to_string(home.join("app-sessions").join("s2").join(REVIEWS_FILE)).unwrap(),
        s2_before
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn apply_migrates_backup_first_and_is_idempotent() {
    let home = fixture_home("apply");
    let state_path = home.join("app-state.json");

    // Apply.
    let g = step_grants_at(&state_path, true);
    assert!(g.error.is_none(), "{:?}", g.error);
    let r = step_review_logs(&home, true);
    assert!(r.error.is_none(), "{:?}", r.error);
    let s = step_schedules(&home, true);
    assert!(s.error.is_none(), "{:?}", s.error);

    // Backup-first: one backup per touched file.
    let baks = bak_files(&home);
    assert_eq!(baks.len(), 4, "expected 4 backups, got {baks:?}");
    assert!(baks
        .iter()
        .any(|p| p.ends_with("app-state.json.pre-migrate-")
            || p.to_str().unwrap().contains("app-state.json.pre-migrate-")));

    // Grants: bare string quarantined (never namespaced), object kept.
    let state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
    let active = &state["mcp_connector_approvals"]["sess-1"];
    assert_eq!(active.as_array().unwrap().len(), 1);
    assert_eq!(active[0]["connector"], "github");
    let q = &state["mcp_connector_approvals_quarantined"]["sess-1"];
    assert_eq!(q, &serde_json::json!(["search"]));

    // s1: re-chained and verifies; all fields preserved.
    let s1 = home.join("app-sessions").join("s1");
    assert_eq!(verify_review_chain(&s1).unwrap(), 2);
    let raw = std::fs::read_to_string(s1.join(REVIEWS_FILE)).unwrap();
    assert!(raw.contains("\"review_id\":\"r-1\""));
    assert!(raw.contains("\"prev_hash\":\"genesis\""));

    // s2: tamper evidence preserved — bytes identical, still broken.
    let s2raw =
        std::fs::read_to_string(home.join("app-sessions").join("s2").join(REVIEWS_FILE)).unwrap();
    assert!(s2raw.contains("\"decision\":\"denied\""));
    assert!(verify_review_chain(&home.join("app-sessions").join("s2")).is_err());

    // s3: untouched.
    assert_eq!(
        verify_review_chain(&home.join("app-sessions").join("s3")).unwrap(),
        1
    );

    // Lease DB: schema ensured, live row preserved with its owner —
    // no stale ownership resurrected, none clobbered.
    let owner_before = std::fs::read_to_string(home.join("lease-owner.txt")).unwrap();
    // Same tenant as the fixture: rows are keyed (tenant, schedule_id).
    let store = ScheduleLeases::open(&home, "fixture").unwrap();
    let row = store.lease_row("sched-1").unwrap().unwrap();
    assert_eq!(row.owner, owner_before);
    drop(store);

    // Second apply: no-op. No new backups, reports say nothing to do.
    let baks_before = bak_files(&home);
    let g2 = step_grants_at(&state_path, true);
    assert!(g2.error.is_none());
    assert!(!g2.changed);
    assert!(g2
        .lines
        .iter()
        .any(|l| l.contains("no legacy bare-string grants")));
    let r2 = step_review_logs(&home, true);
    assert!(r2.error.is_none());
    assert!(!r2.changed);
    // s2 still reported broken, never touched.
    assert!(r2
        .lines
        .iter()
        .any(|l| l.contains("s2") && l.contains("left untouched")));
    let s_2 = step_schedules(&home, true);
    assert!(s_2.error.is_none());
    assert_eq!(
        bak_files(&home),
        baks_before,
        "second apply must create no new backups"
    );

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn run_rejects_unknown_flags() {
    assert_eq!(run(&["--bogus".to_string()]), 2);
    assert_eq!(run(&["--help".to_string()]), 0);
}
