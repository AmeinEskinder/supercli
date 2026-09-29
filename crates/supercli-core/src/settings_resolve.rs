//! Settings resolution for the session-cleanup knobs.
//!
//! Ported from the legacy Swift store module (`SupercliStore`) — the "Advanced session
//! cleanup" section. These are the pure normalize/resolve/label functions; the
//! Swift originals read from `AppStateFile` and `UserDefaults`, the Rust
//! versions take the raw `Option<i64>` so any caller (Host, CLI, TUI) can use
//! them.
//!
//! Resolution semantics (absent vs explicit vs junk) are the point:
//! - `auto_stop_archive_minutes`: absent key = ON at the default cutoff
//!   (opt-out feature); an explicit value — including 0 = Never — wins; junk
//!   reads as 0 (off).
//! - `sidebar_stopped_limit`: absent key = the default window; an explicit
//!   value — including 0 = None — wins; junk reads as the DEFAULT (5), never
//!   as 0 — junk must never silently file extra rows.

/// Allowed values for `auto_stop_archive_minutes` (0 = Never/off).
pub const AUTO_STOP_ARCHIVE_MINUTE_OPTIONS: &[i64] = &[0, 30, 60, 120, 240, 480, 1440];
/// Default when the key is absent: on, archiving stops older than a day.
pub const DEFAULT_AUTO_STOP_ARCHIVE_MINUTES: i64 = 1440;

/// Allowed values for `sidebar_stopped_limit` (0 = None).
pub const SIDEBAR_STOPPED_LIMIT_OPTIONS: &[i64] = &[0, 3, 5, 10, 15, 25];
/// Default when the key is absent.
pub const DEFAULT_SIDEBAR_STOPPED_LIMIT: i64 = 5;

/// Normalize a raw `auto_stop_archive_minutes` value: must be one of the
/// allowed options, otherwise 0 (off). Junk never silently enables archiving.
pub fn normalize_auto_stop_archive_minutes(minutes: i64) -> i64 {
    if AUTO_STOP_ARCHIVE_MINUTE_OPTIONS.contains(&minutes) {
        minutes
    } else {
        0
    }
}

/// Resolve the shared-file value: absent = default on; explicit wins; junk = off.
pub fn resolve_auto_stop_archive_minutes(raw: Option<i64>) -> i64 {
    match raw {
        None => DEFAULT_AUTO_STOP_ARCHIVE_MINUTES,
        Some(v) => normalize_auto_stop_archive_minutes(v),
    }
}

/// Normalize a raw `sidebar_stopped_limit` value: must be one of the allowed
/// options, otherwise the default. Junk never silently files extra rows.
pub fn normalize_sidebar_stopped_limit(limit: i64) -> i64 {
    if SIDEBAR_STOPPED_LIMIT_OPTIONS.contains(&limit) {
        limit
    } else {
        DEFAULT_SIDEBAR_STOPPED_LIMIT
    }
}

/// Resolve the shared-file value: absent = default window; explicit wins;
/// junk = default.
pub fn resolve_sidebar_stopped_limit(raw: Option<i64>) -> i64 {
    match raw {
        None => DEFAULT_SIDEBAR_STOPPED_LIMIT,
        Some(v) => normalize_sidebar_stopped_limit(v),
    }
}

/// Display label for the stopped-session window: 0 renders as "None".
pub fn sidebar_stopped_limit_label(limit: i64) -> String {
    if limit == 0 {
        "None".to_string()
    } else {
        limit.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_stop_absent_means_default_on() {
        assert_eq!(resolve_auto_stop_archive_minutes(None), 1440);
    }

    #[test]
    fn auto_stop_explicit_zero_is_never() {
        assert_eq!(resolve_auto_stop_archive_minutes(Some(0)), 0);
    }

    #[test]
    fn auto_stop_explicit_valid_wins() {
        assert_eq!(resolve_auto_stop_archive_minutes(Some(60)), 60);
        assert_eq!(resolve_auto_stop_archive_minutes(Some(480)), 480);
    }

    #[test]
    fn auto_stop_junk_reads_as_off() {
        // Unlike the sidebar limit, junk here reads as 0 (off), not default.
        assert_eq!(resolve_auto_stop_archive_minutes(Some(45)), 0);
        assert_eq!(resolve_auto_stop_archive_minutes(Some(-1)), 0);
        assert_eq!(resolve_auto_stop_archive_minutes(Some(9999)), 0);
    }

    #[test]
    fn sidebar_absent_means_default_window() {
        assert_eq!(resolve_sidebar_stopped_limit(None), 5);
    }

    #[test]
    fn sidebar_explicit_zero_is_none() {
        assert_eq!(resolve_sidebar_stopped_limit(Some(0)), 0);
    }

    #[test]
    fn sidebar_explicit_valid_wins() {
        assert_eq!(resolve_sidebar_stopped_limit(Some(10)), 10);
        assert_eq!(resolve_sidebar_stopped_limit(Some(25)), 25);
    }

    #[test]
    fn sidebar_junk_reads_as_default_not_zero() {
        // Junk must never silently file extra rows: default, not 0.
        assert_eq!(resolve_sidebar_stopped_limit(Some(7)), 5);
        assert_eq!(resolve_sidebar_stopped_limit(Some(-1)), 5);
        assert_eq!(resolve_sidebar_stopped_limit(Some(100)), 5);
    }

    #[test]
    fn sidebar_label_none_for_zero() {
        assert_eq!(sidebar_stopped_limit_label(0), "None");
    }

    #[test]
    fn sidebar_label_number_otherwise() {
        assert_eq!(sidebar_stopped_limit_label(5), "5");
        assert_eq!(sidebar_stopped_limit_label(25), "25");
    }
}
