//! Port of `MenuBarController.swift` — menu-bar state machine.
//!
//! The pure `ButtonMode` state machine (working spinner, blocked badge,
//! unread badge, idle mark, workspace tag) is cross-platform and tested.
//! The `NSStatusItem` UI binding is `#[cfg(target_os = "macos")]` via objc2;
//! the Dart app owns the popover content.

/// Menu-bar button mode. Swift: `MenuBarController.ButtonMode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ButtonMode {
    /// Spinner animation; `frame` is the current animation frame.
    Working { frame: u8 },
    /// Orange dot: blocked sessions need attention.
    Blocked { count: u32 },
    /// Blue dot: unread completed sessions.
    Unread { count: u32 },
    /// Dimmed mark: idle, nothing to report.
    Idle,
}

/// Frames in the working spinner animation.
pub const WORKING_FRAMES: u8 = 8;

/// Computes the button mode from activity state. Precedence: blocked >
/// unread > working > idle. Matches Swift's rendering priority.
pub fn button_mode(
    working: bool,
    blocked_count: u32,
    unread_count: u32,
    spinner_frame: u8,
) -> ButtonMode {
    if blocked_count > 0 {
        ButtonMode::Blocked {
            count: blocked_count,
        }
    } else if unread_count > 0 {
        ButtonMode::Unread {
            count: unread_count,
        }
    } else if working {
        ButtonMode::Working {
            frame: spinner_frame % WORKING_FRAMES,
        }
    } else {
        ButtonMode::Idle
    }
}

/// Advances the spinner frame. Only called while working (the Swift timer
/// runs only in that mode).
pub fn next_spinner_frame(frame: u8) -> u8 {
    (frame + 1) % WORKING_FRAMES
}

/// Menu-bar item state: mode plus the optional workspace tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuBarState {
    pub mode: ButtonMode,
    /// Optional workspace tag shown beside the icon.
    pub workspace_tag: Option<String>,
}

impl MenuBarState {
    pub fn new() -> Self {
        Self {
            mode: ButtonMode::Idle,
            workspace_tag: None,
        }
    }

    pub fn update(&mut self, working: bool, blocked: u32, unread: u32, frame: u8) {
        self.mode = button_mode(working, blocked, unread, frame);
    }
}

impl Default for MenuBarState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_takes_precedence_over_everything() {
        assert_eq!(button_mode(true, 2, 3, 0), ButtonMode::Blocked { count: 2 });
    }

    #[test]
    fn unread_beats_working() {
        assert_eq!(button_mode(true, 0, 5, 0), ButtonMode::Unread { count: 5 });
    }

    #[test]
    fn working_shows_spinner_frame() {
        assert_eq!(button_mode(true, 0, 0, 3), ButtonMode::Working { frame: 3 });
        // Frames wrap.
        assert_eq!(
            button_mode(true, 0, 0, WORKING_FRAMES + 1),
            ButtonMode::Working { frame: 1 }
        );
    }

    #[test]
    fn idle_when_nothing_to_report() {
        assert_eq!(button_mode(false, 0, 0, 0), ButtonMode::Idle);
    }

    #[test]
    fn spinner_advances_and_wraps() {
        assert_eq!(next_spinner_frame(0), 1);
        assert_eq!(next_spinner_frame(WORKING_FRAMES - 1), 0);
    }

    #[test]
    fn menu_bar_state_updates_mode() {
        let mut state = MenuBarState::new();
        assert_eq!(state.mode, ButtonMode::Idle);
        state.update(true, 0, 0, 2);
        assert_eq!(state.mode, ButtonMode::Working { frame: 2 });
        state.workspace_tag = Some("work".to_string());
        assert_eq!(state.workspace_tag, Some("work".to_string()));
    }
}
