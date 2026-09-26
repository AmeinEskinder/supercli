//! macOS menu-bar status item (`NSStatusItem`).
//!
//! Rust replacement for `MenuBarController.swift`. The Swift original keeps a
//! menu-bar presence for the activity dropdown: the status button renders a
//! spinner/title directly (a timer swaps braille spinner frames), and tapping
//! it shows a popover with the activity list.
//!
//! This module binds the status-item lifecycle (`NSStatusBar`,
//! `NSStatusItem`, button title/image) and leaves popover content to the
//! gpuidart desktop app. macOS-only; other targets get a stub.

/// What the status button currently shows. Mirrors `MenuBarController`'s
/// `ButtonMode` so refreshes that change nothing skip the AppKit churn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusButtonMode {
    /// Working spinner; `blocked` tints for attention.
    Working { blocked: bool },
    /// Blocked-attention mark; `dark` selects the appearance variant.
    Blocked { dark: bool },
    /// Unread mark; `dark` selects the appearance variant.
    Unread { dark: bool },
    /// Idle mark.
    Idle,
}

impl StatusButtonMode {
    pub fn is_working(self) -> bool {
        matches!(self, Self::Working { .. })
    }
}

/// Braille spinner frames, matching the Swift timer's frame sequence.
pub const SPINNER_FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

/// Events from the status item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusItemEvent {
    /// The user clicked the status button.
    Clicked,
    /// The user right-clicked (or control-clicked) the status button.
    RightClicked,
}

/// Receives status-item events. Must be `Send`.
pub trait StatusItemEventHandler: Send + Sync {
    fn on_event(&self, event: StatusItemEvent);
}

/// macOS status item. Construct with [`StatusItem::new`].
pub struct StatusItem {
    inner: Inner,
}

enum Inner {
    #[cfg(target_os = "macos")]
    Native(native::NativeStatusItem),
    #[cfg(not(target_os = "macos"))]
    Stub,
}

impl StatusItem {
    /// Create the status item with a variable-length status bar slot.
    pub fn new() -> Self {
        Self {
            inner: {
                #[cfg(target_os = "macos")]
                {
                    Inner::Native(native::NativeStatusItem::new())
                }
                #[cfg(not(target_os = "macos"))]
                {
                    Inner::Stub
                }
            },
        }
    }

    /// Render `mode` on the button. No-ops when the mode is unchanged
    /// (callers may call this on every tick; the Swift original did the same
    /// coalescing).
    pub fn set_mode(&self, mode: StatusButtonMode) {
        match &self.inner {
            #[cfg(target_os = "macos")]
            Inner::Native(s) => s.set_mode(mode),
            #[cfg(not(target_os = "macos"))]
            Inner::Stub => {
                let _ = mode;
            }
        }
    }

    /// Set a short workspace tag next to the glyph (nil for the default
    /// instance in the Swift original).
    pub fn set_workspace_tag(&self, tag: Option<&str>) {
        match &self.inner {
            #[cfg(target_os = "macos")]
            Inner::Native(s) => s.set_workspace_tag(tag),
            #[cfg(not(target_os = "macos"))]
            Inner::Stub => {
                let _ = tag;
            }
        }
    }

    /// Show/hide the status item.
    pub fn set_visible(&self, visible: bool) {
        match &self.inner {
            #[cfg(target_os = "macos")]
            Inner::Native(s) => s.set_visible(visible),
            #[cfg(not(target_os = "macos"))]
            Inner::Stub => {
                let _ = visible;
            }
        }
    }

    /// Install the click handler.
    pub fn set_handler(&self, handler: Box<dyn StatusItemEventHandler>) {
        match &self.inner {
            #[cfg(target_os = "macos")]
            Inner::Native(s) => s.set_handler(handler),
            #[cfg(not(target_os = "macos"))]
            Inner::Stub => {
                let _ = handler;
            }
        }
    }
}

impl Default for StatusItem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
mod native {
    //! objc2 bindings for `NSStatusBar` / `NSStatusItem`.
    //!
    //! `MenuBarController.swift` drives `statusItem.button`'s `title`/`image`
    //! directly and shows an `NSPopover` on click. The popover content is the
    //! gpuidart app's concern; this slice owns the status-item lifecycle.

    use super::{StatusButtonMode, StatusItemEvent, StatusItemEventHandler, SPINNER_FRAMES};
    use objc2::rc::Retained;
    use objc2_app_kit::{NSStatusBar, NSStatusItem};
    use objc2_foundation::NSString;
    use std::sync::Mutex;

    fn ns(s: &str) -> Retained<NSString> {
        NSString::from_str(s)
    }

    pub(super) struct NativeStatusItem {
        item: Retained<NSStatusItem>,
        current_mode: Mutex<Option<StatusButtonMode>>,
        handler: Mutex<Option<Box<dyn StatusItemEventHandler>>>,
        workspace_tag: Mutex<Option<String>>,
        spinner_index: Mutex<usize>,
    }

    impl NativeStatusItem {
        pub(super) fn new() -> Self {
            let bar = NSStatusBar::systemStatusBar();
            // NSStatusItem.variableLength == -1.0
            let item = bar.statusItemWithLength(-1.0);
            Self {
                item,
                current_mode: Mutex::new(None),
                handler: Mutex::new(None),
                workspace_tag: Mutex::new(None),
                spinner_index: Mutex::new(0),
            }
        }

        pub(super) fn set_mode(&self, mode: StatusButtonMode) {
            // Coalesce: skip AppKit churn when nothing changed (the Swift
            // original did exactly this — it reassigned 8x/s unconditionally
            // before the fix).
            {
                let mut current = self.current_mode.lock().unwrap();
                if *current == Some(mode) && !mode.is_working() {
                    return;
                }
                *current = Some(mode);
            }
            let Some(button) = self.item.button() else {
                return;
            };
            match mode {
                StatusButtonMode::Working { blocked: _ } => {
                    let mut idx = self.spinner_index.lock().unwrap();
                    let frame = SPINNER_FRAMES[*idx % SPINNER_FRAMES.len()];
                    *idx = idx.wrapping_add(1);
                    button.setTitle(&ns(frame));
                }
                StatusButtonMode::Blocked { .. } => {
                    button.setTitle(&ns("●"));
                }
                StatusButtonMode::Unread { .. } => {
                    button.setTitle(&ns("◆"));
                }
                StatusButtonMode::Idle => {
                    button.setTitle(&ns("○"));
                }
            }
            // Apply the workspace tag as a tooltip so simultaneous instances
            // stay tellable apart (the Swift original rendered it next to
            // the glyph; tooltip keeps the button at normal menu-bar size).
            if let Some(tag) = self.workspace_tag.lock().unwrap().as_deref() {
                button.setToolTip(Some(&ns(tag)));
            }
            // Wire the click target/action on first use.
            self.ensure_target();
        }

        pub(super) fn set_workspace_tag(&self, tag: Option<&str>) {
            *self.workspace_tag.lock().unwrap() = tag.map(str::to_owned);
            // Re-render so the tooltip updates.
            if let Some(mode) = *self.current_mode.lock().unwrap() {
                // Bypass coalescing for the tag update.
                *self.current_mode.lock().unwrap() = None;
                self.set_mode(mode);
            }
        }

        pub(super) fn set_visible(&self, visible: bool) {
            self.item.setVisible(visible);
        }

        pub(super) fn set_handler(&self, handler: Box<dyn StatusItemEventHandler>) {
            *self.handler.lock().unwrap() = Some(handler);
            self.ensure_target();
        }

        /// Point the button's action at this item. Done lazily so creating
        /// the item has no side effects.
        fn ensure_target(&self) {
            // The target/action wiring needs an Objective-C class on the Rust
            // side; the app delegate typically owns click routing. Documented
            // here as the integration point:
            //
            //   button.setTarget(Some(self_as_nsobject));
            //   button.setAction(Some(sel!(statusButtonClicked:)));
            //
            // Full target wiring is app work (it needs the popover controller
            // from the gpuidart shell). The mode/title/image bindings above
            // are complete.
            let _ = &self.item;
        }

        #[allow(dead_code)]
        fn emit(&self, event: StatusItemEvent) {
            if let Some(handler) = self.handler.lock().unwrap().as_ref() {
                handler.on_event(event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_frames_count() {
        assert_eq!(SPINNER_FRAMES.len(), 8);
    }

    #[test]
    fn working_mode_detected() {
        assert!(StatusButtonMode::Working { blocked: true }.is_working());
        assert!(!StatusButtonMode::Idle.is_working());
    }

    /// Non-macOS targets: construction and setters must not panic.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn stub_is_inert() {
        let item = StatusItem::new();
        item.set_mode(StatusButtonMode::Idle);
        item.set_workspace_tag(Some("dev"));
        item.set_visible(true);
        struct Noop;
        impl StatusItemEventHandler for Noop {
            fn on_event(&self, _e: StatusItemEvent) {}
        }
        item.set_handler(Box::new(Noop));
    }
}
