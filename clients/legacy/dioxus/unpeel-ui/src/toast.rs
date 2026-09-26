//! Transient in-app toasts for the Dioxus desktop launcher.
//!
//! Swift source: `clients/native/.../Views/ToastCenter.swift` —
//! `ToastCenter.shared.show(...)` posts a brief capsule at the top-right of
//! the window; `ToastOverlayView` renders it. The app had no message surface,
//! so presence changes were only visible as viewer avatar chips.
//!
//! Ported behavior:
//!
//! - `show` replaces any current toast and cancels the pending auto-dismiss
//!   (the launcher owns the timer thread: it sleeps `seconds`, then clears
//!   only if the toast id still matches — the same cancel semantics as
//!   Swift's `dismissTask`).
//! - Default 3.2s lifetime, like Swift.
//! - A tap always dismisses; with an action it also runs it — modeled as
//!   [`ToastOverlay`]'s `on_tap` event; the launcher clears the toast and
//!   runs whatever the tap meant (Swift's closures can't cross the
//!   signal/thread boundary, so actions live launcher-side keyed by toast
//!   id).
//! - The icon: Swift shows an SF Symbol (or a chrome icon for local-site
//!   links). SF Symbols don't exist in the webview; the port takes an
//!   optional text glyph instead and renders it verbatim when present.
//!
//! Toasts are desktop-only: the iOS client has no toast surface.

use dioxus::prelude::*;

/// Default lifetime in seconds, matching Swift.
pub const TOAST_DEFAULT_SECONDS: f64 = 3.2;

/// One toast. `id` is a launcher-side nonce so a stale timer can't dismiss
/// a newer toast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub id: u64,
    pub text: String,
    /// Optional text glyph shown leading the text (SF Symbols superseded).
    pub icon: Option<String>,
}

/// Launcher-owned toast state. `Clone` so it can live in the app signal.
#[derive(Debug, Clone, Default)]
pub struct ToastCenter {
    current: Option<Toast>,
    next_id: u64,
}

impl ToastCenter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> Option<&Toast> {
        self.current.as_ref()
    }

    /// Show a toast, replacing any current one. Returns the new toast's id
    /// so the launcher's timer thread can dismiss exactly this toast.
    pub fn show(&mut self, text: impl Into<String>, seconds: f64) -> (u64, f64) {
        let id = self.next_id;
        self.next_id += 1;
        self.current = Some(Toast {
            id,
            text: text.into(),
            icon: None,
        });
        (id, seconds)
    }

    /// Show with a leading glyph.
    pub fn show_with_icon(
        &mut self,
        text: impl Into<String>,
        icon: impl Into<String>,
        seconds: f64,
    ) -> (u64, f64) {
        let (id, s) = self.show(text, seconds);
        if let Some(t) = self.current.as_mut() {
            t.icon = Some(icon.into());
        }
        (id, s)
    }

    /// Dismiss the current toast if its id matches (stale timers no-op).
    pub fn dismiss_id(&mut self, id: u64) {
        if self.current.as_ref().is_some_and(|t| t.id == id) {
            self.current = None;
        }
    }

    /// Dismiss unconditionally (tap, or replacing).
    pub fn dismiss(&mut self) {
        self.current = None;
    }
}

/// Top-right capsule toast, mounted once over the app layout. A tap calls
/// `on_tap` with the toast id; the launcher dismisses and runs the tap
/// action.
#[component]
pub fn ToastOverlay(toast: Option<Toast>, on_tap: EventHandler<u64>) -> Element {
    let Some(t) = toast else {
        return rsx! {};
    };
    let id = t.id;
    rsx! {
        div {
            class: "toast-overlay",
            div {
                class: "toast-capsule",
                role: "status",
                onclick: move |_| on_tap.call(id),
                if let Some(glyph) = t.icon.clone() {
                    span { class: "toast-icon", "{glyph}" }
                }
                span { class: "toast-text", "{t.text}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_replaces_current() {
        let mut c = ToastCenter::new();
        let (id1, _) = c.show("first", TOAST_DEFAULT_SECONDS);
        let (id2, _) = c.show("second", TOAST_DEFAULT_SECONDS);
        assert_ne!(id1, id2);
        assert_eq!(c.current().unwrap().text, "second");
        assert_eq!(c.current().unwrap().id, id2);
    }

    #[test]
    fn stale_timer_cannot_dismiss_newer_toast() {
        let mut c = ToastCenter::new();
        let (id1, _) = c.show("first", TOAST_DEFAULT_SECONDS);
        let (id2, _) = c.show("second", TOAST_DEFAULT_SECONDS);
        c.dismiss_id(id1); // stale: no-op
        assert_eq!(c.current().unwrap().id, id2);
        c.dismiss_id(id2);
        assert!(c.current().is_none());
    }

    #[test]
    fn dismiss_clears() {
        let mut c = ToastCenter::new();
        c.show("hi", TOAST_DEFAULT_SECONDS);
        c.dismiss();
        assert!(c.current().is_none());
    }

    #[test]
    fn default_lifetime_matches_swift() {
        assert_eq!(TOAST_DEFAULT_SECONDS, 3.2);
    }

    #[test]
    fn icon_rendered_when_present() {
        let mut c = ToastCenter::new();
        c.show_with_icon("site live", "🌐", TOAST_DEFAULT_SECONDS);
        assert_eq!(c.current().unwrap().icon.as_deref(), Some("🌐"));
    }
}
