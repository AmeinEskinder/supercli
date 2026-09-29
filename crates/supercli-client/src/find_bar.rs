//! Find-bar match counter display logic.
//!
//! Ported from `TerminalFindBar.updateCounts(total:selected:)` (SupercliNative).
//! The AppKit NSView (text field, stack view, focus management) is not
//! portable; this is the pure display-string logic:
//! - "3 of 17" while a match is selected (`selected` is 0-based)
//! - bare total ("17") when no match is selected
//! - "No results" for a live (non-empty) query with zero matches
//! - empty string when the query is empty or total is unknown

/// Compute the find-bar count label text.
///
/// - `query`: the current search text (empty → no label)
/// - `total`: total match count (`None` → no label)
/// - `selected`: 0-based index of the selected match (`None` → bare total)
pub fn update_counts(query: &str, total: Option<i64>, selected: Option<i64>) -> String {
    if query.is_empty() {
        return String::new();
    }
    let total = match total {
        Some(t) => t,
        None => return String::new(),
    };
    if total <= 0 {
        "No results".to_string()
    } else if let Some(s) = selected {
        if s >= 0 {
            format!("{} of {total}", s + 1)
        } else {
            format!("{total}")
        }
    } else {
        format!("{total}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_yields_empty() {
        assert_eq!(update_counts("", Some(17), Some(2)), "");
        assert_eq!(update_counts("", None, None), "");
    }

    #[test]
    fn none_total_yields_empty() {
        assert_eq!(update_counts("foo", None, None), "");
        assert_eq!(update_counts("foo", None, Some(0)), "");
    }

    #[test]
    fn zero_total_yields_no_results() {
        assert_eq!(update_counts("foo", Some(0), None), "No results");
        assert_eq!(update_counts("foo", Some(-1), Some(0)), "No results");
    }

    #[test]
    fn selected_yields_n_of_total() {
        // selected is 0-based: index 2 → "3 of 17"
        assert_eq!(update_counts("foo", Some(17), Some(2)), "3 of 17");
        assert_eq!(update_counts("foo", Some(17), Some(0)), "1 of 17");
        assert_eq!(update_counts("foo", Some(1), Some(0)), "1 of 1");
    }

    #[test]
    fn no_selection_yields_bare_total() {
        assert_eq!(update_counts("foo", Some(17), None), "17");
        assert_eq!(update_counts("foo", Some(1), None), "1");
    }

    #[test]
    fn negative_selected_yields_bare_total() {
        assert_eq!(update_counts("foo", Some(17), Some(-1)), "17");
    }
}
