//! QR camera scanning for pairing, ported from the Swift
//! `PairingScannerView` (AVFoundation) in `PairingView.swift`.
//!
//! The camera and QR decode are platform-specific: iOS uses
//! `AVCaptureMetadataOutput`, the Dioxus mobile launcher drives
//! `getUserMedia` + a JS QR decoder in its webview. This module owns the
//! portable half and the JS bridge contract: scanner state, the
//! scan-dedup rule (a repeated sighting of the same code must not
//! re-fire), and pause semantics — pausing stops the camera, not just
//! the decode gate, so the camera (and its status indicator) doesn't
//! stay hot for the whole pairing exchange.

use crate::i18n::t;
use dioxus::prelude::*;

/// Scanner lifecycle, mirroring the Swift preview's states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QrScanState {
    #[default]
    Idle,
    RequestingPermission,
    PermissionDenied,
    Scanning,
    Paused,
}

impl QrScanState {
    pub fn is_live(&self) -> bool {
        matches!(self, QrScanState::Scanning)
    }
}

/// Dedup gate: only the first sighting of a code fires `on_code`.
/// Mirrors the Swift `lastCode` check.
#[derive(Debug, Default)]
pub struct QrDedup {
    last: Option<String>,
}

impl QrDedup {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true when `code` is new (and remembers it).
    pub fn observe(&mut self, code: &str) -> bool {
        if self.last.as_deref() == Some(code) {
            return false;
        }
        self.last = Some(code.to_string());
        true
    }

    pub fn reset(&mut self) {
        self.last = None;
    }
}

/// JS bridge contract (implemented by the launcher):
///
/// - The launcher installs `window.__unpeelQrStart()` (async: resolves
///   when the camera is live and codes stream to Rust via
///   `dioxus.send(code)`; throws with a human-readable reason on
///   denial/failure) and `window.__unpeelQrStop()` (stops the camera and
///   releases it; pausing stops the camera, not just the decode gate, so
///   it doesn't stay hot for the whole pairing exchange).
///
/// The component evals [`QR_CTL_JS`] once on mount and drives it over the
/// eval channel: Rust → JS commands are `"pause"` / `"resume"`, JS → Rust
/// messages are `ctl:ready`, `ctl:denied:<reason>`, `ctl:paused`,
/// `ctl:resumed`, or a scanned code. Messages starting with `ctl:` are
/// reserved — pairing codes never do.
///
/// The launcher resolves camera permission inside `__unpeelQrStart`; a
/// denial surfaces as `QrScanState::PermissionDenied` via `on_state`.
pub const QR_CTL_JS: &str = r#"
(async () => {
  if (typeof window.__unpeelQrStart !== 'function') {
    dioxus.send('ctl:denied:QR scanning is not installed in this launcher');
    return;
  }
  dioxus.send('ctl:ready');
  for (;;) {
    const cmd = await dioxus.recv();
    if (cmd === 'pause') {
      window.__unpeelQrStop();
      dioxus.send('ctl:paused');
    } else if (cmd === 'resume') {
      try {
        await window.__unpeelQrStart();
        dioxus.send('ctl:resumed');
      } catch (e) {
        dioxus.send('ctl:denied:' + (e && e.message ? e.message : 'camera unavailable'));
      }
    }
  }
})()
"#;

/// Kept for API compatibility: delegates to the control-channel script.
pub const QR_START_JS: &str = QR_CTL_JS;

pub const QR_STOP_JS: &str = r#"
if (typeof window.__unpeelQrStop === 'function') { window.__unpeelQrStop(); }
"#;

/// QR scanner view: camera preview (rendered by the launcher's JS into
/// `.qr-preview`) plus scan-state UI. `paused` stops the camera; `on_code`
/// fires once per distinct code.
#[component]
pub fn QrScannerView(
    paused: bool,
    on_code: EventHandler<String>,
    on_state: EventHandler<QrScanState>,
) -> Element {
    let state = use_signal(|| QrScanState::Idle);
    let error = use_signal(|| None::<String>);

    // Mirror the plain `paused` prop into a signal: `use_effect` only
    // re-runs on signal reads, not on plain-prop changes.
    let mut paused_mirror = use_signal(|| paused);
    if *paused_mirror.read() != paused {
        paused_mirror.set(paused);
    }

    // The control-channel eval handle, created once on mount.
    let ctl = use_signal(|| None::<dioxus::document::Eval>);

    // Stop the camera if the scanner unmounts while live.
    use_drop(|| {
        let _ = dioxus::document::eval(QR_STOP_JS);
    });

    // Mount: open the control channel and pump JS → Rust messages.
    use_effect(move || {
        let mut ctl = ctl;
        let mut state = state;
        let mut error = error;
        spawn(async move {
            if ctl.read().is_some() {
                return;
            }
            let mut ev = dioxus::document::eval(QR_CTL_JS);
            ctl.set(Some(ev));
            state.set(QrScanState::RequestingPermission);
            on_state.call(QrScanState::RequestingPermission);
            let mut dedup = QrDedup::new();
            while let Ok(msg) = ev.recv::<String>().await {
                if let Some(rest) = msg.strip_prefix("ctl:") {
                    match rest {
                        "ready" => {}
                        "paused" => {
                            state.set(QrScanState::Paused);
                            on_state.call(QrScanState::Paused);
                        }
                        "resumed" => {
                            state.set(QrScanState::Scanning);
                            on_state.call(QrScanState::Scanning);
                        }
                        r if r.starts_with("denied:") => {
                            let reason = r
                                .strip_prefix("denied:")
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| t("qr.camera_unavailable"));
                            error.set(Some(reason.to_string()));
                            state.set(QrScanState::PermissionDenied);
                            on_state.call(QrScanState::PermissionDenied);
                        }
                        _ => {}
                    }
                } else if *state.read() == QrScanState::Scanning && dedup.observe(&msg) {
                    on_code.call(msg);
                }
            }
        });
    });

    // Pause/resume: forward prop changes as commands over the channel.
    // The camera starts on the first resume (mount is born paused when the
    // parent passes `paused: true`).
    use_effect(move || {
        let want_paused = paused_mirror();
        let ctl = ctl;
        let mut state = state;
        spawn(async move {
            let Some(ev) = ctl.read().as_ref().copied() else {
                return;
            };
            if want_paused {
                let _ = ev.send("pause");
            } else {
                // Kick the camera on when (re)entering the live state.
                state.set(QrScanState::RequestingPermission);
                let _ = ev.send("resume");
            }
        });
    });

    rsx! {
        div { class: "qr-scanner",
            div { class: "qr-preview" }
            match *state.read() {
                QrScanState::RequestingPermission => rsx! {
                    div { class: "qr-status", {t("qr.requesting_camera_access")} }
                },
                QrScanState::PermissionDenied => rsx! {
                    div { class: "qr-status qr-denied",
                        {t("qr.camera_access_denied")}
                        if let Some(reason) = error.read().as_ref() {
                            div { class: "qr-reason", "{reason}" }
                        }
                        div { class: "qr-hint", "Allow camera access in system Settings, then reopen the scanner." }
                    }
                },
                QrScanState::Paused => rsx! {
                    div { class: "qr-status", {t("qr.scanner_paused")} }
                },
                _ => rsx! {
                    div { class: "qr-status", {t("qr.point_the_camera_at_the_pairing_qr_code")} }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_fires_once_per_code() {
        let mut d = QrDedup::new();
        assert!(d.observe("code-a"));
        assert!(!d.observe("code-a"));
        assert!(!d.observe("code-a"));
        assert!(d.observe("code-b"));
        assert!(!d.observe("code-b"));
        // A re-sighting of the first code after a different one fires again,
        // mirroring Swift's single-`lastCode` slot.
        assert!(d.observe("code-a"));
    }

    #[test]
    fn dedup_reset() {
        let mut d = QrDedup::new();
        assert!(d.observe("x"));
        d.reset();
        assert!(d.observe("x"));
    }

    #[test]
    fn scan_state_liveness() {
        assert!(QrScanState::Scanning.is_live());
        assert!(!QrScanState::Paused.is_live());
        assert!(!QrScanState::Idle.is_live());
        assert!(!QrScanState::PermissionDenied.is_live());
        assert!(!QrScanState::RequestingPermission.is_live());
    }
}
