//! Shared Dioxus UI components for the Unpeel cross-platform clients.
//!
//! `unpeel-desktop` and `unpeel-mobile` are thin launchers over these
//! components: one component tree, every screen. Components talk to a Host
//! only through `unpeel_client::HostClient` — they never touch sockets,
//! the relay, or platform APIs directly.

mod activity;
mod annotation;
mod app_lock;
pub mod i18n;
mod banner;
mod clickable_path;
mod command_palette;
mod components;
mod composer;
mod dictation;
mod discovery;
mod drop_targets;
mod find;
mod gallery;
mod notifier;
mod organize;
mod pairing;
mod panes;
mod prediction;
mod presence;
mod presets;
mod project_tree;
mod provider_theme;
mod push;
mod qr;
mod settings;
mod ssh;
mod styles;
mod terminal;
mod toast;
mod viewport;
mod workspaces;

pub use styles::APP_CSS;

pub use activity::{
    component::SessionActivityRow, SessionActivity, SessionAgentState, SessionRowContext,
};

pub use annotation::{
    arrow_svg_d, client_to_normalized, fitted_size, flatten_annotation_png, flatten_spec,
    head_length_for, line_width_for, normalize_pointer, overlay_geometry, remap_through_crop,
    track_annot_rects, AnnotRects, Arrow, ArrowMarkupView, Corner, CropRect, CropView, FlatPath,
    FlattenSpec, FreehandView, Stroke, FLAT_HEAD_FRAC, FLAT_WIDTH_FRAC, PALETTE,
};
pub use app_lock::{
    method_label, AppLockCapability, AppLockManager, AppLockOverlay, AuthError, BiometricBackend,
    BiometryType, NoBiometricBackend, ShellBiometricBackend,
};
pub use banner::{
    shows_always_show_remote_badge, shows_remote_banner, ConnectionMode, RemoteHostRecord,
};
pub use clickable_path::{
    absolute_path, file_url_match, match_in_row, matches_in_row, resolve_file, PathClickRequest,
    PathMatch, TerminalWorkingDirectory,
};
pub use command_palette::{
    filter_palette, palette_fuzzy_score, CommandPalette, PaletteItem, PaletteKind,
    PALETTE_SHORTCUT_JS,
};
pub use components::{ApprovalCard, ChatView, ConnectionBar, SessionList, TranscriptView};
pub use composer::{Composer, ComposerState};
pub use dictation::{
    native_reflect_js, native_speech_cmd_js, parse_native_refined, parse_native_text,
    sanitize_reflection, should_refine, DictationPhase, DictationSession, DictationSettings,
    DictationView, DICTATION_JS, DICTATION_TEARDOWN_JS, NATIVE_REFLECT_PROBE_JS,
    NATIVE_SPEECH_PROBE_JS, NATIVE_SPEECH_PUMP_JS, REFINE_TIMEOUT_MS, REFLECTION_MIN_WORDS,
};
pub use discovery::{
    browse_once, catalog, component::DiscoverySheet, parse_mdns_response, DiscoveryError,
    DiscoveryState, NearbyHostCandidate, MDNS_MULTICAST, MDNS_PORT, MDNS_SERVICE_TYPE,
};
pub use drop_targets::{
    DropTargetEvent, DropTargetEventKind, DropTargetMap, DropTargetRegion, PathDragMap,
    PathDragRow, TERMINAL_DND_JS,
};
pub use find::{
    find_counter_text, find_matches, row_text, split_run_for_find, FindBar, FindHighlight,
    FindMatch, FindSplitRuns, FindState,
};
pub use gallery::{
    share_entry_js, share_entry_payload, AnnotationMode, AnnotationResult, BrowserGalleryPanel,
    GalleryDetailView, GalleryEntry,
};
pub use notifier::{
    notifier_post_js, DesktopNotification, NotificationKind, NotifierState, NOTIFIER_JS,
};
pub use organize::{
    session_organize_resume_presentation, ArchiveAction, ArchivedSessionsSheet,
    ProjectOrganizePatch, ProjectOrganizeSheet, ProjectOrganizeTarget, SessionOrganizePatch,
    SessionOrganizeResumePresentation, SessionOrganizeSheet, SessionSheetAction, FOLDER_COLORS,
};
pub use pairing::{PairingStatus, PairingView};
pub use panes::{
    canonical_id, clamp_ratio, component::PaneTreeView, layout_bind_launcher, layout_close_group,
    layout_detach_pane, layout_equalize, layout_focus_neighbor, layout_insert_launcher,
    layout_insert_session, layout_remove_launcher, layout_resize_split, layout_swap_panes,
    live_leaves, reconcile, DurableLayout, DurablePane, DurablePaneGroup, DurablePaneNode,
    DurableSplit, FocusDirection, InsertTarget, LiveLeaf, Pane, PaneContent, PaneDropTarget,
    PaneEdge, PaneGroup, PaneNode, PaneOpError, PaneSplit, PaneSplitBranch, PaneSplitPath,
    SplitDirection, DURABLE_VERSION, MAXIMUM_SPLIT_RATIO, MINIMUM_SPLIT_RATIO, SESSION_LEAF_CAP,
};
pub use prediction::{
    detect_scroll_shift, KeystrokePredictionEngine, Prediction, ScrollPredictionEngine,
};
pub use presence::{
    display_name_from_device, merge_presence, parse_presence, presence_file_paths, PresenceStore,
    ViewerAvatars, ViewerInfo, FILE_ENTRY_TTL_MS, MOBILE_ENTRY_TTL_MS, POLL_INTERVAL_MS,
};
pub use presets::{launchable_presets, preset_display_title, preset_icon_initial, PresetDrawer};
pub use project_tree::{
    can_file, component::ProjectTreeView, crosses_checkout, destinations, home_project_id,
    is_worktree_bound, project_tree, Project,
};
pub use provider_theme::{ProviderTheme, ProviderThemeCache, ProviderThemeReadRequest};
pub use push::{
    hex_token, parse_push_bridge_message, PushBridgeEvent, PushManager, PushRegistrationState,
    PUSH_BRIDGE_JS, PUSH_TOKEN_PROBE_JS,
};
pub use qr::{QrDedup, QrScanState, QrScannerView, QR_CTL_JS, QR_START_JS, QR_STOP_JS};
pub use settings::{
    component::SettingsView, experimental_features, is_enabled, mcp_policy_sections,
    plugin_settings_list, prefers_remote_mouse_wheel, shipped_features, wheel_forwarding,
    AppFeature, DevSettings, McpPolicyRow, McpPolicySection, PluginSettingsEntry, SettingsTab,
    WheelForwarding, FEATURE_BROWSER_MCP, FEATURE_REMOTE_WORKSPACES, FEATURE_SESSIONS_MCP,
    FEATURE_WORKSPACES, FEATURE_WORKTREES, REMOTE_MOUSE_WHEEL_PROVIDERS,
};
pub use ssh::{
    component::SshHostRows, validate_ssh_target, MemorySshSecretStore, RemoteSshConnectionMode,
    SshHostRecord, SshHostSetupError, SshHostStore, SshSecretStore, SshTransportRunner,
};
pub use terminal::{
    key_to_sequence, trimmed_viewport_text, viewport_text, word_anchor_at, PredictionOverlay,
    SelectionRequest, StyleRun, TermColor, TerminalModel, TerminalQueryFilter, TerminalRow,
    TerminalSnapshot, TerminalView, TextSelectionSheet, WordAnchor, DEFAULT_COLS, DEFAULT_ROWS,
    SCROLLBACK,
};
pub use toast::{Toast, ToastCenter, ToastOverlay, TOAST_DEFAULT_SECONDS};
pub use unpeel_client as client;
pub use viewport::{fit_grid, should_resize_remote};
pub use workspaces::{
    can_create_worktree, component::WorkspaceOpenMenu, open_in_target, workspace_picker_rows,
    RemoteFolderPick, WorkspaceOpenTarget, WorkspaceRef, WorktreeInfo, WorktreeSettings,
};
