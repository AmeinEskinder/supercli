//! Session and project organization sheets — port of the iOS client's
//! `SessionOrganizeSheet`, `ProjectOrganizeSheet`, and
//! `ArchivedSessionsSheet`.
//!
//! The iOS sheets save through `POST /mobile/session-organization` and
//! `POST /mobile/project-organization`, which land in the Host's shared
//! organization state; the refreshed bootstrap snapshot is what updates
//! the UI. I/O stays in the app shell per the crate convention: the
//! sheets emit patches and action verbs through `EventHandler`s, and the
//! launcher performs the client calls on a background thread and then
//! re-bootstraps.
//!
//! Resume affordances are deliberately independent of the UI (as in the
//! Swift source) so the capability gating is unit-testable.

use crate::i18n::t;
use dioxus::prelude::*;
use unpeel_client::dto::{ProjectSummary, SessionStatus, SessionSummary};
use unpeel_client::protocol::{capabilities, HostProtocolDescriptor};

/// Which resume affordance the organize sheet offers for a session.
///
/// Port of Swift's `SessionOrganizeResumePresentation`. Verb support is
/// computed from the Host-advertised per-session capabilities — the sheet
/// offers exactly what the desktop context menu offers for the same
/// session. Missing capability data fails closed so a Controller never
/// promises a resume the Host cannot do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionOrganizeResumePresentation {
    None,
    ResumeAgent,
    ResumeSession,
    Restore,
    RestoreAndResume,
}

/// Port of Swift's `sessionOrganizeResumePresentation(session:hostProtocol:)`.
pub fn session_organize_resume_presentation(
    session: &SessionSummary,
    host_protocol: Option<&HostProtocolDescriptor>,
) -> SessionOrganizeResumePresentation {
    // The Rust SessionStatus collapses every non-"running" wire value into
    // `Other`; the Host only ever emits "running"/"exited", so `Other` is
    // the exited case the Swift `.exited` check means.
    let exited = !matches!(session.status, SessionStatus::Running);
    if session.archived {
        return if exited && session.capabilities.restart {
            SessionOrganizeResumePresentation::RestoreAndResume
        } else {
            SessionOrganizeResumePresentation::Restore
        };
    }
    if matches!(session.status, SessionStatus::Running) {
        let resume_agent = session.active_runtime_id.is_none()
            && !session.runtime_launch_pending
            && session.capabilities.resume_agent
            && host_protocol.map(|p| p.is_compatible()).unwrap_or(false)
            && host_protocol
                .map(|p| p.supports(capabilities::SESSION_RUNTIME_RESUME))
                .unwrap_or(false);
        return if resume_agent {
            SessionOrganizeResumePresentation::ResumeAgent
        } else {
            SessionOrganizeResumePresentation::None
        };
    }
    if session.capabilities.restart {
        SessionOrganizeResumePresentation::ResumeSession
    } else {
        SessionOrganizeResumePresentation::None
    }
}

/// Destructive/session verbs the organize sheet can fire. Port of Swift's
/// `SessionSheetAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionSheetAction {
    Restart,
    ResumeAgent,
    Stop,
    Archive,
    Restore,
    Remove,
}

impl SessionSheetAction {
    /// The `/mobile/session-action` verb, or `None` for archive/restore,
    /// which travel on the organization patch instead — mirroring Swift's
    /// `remoteAction`.
    pub fn remote_verb(self) -> Option<&'static str> {
        match self {
            SessionSheetAction::Restart => Some("restart"),
            SessionSheetAction::ResumeAgent => Some("resume_agent"),
            SessionSheetAction::Stop => Some("stop"),
            SessionSheetAction::Archive | SessionSheetAction::Restore => None,
            SessionSheetAction::Remove => Some("remove"),
        }
    }

    /// Confirmation prompt title, mirroring Swift's `confirmationTitle`.
    /// Restore is non-destructive and never confirms.
    pub fn confirmation_title(self, session_running: bool) -> Option<String> {
        match self {
            SessionSheetAction::Restart => Some({ t("organize.resume_session") }),
            SessionSheetAction::ResumeAgent => Some({ t("organize.resume_agent") }),
            SessionSheetAction::Stop => Some({ t("organize.stop_session") }),
            SessionSheetAction::Archive => Some({ t("organize.stop_and_archive_session") }),
            SessionSheetAction::Restore => None,
            SessionSheetAction::Remove => Some(if session_running {
                {
                    t("organize.remove_session")
                }
            } else {
                {
                    t("organize.remove_from_list")
                }
            }),
        }
    }
}

/// Patch emitted by [`SessionOrganizeSheet`] on save. `None` fields are
/// left untouched on the Host. An unchanged (or emptied) title stays
/// `None` so the Host keeps its current label — mirroring Swift's
/// `titlePatch`. A `Some` project id files the session under that project
/// via the portable move verb.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionOrganizePatch {
    pub title: Option<String>,
    pub pinned: Option<bool>,
    pub notify_when_done: Option<bool>,
    pub project_id: Option<String>,
}

/// Session editor: rename, pin, notify-when-done, archive/restore, and the
/// resume/remove verbs. Opened from the session list; the shell performs
/// the save via `update_session_organization_full` / `session_action`
/// and re-bootstraps.
#[component]
pub fn SessionOrganizeSheet(
    session: SessionSummary,
    host_protocol: Option<HostProtocolDescriptor>,
    /// All projects from the Host's bootstrap; the {t("organize.move_to")} filing
    /// destinations are derived from these.
    projects: Vec<ProjectSummary>,
    on_save: EventHandler<SessionOrganizePatch>,
    on_action: EventHandler<SessionSheetAction>,
    on_open_archive: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let mut title = use_signal(|| session.title.clone());
    let mut pinned = use_signal(|| session.pinned);
    let mut notify = use_signal(|| session.notify_when_done);
    let mut move_target = use_signal(|| None::<String>);
    let mut confirming = use_signal(|| None::<SessionSheetAction>);
    let running = matches!(session.status, SessionStatus::Running);
    // Clone the compared-against values up front: the save closure is
    // `move` and must not move fields out of `session`, which the sheet
    // keeps reading below.
    let orig_title = session.title.clone();
    let orig_pinned = session.pinned;
    let orig_notify = session.notify_when_done;
    let archived = session.archived;

    let presentation = session_organize_resume_presentation(&session, host_protocol.as_ref());
    let can_notify = session.capabilities.notify_when_done;
    // Archive needs the Host to understand `archived` on the organization
    // patch — the fallback is restrictive (hide the verb), mirroring Swift.
    let can_archive = session.capabilities.archive;

    let can_move = unpeel_client::protocol::supports_session_project_move(host_protocol.as_ref());
    let destinations =
        unpeel_client::dto::move_destinations(&session.project_id, &session.project_id, &projects);

    let do_save = move |_| {
        let trimmed = title.read().trim().to_string();
        let title_patch = if trimmed.is_empty() || trimmed == orig_title {
            None
        } else {
            Some(trimmed)
        };
        let pinned_patch = if *pinned.read() == orig_pinned {
            None
        } else {
            Some(*pinned.read())
        };
        let notify_patch = if *notify.read() == orig_notify {
            None
        } else {
            Some(*notify.read())
        };
        let project_patch = move_target.read().clone();
        if title_patch.is_none()
            && pinned_patch.is_none()
            && notify_patch.is_none()
            && project_patch.is_none()
        {
            on_close.call(());
            return;
        }
        on_save.call(SessionOrganizePatch {
            title: title_patch,
            pinned: pinned_patch,
            notify_when_done: notify_patch,
            project_id: project_patch,
        });
    };

    let mut fire_action = move |action: SessionSheetAction| {
        if action.confirmation_title(running).is_some() && *confirming.read() != Some(action) {
            confirming.set(Some(action));
        } else {
            confirming.set(None);
            on_action.call(action);
        }
    };

    rsx! {
        div { class: "sheet-backdrop", onclick: move |_| on_close.call(()),
            div { class: "sheet organize-sheet", onclick: move |e| e.stop_propagation(),
                div { class: "sheet-header",
                    span { class: "sheet-title", {t("organize.organize_session")} }
                    button { class: "sheet-close", onclick: move |_| on_close.call(()), {t("organize.done")} }
                }
                div { class: "sheet-body",
                    label { class: "field-label", {t("organize.session_name")} }
                    input {
                        class: "text-field",
                        value: "{title}",
                        oninput: move |e| title.set(e.value()),
                    }
                    label { class: "toggle-row",
                        input {
                            r#type: "checkbox",
                            checked: *pinned.read(),
                            onchange: move |e| pinned.set(e.checked()),
                        }
                        {t("organize.pinned")}
                    }
                    if can_notify {
                        label { class: "toggle-row",
                            input {
                                r#type: "checkbox",
                                checked: *notify.read(),
                                onchange: move |e| notify.set(e.checked()),
                            }
                            {t("organize.notify_when_done")}
                        }
                    }
                    button { class: "primary-button", onclick: do_save, {t("organize.save")} }
                    if can_move && !destinations.is_empty() {
                        label { class: "field-label", {t("organize.move_to_project")} }
                        select {
                            class: "text-field",
                            onchange: move |e| {
                                let v = e.value();
                                move_target.set(if v.is_empty() { None } else { Some(v) });
                            },
                            option { value: "", {t("organize.keep_current_location")} }
                            for dest in destinations.iter() {
                                option { value: "{dest.id}", "{dest.name}" }
                            }
                        }
                    }
                    button {
                        class: "action-button",
                        onclick: move |_| on_open_archive.call(()),
                        {t("organize.archive_library")}
                    }

                    div { class: "action-list",
                        match presentation {
                            SessionOrganizeResumePresentation::ResumeSession => rsx! {
                                button {
                                    class: "action-button",
                                    onclick: move |_| fire_action(SessionSheetAction::Restart),
                                    if *confirming.read() == Some(SessionSheetAction::Restart) {
                                        {t("organize.tap_again_to_confirm_resume")}
                                    } else {
                                        {t("organize.resume_session_2")}
                                    }
                                }
                            },
                            SessionOrganizeResumePresentation::ResumeAgent => rsx! {
                                button {
                                    class: "action-button",
                                    onclick: move |_| fire_action(SessionSheetAction::ResumeAgent),
                                    if *confirming.read() == Some(SessionSheetAction::ResumeAgent) {
                                        {t("organize.tap_again_to_confirm_resume_agent")}
                                    } else {
                                        {t("organize.resume_agent_2")}
                                    }
                                }
                            },
                            _ => rsx! {},
                        }
                        if running {
                            button {
                                class: "action-button destructive",
                                onclick: move |_| fire_action(SessionSheetAction::Stop),
                                if *confirming.read() == Some(SessionSheetAction::Stop) {
                                    {t("organize.tap_again_to_confirm_stop")}
                                } else {
                                    {t("organize.stop_session_2")}
                                }
                            }
                        }
                        if can_archive {
                            if archived {
                                button {
                                    class: "action-button",
                                    onclick: move |_| on_action.call(SessionSheetAction::Restore),
                                    {t("organize.restore_from_archive")}
                                }
                            } else {
                                button {
                                    class: "action-button",
                                    onclick: move |_| fire_action(SessionSheetAction::Archive),
                                    if *confirming.read() == Some(SessionSheetAction::Archive) {
                                        {t("organize.tap_again_to_confirm_archive")}
                                    } else {
                                        {t("organize.archive_session")}
                                    }
                                }
                            }
                        }
                        button {
                            class: "action-button destructive",
                            onclick: move |_| fire_action(SessionSheetAction::Remove),
                            if *confirming.read() == Some(SessionSheetAction::Remove) {
                                {t("organize.tap_again_to_confirm_remove")}
                            } else if running {
                                {t("organize.remove_session_2")}
                            } else {
                                {t("organize.remove_from_list_2")}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One archived session row's actions in [`ArchivedSessionsSheet`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveAction {
    pub session_id: String,
    /// `true` = restore + the restart path (continue the conversation).
    pub and_resume: bool,
}

/// A project's archive library: archived sessions newest-first with
/// Restore and Restore & Resume per row. Port of Swift's
/// `ArchivedSessionsSheet`. The shell loads rows via
/// `HostClient::archived_sessions` and performs restores.
#[component]
pub fn ArchivedSessionsSheet(
    project_name: String,
    sessions: Option<Vec<SessionSummary>>,
    load_error: Option<String>,
    on_restore: EventHandler<ArchiveAction>,
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "sheet-backdrop", onclick: move |_| on_close.call(()),
            div { class: "sheet archive-sheet", onclick: move |e| e.stop_propagation(),
                div { class: "sheet-header",
                    span { class: "sheet-title", "Archive — {project_name}" }
                    button { class: "sheet-close", onclick: move |_| on_close.call(()), {t("organize.done")} }
                }
                div { class: "sheet-body",
                    match sessions {
                        None => rsx! {
                            if let Some(err) = load_error {
                                div { class: "empty-state", "Could not load the archive: {err}" }
                            } else {
                                div { class: "loading-state", "Loading…" }
                            }
                        },
                        Some(list) => rsx! {
                            if list.is_empty() {
                                div { class: "empty-state", "No archived sessions." }
                            } else {
                                for s in list {
                                    {
                                        let id = s.id.clone();
                                        let resume_id = s.id.clone();
                                        let can_resume = s.capabilities.restart;
                                        rsx! {
                                            div { key: "{id}", class: "archive-row",
                                                span { class: "archive-title", "{s.title}" }
                                                button {
                                                    class: "action-button",
                                                    onclick: move |_| on_restore.call(ArchiveAction {
                                                        session_id: id.clone(),
                                                        and_resume: false,
                                                    }),
                                                    {t("organize.restore")}
                                                }
                                                if can_resume {
                                                    button {
                                                        class: "action-button",
                                                        onclick: move |_| on_restore.call(ArchiveAction {
                                                            session_id: resume_id.clone(),
                                                            and_resume: true,
                                                        }),
                                                        {t("organize.restore_resume")}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

/// Folder color ids and order, identical to the desktop's
/// `ProjectFolderColor` and the iOS sheet's palette.
pub const FOLDER_COLORS: [&str; 8] = [
    "sky", "blue", "violet", "rose", "amber", "moss", "teal", "graphite",
];

/// The project or group the [`ProjectOrganizeSheet`] edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectOrganizeTarget {
    pub id: String,
    pub name: String,
    /// Groups can be renamed/pinned/sorted; main projects can be colored.
    pub is_group: bool,
    pub date_sorted: bool,
    pub color_id: Option<String>,
}

/// Patch emitted by [`ProjectOrganizeSheet`]; `None` fields are untouched.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectOrganizePatch {
    pub display_name: Option<String>,
    pub color_id: Option<String>,
    pub date_sorted: Option<bool>,
    pub pinned: Option<bool>,
}

/// Folder organize sheet: rename (groups only), per-group session sort,
/// folder color (main projects only), and pin. Port of Swift's
/// `ProjectOrganizeSheet`; saves through `POST /mobile/project-organization`
/// (capability `project.organization.set`).
#[component]
pub fn ProjectOrganizeSheet(
    target: ProjectOrganizeTarget,
    on_save: EventHandler<ProjectOrganizePatch>,
    on_open_archive: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let mut name = use_signal(|| target.name.clone());
    let mut date_sorted = use_signal(|| target.date_sorted);
    let mut color = use_signal(|| target.color_id.clone().unwrap_or_default());
    // Clone the compared-against values up front: the save closure is
    // `move` and must not move fields out of `target`, which the sheet
    // keeps reading below.
    let target_name = target.name.clone();
    let target_is_group = target.is_group;
    let target_date_sorted = target.date_sorted;
    let target_color = target.color_id.clone().unwrap_or_default();
    let sheet_title = target.name.clone();

    let do_save = move |_| {
        let trimmed = name.read().trim().to_string();
        let display_name = if target_is_group && !trimmed.is_empty() && trimmed != target_name {
            Some(trimmed)
        } else {
            None
        };
        let color_id = if !target_is_group {
            let c = color.read().clone();
            if c != target_color {
                Some(c)
            } else {
                None
            }
        } else {
            None
        };
        let date_sorted_patch = if target_is_group && *date_sorted.read() != target_date_sorted {
            Some(*date_sorted.read())
        } else {
            None
        };
        on_save.call(ProjectOrganizePatch {
            display_name,
            color_id,
            date_sorted: date_sorted_patch,
            pinned: None,
        });
    };

    rsx! {
        div { class: "sheet-backdrop", onclick: move |_| on_close.call(()),
            div { class: "sheet organize-sheet", onclick: move |e| e.stop_propagation(),
                div { class: "sheet-header",
                    span { class: "sheet-title", "Organize {sheet_title}" }
                    button { class: "sheet-close", onclick: move |_| on_close.call(()), {t("organize.done")} }
                }
                div { class: "sheet-body",
                    if target_is_group {
                        label { class: "field-label", {t("organize.group_name")} }
                        input {
                            class: "text-field",
                            value: "{name}",
                            oninput: move |e| name.set(e.value()),
                        }
                        label { class: "toggle-row",
                            input {
                                r#type: "checkbox",
                                checked: *date_sorted.read(),
                                onchange: move |e| date_sorted.set(e.checked()),
                            }
                            {t("organize.sort_sessions_by_date")}
                        }
                    } else {
                        label { class: "field-label", {t("organize.folder_color")} }
                        div { class: "color-swatches",
                            for c in FOLDER_COLORS {
                                {
                                    let selected = *color.read() == c;
                                    rsx! {
                                        button {
                                            key: "{c}",
                                            class: if selected { "color-swatch selected" } else { "color-swatch" },
                                            onclick: move |_| color.set(c.to_string()),
                                            "{c}"
                                        }
                                    }
                                }
                            }
                            button {
                                class: if color.read().is_empty() { "color-swatch selected" } else { "color-swatch" },
                                onclick: move |_| color.set(String::new()),
                                {t("organize.none")}
                            }
                        }
                    }
                    button { class: "primary-button", onclick: do_save, {t("organize.save")} }
                    button {
                        class: "action-button",
                        onclick: move |_| on_open_archive.call(()),
                        {t("organize.archive_library")}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(archived: bool, running: bool, restart: bool, resume_agent: bool) -> SessionSummary {
        SessionSummary {
            id: "s".to_string(),
            project_id: String::new(),
            active_runtime_id: None,
            runtime_launch_pending: false,
            provider_id: None,
            title: String::new(),
            command: String::new(),
            created_at_unix_ms: 0,
            updated_at_unix_ms: None,
            status: if running {
                SessionStatus::Running
            } else {
                SessionStatus::Other
            },
            activity: unpeel_client::dto::ActivityState::Idle,
            activity_source: None,
            unread: false,
            pinned: false,
            worktree_path: None,
            worktree_branch: None,
            parent_session_id: None,
            last_output_preview: None,
            notify_when_done: false,
            terminal_background_hex: None,
            archived,
            capabilities: unpeel_client::dto::SessionCapabilities {
                restart,
                resume_agent,
                archive: false,
                notify_when_done: false,
            },
        }
    }

    fn proto() -> HostProtocolDescriptor {
        HostProtocolDescriptor {
            major_version: unpeel_client::protocol::PROTOCOL_MAJOR,
            minor_version: 0,
            capabilities: vec![unpeel_client::protocol::Capability::Id(
                capabilities::SESSION_RUNTIME_RESUME.to_string(),
            )],
        }
    }

    #[test]
    fn archived_exited_with_restart_offers_restore_and_resume() {
        let s = summary(true, false, true, false);
        assert_eq!(
            session_organize_resume_presentation(&s, None),
            SessionOrganizeResumePresentation::RestoreAndResume
        );
    }

    #[test]
    fn archived_without_restart_offers_plain_restore() {
        let s = summary(true, false, false, false);
        assert_eq!(
            session_organize_resume_presentation(&s, None),
            SessionOrganizeResumePresentation::Restore
        );
    }

    #[test]
    fn running_with_resume_agent_capability_offers_resume_agent() {
        let s = summary(false, true, false, true);
        assert_eq!(
            session_organize_resume_presentation(&s, Some(&proto())),
            SessionOrganizeResumePresentation::ResumeAgent
        );
    }

    #[test]
    fn running_resume_agent_gated_on_protocol_compatibility() {
        let s = summary(false, true, false, true);
        // No protocol descriptor: fails closed, like Swift's optional chain.
        assert_eq!(
            session_organize_resume_presentation(&s, None),
            SessionOrganizeResumePresentation::None
        );
        let mut bad = proto();
        bad.major_version += 1;
        assert_eq!(
            session_organize_resume_presentation(&s, Some(&bad)),
            SessionOrganizeResumePresentation::None
        );
    }

    #[test]
    fn exited_with_restart_offers_resume_session() {
        let s = summary(false, false, true, false);
        assert_eq!(
            session_organize_resume_presentation(&s, None),
            SessionOrganizeResumePresentation::ResumeSession
        );
    }

    #[test]
    fn exited_without_capabilities_offers_nothing() {
        let s = summary(false, false, false, false);
        assert_eq!(
            session_organize_resume_presentation(&s, None),
            SessionOrganizeResumePresentation::None
        );
    }

    #[test]
    fn archive_and_restore_ride_the_patch_not_verbs() {
        assert_eq!(SessionSheetAction::Archive.remote_verb(), None);
        assert_eq!(SessionSheetAction::Restore.remote_verb(), None);
        assert_eq!(SessionSheetAction::Restart.remote_verb(), Some("restart"));
        assert_eq!(
            SessionSheetAction::ResumeAgent.remote_verb(),
            Some("resume_agent")
        );
        assert_eq!(SessionSheetAction::Remove.remote_verb(), Some("remove"));
    }

    #[test]
    fn restore_never_confirms() {
        assert_eq!(SessionSheetAction::Restore.confirmation_title(false), None);
        assert!(SessionSheetAction::Remove
            .confirmation_title(true)
            .is_some());
    }
}
