//! Terminal mouse-mode tracking and wheel-forwarding decisions.
//!
//! Port of `RemoteTerminalMouseModeTracker` and the wheel-forwarding helpers
//! (`prefersRemoteMouseWheel`, `wheelForwarding`, `alternateScrollSequence`)
//! from `RemoteGhosttyTerminalView.swift` (iOS client).
//!
//! This is protocol logic, not UI: it parses the ANSI mode-set/reset sequences
//! the Host's PTY stream carries and decides how a flick/wheel gesture should
//! reach the remote program.

use std::collections::HashSet;
use std::sync::Mutex;

/// Alternate-screen DEC private modes.
const ALTERNATE_SCREEN_MODES: &[i32] = &[47, 1047, 1049];
/// Mouse-tracking DEC private modes.
const MOUSE_TRACKING_MODES: &[i32] = &[9, 1000, 1002, 1003];
/// DECCKM (mode 1): application cursor keys.
const APPLICATION_CURSOR_KEY_MODES: &[i32] = &[1];
/// A pending (unterminated) escape prefix longer than this is not a real
/// CSI — drop it entirely rather than truncating it into a byte soup that
/// could parse as a different sequence.
const MAXIMUM_PENDING_BYTES: usize = 96;

/// Providers whose TUI owns wheel scrolling itself (their TUI scrolls its
/// own transcript), so wheel events should be forwarded as mouse reports
/// rather than scrolling the local scrollback.
const REMOTE_MOUSE_WHEEL_PROVIDERS: &[&str] = &["claude", "grok", "opencode"];

/// How a flick inside the terminal reaches the remote program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteWheelForwarding {
    /// Do not forward: the flick scrolls the local scrollback, like any shell.
    None,
    /// SGR wheel reports: the program tracks the mouse.
    Mouse,
    /// xterm/Ghostty "alternate scroll" (DEC 1007 behavior): the program owns
    /// the alternate screen but tracks no mouse, so each wheel step becomes a
    /// cursor Up/Down key, in application-cursor form when DECCKM is on.
    AlternateScroll,
}

/// Wheel direction for [`alternate_scroll_sequence`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Whether the session's agent is one that owns wheel scrolling itself
/// (its TUI scrolls its own transcript), judged from any of the three
/// identities a summary can carry: the legacy launch-derived provider,
/// the command head, or the Host-observed foreground runtime — the last
/// is what qualifies a Claude started by hand in a shell or through a
/// wrapper whose command head is not literally `claude`.
pub fn prefers_remote_mouse_wheel(
    provider_id: Option<&str>,
    command: &str,
    active_runtime_id: Option<&str>,
) -> bool {
    if let Some(runtime) = active_runtime_id {
        if REMOTE_MOUSE_WHEEL_PROVIDERS.contains(&runtime.to_lowercase().as_str()) {
            return true;
        }
    }
    let provider = provider_id.unwrap_or("").to_lowercase();
    if REMOTE_MOUSE_WHEEL_PROVIDERS.contains(&provider.as_str()) {
        return true;
    }
    let executable = command
        .split_whitespace()
        .next()
        .map(|head| head.rsplit('/').next().unwrap_or(head).to_lowercase());
    match executable {
        Some(exe) => REMOTE_MOUSE_WHEEL_PROVIDERS.contains(&exe.as_str()),
        None => false,
    }
}

/// Once the stream carried a Host mode snapshot, the tracked modes are
/// authoritative: forward wheel reports iff the program tracks the mouse,
/// emulate alternate scroll for a mouse-less alternate screen, and otherwise
/// leave the flick to the local scrollback. The provider heuristic survives
/// only as the pre-snapshot fallback for Hosts that never send a preamble,
/// and even then only until a disable is seen.
pub fn wheel_forwarding(
    has_host_mode_snapshot: bool,
    mouse_tracking_enabled: bool,
    alternate_screen_enabled: bool,
    saw_mouse_or_alternate_disable: bool,
    provider_prefers_remote_mouse_wheel: bool,
) -> RemoteWheelForwarding {
    if mouse_tracking_enabled {
        return RemoteWheelForwarding::Mouse;
    }
    if alternate_screen_enabled {
        return RemoteWheelForwarding::AlternateScroll;
    }
    if has_host_mode_snapshot {
        return RemoteWheelForwarding::None;
    }
    if provider_prefers_remote_mouse_wheel && !saw_mouse_or_alternate_disable {
        RemoteWheelForwarding::Mouse
    } else {
        RemoteWheelForwarding::None
    }
}

/// Alternate-scroll translation of a vertical wheel burst: one cursor
/// Up/Down per step (`ESC [ A/B`, or `ESC O A/B` under DECCKM). Returns `None`
/// for horizontal wheel (no key equivalent) or non-positive steps.
pub fn alternate_scroll_sequence(
    direction: WheelDirection,
    steps: usize,
    application_cursor_keys: bool,
) -> Option<String> {
    if steps == 0 {
        return None;
    }
    let key = match direction {
        WheelDirection::Up => {
            if application_cursor_keys {
                "\x1BOA"
            } else {
                "\x1B[A"
            }
        }
        WheelDirection::Down => {
            if application_cursor_keys {
                "\x1BOB"
            } else {
                "\x1B[B"
            }
        }
        WheelDirection::Left | WheelDirection::Right => return None,
    };
    Some(key.repeat(steps))
}

#[derive(Debug, Default)]
struct TrackerState {
    alternate_screen_stack: HashSet<i32>,
    mouse_tracking_stack: HashSet<i32>,
    application_cursor_keys_on: bool,
    pending: Vec<u8>,
    saw_disable: bool,
    host_mode_snapshot: bool,
}

/// Tracks the DEC private modes a PTY stream enables/disables.
///
/// Port of `RemoteTerminalMouseModeTracker`: feeds raw output bytes, parses
/// `CSI ? <params> h/l` sequences (carrying split sequences across `feed`
/// calls), and answers whether mouse tracking / the alternate screen are
/// currently enabled.
#[derive(Debug, Default)]
pub struct RemoteTerminalMouseModeTracker {
    state: Mutex<TrackerState>,
}

impl RemoteTerminalMouseModeTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// True once this stream's reset baseline carried the Host's mode
    /// snapshot (the hello / fresh-tail `modePreamble`): from then on the
    /// tracked modes are authoritative, not an inference from whatever
    /// bytes happened to survive in the tail. Cleared with [`reset`](Self::reset).
    pub fn has_host_mode_snapshot(&self) -> bool {
        self.state.lock().unwrap().host_mode_snapshot
    }

    pub fn mark_host_mode_snapshot(&self) {
        self.state.lock().unwrap().host_mode_snapshot = true;
    }

    pub fn alternate_screen_enabled(&self) -> bool {
        !self.state.lock().unwrap().alternate_screen_stack.is_empty()
    }

    /// DECCKM state: true once the remote enabled application cursor keys.
    pub fn application_cursor_keys_enabled(&self) -> bool {
        self.state.lock().unwrap().application_cursor_keys_on
    }

    pub fn mouse_tracking_enabled(&self) -> bool {
        !self.state.lock().unwrap().mouse_tracking_stack.is_empty()
    }

    pub fn saw_mouse_or_alternate_disable(&self) -> bool {
        self.state.lock().unwrap().saw_disable
    }

    pub fn reset(&self) {
        let mut s = self.state.lock().unwrap();
        s.alternate_screen_stack.clear();
        s.mouse_tracking_stack.clear();
        s.application_cursor_keys_on = false;
        s.saw_disable = false;
        s.host_mode_snapshot = false;
        s.pending.clear();
    }

    pub fn feed(&self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let mut s = self.state.lock().unwrap();
        // Common case: no carried prefix — scan the chunk in place, no copy.
        if s.pending.is_empty() {
            Self::scan_locked(&mut s, data);
        } else {
            let mut bytes = std::mem::take(&mut s.pending);
            bytes.extend_from_slice(data);
            Self::scan_locked(&mut s, &bytes);
        }
        if s.pending.len() > MAXIMUM_PENDING_BYTES {
            s.pending.clear();
        }
    }

    fn scan_locked(s: &mut TrackerState, bytes: &[u8]) {
        let count = bytes.len();
        let mut index = 0;
        while index < count {
            if bytes[index] != 0x1B {
                index += 1;
                continue;
            }
            if index + 1 >= count {
                s.pending.push(bytes[index]);
                break;
            }
            let next = bytes[index + 1];
            if next == 0x63 {
                // ESC c — full terminal reset clears all modes.
                Self::reset_modes_locked(s);
                s.saw_disable = true;
                index += 2;
                continue;
            }
            if next != 0x5B {
                index += 2;
                continue;
            }
            if index + 2 >= count {
                s.pending.extend_from_slice(&bytes[index..]);
                break;
            }

            let mut cursor = index + 2;
            let private_mode = bytes[cursor] == 0x3F;
            if private_mode {
                cursor += 1;
            }

            let params_start = cursor;
            while cursor < count && !is_final_byte(bytes[cursor]) {
                cursor += 1;
            }
            if cursor >= count {
                s.pending.extend_from_slice(&bytes[index..]);
                break;
            }

            if private_mode && (bytes[cursor] == 0x68 || bytes[cursor] == 0x6C) {
                let params = parse_params(&bytes[params_start..cursor]);
                Self::update_modes_locked(s, &params, bytes[cursor] == 0x68);
            }
            index = cursor + 1;
        }
    }

    fn update_modes_locked(s: &mut TrackerState, params: &[i32], enabled: bool) {
        for &param in params {
            if ALTERNATE_SCREEN_MODES.contains(&param) {
                update_set(&mut s.alternate_screen_stack, param, enabled);
                s.saw_disable = !enabled;
            }
            if MOUSE_TRACKING_MODES.contains(&param) {
                update_set(&mut s.mouse_tracking_stack, param, enabled);
                s.saw_disable = !enabled;
            }
            if APPLICATION_CURSOR_KEY_MODES.contains(&param) {
                s.application_cursor_keys_on = enabled;
            }
        }
    }

    fn reset_modes_locked(s: &mut TrackerState) {
        s.alternate_screen_stack.clear();
        s.mouse_tracking_stack.clear();
        s.application_cursor_keys_on = false;
    }
}

fn update_set(set: &mut HashSet<i32>, mode: i32, enabled: bool) {
    if enabled {
        set.insert(mode);
    } else {
        set.remove(&mode);
    }
}

fn parse_params(bytes: &[u8]) -> Vec<i32> {
    let s = String::from_utf8_lossy(bytes);
    s.split(';').filter_map(|p| p.parse::<i32>().ok()).collect()
}

fn is_final_byte(byte: u8) -> bool {
    (0x40..=0x7E).contains(&byte)
}

/// A non-2xx reply from the Mac. Carries the HTTP status and the server's
/// `{"error": ...}` body instead of collapsing everything into a generic
/// transport error.
///
/// Port of `RemoteMacClientError` from `RemoteMacClient.swift`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteMacClientError {
    pub status_code: u16,
    pub server_message: Option<String>,
}

impl RemoteMacClientError {
    pub fn new(status_code: u16, server_message: Option<String>) -> Self {
        Self {
            status_code,
            server_message,
        }
    }
}

impl std::fmt::Display for RemoteMacClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.server_message {
            Some(msg) if !msg.is_empty() => write!(f, "HTTP {}: {}", self.status_code, msg),
            _ => write!(f, "HTTP {}", self.status_code),
        }
    }
}

impl std::error::Error for RemoteMacClientError {}

#[cfg(test)]
mod tests {
    use super::*;

    // --- prefers_remote_mouse_wheel ---

    #[test]
    fn observed_runtime_qualifies_without_provider_or_command_head() {
        assert!(prefers_remote_mouse_wheel(None, "/bin/zsh", Some("Claude")));
        assert!(prefers_remote_mouse_wheel(
            None,
            "~/bin/agent-wrapper --profile work",
            Some("claude")
        ));
    }

    #[test]
    fn legacy_provider_and_command_head_still_qualify() {
        assert!(prefers_remote_mouse_wheel(Some("claude"), "/bin/zsh", None));
        assert!(prefers_remote_mouse_wheel(
            None,
            "/opt/homebrew/bin/claude --resume",
            None
        ));
    }

    #[test]
    fn plain_shell_does_not_qualify() {
        assert!(!prefers_remote_mouse_wheel(None, "/bin/zsh", None));
        assert!(!prefers_remote_mouse_wheel(None, "/bin/zsh", Some("vim")));
    }

    // --- wheel_forwarding ---

    #[test]
    fn classic_claude_with_snapshot_leaves_flick_to_local_scrollback() {
        // Classic (non-full-screen) Claude: the Host snapshot says no alt
        // screen, no mouse. Forwarding SGR wheel bytes would be ignored by
        // the TUI and the phone could not scroll at all.
        assert_eq!(
            wheel_forwarding(true, false, false, false, true),
            RemoteWheelForwarding::None
        );
    }

    #[test]
    fn full_screen_claude_with_snapshot_forwards_wheel() {
        assert_eq!(
            wheel_forwarding(true, true, true, false, true),
            RemoteWheelForwarding::Mouse
        );
    }

    #[test]
    fn no_snapshot_yet_keeps_provider_heuristic_until_disable_seen() {
        assert_eq!(
            wheel_forwarding(false, false, false, false, true),
            RemoteWheelForwarding::Mouse
        );
        assert_eq!(
            wheel_forwarding(false, false, false, true, true),
            RemoteWheelForwarding::None
        );
        assert_eq!(
            wheel_forwarding(false, false, false, false, false),
            RemoteWheelForwarding::None
        );
    }

    #[test]
    fn alternate_screen_without_mouse_emulates_alternate_scroll() {
        assert_eq!(
            wheel_forwarding(true, false, true, false, false),
            RemoteWheelForwarding::AlternateScroll
        );
        assert_eq!(
            alternate_scroll_sequence(WheelDirection::Down, 2, false),
            Some("\x1B[B\x1B[B".to_string())
        );
        assert_eq!(
            alternate_scroll_sequence(WheelDirection::Up, 1, true),
            Some("\x1BOA".to_string())
        );
        assert_eq!(
            alternate_scroll_sequence(WheelDirection::Left, 1, false),
            None
        );
        assert_eq!(
            alternate_scroll_sequence(WheelDirection::Down, 0, false),
            None
        );
    }

    // --- RemoteTerminalMouseModeTracker ---

    #[test]
    fn enables_and_disables_mouse_tracking() {
        let tracker = RemoteTerminalMouseModeTracker::new();
        tracker.feed(b"\x1B[?1002h");
        assert!(tracker.mouse_tracking_enabled());
        assert!(!tracker.saw_mouse_or_alternate_disable());

        tracker.feed(b"\x1B[?1002l");
        assert!(!tracker.mouse_tracking_enabled());
        assert!(tracker.saw_mouse_or_alternate_disable());
    }

    #[test]
    fn tracks_alternate_screen() {
        let tracker = RemoteTerminalMouseModeTracker::new();
        tracker.feed(b"\x1B[?1049h");
        assert!(tracker.alternate_screen_enabled());

        tracker.feed(b"\x1B[?1049l");
        assert!(!tracker.alternate_screen_enabled());
    }

    #[test]
    fn multiple_params_in_one_sequence() {
        let tracker = RemoteTerminalMouseModeTracker::new();
        tracker.feed(b"\x1B[?1049;1002h");
        assert!(tracker.alternate_screen_enabled());
        assert!(tracker.mouse_tracking_enabled());
    }

    #[test]
    fn sequence_split_across_chunks_is_carried() {
        let tracker = RemoteTerminalMouseModeTracker::new();
        tracker.feed(b"prefix\x1B[?10");
        assert!(!tracker.mouse_tracking_enabled());

        tracker.feed(b"02h suffix");
        assert!(tracker.mouse_tracking_enabled());
    }

    #[test]
    fn bare_escape_split_across_chunks_is_carried() {
        let tracker = RemoteTerminalMouseModeTracker::new();
        tracker.feed(b"\x1B");
        tracker.feed(b"[?1000h");
        assert!(tracker.mouse_tracking_enabled());
    }

    #[test]
    fn terminal_reset_clears_modes() {
        let tracker = RemoteTerminalMouseModeTracker::new();
        tracker.feed(b"\x1B[?1002h\x1B[?1049h");
        tracker.feed(b"\x1Bc");
        assert!(!tracker.mouse_tracking_enabled());
        assert!(!tracker.alternate_screen_enabled());
        assert!(tracker.saw_mouse_or_alternate_disable());
    }

    #[test]
    fn reset_clears_carried_prefix() {
        let tracker = RemoteTerminalMouseModeTracker::new();
        tracker.feed(b"\x1B[?10");
        tracker.reset();
        tracker.feed(b"02h");
        assert!(!tracker.mouse_tracking_enabled());
    }

    #[test]
    fn unterminated_oversized_sequence_is_dropped_entirely() {
        let tracker = RemoteTerminalMouseModeTracker::new();
        // A pathological "CSI" that never terminates: overflow must drop the
        // pending buffer entirely (truncating it could bisect the sequence
        // into bytes that parse as something else).
        let mut junk = b"\x1B[?".to_vec();
        junk.extend(std::iter::repeat_n(b'1', 4096));
        tracker.feed(&junk);

        // A follow-up final byte must not combine with the dropped prefix.
        tracker.feed(b"h");
        assert!(!tracker.mouse_tracking_enabled());

        // And the tracker keeps working for later well-formed sequences.
        tracker.feed(b"\x1B[?1002h");
        assert!(tracker.mouse_tracking_enabled());
    }

    #[test]
    fn non_private_sequences_are_ignored() {
        let tracker = RemoteTerminalMouseModeTracker::new();
        tracker.feed(b"\x1B[1002h\x1B[2J\x1B[H");
        assert!(!tracker.mouse_tracking_enabled());
        assert!(!tracker.alternate_screen_enabled());
    }

    #[test]
    fn host_mode_snapshot_flag_is_explicit_and_cleared_by_reset() {
        let tracker = RemoteTerminalMouseModeTracker::new();
        assert!(!tracker.has_host_mode_snapshot());
        tracker.feed(b"\x1B[?1049h");
        assert!(
            !tracker.has_host_mode_snapshot(),
            "bytes alone are not a snapshot"
        );
        tracker.mark_host_mode_snapshot();
        assert!(tracker.has_host_mode_snapshot());
        tracker.reset();
        assert!(!tracker.has_host_mode_snapshot());
    }

    // --- RemoteMacClientError ---

    #[test]
    fn error_description_carries_status_and_server_message() {
        let error = RemoteMacClientError::new(404, Some("unknown session".to_string()));
        assert_eq!(error.to_string(), "HTTP 404: unknown session");
    }

    #[test]
    fn error_description_without_server_message() {
        let error = RemoteMacClientError::new(500, None);
        assert_eq!(error.to_string(), "HTTP 500");
    }
}
