//! Command palette — port of `CommandPaletteView.swift` (fuzzy matcher +
//! item model) and `Models.swift`'s `CommandTAction`.
//!
//! The palette is the keyboard "jump to anything": sessions across all
//! projects, preset launches, and a few app commands. It is deliberately
//! NOT a code-tool palette (no files, no symbols); everything it lists is
//! a session-level noun or verb that already exists elsewhere in the UI.

use crate::i18n::t;
use dioxus::html::Key;
use dioxus::prelude::*;

/// Small case-insensitive matcher: exact substring beats subsequence,
/// earlier and word-start matches beat scattered ones. `None` = no match.
pub fn palette_fuzzy_score(query: &str, candidate: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    let lowered_query = query.to_lowercase();
    let lowered_candidate = candidate.to_lowercase();
    if let Some(pos) = lowered_candidate.find(lowered_query.as_str()) {
        let position = lowered_candidate[..pos].chars().count();
        return Some(1000 - position.min(500) as u32);
    }
    let query_chars: Vec<char> = lowered_query.chars().collect();
    let candidate_chars: Vec<char> = lowered_candidate.chars().collect();
    let mut query_index = 0usize;
    let mut score = 0u32;
    let mut last_match: i64 = -2;
    for (index, &ch) in candidate_chars.iter().enumerate() {
        if query_index >= query_chars.len() {
            break;
        }
        if ch != query_chars[query_index] {
            continue;
        }
        score += if last_match == index as i64 - 1 {
            15
        } else {
            5
        };
        let is_word_start = index == 0 || " -_/.".contains(candidate_chars[index - 1]);
        if is_word_start {
            score += 10;
        }
        last_match = index as i64;
        query_index += 1;
    }
    if query_index == query_chars.len() {
        Some(score)
    } else {
        None
    }
}

/// What a palette row acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteKind {
    Session,
    Project,
    Launch,
    Command,
}

impl PaletteKind {
    pub fn label(self) -> String {
        match self {
            PaletteKind::Session => t("command_palette.session"),
            PaletteKind::Project => t("command_palette.project"),
            PaletteKind::Launch => t("command_palette.launch"),
            PaletteKind::Command => t("command_palette.command"),
        }
    }
}

/// One row in the palette. The launcher owns the action: `on_run` receives
/// the row's id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteItem {
    pub id: String,
    pub kind: PaletteKind,
    pub title: String,
    pub subtitle: Option<String>,
    /// Extra text the fuzzy matcher may hit (e.g. the session's command).
    pub keywords: String,
    /// Sidebar-parity unread marker (blue dot after the title).
    pub unread: bool,
}

impl PaletteItem {
    fn haystack(&self) -> String {
        let mut h = self.title.clone();
        if let Some(sub) = &self.subtitle {
            h.push(' ');
            h.push_str(sub);
        }
        if !self.keywords.is_empty() {
            h.push(' ');
            h.push_str(&self.keywords);
        }
        h
    }
}

/// Filter + rank items for a query, best first. Ties keep input order.
pub fn filter_palette(items: &[PaletteItem], query: &str) -> Vec<usize> {
    let mut scored: Vec<(usize, u32)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| palette_fuzzy_score(query, &item.haystack()).map(|s| (i, s)))
        .collect();
    scored.sort_by_key(|a| std::cmp::Reverse(a.1));
    scored.into_iter().map(|(i, _)| i).collect()
}

/// The ⌘K/Ctrl+K overlay. The launcher supplies the items and handles
/// `on_run`; the component owns query + selection state.
#[component]
pub fn CommandPalette(
    items: Vec<PaletteItem>,
    on_run: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);
    // Share the item list across the key handler and the row closures.
    let items = std::rc::Rc::new(items);
    let ranked = filter_palette(&items, &query.read());
    let count = ranked.len();
    let ranked_ids: Vec<String> = ranked.iter().map(|&i| items[i].id.clone()).collect();
    rsx! {
        div { class: "palette-backdrop", onclick: move |_| on_close.call(()),
            div {
                class: "palette-card",
                onclick: move |e| e.stop_propagation(),
                onkeydown: {
                    let ranked_ids = ranked_ids.clone();
                    move |e: KeyboardEvent| {
                        match e.key() {
                            Key::Escape => on_close.call(()),
                            Key::ArrowDown => {
                                if count > 0 {
                                    let cur = *selected.read();
                                    selected.set((cur + 1) % count);
                                }
                                e.prevent_default();
                            }
                            Key::ArrowUp => {
                                if count > 0 {
                                    let cur = *selected.read();
                                    selected.set((cur + count - 1) % count);
                                }
                                e.prevent_default();
                            }
                            Key::Enter => {
                                if let Some(id) = ranked_ids.get(*selected.read()) {
                                    on_run.call(id.clone());
                                }
                            }
                            _ => {}
                        }
                    }
                },
                input {
                    class: "palette-input",
                    placeholder: {t("command_palette.type_a_command_or_search_sessions")},
                    autofocus: true,
                    value: "{query}",
                    oninput: move |e| {
                        query.set(e.value());
                        selected.set(0);
                    },
                }
                div { class: "palette-list",
                    for (pos, idx) in ranked.iter().enumerate() {
                        {
                            let item = &items[*idx];
                            let id = item.id.clone();
                            let is_sel = pos == *selected.read();
                            let kind_label = item.kind.label();
                            let title = item.title.clone();
                            let subtitle = item.subtitle.clone();
                            let unread = item.unread;
                            rsx! {
                                div {
                                    key: "{id}",
                                    class: if is_sel { "palette-row selected" } else { "palette-row" },
                                    onclick: move |_| on_run.call(id.clone()),
                                    onmouseenter: move |_| selected.set(pos),
                                    span { class: "palette-kind", "{kind_label}" }
                                    span { class: "palette-title", "{title}" }
                                    if unread {
                                        span { class: "palette-unread" }
                                    }
                                    if let Some(sub) = &subtitle {
                                        span { class: "palette-subtitle", "{sub}" }
                                    }
                                }
                            }
                        }
                    }
                    if ranked.is_empty() {
                        div { class: "palette-empty", "No matches." }
                    }
                }
            }
        }
    }
}

/// Global ⌘K / Ctrl+K listener for the desktop shell. The launcher evals
/// this once and pumps `palette:toggle` messages (Swift's palette is
/// bound to ⌘K in the main menu). The launcher toggles its palette
/// state on each message.
pub const PALETTE_SHORTCUT_JS: &str = r#"
document.addEventListener('keydown', (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    dioxus.send('palette:toggle');
  }
});
"#;

#[cfg(test)]
mod tests {
    #[test]
    fn palette_shortcut_js_sends_the_toggle_message() {
        // The desktop pump matches on this exact message; the test guards
        // the JS/Rust contract.
        assert!(PALETTE_SHORTCUT_JS.contains("palette:toggle"));
        assert!(PALETTE_SHORTCUT_JS.contains("metaKey"));
        assert!(PALETTE_SHORTCUT_JS.contains("ctrlKey"));
    }

    use super::*;

    #[test]
    fn empty_query_matches_everything_at_zero() {
        assert_eq!(palette_fuzzy_score("", "anything"), Some(0));
    }

    #[test]
    fn exact_substring_beats_subsequence() {
        let sub = palette_fuzzy_score("term", "terminal").unwrap();
        let seq = palette_fuzzy_score("term", "t-e-r-m-ish").unwrap();
        assert!(sub > seq);
    }

    #[test]
    fn earlier_and_word_start_win() {
        let early = palette_fuzzy_score("up", "update session").unwrap();
        let late = palette_fuzzy_score("up", "backup script").unwrap();
        assert!(early > late);
        // Exact substring beats subsequence even mid-word ("ns" is inside "ansel").
        let sub = palette_fuzzy_score("ns", "ansel").unwrap();
        let seq = palette_fuzzy_score("ns", "new session").unwrap();
        assert!(sub > seq);
        // Among subsequence matches, word starts win.
        let word_start = palette_fuzzy_score("nse", "new session").unwrap();
        let scattered = palette_fuzzy_score("nse", "anxsery").unwrap();
        assert!(word_start > scattered);
    }

    #[test]
    fn no_match_returns_none() {
        assert_eq!(palette_fuzzy_score("zzz", "alpha"), None);
    }

    #[test]
    fn filter_ranks_and_keeps_order_on_ties() {
        let items = vec![
            PaletteItem {
                id: "a".into(),
                kind: PaletteKind::Session,
                title: "backend".into(),
                subtitle: None,
                keywords: String::new(),
                unread: false,
            },
            PaletteItem {
                id: "b".into(),
                kind: PaletteKind::Session,
                title: "frontend".into(),
                subtitle: None,
                keywords: String::new(),
                unread: false,
            },
            PaletteItem {
                id: "c".into(),
                kind: PaletteKind::Command,
                title: "end session".into(),
                subtitle: None,
                keywords: String::new(),
                unread: false,
            },
        ];
        let ranked = filter_palette(&items, "end");
        assert_eq!(ranked, vec![2, 0, 1]);
    }

    #[test]
    fn keywords_are_searchable() {
        let items = vec![PaletteItem {
            id: "a".into(),
            kind: PaletteKind::Session,
            title: "mystery".into(),
            subtitle: None,
            keywords: "cargo test".into(),
            unread: false,
        }];
        assert_eq!(filter_palette(&items, "cargo"), vec![0]);
    }
}
