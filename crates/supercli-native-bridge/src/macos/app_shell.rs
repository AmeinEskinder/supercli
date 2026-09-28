//! Port of `AppDelegate.swift` (+ `main.swift`, `ModuleResources.swift`) —
//! macOS app shell: launch lifecycle, main-window chrome, NSMenu model
//! contract, and menu validation.
//!
//! Source files (clients/legacy):
//! - `native/SupercliNative/Sources/SupercliNative/AppDelegate.swift`
//!   (`applicationDidFinishLaunching`, window creation, `installMainMenu`,
//!   menu-action dispatch, `validateMenuItem`, Finder service, traffic
//!   lights, titlebar-background hiding, `ChromeHostingView`).
//! - `native/SupercliNative/Sources/SupercliNative/main.swift` (AppKit
//!   bootstrap: activation policy, Dock icon, `app.run()`).
//! - `native/SupercliNative/Sources/SupercliNative/ModuleResources.swift`
//!   (resource-bundle file lookup).
//!
//! Deliberately NOT ported here:
//! - The Sparkle delegate (`installID`, `licenseHeadersForUpdateFeed`,
//!   `feedURLString(for:)`) lives in [`super::updater`]; do not duplicate.
//! - The menu *content* (labels, key equivalents) is owned by the Dart app
//!   (`clients/supercli-app/lib/app_shell_menu.dart`); this module defines
//!   the shared action vocabulary and the validation decision table, and the
//!   actual `NSMenu` construction is `#[cfg(target_os = "macos")]` via objc2.
//!
//! Pure logic is cross-platform and tested. Actual macOS API calls (objc2)
//! are `#[cfg(target_os = "macos")]` gated.

// ---------------------------------------------------------------------------
// Launch lifecycle (main.swift + applicationDidFinishLaunching)
// ---------------------------------------------------------------------------

/// Whether the main window should be shown at launch.
/// Swift `AppDelegate.applicationDidFinishLaunching`: a service launch
/// (`SUPERCLI_LAUNCH_HIDDEN=1` — a peer instance started this workspace to
/// serve pairing) begins windowless.
pub fn show_window_at_launch(launch_hidden: Option<&str>) -> bool {
    launch_hidden != Some("1")
}

/// Whether the store should defer its initial scan.
/// Swift: `deferInitialScan` is true unless any environment key has the
/// `SUPERCLI_TEST_` or `SUPERCLI_SNAPSHOT` prefix (self-test runs scan
/// synchronously).
pub fn defer_initial_scan<'a>(env_keys: impl IntoIterator<Item = &'a str>) -> bool {
    !env_keys
        .into_iter()
        .any(|k| k.starts_with("SUPERCLI_TEST_") || k.starts_with("SUPERCLI_SNAPSHOT"))
}

/// Whether Ghostty debug logging is enabled.
/// Swift: `ProcessInfo.processInfo.environment["SUPERCLI_DEBUG"] == "1"`.
pub fn debug_logging_enabled(supercli_debug: Option<&str>) -> bool {
    supercli_debug == Some("1")
}

/// Whether to set the Dock icon from the resource bundle.
/// Swift `main.swift`: skip when the bundle already declares an icon
/// (`CFBundleIconFile`/`CFBundleIconName`) — overriding it with the raw
/// full-bleed PNG would make the *running* Dock icon differ from the
/// not-running one (the `.icns`, drawn with macOS's standard margin).
pub fn should_set_dock_icon(
    bundle_icon_file: Option<&str>,
    bundle_icon_name: Option<&str>,
) -> bool {
    bundle_icon_file.is_none_or(|f| f.is_empty()) && bundle_icon_name.is_none_or(|n| n.is_empty())
}

/// Launch gate for the update flow. Swift `AppDelegate.sparkleCanStart`.
///
/// Only the default instance updates: two updaters would double-install,
/// Sparkle's relaunch goes through `open` (dropping `SUPERCLI_HOME`, so a
/// workspace would come back as the default workspace), and clearing the
/// feed URL writes the shared `.standard` domain. Workspace instances pick
/// the new binary up on their next relaunch.
pub fn updater_can_start(
    is_default_instance: bool,
    bundle_is_app: bool,
    feed_url: Option<&str>,
    public_key: Option<&str>,
) -> bool {
    is_default_instance
        && bundle_is_app
        && feed_url.is_some_and(|f| !f.is_empty())
        && public_key.is_some_and(|k| !k.is_empty())
}

// ---------------------------------------------------------------------------
// Main-window chrome (showMainWindow)
// ---------------------------------------------------------------------------

/// Default main-window width. Swift `showMainWindow`: 1200.
pub const DEFAULT_WINDOW_WIDTH: f64 = 1200.0;
/// Default main-window height. Swift `showMainWindow`: 800.
pub const DEFAULT_WINDOW_HEIGHT: f64 = 800.0;
/// Minimum window width. Swift: `contentMinSize` 800x600.
pub const MIN_WINDOW_WIDTH: f64 = 800.0;
/// Minimum window height.
pub const MIN_WINDOW_HEIGHT: f64 = 600.0;
/// Custom titlebar strip height (DESIGN.md §1). Swift: `Theme.titlebarHeight`.
pub const TITLEBAR_HEIGHT: f64 = 38.0;
/// Traffic-light x origin (DESIGN.md §1). Swift `positionTrafficLights`.
pub const TRAFFIC_LIGHT_X_START: f64 = 12.0;
/// Traffic-light horizontal spacing. Swift `positionTrafficLights`.
pub const TRAFFIC_LIGHT_SPACING: f64 = 20.0;

/// `ChromeHostingView` behaviour: the SwiftUI tree must never start an
/// AppKit titlebar-region window drag. With a transparent full-size-content
/// titlebar the theme frame would otherwise start a window drag for any drag
/// in the top strip IN PARALLEL with SwiftUI gestures (dragging a pane title
/// chip moved the whole window). All window dragging is explicit via
/// `WindowDragArea`, so the frame's implicit drag is never needed.
pub const HOSTING_VIEW_MOUSE_DOWN_CAN_MOVE_WINDOW: bool = false;

/// Whether the hosting view may start a window drag on mouse-down.
/// Swift `ChromeHostingView.mouseDownCanMoveWindow` returns false (see the
/// constant above for the rationale).
pub fn hosting_view_can_move_window() -> bool {
    HOSTING_VIEW_MOUSE_DOWN_CAN_MOVE_WINDOW
}

/// Main-window spec. Swift `showMainWindow`: chromeless window (hidden
/// title, transparent titlebar, `titlebarSeparatorStyle = .none`),
/// `isMovable = false`, `isMovableByWindowBackground = false`,
/// `isReleasedWhenClosed = false` (the delegate nils its reference on
/// close and rebuilds on demand).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowSpec {
    pub width: f64,
    pub height: f64,
    pub min_width: f64,
    pub min_height: f64,
}

impl Default for WindowSpec {
    fn default() -> Self {
        Self {
            width: DEFAULT_WINDOW_WIDTH,
            height: DEFAULT_WINDOW_HEIGHT,
            min_width: MIN_WINDOW_WIDTH,
            min_height: MIN_WINDOW_HEIGHT,
        }
    }
}

/// Frame origin for traffic-light button `index` (0 = close, 1 = minimize,
/// 2 = zoom). Swift `positionTrafficLights`: x = 12 + index * 20; y centers
/// the button in the 38px titlebar, clamped inside the container.
pub fn traffic_light_origin(index: usize, container_height: f64, button_height: f64) -> (f64, f64) {
    let x = TRAFFIC_LIGHT_X_START + index as f64 * TRAFFIC_LIGHT_SPACING;
    let center_from_top = TITLEBAR_HEIGHT / 2.0;
    let y = container_height - center_from_top - button_height / 2.0;
    // Mirror Swift's `max(0, min(y, container.height - button.height))`:
    // when the button is taller than the container this yields 0 rather
    // than panicking (Rust's `f64::clamp` requires min <= max).
    (x, y.min(container_height - button_height).max(0.0))
}

/// Window corner radius. Swift `showMainWindow`: read off the frame view
/// (`NSThemeFrame.cornerRadius`) before the hosting view is attached so the
/// first render sees it; `Theme.windowCornerRadius` keeps its default when
/// the key is missing or non-positive.
pub const FALLBACK_WINDOW_CORNER_RADIUS: f64 = 16.0;

pub fn resolve_window_corner_radius(read: Option<f64>) -> f64 {
    match read {
        Some(r) if r > 0.0 => r,
        _ => FALLBACK_WINDOW_CORNER_RADIUS,
    }
}

// ---------------------------------------------------------------------------
// Titlebar-background hiding (hideSystemTitlebarBackground)
// ---------------------------------------------------------------------------

/// Whether a titlebar-container subview should be hidden.
/// Swift `hideSystemTitlebarBackground`: even with `titlebarAppearsTransparent`,
/// macOS 26 paints a scroll-edge backdrop band across the window top once a
/// scroll view passes under the titlebar region. The titlebar is fully custom,
/// so hide anything that paints (the same approach Ghostty's
/// transparent-titlebar window uses on Tahoe) — keep buttons alive.
pub fn titlebar_subview_should_hide(view_type_name: &str) -> bool {
    ["Background", "Backdrop", "Separator", "Pocket", "Glass"]
        .iter()
        .any(|fragment| view_type_name.contains(fragment))
}

/// Whether the view's layer should be cleared instead of hiding it.
/// Swift: `NSTitlebarView` keeps its layer but gets a clear background.
pub fn titlebar_view_should_clear_background(view_type_name: &str) -> bool {
    view_type_name == "NSTitlebarView"
}

// ---------------------------------------------------------------------------
// ModuleResources (resource-bundle lookup)
// ---------------------------------------------------------------------------

/// SwiftPM resource-bundle name. Swift `ModuleResources.bundleName`.
pub const RESOURCE_BUNDLE_NAME: &str = "SupercliNative_SupercliNative.bundle";

/// Candidate paths for `name.ext` in the resource bundle, in priority order.
/// Swift `ModuleResources.url(forResource:withExtension:)`: packaged app →
/// `Contents/Resources/<bundle>/`; bare executable → next to the binary
/// (the old `Bundle.module` accessor fatal-errored for executable targets,
/// which crashed Settings ▸ Remote on every install except the build Mac).
pub fn resource_candidate_paths(
    name: &str,
    ext: &str,
    resources_dir: Option<&str>,
    bundle_dir: &str,
) -> Vec<String> {
    let relative = format!("{RESOURCE_BUNDLE_NAME}/{name}.{ext}");
    let mut out = Vec::with_capacity(2);
    if let Some(dir) = resources_dir {
        out.push(format!("{dir}/{relative}"));
    }
    out.push(format!("{bundle_dir}/{relative}"));
    out
}

// ---------------------------------------------------------------------------
// Menu action vocabulary (installMainMenu + @objc dispatch)
// ---------------------------------------------------------------------------

/// Menu actions the native shell dispatches to the store.
/// Swift: the `@objc` selectors in `AppDelegate`. The Dart menu model owns
/// labels and key equivalents; this enum is the shared vocabulary between
/// the Dart model and the Rust-built `NSMenu`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
    // App menu
    CheckForUpdates,
    OpenSettings,
    // Session menu
    NewSession,
    NewTerminal,
    SplitPaneRight,
    SplitPaneDown,
    ZoomPane,
    EqualizeSplits,
    FocusPaneLeft,
    FocusPaneRight,
    FocusPaneUp,
    FocusPaneDown,
    CollapseAllFolders,
    ToggleCommandPalette,
    TakeScreenshot,
    // Edit menu
    Find,
    FindNext,
    FindPrevious,
    // Window menu
    ClosePaneOrWindow,
    // Help menu
    OpenHelp,
}

/// Store snapshot consulted by menu validation. Swift `validateMenuItem`
/// reads these off `SupercliStore` / `TerminalFontModel`.
#[derive(Debug, Clone, Default)]
pub struct MenuValidationState {
    /// A main window currently exists (`window != nil`).
    pub window_exists: bool,
    /// The active terminal pane can be closed (`canCloseActiveTerminalPane`).
    pub can_close_active_terminal_pane: bool,
    /// The selected host scope is the local machine.
    pub selected_host_scope_is_local: bool,
    /// Appearance ▸ "Session gallery" is enabled.
    pub show_session_gallery: bool,
    /// ⌘T currently opens the preset picker (`commandTAction == .presetPicker`).
    pub command_t_opens_preset_picker: bool,
    /// A pane launcher can open (`canOpenPaneLauncher()`).
    pub can_open_pane_launcher: bool,
    /// The project-sidebar launcher can open (`canOpenProjectSidebarLauncher()`).
    pub can_open_project_sidebar_launcher: bool,
    /// A multi-pane group is active (`canZoomTerminalPane`).
    pub can_zoom_terminal_pane: bool,
    /// `TerminalFontModel.shared.canIncreaseSize`.
    pub can_increase_font_size: bool,
    /// `TerminalFontModel.shared.canDecreaseSize`.
    pub can_decrease_font_size: bool,
    /// `TerminalFontModel.shared.isDefaultSize`.
    pub font_size_is_default: bool,
    /// At least one project folder is expanded (`!expandedProjectIDs.isEmpty`).
    pub any_folder_expanded: bool,
}

/// Validation outcome for one menu item. `title` overrides the model title
/// when `Some` — Swift mutates the item title during validation ("Close
/// Pane"/"Close Window", "Choose Preset…"/"New Terminal").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItemValidation {
    pub enabled: bool,
    pub title: Option<&'static str>,
}

impl MenuItemValidation {
    fn enabled() -> Self {
        Self {
            enabled: true,
            title: None,
        }
    }

    fn disabled() -> Self {
        Self {
            enabled: false,
            title: None,
        }
    }
}

/// Validates one menu item against the store snapshot.
/// Swift `AppDelegate.validateMenuItem(_:)` (explicit `autoenablesItems =
/// false` management: AppKit's default re-enables targetless items via the
/// responder chain, so every rule below is stated explicitly).
pub fn validate_menu_item(action: MenuAction, state: &MenuValidationState) -> MenuItemValidation {
    use MenuAction::*;
    match action {
        // ⌘W follows Ghostty's active-surface convention while a terminal is
        // shown, then falls back to closing the window on
        // settings/library/empty screens. Validation keeps the label honest.
        ClosePaneOrWindow => MenuItemValidation {
            enabled: state.window_exists,
            title: Some(if state.can_close_active_terminal_pane {
                "Close Pane"
            } else {
                "Close Window"
            }),
        },
        // Greys out while the session gallery is disabled (Appearance ▸
        // "Session gallery") — the gallery chip owns the capture flow, so
        // with no chip mounted the notification goes nowhere.
        TakeScreenshot => {
            if state.selected_host_scope_is_local && state.show_session_gallery {
                MenuItemValidation::enabled()
            } else {
                MenuItemValidation::disabled()
            }
        }
        // These require the local scope; ⌘T's title follows the Appearance
        // preference (immediate shell vs preset picker).
        NewSession | ToggleCommandPalette => {
            if state.selected_host_scope_is_local {
                MenuItemValidation::enabled()
            } else {
                MenuItemValidation::disabled()
            }
        }
        NewTerminal => MenuItemValidation {
            enabled: state.selected_host_scope_is_local,
            title: Some(if state.command_t_opens_preset_picker {
                "Choose Preset…"
            } else {
                "New Terminal"
            }),
        },
        // ⌘D opens the pane launcher (or the project-sidebar launcher while
        // a right-panel member is selected).
        SplitPaneRight | SplitPaneDown => {
            if state.can_open_pane_launcher || state.can_open_project_sidebar_launcher {
                MenuItemValidation::enabled()
            } else {
                MenuItemValidation::disabled()
            }
        }
        // Zoom, equalize, and spatial focus need a validated multi-pane group.
        ZoomPane | EqualizeSplits | FocusPaneLeft | FocusPaneRight | FocusPaneUp
        | FocusPaneDown => {
            if state.can_zoom_terminal_pane {
                MenuItemValidation::enabled()
            } else {
                MenuItemValidation::disabled()
            }
        }
        // Mirrors the old footer button's disabled state: nothing to
        // collapse when no folder is expanded.
        CollapseAllFolders => {
            if state.any_folder_expanded {
                MenuItemValidation::enabled()
            } else {
                MenuItemValidation::disabled()
            }
        }
        // Find drives the displayed Local pane's find bar; remote panes
        // don't listen (yet).
        Find | FindNext | FindPrevious => {
            if state.selected_host_scope_is_local {
                MenuItemValidation::enabled()
            } else {
                MenuItemValidation::disabled()
            }
        }
        // App/Help items and the responder-chain Edit items need no validation.
        _ => MenuItemValidation::enabled(),
    }
}

/// Font-zoom validation, kept separate because it reads `TerminalFontModel`
/// rather than the store. Swift `validateMenuItem(_:)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontZoomAction {
    Increase,
    Decrease,
    Reset,
}

pub fn validate_font_zoom(
    action: FontZoomAction,
    can_increase: bool,
    can_decrease: bool,
    is_default_size: bool,
) -> bool {
    match action {
        FontZoomAction::Increase => can_increase,
        FontZoomAction::Decrease => can_decrease,
        FontZoomAction::Reset => !is_default_size,
    }
}

/// Which launcher ⌘D / ⇧⌘D opens. Swift `splitPaneFromMenu` /
/// `splitPaneDownFromMenu`: while a right-panel member is selected, ⌘D adds
/// a pane to the PANEL (its launcher row); the main pane launcher would
/// otherwise pull the panel session into the main layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneLauncherTarget {
    ProjectSidebar,
    Right,
    Down,
}

pub fn split_pane_target(
    can_open_project_sidebar_launcher: bool,
    down: bool,
) -> PaneLauncherTarget {
    if can_open_project_sidebar_launcher {
        PaneLauncherTarget::ProjectSidebar
    } else if down {
        PaneLauncherTarget::Down
    } else {
        PaneLauncherTarget::Right
    }
}

/// ⌘W dispatch policy. Swift `closePaneOrWindowFromMenu`: closes the active
/// terminal pane/session when one is mounted (the pane container owns the
/// exact close policy and its in-content agent confirmation); non-terminal
/// screens retain the ordinary window close.
pub fn close_active_pane_instead_of_window(can_close_active_terminal_pane: bool) -> bool {
    can_close_active_terminal_pane
}

// ---------------------------------------------------------------------------
// Finder service (newSupercliSession)
// ---------------------------------------------------------------------------

/// Resolves the Finder "New Supercli Session Here" folder.
/// Swift `newSupercliSession(_:userData:error:)`: only valid when the local
/// machine is the selected scope (a scoped local workspace targets its own
/// home); takes the first directory URL from the pasteboard (folders only —
/// `NSSendFileTypes` filters to `public.folder`, but be defensive).
pub fn finder_service_folder<'a>(
    selected_scope_is_local: bool,
    urls: &[(&'a str, bool)],
) -> Option<&'a str> {
    if !selected_scope_is_local {
        return None;
    }
    urls.iter()
        .find(|(_, is_directory)| *is_directory)
        .map(|(url, _)| *url)
}

// ---------------------------------------------------------------------------
// Window / app lifecycle
// ---------------------------------------------------------------------------

/// Closing the window does NOT quit: the app lives on as a menu-bar agent so
/// hosted sessions keep their live spinners and stay one click away.
/// Swift `applicationShouldTerminateAfterLastWindowClosed` returns false;
/// ⌘Q is the explicit teardown (sessions still survive as hosted PTYs).
pub const QUIT_AFTER_LAST_WINDOW_CLOSED: bool = false;

/// Whether the app quits after the last window closes.
/// Swift `applicationShouldTerminateAfterLastWindowClosed` returns false.
pub fn quit_after_last_window_closed() -> bool {
    QUIT_AFTER_LAST_WINDOW_CLOSED
}

/// Dock-icon click (or other re-open) with no window on screen rebuilds it.
/// Swift `applicationShouldHandleReopen`: `if !flag { showMainWindow() }`.
pub fn should_rebuild_window_on_reopen(has_visible_windows: bool) -> bool {
    !has_visible_windows
}

/// Clean-shutdown order. Swift `applicationWillTerminate`: drop the
/// `~/.supercli/app-ports` entry so hook scripts stop broadcasting to a
/// dead listener (HookServer Drop parity, `hook_server.rs`), after stopping
/// the local Host control client, the platform adapter, and the hook server;
/// the pidfile removal is best-effort (the identity check in `runningPid`
/// makes a leftover pidfile harmless after a crash).
pub const TERMINATION_ORDER: &[&str] = &[
    "stop_local_host_control_client",
    "stop_platform_adapter",
    "stop_hook_server",
    "remove_pidfile",
];

#[cfg(test)]
mod tests {
    use super::*;

    // --- Launch lifecycle ---

    #[test]
    fn service_launch_starts_windowless() {
        assert!(!show_window_at_launch(Some("1")));
    }

    #[test]
    fn normal_launch_shows_window() {
        assert!(show_window_at_launch(None));
        assert!(show_window_at_launch(Some("0")));
    }

    #[test]
    fn initial_scan_deferred_by_default() {
        assert!(defer_initial_scan(Vec::<&str>::new()));
        assert!(defer_initial_scan(["PATH", "HOME"]));
    }

    #[test]
    fn test_env_forces_synchronous_scan() {
        assert!(!defer_initial_scan(["SUPERCLI_TEST_FOO"]));
        assert!(!defer_initial_scan(["SUPERCLI_SNAPSHOT_1", "PATH"]));
        assert!(!defer_initial_scan(["SUPERCLI_TEST_"]));
    }

    #[test]
    fn debug_logging_flag() {
        assert!(debug_logging_enabled(Some("1")));
        assert!(!debug_logging_enabled(None));
        assert!(!debug_logging_enabled(Some("0")));
    }

    #[test]
    fn dock_icon_set_only_when_bundle_declares_none() {
        assert!(should_set_dock_icon(None, None));
        assert!(should_set_dock_icon(Some(""), Some("")));
        assert!(!should_set_dock_icon(Some("AppIcon"), None));
        assert!(!should_set_dock_icon(None, Some("AppIcon")));
        assert!(!should_set_dock_icon(Some("AppIcon"), Some("AppIcon")));
    }

    #[test]
    fn updater_starts_only_for_default_app_instance_with_feed_and_key() {
        assert!(updater_can_start(true, true, Some("https://x"), Some("k")));
        assert!(!updater_can_start(
            false,
            true,
            Some("https://x"),
            Some("k")
        ));
        assert!(!updater_can_start(
            true,
            false,
            Some("https://x"),
            Some("k")
        ));
        assert!(!updater_can_start(true, true, None, Some("k")));
        assert!(!updater_can_start(true, true, Some(""), Some("k")));
        assert!(!updater_can_start(true, true, Some("https://x"), None));
        assert!(!updater_can_start(true, true, Some("https://x"), Some("")));
    }

    // --- Window chrome ---

    #[test]
    fn window_spec_matches_swift_defaults() {
        let spec = WindowSpec::default();
        assert_eq!(spec.width, 1200.0);
        assert_eq!(spec.height, 800.0);
        assert_eq!(spec.min_width, 800.0);
        assert_eq!(spec.min_height, 600.0);
    }

    #[test]
    fn traffic_lights_positioned_per_design() {
        // 38px titlebar; buttons at x = 12 + index*20, vertically centered.
        let (x0, y0) = traffic_light_origin(0, 38.0, 16.0);
        assert_eq!(x0, 12.0);
        assert_eq!(y0, 38.0 - 19.0 - 8.0);
        let (x1, _) = traffic_light_origin(1, 38.0, 16.0);
        assert_eq!(x1, 32.0);
        let (x2, _) = traffic_light_origin(2, 38.0, 16.0);
        assert_eq!(x2, 52.0);
    }

    #[test]
    fn traffic_light_y_is_clamped_inside_container() {
        // Button taller than the container: Swift's max(0, min(...)) yields 0.
        let (_, y) = traffic_light_origin(0, 10.0, 40.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn hosting_view_never_drags_window() {
        assert!(!hosting_view_can_move_window());
    }

    #[test]
    fn corner_radius_uses_read_value_when_positive() {
        assert_eq!(resolve_window_corner_radius(Some(12.0)), 12.0);
    }

    #[test]
    fn corner_radius_falls_back_to_theme_default() {
        assert_eq!(
            resolve_window_corner_radius(None),
            FALLBACK_WINDOW_CORNER_RADIUS
        );
        assert_eq!(
            resolve_window_corner_radius(Some(0.0)),
            FALLBACK_WINDOW_CORNER_RADIUS
        );
        assert_eq!(
            resolve_window_corner_radius(Some(-3.0)),
            FALLBACK_WINDOW_CORNER_RADIUS
        );
    }

    // --- Titlebar-background hiding ---

    #[test]
    fn paint_views_are_hidden() {
        for name in [
            "NSTitlebarBackgroundView",
            "NSBackdropView",
            "NSSeparatorView",
            "_NSPocketView",
            "NSGlassView",
        ] {
            assert!(titlebar_subview_should_hide(name), "{name}");
        }
    }

    #[test]
    fn buttons_and_container_stay_visible() {
        for name in ["NSTitlebarContainerView", "NSButton", "NSWindowButton"] {
            assert!(!titlebar_subview_should_hide(name), "{name}");
        }
    }

    #[test]
    fn titlebar_view_is_cleared_not_hidden() {
        assert!(titlebar_view_should_clear_background("NSTitlebarView"));
        assert!(!titlebar_subview_should_hide("NSTitlebarView"));
        assert!(!titlebar_view_should_clear_background("NSButton"));
    }

    // --- ModuleResources ---

    #[test]
    fn resource_lookup_prefers_packaged_app_dir() {
        let paths = resource_candidate_paths(
            "AppIcon",
            "png",
            Some("/App/Contents/Resources"),
            "/App/Contents/MacOS",
        );
        assert_eq!(
            paths,
            vec![
                "/App/Contents/Resources/SupercliNative_SupercliNative.bundle/AppIcon.png"
                    .to_string(),
                "/App/Contents/MacOS/SupercliNative_SupercliNative.bundle/AppIcon.png".to_string(),
            ]
        );
    }

    #[test]
    fn resource_lookup_bare_executable_has_single_candidate() {
        let paths = resource_candidate_paths("AppIcon", "png", None, "/build/debug");
        assert_eq!(
            paths,
            vec!["/build/debug/SupercliNative_SupercliNative.bundle/AppIcon.png".to_string()]
        );
    }

    // --- Menu validation ---

    fn local_state() -> MenuValidationState {
        MenuValidationState {
            window_exists: true,
            can_close_active_terminal_pane: true,
            selected_host_scope_is_local: true,
            show_session_gallery: true,
            command_t_opens_preset_picker: false,
            can_open_pane_launcher: true,
            can_open_project_sidebar_launcher: false,
            can_zoom_terminal_pane: true,
            can_increase_font_size: true,
            can_decrease_font_size: true,
            font_size_is_default: false,
            any_folder_expanded: true,
        }
    }

    #[test]
    fn close_item_label_follows_pane_state() {
        let state = local_state();
        let v = validate_menu_item(MenuAction::ClosePaneOrWindow, &state);
        assert!(v.enabled);
        assert_eq!(v.title, Some("Close Pane"));

        let mut no_pane = state.clone();
        no_pane.can_close_active_terminal_pane = false;
        let v = validate_menu_item(MenuAction::ClosePaneOrWindow, &no_pane);
        assert_eq!(v.title, Some("Close Window"));

        let mut no_window = state;
        no_window.window_exists = false;
        assert!(!validate_menu_item(MenuAction::ClosePaneOrWindow, &no_window).enabled);
    }

    #[test]
    fn screenshot_needs_local_scope_and_gallery() {
        let state = local_state();
        assert!(validate_menu_item(MenuAction::TakeScreenshot, &state).enabled);

        let mut no_gallery = state.clone();
        no_gallery.show_session_gallery = false;
        assert!(!validate_menu_item(MenuAction::TakeScreenshot, &no_gallery).enabled);

        let mut remote = state;
        remote.selected_host_scope_is_local = false;
        assert!(!validate_menu_item(MenuAction::TakeScreenshot, &remote).enabled);
    }

    #[test]
    fn session_actions_need_local_scope() {
        let state = local_state();
        for action in [
            MenuAction::NewSession,
            MenuAction::ToggleCommandPalette,
            MenuAction::Find,
            MenuAction::FindNext,
            MenuAction::FindPrevious,
        ] {
            assert!(validate_menu_item(action, &state).enabled);
            let mut remote = state.clone();
            remote.selected_host_scope_is_local = false;
            assert!(!validate_menu_item(action, &remote).enabled, "{action:?}");
        }
    }

    #[test]
    fn new_terminal_title_follows_preset_preference() {
        let state = local_state();
        assert_eq!(
            validate_menu_item(MenuAction::NewTerminal, &state).title,
            Some("New Terminal")
        );
        let mut picker = state;
        picker.command_t_opens_preset_picker = true;
        assert_eq!(
            validate_menu_item(MenuAction::NewTerminal, &picker).title,
            Some("Choose Preset…")
        );
    }

    #[test]
    fn split_pane_needs_a_launcher() {
        let state = local_state();
        assert!(validate_menu_item(MenuAction::SplitPaneRight, &state).enabled);
        assert!(validate_menu_item(MenuAction::SplitPaneDown, &state).enabled);

        let mut sidebar = state.clone();
        sidebar.can_open_pane_launcher = false;
        sidebar.can_open_project_sidebar_launcher = true;
        assert!(validate_menu_item(MenuAction::SplitPaneRight, &sidebar).enabled);

        let mut none = state;
        none.can_open_pane_launcher = false;
        none.can_open_project_sidebar_launcher = false;
        assert!(!validate_menu_item(MenuAction::SplitPaneRight, &none).enabled);
    }

    #[test]
    fn zoom_equalize_focus_need_multipane_group() {
        let state = local_state();
        for action in [
            MenuAction::ZoomPane,
            MenuAction::EqualizeSplits,
            MenuAction::FocusPaneLeft,
            MenuAction::FocusPaneRight,
            MenuAction::FocusPaneUp,
            MenuAction::FocusPaneDown,
        ] {
            assert!(validate_menu_item(action, &state).enabled);
            let mut single = state.clone();
            single.can_zoom_terminal_pane = false;
            assert!(!validate_menu_item(action, &single).enabled, "{action:?}");
        }
    }

    #[test]
    fn font_zoom_validation() {
        assert!(validate_font_zoom(
            FontZoomAction::Increase,
            true,
            true,
            false
        ));
        assert!(!validate_font_zoom(
            FontZoomAction::Increase,
            false,
            true,
            false
        ));
        assert!(validate_font_zoom(
            FontZoomAction::Decrease,
            true,
            true,
            false
        ));
        assert!(!validate_font_zoom(
            FontZoomAction::Decrease,
            true,
            false,
            false
        ));
        assert!(validate_font_zoom(FontZoomAction::Reset, true, true, false));
        assert!(!validate_font_zoom(FontZoomAction::Reset, true, true, true));
    }

    #[test]
    fn collapse_all_needs_an_expanded_folder() {
        let state = local_state();
        assert!(validate_menu_item(MenuAction::CollapseAllFolders, &state).enabled);
        let mut none = state;
        none.any_folder_expanded = false;
        assert!(!validate_menu_item(MenuAction::CollapseAllFolders, &none).enabled);
    }

    #[test]
    fn static_items_are_always_enabled() {
        let state = MenuValidationState::default();
        for action in [
            MenuAction::CheckForUpdates,
            MenuAction::OpenSettings,
            MenuAction::OpenHelp,
        ] {
            let v = validate_menu_item(action, &state);
            assert!(v.enabled, "{action:?}");
            assert_eq!(v.title, None);
        }
    }

    // --- Pane launcher / close dispatch ---

    #[test]
    fn split_pane_target_prefers_project_sidebar_launcher() {
        assert_eq!(
            split_pane_target(true, false),
            PaneLauncherTarget::ProjectSidebar
        );
        assert_eq!(
            split_pane_target(true, true),
            PaneLauncherTarget::ProjectSidebar
        );
        assert_eq!(split_pane_target(false, false), PaneLauncherTarget::Right);
        assert_eq!(split_pane_target(false, true), PaneLauncherTarget::Down);
    }

    #[test]
    fn cmd_w_closes_pane_when_mounted() {
        assert!(close_active_pane_instead_of_window(true));
        assert!(!close_active_pane_instead_of_window(false));
    }

    // --- Finder service ---

    #[test]
    fn finder_service_takes_first_folder_when_local() {
        let urls = [
            ("/tmp/file.txt", false),
            ("/Users/a/proj", true),
            ("/Users/a/other", true),
        ];
        assert_eq!(finder_service_folder(true, &urls), Some("/Users/a/proj"));
    }

    #[test]
    fn finder_service_rejects_remote_scope_and_files() {
        let urls = [("/Users/a/proj", true)];
        assert_eq!(finder_service_folder(false, &urls), None);
        let files = [("/tmp/a.txt", false)];
        assert_eq!(finder_service_folder(true, &files), None);
        let empty: [(&str, bool); 0] = [];
        assert_eq!(finder_service_folder(true, &empty), None);
    }

    // --- Lifecycle ---

    #[test]
    fn window_close_does_not_quit() {
        assert!(!quit_after_last_window_closed());
    }

    #[test]
    fn reopen_rebuilds_window_only_when_none_visible() {
        assert!(should_rebuild_window_on_reopen(false));
        assert!(!should_rebuild_window_on_reopen(true));
    }

    #[test]
    fn termination_order_stops_services_before_pidfile() {
        assert_eq!(
            TERMINATION_ORDER,
            &[
                "stop_local_host_control_client",
                "stop_platform_adapter",
                "stop_hook_server",
                "remove_pidfile",
            ]
        );
    }
}
