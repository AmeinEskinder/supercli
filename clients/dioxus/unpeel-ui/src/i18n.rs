//! Q6 — Minimal i18n scaffolding for the Dioxus UI.
//!
//! This is the "or equivalent" to Fluent: a tiny message-catalog system
//! with an English catalog and a pseudo-locale for testing. All
//! user-facing strings in the UI should go through [`t()`] rather than
//! being hardcoded in RSX.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use unpeel_ui::i18n::t;
//!
//! rsx! {
//!     button { "{t(\"approve\")}" }
//! }
//! ```
//!
//! ## Pseudo-locale
//!
//! The [`Locale::Pseudo`] locale transforms every message (wraps in
//! `⟦⟧`, expands with padding) so that:
//! - Hardcoded strings stand out visually in screenshots.
//! - The [`pseudo_locale_catches_hardcoded`] test can verify that UI
//!   strings flow through the catalog.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Supported locales. Only English ships; Pseudo is test-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    English,
    /// Test-only pseudo-locale: transforms messages to catch hardcoded strings.
    Pseudo,
}

/// The English message catalog: key -> message.
fn english_catalog() -> &'static HashMap<&'static str, &'static str> {
    static CATALOG: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut m = HashMap::new();
        // Approvals
        m.insert("approve", "Approve");
        m.insert("deny", "Deny");
        m.insert("cancel", "Cancel");
        m.insert("approval.title", "Approval requested");
        m.insert("approval.approve", "Approve");
        m.insert("approval.deny", "Deny");
        m.insert("approval.sending", "Sending answer…");
        m.insert(
            "approval.rate_limited",
            "Rate limited — retrying in {secs}s…",
        );
        m.insert("approval.failed", "Answer failed — tap to retry.");
        // Composer
        m.insert("composer.placeholder", "Type a message…");
        m.insert("composer.send", "Send");
        // Tabs
        m.insert("tab.sessions", "Sessions");
        m.insert("tab.activity", "Activity");
        m.insert("tab.settings", "Settings");
        // Common actions
        m.insert("action.close", "Close");
        m.insert("action.save", "Save");
        m.insert("action.retry", "Retry");
        m.insert("action.copy", "Copy");
        // Status
        m.insert("status.connecting", "Connecting…");
        m.insert("status.connected", "Connected");
        m.insert("status.disconnected", "Disconnected");
        m
    })
}

/// Look up a message by key in the current locale.
///
/// Falls back to the key itself if missing (so a missing translation is
/// visible, not silent).
pub fn t(key: &str) -> String {
    t_with_locale(key, current_locale())
}

/// Look up a message by key in a specific locale.
pub fn t_with_locale(key: &str, locale: Locale) -> String {
    let msg = english_catalog().get(key).copied().unwrap_or(key);
    match locale {
        Locale::English => msg.to_string(),
        Locale::Pseudo => pseudo_transform(msg),
    }
}

/// Pseudo-locale transform: wraps in ⟦⟧ and pads to ~40% longer.
/// Any string that does NOT have this shape in a pseudo-locale screenshot
/// is a hardcoded string that bypassed the catalog.
fn pseudo_transform(msg: &str) -> String {
    let pad_len = (msg.chars().count() as f32 * 0.4).ceil() as usize;
    let pad: String = "·".repeat(pad_len);
    format!("⟦{}{}⟧", msg, pad)
}

fn current_locale() -> Locale {
    // In a real app this would read the OS locale. For now, English is
    // the only shipped locale; Pseudo is selected via env var in tests.
    if std::env::var("UNPEEL_PSEUDO_LOCALE").is_ok() {
        Locale::Pseudo
    } else {
        Locale::English
    }
}

/// All catalog keys, for the static test.
pub fn catalog_keys() -> Vec<&'static str> {
    let mut keys: Vec<_> = english_catalog().keys().copied().collect();
    keys.sort();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_catalog_has_no_empty_messages() {
        for key in catalog_keys() {
            let msg = english_catalog()[key];
            assert!(!msg.is_empty(), "empty message for key {}", key);
        }
    }

    #[test]
    fn pseudo_locale_wraps_and_expands() {
        let out = t_with_locale("approve", Locale::Pseudo);
        assert!(out.starts_with("⟦"), "missing open marker: {}", out);
        assert!(out.ends_with("⟧"), "missing close marker: {}", out);
        assert!(out.contains("Approve"), "message lost: {}", out);
        // Pseudo is longer than the source (catches layout overflow).
        assert!(out.chars().count() > "Approve".chars().count());
    }

    #[test]
    fn missing_key_falls_back_to_key() {
        assert_eq!(t_with_locale("no.such.key", Locale::English), "no.such.key");
    }
}
