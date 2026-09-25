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
        m.insert("approval.already_approved", "Already approved");
        m.insert("approval.already_denied", "Already denied");
        m.insert("approval.resolved_unknown", "Resolved — see activity log");
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
        m.insert("activity.needs_approval", "Needs approval");
        m.insert("annotation.cancel", "Cancel");
        m.insert("annotation.clear", "Clear");
        m.insert("annotation.color", "Color");
        m.insert("annotation.done", "Done");
        m.insert("annotation.reset", "Reset");
        m.insert("annotation.swatch_selected", "swatch selected");
        m.insert("annotation.undo", "Undo");
        m.insert("app_lock.authentication_failed", "Authentication failed");
        m.insert(
            "app_lock.authentication_was_cancelled",
            "Authentication was cancelled",
        );
        m.insert(
            "app_lock.authentication_was_interrupted",
            "Authentication was interrupted",
        );
        m.insert(
            "app_lock.biometric_unlock_is_not_available_in_thi",
            "Biometric unlock is not available in this build",
        );
        m.insert("app_lock.face_id", "Face ID");
        m.insert("app_lock.optic_id", "Optic ID");
        m.insert("app_lock.passcode", "Passcode");
        m.insert("app_lock.touch_id", "Touch ID");
        m.insert("app_lock.unlock_unpeel", "Unlock Unpeel");
        m.insert("app_lock.unpeel_is_locked", "Unpeel is locked");
        m.insert("clickable_path.home", "HOME");
        m.insert("command_palette.command", "Command");
        m.insert("command_palette.launch", "Launch");
        m.insert("command_palette.project", "Project");
        m.insert("command_palette.session", "Session");
        m.insert(
            "command_palette.type_a_command_or_search_sessions",
            "Type a command or search sessions…",
        );
        m.insert("components.archive", "Archive");
        m.insert("components.bubble_mine", "bubble mine");
        m.insert("components.bubble_theirs", "bubble theirs");
        m.insert("components.dot_bad", "dot bad");
        m.insert("components.dot_ok", "dot ok");
        m.insert("components.hide_archived", "Hide archived");
        m.insert("components.no_host", "No host");
        m.insert("components.refresh", "Refresh");
        m.insert("components.restore", "Restore");
        m.insert("composer.cancel", "Cancel");
        m.insert("composer.edit", "Edit");
        m.insert("composer.message_the_agent", "Message the agent");
        m.insert(
            "composer.message_the_agent_enter_to_send",
            "Message the agent… (Enter to send)",
        );
        m.insert("composer.queue", "Queue");
        m.insert("composer.remove", "Remove");
        m.insert("composer.save", "Save");
        m.insert("composer.send", "Send");
        m.insert("composer.stop", "Stop");
        m.insert("dictation.dictate", "Dictate");
        m.insert(
            "dictation.dictation_was_interrupted",
            "Dictation was interrupted",
        );
        m.insert("dictation.dismiss_dictation", "Dismiss dictation");
        m.insert(
            "dictation.microphone_unavailable_check_permission",
            "Microphone unavailable — check permission",
        );
        m.insert("dictation.paste", "Paste");
        m.insert(
            "dictation.speech_recognition_isn_t_available_here",
            "Speech recognition isn't available here",
        );
        m.insert(
            "dictation.speech_recognizer_failed",
            "Speech recognizer failed",
        );
        m.insert("dictation.start_dictation", "Start dictation");
        m.insert("dictation.stop", "Stop");
        m.insert("dictation.stop_dictation", "Stop dictation");
        m.insert("discovery.close", "Close");
        m.insert("discovery.discovery_timed_out", "discovery timed out");
        m.insert("discovery.nearby_hosts", "Nearby Hosts");
        m.insert(
            "discovery.searching_the_local_network",
            "Searching the local network…",
        );
        m.insert("discovery.unpeel_host", "Unpeel Host");
        m.insert("find.close_find", "Close find");
        m.insert("find.find", "Find");
        m.insert("find.find_in_terminal", "Find in terminal");
        m.insert("find.next_match", "Next match");
        m.insert("find.no_results", "No results");
        m.insert("find.previous_match", "Previous match");
        m.insert("gallery.add_to_message", "Add to message");
        m.insert("gallery.arrows", "Arrows");
        m.insert(
            "gallery.ask_the_host_to_capture_a_screenshot_int",
            "Ask the Host to capture a screenshot into this gallery",
        );
        m.insert("gallery.browser_gallery", "Browser Gallery");
        m.insert("gallery.confirm_delete", "Confirm delete");
        m.insert("gallery.crop", "Crop");
        m.insert("gallery.delete", "Delete");
        m.insert("gallery.draw", "Draw");
        m.insert("gallery.gallery", "‹ Gallery");
        m.insert("gallery.refresh", "Refresh");
        m.insert("gallery.screenshot", "Screenshot");
        m.insert("gallery.share", "Share");
        m.insert("gallery.upload", "Upload");
        m.insert("notifier.needs_your_input", "Needs your input");
        m.insert("notifier.notification", "Notification");
        m.insert("organize.archive_library", "Archive library");
        m.insert("organize.archive_session", "Archive session");
        m.insert("organize.done", "Done");
        m.insert("organize.folder_color", "Folder color");
        m.insert("organize.group_name", "Group name");
        m.insert("organize.keep_current_location", "Keep current location");
        m.insert("organize.move_to_project", "Move to project");
        m.insert("organize.none", "None");
        m.insert("organize.notify_when_done", "Notify when done");
        m.insert("organize.organize_session", "Organize session");
        m.insert("organize.pinned", "Pinned");
        m.insert("organize.remove_from_list", "Remove From List?");
        m.insert("organize.remove_from_list_2", "Remove from list");
        m.insert("organize.remove_session", "Remove Session?");
        m.insert("organize.remove_session_2", "Remove session");
        m.insert("organize.restore", "Restore");
        m.insert("organize.restore_from_archive", "Restore from archive");
        m.insert("organize.restore_resume", "Restore & Resume");
        m.insert("organize.resume_agent", "Resume Agent?");
        m.insert("organize.resume_agent_2", "Resume agent");
        m.insert("organize.resume_session", "Resume Session?");
        m.insert("organize.resume_session_2", "Resume session");
        m.insert("organize.save", "Save");
        m.insert("organize.session_name", "Session name");
        m.insert("organize.sort_sessions_by_date", "Sort sessions by date");
        m.insert(
            "organize.stop_and_archive_session",
            "Stop and Archive Session?",
        );
        m.insert("organize.stop_session", "Stop Session?");
        m.insert("organize.stop_session_2", "Stop session");
        m.insert(
            "organize.tap_again_to_confirm_archive",
            "Tap again to confirm archive",
        );
        m.insert(
            "organize.tap_again_to_confirm_remove",
            "Tap again to confirm remove",
        );
        m.insert(
            "organize.tap_again_to_confirm_resume",
            "Tap again to confirm resume",
        );
        m.insert(
            "organize.tap_again_to_confirm_resume_agent",
            "Tap again to confirm resume agent",
        );
        m.insert(
            "organize.tap_again_to_confirm_stop",
            "Tap again to confirm stop",
        );
        m.insert("pairing.connect", "Connect");
        m.insert("pairing.forget", "Forget");
        m.insert("pairing.pair", "Pair");
        m.insert(
            "pairing.pair_with_an_unpeel_host",
            "Pair with an Unpeel Host",
        );
        m.insert(
            "pairing.pairing_code_from_the_host",
            "Pairing code from the Host",
        );
        m.insert("pairing.previously_paired", "Previously paired");
        m.insert("pairing.switch", "Switch");
        m.insert("pairing.viewing", "● viewing");
        m.insert("pairing.viewing_2", "Viewing");
        m.insert("presence.remote_viewer", "Remote viewer");
        m.insert("presets.close_presets", "Close presets");
        m.insert("presets.new_session", "New session");
        m.insert("push.not_requested", "Not requested");
        m.insert(
            "push.notifications_are_denied_in_system_setti",
            "Notifications are denied in system Settings",
        );
        m.insert("push.notifications_are_off", "Notifications are off");
        m.insert(
            "push.notifications_aren_t_working",
            "Notifications aren't working",
        );
        m.insert("push.ready_production", "Ready (production)");
        m.insert("push.ready_sandbox", "Ready (sandbox)");
        m.insert(
            "push.waiting_for_a_push_device_token",
            "Waiting for a push device token…",
        );
        m.insert(
            "push.waiting_for_notification_permission",
            "Waiting for notification permission…",
        );
        m.insert("qr.camera_access_denied", "Camera access denied");
        m.insert("qr.camera_unavailable", "camera unavailable");
        m.insert(
            "qr.point_the_camera_at_the_pairing_qr_code",
            "Point the camera at the pairing QR code",
        );
        m.insert("qr.requesting_camera_access", "Requesting camera access…");
        m.insert("qr.scanner_paused", "Scanner paused");
        m.insert("settings.agents", "Agents");
        m.insert("settings.allow_agent_mcp_access", "Allow agent MCP access");
        m.insert("settings.allow_browser_use", "Allow browser use");
        m.insert("settings.ask_before_writes", "Ask before writes");
        m.insert(
            "settings.ask_before_writing_to_another_session",
            "Ask before writing to another session",
        );
        m.insert("settings.browser", "Browser");
        m.insert("settings.browser_use", "Browser use");
        m.insert("settings.developer", "Developer");
        m.insert("settings.experimental", "Experimental");
        m.insert("settings.features", "Features");
        m.insert("settings.git_worktrees", "Git worktrees");
        m.insert("settings.persist_logins", "Persist logins");
        m.insert("settings.plugins", "Plugins");
        m.insert("settings.read_other_sessions", "Read other sessions");
        m.insert("settings.remote_workspaces", "Remote workspaces");
        m.insert("settings.sessions", "Sessions");
        m.insert("settings.sessions_use", "Sessions use");
        m.insert("settings.workspaces", "Workspaces");
        m.insert("settings.worktrees", "Worktrees");
        m.insert("ssh.forget", "Forget");
        m.insert("ssh.interactive_shell", "Interactive shell");
        m.insert("ssh.standard_ssh", "Standard SSH");
        m.insert("terminal.close", "Close");
        m.insert("terminal.copy_all", "Copy All");
        m.insert("terminal.select", "Select");
        m.insert("terminal.select_text", "Select text");
        m.insert("workspaces.alacritty", "Alacritty");
        m.insert("workspaces.cursor", "Cursor");
        m.insert("workspaces.finder", "Finder");
        m.insert("workspaces.fork", "Fork");
        m.insert("workspaces.ghostty", "Ghostty");
        m.insert("workspaces.iterm2", "iTerm2");
        m.insert("workspaces.kitty", "kitty");
        m.insert("workspaces.git_worktrees", "Git worktrees");
        m.insert("workspaces.github_desktop", "GitHub Desktop");
        m.insert("workspaces.gitkraken", "GitKraken");
        m.insert("workspaces.hyper", "Hyper");
        m.insert("workspaces.intellij", "IntelliJ");
        m.insert("workspaces.path", "PATH");
        m.insert("workspaces.remove", "Remove");
        m.insert(
            "workspaces.require_a_clean_tree_before_creating_a_w",
            "Require a clean tree before creating a worktree",
        );
        m.insert("workspaces.rio", "Rio");
        m.insert("workspaces.sourcetree", "Sourcetree");
        m.insert("workspaces.sublime_merge", "Sublime Merge");
        m.insert("workspaces.tabby", "Tabby");
        m.insert("workspaces.terminal", "Terminal");
        m.insert("workspaces.tower", "Tower");
        m.insert("workspaces.uncommitted_changes", "uncommitted changes");
        m.insert("workspaces.vs_code", "VS Code");
        m.insert("workspaces.warp", "Warp");
        m.insert("workspaces.wave", "Wave");
        m.insert("workspaces.webstorm", "WebStorm");
        m.insert("workspaces.wezterm", "WezTerm");
        m.insert("workspaces.xcode", "Xcode");
        m.insert("workspaces.zed", "Zed");
        m.insert("components.approval_requested", "Approval requested");
        m.insert("composer.send", "Send");
        m.insert("composer.stop", "Stop");
        m.insert("dictation.and", " and ");
        m.insert("organize.move_to", "Move to");
        m.insert("pairing.switch", "Switch");
        m.insert("pairing.connect", "Connect");
        m.insert("presence.name_id", "Name (id)");
        m.insert("presence.name_connected", "<name> connected");
        m.insert("presence.no_viewers", "no viewers");
        m.insert("presets.new_session", "New session");
        m.insert(
            "terminal.inherit_terminal_default",
            "inherit the terminal default",
        );
        m.insert("terminal.copy_all", "Copy All");
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
