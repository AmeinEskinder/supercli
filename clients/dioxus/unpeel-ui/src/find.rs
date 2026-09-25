//! Terminal find: the ⌘F find bar over a terminal surface.
//!
//! Swift source: `clients/native/.../TerminalFindBar.swift` — a thin AppKit
//! view (text field, match counter, prev/next/close) whose actual search was
//! libghostty's (incremental match, scrollback walk, highlight rendering).
//!
//! In the Dioxus port the terminal surface is [`TerminalModel`], and the UI
//! only ever sees [`TerminalSnapshot`]s, so the search runs over the
//! snapshot's visible rows:
//!
//! - **Adaptation (documented):** Swift searches visible screen + scrollback
//!   via libghostty. This port searches the visible snapshot only — the
//!   Dioxus clients cannot display scrollback at all (the mobile terminal has
//!   no scrollback gesture; the wheel forwards scroll to the remote TUI), so
//!   searching lines the user could never see would be dishonest UI. If a
//!   scrollback view is ever added, extend [`TerminalSnapshot`] and re-run
//!   [`find_matches`] over it; the component and counter need no changes.
//! - Case-insensitive substring search, non-overlapping matches, row-major
//!   order — matching the observable behavior of an incremental find.
//! - Counter text mirrors Swift's `updateCounts`: "" for an empty query,
//!   "No results" for a live query with zero matches, "3 of 17" while a
//!   match is selected, bare "17" otherwise.
//! - Enter = next, Shift+Enter = previous, Esc = close (Swift's
//!   `control(_:textView:doCommandBy:)`).
//!
//! The native find bar is macOS-only (iOS has no find UI); in the Dioxus
//! clients the terminal surface lives in the mobile launcher, which wires
//! [`FindBar`] over its live terminal. The desktop launcher has no terminal
//! pane, so find is superseded there.

use dioxus::prelude::*;

use super::terminal::{TerminalRow, TerminalSnapshot};
use crate::i18n::t;

/// One match: char offsets into the row's plain text (the concatenation of
/// its style runs), so the renderer can split runs at match boundaries
/// without any column math — highlight spans flow inline with the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindMatch {
    pub row: usize,
    pub col: usize,
    pub len: usize,
}

/// Plain text of one snapshot row.
pub fn row_text(row: &TerminalRow) -> String {
    row.runs.iter().map(|r| r.text.as_str()).collect()
}

/// Case-insensitive, non-overlapping substring matches over the snapshot's
/// visible rows, in row-major order. An empty query yields no matches.
pub fn find_matches(snapshot: &TerminalSnapshot, query: &str) -> Vec<FindMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let needle = query.to_lowercase();
    let mut out = Vec::new();
    for (row, term_row) in snapshot.rows.iter().enumerate() {
        let text = row_text(term_row);
        let hay = text.to_lowercase();
        // Byte offsets from `str::find` must map back to char offsets for
        // the renderer, which splits run text on chars.
        let mut from = 0usize;
        while from <= hay.len() {
            let Some(rel) = hay[from..].find(&needle) else {
                break;
            };
            let byte_start = from + rel;
            let byte_end = byte_start + needle.len();
            let col = hay[..byte_start].chars().count();
            let len = hay[byte_start..byte_end].chars().count();
            out.push(FindMatch { row, col, len });
            from = byte_end; // non-overlapping, like an incremental find
        }
    }
    out
}

/// Counter label mirroring Swift's `updateCounts(total:selected:)`.
pub fn find_counter_text(query: &str, total: usize, selected: Option<usize>) -> String {
    if query.is_empty() {
        return String::new();
    }
    if total == 0 {
        return { t("find.no_results") }.to_string();
    }
    match selected {
        Some(i) => format!("{} of {total}", i + 1),
        None => format!("{total}"),
    }
}

/// Mutable find session owned by the launcher: query, current matches, and
/// the selected match index. `next`/`prev` wrap around the match list.
#[derive(Debug, Clone, Default)]
pub struct FindState {
    query: String,
    matches: Vec<FindMatch>,
    selected: Option<usize>,
}

impl FindState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn matches(&self) -> &[FindMatch] {
        &self.matches
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn selected_match(&self) -> Option<FindMatch> {
        self.selected.and_then(|i| self.matches.get(i).copied())
    }

    pub fn counter_text(&self) -> String {
        find_counter_text(&self.query, self.matches.len(), self.selected)
    }

    /// New query: re-run the search and select the first match (or nothing).
    pub fn set_query(&mut self, snapshot: &TerminalSnapshot, query: String) {
        self.query = query;
        self.matches = find_matches(snapshot, &self.query);
        self.selected = if self.matches.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    /// Re-run the current query against a fresh snapshot (new terminal
    /// output arrived while the bar is open). Keeps the selection stable
    /// when the same match still exists, else clamps into range.
    pub fn refresh(&mut self, snapshot: &TerminalSnapshot) {
        if self.query.is_empty() {
            return;
        }
        let anchor = self.selected_match();
        self.matches = find_matches(snapshot, &self.query);
        self.selected = if self.matches.is_empty() {
            None
        } else if let Some(a) = anchor {
            // Prefer the same (row, col); fall back to the first match.
            self.matches
                .iter()
                .position(|m| m.row == a.row && m.col == a.col)
                .or(Some(0))
        } else {
            Some(0)
        };
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.selected = None;
    }

    pub fn next(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        let n = self.matches.len();
        self.selected = Some(self.selected.map_or(0, |i| (i + 1) % n));
    }

    pub fn prev(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        let n = self.matches.len();
        self.selected = Some(self.selected.map_or(n - 1, |i| (i + n - 1) % n));
    }

    /// Highlight payload for [`super::terminal::TerminalView`]: all matches
    /// plus which one is current.
    pub fn highlight(&self) -> Option<FindHighlight> {
        if self.query.is_empty() {
            None
        } else {
            Some(FindHighlight {
                matches: self.matches.clone(),
                selected: self.selected,
            })
        }
    }
}

/// What the terminal renderer needs: every match and the current one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FindHighlight {
    pub matches: Vec<FindMatch>,
    pub selected: Option<usize>,
}

/// Pre-split terminal runs for find highlighting: per row, per run
/// `(style_attr, segments)`; each segment is `(text, highlight)` where the
/// highlight flag is `Some(true)` for the current match, `Some(false)` for
/// other matches, `None` for plain text.
pub type FindSplitRuns = Vec<Vec<(String, Vec<(String, Option<bool>)>)>>;

/// Split one run's text at match boundaries. Returns `(segment, highlight)`
/// pieces where `highlight` is `Some(is_current)` for match segments and
/// `None` for plain text. `run_start` is the run's char offset within the
/// row; `ranges` are `(start, end, is_current)` char ranges in the row.
pub fn split_run_for_find(
    run_text: &str,
    run_start: usize,
    ranges: &[(usize, usize, bool)],
) -> Vec<(String, Option<bool>)> {
    let chars: Vec<char> = run_text.chars().collect();
    if chars.is_empty() || ranges.is_empty() {
        return vec![(run_text.to_string(), None)];
    }
    let run_end = run_start + chars.len();
    // Boundaries inside this run: 0 and len always, plus every range edge
    // clipped to the run.
    let mut cuts = vec![0usize, chars.len()];
    for &(s, e, _) in ranges {
        let cs = s.clamp(run_start, run_end) - run_start;
        let ce = e.clamp(run_start, run_end) - run_start;
        if cs < ce {
            cuts.push(cs);
            cuts.push(ce);
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut out = Vec::new();
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a == b {
            continue;
        }
        let seg: String = chars[a..b].iter().collect();
        let abs_a = run_start + a;
        let hl = ranges
            .iter()
            .find(|&&(s, e, _)| s <= abs_a && abs_a < e)
            .map(|&(_, _, cur)| cur);
        out.push((seg, hl));
    }
    if out.is_empty() {
        out.push((run_text.to_string(), None));
    }
    out
}

/// The find bar: query field, match counter, previous/next/close.
///
/// Props mirror Swift's `TerminalFindBar` callbacks (`onQueryChange`,
/// `onNext`, `onPrevious`, `onClose`) plus the preformatted counter text
/// from [`find_counter_text`].
#[component]
pub fn FindBar(
    query: String,
    counter: String,
    on_query: EventHandler<String>,
    on_next: EventHandler<()>,
    on_prev: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: "find-bar",
            // Don't let the terminal's press-and-hold tracking see
            // interactions with the bar.
            onpointerdown: move |evt| evt.stop_propagation(),
            role: "search",
            input {
                class: "find-field",
                r#type: "text",
                placeholder: {t("find.find")},
                value: "{query}",
                autofocus: true,
                aria_label: {t("find.find_in_terminal")},
                oninput: move |e| on_query.call(e.value()),
                onkeydown: move |e| {
                    match e.key() {
                        Key::Enter => {
                            e.prevent_default();
                            if e.modifiers().shift() {
                                on_prev.call(());
                            } else {
                                on_next.call(());
                            }
                        }
                        Key::Escape => {
                            e.prevent_default();
                            on_close.call(());
                        }
                        _ => {}
                    }
                },
            }
            span { class: "find-count", "{counter}" }
            button {
                class: "find-btn",
                aria_label: {t("find.previous_match")},
                onclick: move |_| on_prev.call(()),
                "▲"
            }
            button {
                class: "find-btn",
                aria_label: {t("find.next_match")},
                onclick: move |_| on_next.call(()),
                "▼"
            }
            button {
                class: "find-btn",
                aria_label: {t("find.close_find")},
                onclick: move |_| on_close.call(()),
                "✕"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::terminal::{StyleRun, TermColor};
    use super::*;

    fn snap(rows: &[&str]) -> TerminalSnapshot {
        TerminalSnapshot {
            rows: rows
                .iter()
                .map(|t| TerminalRow {
                    runs: vec![StyleRun {
                        text: t.to_string(),
                        fg: TermColor::Default,
                        bg: TermColor::Default,
                        bold: false,
                        dim: false,
                        italic: false,
                        underline: false,
                        inverse: false,
                    }],
                })
                .collect(),
            cursor: (0, 0),
            cols: 80,
            rows_count: rows.len() as u16,
        }
    }

    #[test]
    fn empty_query_matches_nothing() {
        let s = snap(&["hello world"]);
        assert!(find_matches(&s, "").is_empty());
    }

    #[test]
    fn case_insensitive_row_major() {
        let s = snap(&["Hello World", "say HELLO again", "nothing here"]);
        let m = find_matches(&s, "hello");
        assert_eq!(m.len(), 2);
        assert_eq!(
            m[0],
            FindMatch {
                row: 0,
                col: 0,
                len: 5
            }
        );
        assert_eq!(
            m[1],
            FindMatch {
                row: 1,
                col: 4,
                len: 5
            }
        );
    }

    #[test]
    fn matches_do_not_overlap() {
        let s = snap(&["aaa"]);
        let m = find_matches(&s, "aa");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].col, 0);
    }

    #[test]
    fn unicode_char_offsets() {
        let s = snap(&["héllo wörld"]);
        let m = find_matches(&s, "wörld");
        assert_eq!(m.len(), 1);
        // char offset, not byte offset: "héllo " is 6 chars.
        assert_eq!(m[0].col, 6);
        assert_eq!(m[0].len, 5);
    }

    #[test]
    fn counter_text_branches() {
        assert_eq!(find_counter_text("", 5, Some(0)), "");
        assert_eq!(find_counter_text("x", 0, None), "No results");
        assert_eq!(find_counter_text("x", 17, None), "17");
        assert_eq!(find_counter_text("x", 17, Some(2)), "3 of 17");
    }

    #[test]
    fn state_next_prev_wrap() {
        let s = snap(&["a a a"]);
        let mut st = FindState::new();
        st.set_query(&s, "a".to_string());
        assert_eq!(st.matches().len(), 3);
        assert_eq!(st.selected(), Some(0));
        st.next();
        assert_eq!(st.selected(), Some(1));
        st.next();
        st.next();
        assert_eq!(st.selected(), Some(0)); // wraps
        st.prev();
        assert_eq!(st.selected(), Some(2)); // wraps backward
    }

    #[test]
    fn state_no_matches_next_is_noop() {
        let s = snap(&["abc"]);
        let mut st = FindState::new();
        st.set_query(&s, "zzz".to_string());
        assert_eq!(st.counter_text(), "No results");
        st.next();
        st.prev();
        assert_eq!(st.selected(), None);
    }

    #[test]
    fn state_refresh_keeps_anchor() {
        let s1 = snap(&["foo bar", "foo baz"]);
        let mut st = FindState::new();
        st.set_query(&s1, "foo".to_string());
        st.next(); // select (1, 0)
        assert_eq!(
            st.selected_match(),
            Some(FindMatch {
                row: 1,
                col: 0,
                len: 3
            })
        );
        // Same content, new snapshot object: anchor survives.
        let s2 = snap(&["foo bar", "foo baz"]);
        st.refresh(&s2);
        assert_eq!(st.selected(), Some(1));
        // Anchor row gone: falls back to first match.
        let s3 = snap(&["foo bar"]);
        st.refresh(&s3);
        assert_eq!(st.selected(), Some(0));
    }

    #[test]
    fn split_run_segments() {
        // "hello world", match "world" at (6, 11), current.
        let segs = split_run_for_find("hello world", 0, &[(6, 11, true)]);
        assert_eq!(
            segs,
            vec![
                ("hello ".to_string(), None),
                ("world".to_string(), Some(true)),
            ]
        );
    }

    #[test]
    fn split_run_partial_overlap_and_current_flag() {
        // Run starts mid-row; two matches, only the second is current.
        let segs = split_run_for_find("abXXcdYYef", 10, &[(12, 14, false), (16, 18, true)]);
        assert_eq!(
            segs,
            vec![
                ("ab".to_string(), None),
                ("XX".to_string(), Some(false)),
                ("cd".to_string(), None),
                ("YY".to_string(), Some(true)),
                ("ef".to_string(), None),
            ]
        );
    }

    #[test]
    fn split_run_no_ranges_passthrough() {
        let segs = split_run_for_find("plain", 0, &[]);
        assert_eq!(segs, vec![("plain".to_string(), None)]);
    }
}
