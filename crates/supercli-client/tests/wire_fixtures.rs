//! Wire-format drift guard.
//!
//! `protocol/fixtures/*.json` are canonical wire examples for every message
//! struct in [`supercli_client::dto`]. The *values* in each fixture come from
//! the Swift XCTest expectations in
//! `clients/legacy/shared/SupercliShared/Tests/SupercliSharedTests/RemoteControlProtocolTests.swift`
//! (each builder notes its source test); the *bytes* are the Rust
//! serializer's canonical form (compact JSON, fields in declaration order).
//!
//! - `wire_fixtures_decode_and_reencode_byte_stable` (runs in CI): every
//!   fixture must decode into its DTO and re-encode to byte-identical JSON.
//!   Any Rust-side wire change — renamed field, new enum spelling, added or
//!   removed field — breaks this test instead of silently breaking the app
//!   or the Dart client at runtime.
//! - `regenerate_wire_fixtures` (ignored): rewrites the fixtures from the
//!   current DTO definitions. Run `cargo test -p supercli-client --test
//!   wire_fixtures -- --ignored` after an *intentional* wire change, then
//!   review the `protocol/fixtures/` diff before committing.
//!
//! The Dart side (`clients/supercli-app/test/wire_fixtures_test.dart`)
//! decodes the same files through `host_models.dart`, so a wire change that
//! Rust accepts but Dart cannot also breaks CI.

use std::path::PathBuf;
use supercli_client::dto::*;
use supercli_client::protocol::{Capability, HostProtocolDescriptor};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../protocol/fixtures")
}

// ---------------------------------------------------------------------------
// Builders: representative DTO values taken from the Swift XCTest expectations.
// ---------------------------------------------------------------------------

/// Swift: `testSessionSummaryRoundTripsCapabilities` (restart/resumeAgent).
fn b_session_capabilities() -> SessionCapabilities {
    SessionCapabilities {
        restart: true,
        resume_agent: true,
        archive: false,
        notify_when_done: false,
    }
}

/// Swift: `testSessionSummaryCarriesRemoteControllerState`.
fn b_session_summary() -> SessionSummary {
    SessionSummary {
        id: "session-1".into(),
        project_id: "project-1".into(),
        active_runtime_id: None,
        runtime_launch_pending: false,
        provider_id: Some("codex".into()),
        title: "iOS remote PRD".into(),
        command: "codex".into(),
        created_at_unix_ms: 1789996800000,
        updated_at_unix_ms: Some(1789996860000),
        status: SessionStatus::Running,
        activity: ActivityState::Blocked,
        activity_source: Some(ActivitySource::Screen),
        unread: true,
        pinned: true,
        worktree_path: None,
        worktree_branch: None,
        parent_session_id: None,
        last_output_preview: Some("Permission required".into()),
        notify_when_done: false,
        terminal_background_hex: None,
        archived: false,
        spinner_color_hex: None,
        latest_alert_body: None,
        latest_alert_at_unix_ms: None,
        capabilities: b_session_capabilities(),
    }
}

/// Swift: `testProjectGroupFieldsRoundTripAndRemainAdditive`.
/// Note: Swift and the Host both use `isGroup` for the group flag
/// (Host: sessions.rs sidebar; Swift: RemoteControlProtocol). The fixture
/// uses `isGroup` (byte-stability is defined against Rust's output).
fn b_project_summary() -> ProjectSummary {
    ProjectSummary {
        id: "group-research".into(),
        name: "Research".into(),
        path: "/dev/supercli".into(),
        parent_project_id: Some("project-supercli".into()),
        sort_order: None,
        is_group: Some(true),
        worktree_branch: None,
    }
}

/// Swift: `testTranscriptSnapshotRoundTripsSemanticBlocks` (entry-1).
fn b_transcript_block() -> TranscriptBlock {
    TranscriptBlock {
        kind: "text".into(),
        text: Some("hi".into()),
    }
}

/// Swift: `testTranscriptSnapshotRoundTripsSemanticBlocks` (entry-1).
fn b_transcript_entry() -> TranscriptEntry {
    TranscriptEntry {
        id: "entry-1".into(),
        role: TranscriptRole::User,
        blocks: vec![b_transcript_block()],
    }
}

/// Swift: `testTranscriptSnapshotRoundTripsSemanticBlocks`.
fn b_transcript_snapshot() -> TranscriptSnapshot {
    TranscriptSnapshot {
        entries: vec![
            b_transcript_entry(),
            TranscriptEntry {
                id: "entry-2".into(),
                role: TranscriptRole::Agent,
                blocks: vec![
                    TranscriptBlock {
                        kind: "reasoning".into(),
                        text: Some("Thinking through the request.".into()),
                    },
                    TranscriptBlock {
                        kind: "text".into(),
                        text: Some("Hi. What should we work on?".into()),
                    },
                ],
            },
        ],
        has_more: false,
    }
}

/// Swift: `testPendingApprovalPresentsWriteOnKnownTargetOtherwiseCaller`.
/// The Rust DTO carries the resolved target in `sessionID`.
fn b_pending_approval() -> PendingApproval {
    PendingApproval {
        id: "a1".into(),
        session_id: Some("target".into()),
        title: Some("Allow write?".into()),
        detail: Some("body".into()),
    }
}

/// Swift: `testAppSummaryDefaultsNewHandlerFieldsFromOlderHosts` shape, with
/// values from the Rust `preset_decodes_host_wire_shape` test.
fn b_preset_summary() -> PresetSummary {
    PresetSummary {
        id: "p1".into(),
        label: "Claude".into(),
        command: "claude".into(),
        plugin_id: None,
        project_id: Some("proj-1".into()),
        cli_id: None,
        enabled: true,
        quick_launch: false,
        is_default: false,
        tint_color_hex: None,
    }
}

/// Swift: `testBootstrapDecodesWithoutHostProtocolForLegacyHosts` shape,
/// extended with one row per collection (values from the Swift tests above).
fn b_bootstrap_snapshot() -> BootstrapSnapshot {
    BootstrapSnapshot {
        protocol_version: 1,
        host_protocol: Some(HostProtocolDescriptor {
            major_version: 1,
            minor_version: 0,
            capabilities: vec![
                Capability::Id("host.bootstrap".into()),
                Capability::Id("session.output.read".into()),
            ],
        }),
        host_id: Some("mac-1".into()),
        host_name: Some("Studio Mac".into()),
        folders: vec![],
        presets: vec![b_preset_summary()],
        sessions: vec![b_session_summary()],
        projects: vec![b_project_summary()],
        pending_approvals: vec![b_pending_approval()],
        captured_at_unix_ms: 42,
        remote_server_port: None,
        remote_server_certificate_fingerprint: None,
        pro_entitled: Some(true),
        workspace_settings: None,
        available_apps: None,
        installed_apps: None,
        openers: None,
        app_presentations: None,
        experimental_worktrees_enabled: None,
        host_tint_hue: None,
        host_device_kind: None,
        host_device_model: None,
    }
}

/// Swift: `testPairedDeviceSummaryRoundTripsLastSeen`.
fn b_paired_device_summary() -> PairedDeviceSummary {
    PairedDeviceSummary {
        id: "phone-1".into(),
        name: "iPhone".into(),
        platform: "iOS".into(),
        app_version: Some("1.0".into()),
        paired_at_unix_ms: 1789996800000,
        last_seen_at_unix_ms: Some(1789996860000),
        relay_allowed: None,
    }
}

/// Swift: `testBootstrapRoundTripsHostWorkspaceList` (first workspace).
fn b_workspace_summary() -> WorkspaceSummary {
    WorkspaceSummary {
        id: "local:/Users/t/.supercli".into(),
        name: "Personal".into(),
        tint_hue: None,
        is_current: true,
        is_running: true,
        kind: Some("local".into()),
    }
}

/// Swift: `testResumableArtifactUploadProgressRoundTrips` (partial upload).
fn b_artifact_upload_progress() -> ArtifactUploadProgress {
    ArtifactUploadProgress {
        upload_id: "upload-1".into(),
        session_id: "session-1".into(),
        file_name: "upload.jpg".into(),
        mime_type: Some("image/jpeg".into()),
        total_bytes: 300000,
        received_bytes: 262144,
        chunk_size: 16384,
        next_offset: 262144,
        complete: false,
        artifact_id: None,
        updated_at_unix_ms: 1789996800000,
    }
}

/// Swift: `testCreateSessionRequestRoundTripsInitialPrompt`.
fn b_create_session_request() -> CreateSessionRequest {
    CreateSessionRequest {
        project_id: "project-1".into(),
        preset_id: Some("claude".into()),
        command: None,
        worktree_path: Some("/tmp/supercli-worktree".into()),
        worktree_branch: Some("feature/ios-remote".into()),
        initial_text: Some("Review the iOS pairing flow.".into()),
        initial_text_submit_mode: TextSubmitMode::PasteAndSubmit,
    }
}

/// Swift: `testTerminalWriteResizeAndCreateResponseRoundTrip` (the `created`
/// response; its nested session mirrors that test's `RemoteSessionSummary`).
fn b_create_session_response() -> CreateSessionResponse {
    CreateSessionResponse {
        session_id: "session-new".into(),
        captured_at_unix_ms: Some(1789996800000),
        session: Some(SessionSummary {
            id: "session-new".into(),
            project_id: "project-1".into(),
            active_runtime_id: None,
            runtime_launch_pending: false,
            provider_id: Some("opencode".into()),
            title: "opencode".into(),
            command: "opencode".into(),
            created_at_unix_ms: 1789996800000,
            updated_at_unix_ms: None,
            status: SessionStatus::Running,
            activity: ActivityState::Starting,
            activity_source: None,
            unread: false,
            pinned: false,
            worktree_path: None,
            worktree_branch: None,
            parent_session_id: None,
            last_output_preview: None,
            notify_when_done: false,
            terminal_background_hex: None,
            archived: false,
            spinner_color_hex: None,
            latest_alert_body: None,
            latest_alert_at_unix_ms: None,
            capabilities: SessionCapabilities {
                restart: false,
                resume_agent: false,
                archive: false,
                notify_when_done: false,
            },
        }),
    }
}

/// Shape: `RemoteSessionTextInput` in `RemoteControlProtocol.swift`.
fn b_session_text_input() -> SessionTextInput {
    SessionTextInput {
        session_id: "session-1".into(),
        text: "hello".into(),
        submit_mode: TextSubmitMode::PasteAndSubmit,
    }
}

/// Swift: `testTerminalWriteResizeAndCreateResponseRoundTrip` (the `write`
/// request; Swift `writeID` maps to the idempotency key).
fn b_terminal_write_request() -> TerminalWriteRequest {
    TerminalWriteRequest {
        session_id: "session-1".into(),
        data: "\u{1b}[A".into(),
        idempotency_key: Some("write-123".into()),
    }
}

/// Swift: `testTerminalWriteResizeAndCreateResponseRoundTrip` (resize).
fn b_terminal_resize_request() -> TerminalResizeRequest {
    TerminalResizeRequest {
        session_id: "session-1".into(),
        columns: 120,
        rows: 42,
    }
}

/// Swift: `testViewportPatchRoundTripsChangedRuns` (`.ansi(2)` foreground).
fn b_terminal_color() -> TerminalColor {
    TerminalColor::ansi(2)
}

/// Swift: `testViewportFrameRoundTripsStyledCells` (`.init(bold: true)`).
fn b_terminal_style() -> TerminalStyle {
    TerminalStyle {
        bold: true,
        ..Default::default()
    }
}

/// Swift: `testViewportFrameRoundTripsStyledCells` (first cell).
fn b_terminal_cell() -> TerminalCell {
    TerminalCell {
        text: "A".into(),
        foreground: Some(TerminalColor::rgb(255, 255, 255)),
        background: Some(TerminalColor::ansi(4)),
        style: b_terminal_style(),
    }
}

/// Swift: `testViewportFrameRoundTripsStyledCells` (cursor).
fn b_terminal_cursor() -> TerminalCursor {
    TerminalCursor {
        row: 0,
        column: 1,
        shape: CursorShape::Beam,
        visible: true,
    }
}

/// Swift: `testViewportFrameRoundTripsStyledCells`.
fn b_viewport_frame() -> ViewportFrame {
    ViewportFrame {
        session_id: "session-1".into(),
        sequence: 42,
        rows: 1,
        columns: 2,
        cells: vec![
            b_terminal_cell(),
            TerminalCell {
                text: "界".into(),
                foreground: Some(TerminalColor::default_foreground()),
                background: None,
                style: TerminalStyle::default(),
            },
        ],
        cursor: Some(b_terminal_cursor()),
        alternate_screen: true,
        captured_at_unix_ms: 1789996800000,
    }
}

/// Shape: `RemoteViewportSubscription` in `RemoteControlProtocol.swift`.
fn b_viewport_subscription() -> ViewportSubscription {
    ViewportSubscription {
        session_id: "session-1".into(),
        rows: 24,
        columns: 80,
    }
}

/// Swift: `testViewportPatchRoundTripsChangedRuns` (the changed run).
fn b_terminal_cell_run() -> TerminalCellRun {
    TerminalCellRun {
        row: 12,
        column: 4,
        cells: vec![
            TerminalCell {
                text: "O".into(),
                foreground: Some(TerminalColor::ansi(2)),
                background: None,
                style: b_terminal_style(),
            },
            TerminalCell {
                text: "K".into(),
                foreground: Some(TerminalColor::ansi(2)),
                background: None,
                style: b_terminal_style(),
            },
        ],
    }
}

/// Swift: `testViewportPatchRoundTripsChangedRuns`.
fn b_viewport_patch() -> ViewportPatch {
    ViewportPatch {
        session_id: "session-1".into(),
        sequence: 43,
        base_sequence: 42,
        changed_runs: vec![b_terminal_cell_run()],
        cursor: Some(TerminalCursor {
            row: 12,
            column: 6,
            shape: CursorShape::Block,
            visible: true,
        }),
        captured_at_unix_ms: 1789996900000,
    }
}

/// Swift: `testStreamEventCanCarryEncodedViewportFrame`. The payload is opaque
/// bytes on the wire (base64 in JSON); here a small encoded frame.
fn b_stream_event() -> StreamEvent {
    StreamEvent {
        protocol_version: 1,
        id: "event-1".into(),
        request_id: None,
        kind: StreamEventKind::ViewportFrame,
        session_id: Some("session-1".into()),
        payload: Some(br#"{"sessionID":"session-1","sequence":42}"#.to_vec()),
        created_at_unix_ms: 1789996800000,
    }
}

/// Shape: `RemoteApprovalAnswerRequest` in `RemoteControlProtocol.swift`.
fn b_approval_answer_request() -> ApprovalAnswerRequest {
    ApprovalAnswerRequest {
        id: "a1".into(),
        approved: true,
    }
}

/// Shape: `RemoteMarkReadRequest` in `RemoteControlProtocol.swift`.
fn b_mark_read_request() -> MarkReadRequest {
    MarkReadRequest {
        session_id: "session-1".into(),
    }
}

/// Swift: `testSessionActionRequestRoundTrips` (`.remove`).
fn b_session_action_request() -> SessionActionRequest {
    SessionActionRequest {
        session_id: "session-1".into(),
        action: SessionAction::Remove,
    }
}

/// Swift: `testSessionOrganizationPatchRoundTripsPartialFields`.
fn b_session_organization_patch() -> SessionOrganizationPatch {
    SessionOrganizationPatch {
        session_id: "session-1".into(),
        title: Some("Renamed from phone".into()),
        pinned: Some(true),
        archived: None,
        notify_when_done: None,
        project_id: None,
    }
}

/// Swift: `testScreenshotRequestAndAcknowledgementRoundTrip` (request).
fn b_screenshot_request() -> ScreenshotRequest {
    ScreenshotRequest {
        session_id: "session-1".into(),
    }
}

/// Swift: `testScreenshotRequestAndAcknowledgementRoundTrip` (response).
fn b_screenshot_request_response() -> ScreenshotRequestResponse {
    ScreenshotRequestResponse {
        accepted: true,
        requested_at_unix_ms: 1789996800000,
    }
}

/// Values from the Rust `plugin_updates_round_trip` test (mirrors the Swift
/// plugin-updates wire shape).
fn b_plugin_update() -> PluginUpdate {
    PluginUpdate {
        id: "plugin-1".into(),
        state: "active".into(),
        installed_version: Some("1.0.0".into()),
        latest_version: Some("1.1.0".into()),
        update_available: true,
    }
}

fn b_plugin_updates() -> PluginUpdates {
    PluginUpdates {
        checking: false,
        items: vec![b_plugin_update()],
    }
}

/// Shape: `RemoteRestartSessionRequest` in `RemoteControlProtocol.swift`.
fn b_restart_session_request() -> RestartSessionRequest {
    RestartSessionRequest {
        session_id: "session-1".into(),
    }
}

/// Shape: `RemotePushTokenRegistration` in `RemoteControlProtocol.swift`.
fn b_push_token_registration() -> PushTokenRegistration {
    PushTokenRegistration {
        token: "push-token".into(),
        platform: "ios".into(),
        app_version: Some("1.0".into()),
    }
}

// ---------------------------------------------------------------------------
// Fixture table: file name <-> builder.
// ---------------------------------------------------------------------------

fn build_cases() -> Vec<(&'static str, String)> {
    vec![
        (
            "session_capabilities.json",
            serde_json::to_string(&b_session_capabilities()).unwrap(),
        ),
        (
            "session_summary.json",
            serde_json::to_string(&b_session_summary()).unwrap(),
        ),
        (
            "project_summary.json",
            serde_json::to_string(&b_project_summary()).unwrap(),
        ),
        (
            "transcript_block.json",
            serde_json::to_string(&b_transcript_block()).unwrap(),
        ),
        (
            "transcript_entry.json",
            serde_json::to_string(&b_transcript_entry()).unwrap(),
        ),
        (
            "transcript_snapshot.json",
            serde_json::to_string(&b_transcript_snapshot()).unwrap(),
        ),
        (
            "pending_approval.json",
            serde_json::to_string(&b_pending_approval()).unwrap(),
        ),
        (
            "preset_summary.json",
            serde_json::to_string(&b_preset_summary()).unwrap(),
        ),
        (
            "bootstrap_snapshot.json",
            serde_json::to_string(&b_bootstrap_snapshot()).unwrap(),
        ),
        (
            "paired_device_summary.json",
            serde_json::to_string(&b_paired_device_summary()).unwrap(),
        ),
        (
            "workspace_summary.json",
            serde_json::to_string(&b_workspace_summary()).unwrap(),
        ),
        (
            "artifact_upload_progress.json",
            serde_json::to_string(&b_artifact_upload_progress()).unwrap(),
        ),
        (
            "create_session_request.json",
            serde_json::to_string(&b_create_session_request()).unwrap(),
        ),
        (
            "create_session_response.json",
            serde_json::to_string(&b_create_session_response()).unwrap(),
        ),
        (
            "session_text_input.json",
            serde_json::to_string(&b_session_text_input()).unwrap(),
        ),
        (
            "terminal_write_request.json",
            serde_json::to_string(&b_terminal_write_request()).unwrap(),
        ),
        (
            "terminal_resize_request.json",
            serde_json::to_string(&b_terminal_resize_request()).unwrap(),
        ),
        (
            "terminal_color.json",
            serde_json::to_string(&b_terminal_color()).unwrap(),
        ),
        (
            "terminal_style.json",
            serde_json::to_string(&b_terminal_style()).unwrap(),
        ),
        (
            "terminal_cell.json",
            serde_json::to_string(&b_terminal_cell()).unwrap(),
        ),
        (
            "terminal_cursor.json",
            serde_json::to_string(&b_terminal_cursor()).unwrap(),
        ),
        (
            "viewport_frame.json",
            serde_json::to_string(&b_viewport_frame()).unwrap(),
        ),
        (
            "viewport_subscription.json",
            serde_json::to_string(&b_viewport_subscription()).unwrap(),
        ),
        (
            "terminal_cell_run.json",
            serde_json::to_string(&b_terminal_cell_run()).unwrap(),
        ),
        (
            "viewport_patch.json",
            serde_json::to_string(&b_viewport_patch()).unwrap(),
        ),
        (
            "stream_event.json",
            serde_json::to_string(&b_stream_event()).unwrap(),
        ),
        (
            "approval_answer_request.json",
            serde_json::to_string(&b_approval_answer_request()).unwrap(),
        ),
        (
            "mark_read_request.json",
            serde_json::to_string(&b_mark_read_request()).unwrap(),
        ),
        (
            "session_action_request.json",
            serde_json::to_string(&b_session_action_request()).unwrap(),
        ),
        (
            "session_organization_patch.json",
            serde_json::to_string(&b_session_organization_patch()).unwrap(),
        ),
        (
            "screenshot_request.json",
            serde_json::to_string(&b_screenshot_request()).unwrap(),
        ),
        (
            "screenshot_request_response.json",
            serde_json::to_string(&b_screenshot_request_response()).unwrap(),
        ),
        (
            "plugin_update.json",
            serde_json::to_string(&b_plugin_update()).unwrap(),
        ),
        (
            "plugin_updates.json",
            serde_json::to_string(&b_plugin_updates()).unwrap(),
        ),
        (
            "restart_session_request.json",
            serde_json::to_string(&b_restart_session_request()).unwrap(),
        ),
        (
            "push_token_registration.json",
            serde_json::to_string(&b_push_token_registration()).unwrap(),
        ),
    ]
}

/// Rewrite every fixture from the current DTO definitions. Run only after an
/// intentional wire change, then review the `protocol/fixtures/` diff.
#[test]
#[ignore]
fn regenerate_wire_fixtures() {
    let dir = fixtures_dir();
    std::fs::create_dir_all(&dir).unwrap();
    for (name, bytes) in build_cases() {
        // Canonical form: compact JSON, no trailing newline.
        std::fs::write(dir.join(name), bytes).unwrap();
    }
}

// ---------------------------------------------------------------------------
// Checkers: decode the fixture, verify it matches the expected value, then
// verify decode -> encode is byte-identical (the drift guard).
// ---------------------------------------------------------------------------

macro_rules! checker {
    ($name:ident, $ty:ty, $build:expr) => {
        fn $name(s: &str) {
            let v: $ty =
                serde_json::from_str(s).unwrap_or_else(|e| panic!("fixture failed to decode: {e}"));
            assert_eq!(v, $build, "fixture decoded to an unexpected value");
            let back = serde_json::to_string(&v).unwrap();
            assert_eq!(back, s, "fixture is not byte-stable under re-encode");
        }
    };
}

checker!(
    c_session_capabilities,
    SessionCapabilities,
    b_session_capabilities()
);
checker!(c_session_summary, SessionSummary, b_session_summary());
checker!(c_project_summary, ProjectSummary, b_project_summary());
checker!(c_transcript_block, TranscriptBlock, b_transcript_block());
checker!(c_transcript_entry, TranscriptEntry, b_transcript_entry());
checker!(
    c_transcript_snapshot,
    TranscriptSnapshot,
    b_transcript_snapshot()
);
checker!(c_pending_approval, PendingApproval, b_pending_approval());
checker!(c_preset_summary, PresetSummary, b_preset_summary());
checker!(
    c_bootstrap_snapshot,
    BootstrapSnapshot,
    b_bootstrap_snapshot()
);
checker!(
    c_paired_device_summary,
    PairedDeviceSummary,
    b_paired_device_summary()
);
checker!(c_workspace_summary, WorkspaceSummary, b_workspace_summary());
checker!(
    c_artifact_upload_progress,
    ArtifactUploadProgress,
    b_artifact_upload_progress()
);
checker!(
    c_create_session_request,
    CreateSessionRequest,
    b_create_session_request()
);
checker!(
    c_create_session_response,
    CreateSessionResponse,
    b_create_session_response()
);
checker!(
    c_session_text_input,
    SessionTextInput,
    b_session_text_input()
);
checker!(
    c_terminal_write_request,
    TerminalWriteRequest,
    b_terminal_write_request()
);
checker!(
    c_terminal_resize_request,
    TerminalResizeRequest,
    b_terminal_resize_request()
);
checker!(c_terminal_color, TerminalColor, b_terminal_color());
checker!(c_terminal_style, TerminalStyle, b_terminal_style());
checker!(c_terminal_cell, TerminalCell, b_terminal_cell());
checker!(c_terminal_cursor, TerminalCursor, b_terminal_cursor());
checker!(c_viewport_frame, ViewportFrame, b_viewport_frame());
checker!(
    c_viewport_subscription,
    ViewportSubscription,
    b_viewport_subscription()
);
checker!(c_terminal_cell_run, TerminalCellRun, b_terminal_cell_run());
checker!(c_viewport_patch, ViewportPatch, b_viewport_patch());
checker!(c_stream_event, StreamEvent, b_stream_event());
checker!(
    c_approval_answer_request,
    ApprovalAnswerRequest,
    b_approval_answer_request()
);
checker!(c_mark_read_request, MarkReadRequest, b_mark_read_request());
checker!(
    c_session_action_request,
    SessionActionRequest,
    b_session_action_request()
);
checker!(
    c_session_organization_patch,
    SessionOrganizationPatch,
    b_session_organization_patch()
);
checker!(
    c_screenshot_request,
    ScreenshotRequest,
    b_screenshot_request()
);
checker!(
    c_screenshot_request_response,
    ScreenshotRequestResponse,
    b_screenshot_request_response()
);
checker!(c_plugin_update, PluginUpdate, b_plugin_update());
checker!(c_plugin_updates, PluginUpdates, b_plugin_updates());
checker!(
    c_restart_session_request,
    RestartSessionRequest,
    b_restart_session_request()
);
checker!(
    c_push_token_registration,
    PushTokenRegistration,
    b_push_token_registration()
);

type CheckCase = (&'static str, fn(&str));

fn check_cases() -> Vec<CheckCase> {
    vec![
        ("session_capabilities.json", c_session_capabilities),
        ("session_summary.json", c_session_summary),
        ("project_summary.json", c_project_summary),
        ("transcript_block.json", c_transcript_block),
        ("transcript_entry.json", c_transcript_entry),
        ("transcript_snapshot.json", c_transcript_snapshot),
        ("pending_approval.json", c_pending_approval),
        ("preset_summary.json", c_preset_summary),
        ("bootstrap_snapshot.json", c_bootstrap_snapshot),
        ("paired_device_summary.json", c_paired_device_summary),
        ("workspace_summary.json", c_workspace_summary),
        ("artifact_upload_progress.json", c_artifact_upload_progress),
        ("create_session_request.json", c_create_session_request),
        ("create_session_response.json", c_create_session_response),
        ("session_text_input.json", c_session_text_input),
        ("terminal_write_request.json", c_terminal_write_request),
        ("terminal_resize_request.json", c_terminal_resize_request),
        ("terminal_color.json", c_terminal_color),
        ("terminal_style.json", c_terminal_style),
        ("terminal_cell.json", c_terminal_cell),
        ("terminal_cursor.json", c_terminal_cursor),
        ("viewport_frame.json", c_viewport_frame),
        ("viewport_subscription.json", c_viewport_subscription),
        ("terminal_cell_run.json", c_terminal_cell_run),
        ("viewport_patch.json", c_viewport_patch),
        ("stream_event.json", c_stream_event),
        ("approval_answer_request.json", c_approval_answer_request),
        ("mark_read_request.json", c_mark_read_request),
        ("session_action_request.json", c_session_action_request),
        (
            "session_organization_patch.json",
            c_session_organization_patch,
        ),
        ("screenshot_request.json", c_screenshot_request),
        (
            "screenshot_request_response.json",
            c_screenshot_request_response,
        ),
        ("plugin_update.json", c_plugin_update),
        ("plugin_updates.json", c_plugin_updates),
        ("restart_session_request.json", c_restart_session_request),
        ("push_token_registration.json", c_push_token_registration),
    ]
}

/// The drift guard: every fixture must decode into its DTO and re-encode to
/// byte-identical JSON.
#[test]
fn wire_fixtures_decode_and_reencode_byte_stable() {
    let dir = fixtures_dir();
    assert_eq!(
        check_cases().len(),
        36,
        "every DTO in dto.rs needs a fixture"
    );
    for (name, check) in check_cases() {
        let path = dir.join(name);
        let s = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("missing fixture {}: {e}", path.display()));
        check(&s);
    }
}
