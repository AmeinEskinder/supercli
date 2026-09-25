//! Desktop settings surface: feature flags, MCP policy sections, plugin
//! settings, the mouse-wheel preference, and debug toggles. Ported from
//! `clients/native/SupercliNative/Sources/SupercliNative/FeatureFlags.swift`,
//! `Views/SettingsView.swift`, `Views/AgentAccessSettingsPanel.swift`,
//! `Views/BrowserAccessSections.swift`, `Views/SessionsAccessSections.swift`,
//! `Views/PluginSettingsPanel.swift`, `Views/PluginSettingsList.swift`,
//! `clients/ios/SupercliIOS/Sources/SupercliIOS/DevSettings.swift`, and the wheel
//! preference in `clients/ios/SupercliIOS/Sources/SupercliIOS/RemoteGhosttyTerminalView.swift`.

use crate::i18n::t;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A user-facing optional feature, toggleable in Settings ▸ Features.
/// Mirrors `AppFeature`. Adding a feature is a single entry in [`all_features`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppFeature {
    /// Stable id; also the defaults key suffix. Never rename once shipped.
    pub key: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub env_override: Option<&'static str>,
    pub legacy_env_overrides: &'static [&'static str],
    pub default_on: bool,
    /// Still being shaped: listed under the Experimental section.
    pub experimental: bool,
}

impl AppFeature {
    pub const fn new(
        key: &'static str,
        title: &'static str,
        summary: &'static str,
        env_override: Option<&'static str>,
        legacy_env_overrides: &'static [&'static str],
        default_on: bool,
        experimental: bool,
    ) -> Self {
        Self {
            key,
            title,
            summary,
            env_override,
            legacy_env_overrides,
            default_on,
            experimental,
        }
    }

    /// The persisted key — the `supercli.experimental.` prefix is the shipped
    /// spelling for every feature, graduated or not.
    pub fn defaults_key(&self) -> String {
        format!("supercli.experimental.{}", self.key)
    }

    pub fn env_overrides(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if let Some(e) = self.env_override {
            out.push(e);
        }
        out.extend_from_slice(self.legacy_env_overrides);
        out
    }
}

pub const FEATURE_WORKTREES: AppFeature = AppFeature::new(
    "worktrees",
    "Git worktrees",
    "Run sessions in an isolated git worktree of a project so multiple agents can work the same repo in parallel without touching each other's files.",
    Some("SUPERCLI_DEV_WORKTREES"),
    &[],
    true,
    false,
);
pub const FEATURE_SESSIONS_MCP: AppFeature = AppFeature::new(
    "sessionsMcp",
    "Sessions use",
    "Let an agent session see your other sessions: it can read them all, and asks before writing to another session unless you already approved that pair.",
    Some("SUPERCLI_DEV_SESSIONS_MCP"),
    &[],
    true,
    false,
);
/// The persisted key is deliberately still `profiles`: shipped
/// experimental-feature keys are immutable.
pub const FEATURE_WORKSPACES: AppFeature = AppFeature::new(
    "profiles",
    "Workspaces",
    "Use extra, fully separate workspaces — each has its own sessions, projects, presets, settings, and pairs with your phone as its own workspace.",
    Some("SUPERCLI_DEV_WORKSPACES"),
    &["SUPERCLI_DEV_PROFILES"],
    true,
    false,
);
pub const FEATURE_BROWSER_MCP: AppFeature = AppFeature::new(
    "browserMcp",
    "Browser use",
    "Let agent sessions drive a real browser — open pages, click, fill forms, and take screenshots. Each session gets its own isolated browser.",
    Some("SUPERCLI_DEV_BROWSER_MCP"),
    &[],
    true,
    true,
);
pub const FEATURE_REMOTE_WORKSPACES: AppFeature = AppFeature::new(
    "remoteWorkspaces",
    "Remote workspaces",
    "Add and control workspaces on other machines — pair another Mac, a headless `supercli serve` box, or an SSH host. Direct connections are for your own network or VPN; Supercli Link carries the encrypted path when you are away.",
    Some("SUPERCLI_DEV_REMOTE_WORKSPACES"),
    &[],
    true,
    false,
);

/// Everything shown in Settings ▸ Features, in display order.
pub fn all_features() -> [&'static AppFeature; 5] {
    [
        &FEATURE_REMOTE_WORKSPACES,
        &FEATURE_WORKTREES,
        &FEATURE_SESSIONS_MCP,
        &FEATURE_WORKSPACES,
        &FEATURE_BROWSER_MCP,
    ]
}

/// Feature-flag evaluation. Mirrors `SupercliFeatureFlags`: env override
/// first (dev escape hatch), then the stored preference, then the built-in
/// default. (The workspace-inheritance tier is launcher state; the pure
/// rule lives here and is what the tests pin.)
pub fn is_enabled(
    feature: &AppFeature,
    stored: &HashMap<String, bool>,
    env: &dyn Fn(&str) -> Option<String>,
) -> bool {
    if feature
        .env_overrides()
        .iter()
        .any(|name| env(name).as_deref() == Some("1"))
    {
        return true;
    }
    if let Some(own) = stored.get(&feature.defaults_key()) {
        return *own;
    }
    feature.default_on
}

pub fn shipped_features() -> Vec<&'static AppFeature> {
    all_features()
        .into_iter()
        .filter(|f| !f.experimental)
        .collect()
}

pub fn experimental_features() -> Vec<&'static AppFeature> {
    all_features()
        .into_iter()
        .filter(|f| f.experimental)
        .collect()
}

/// One MCP policy settings section. Mirrors the Agents/Browser/Sessions
/// access panels' shared shape: a titled section with toggle rows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPolicySection {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub rows: Vec<McpPolicyRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPolicyRow {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub enabled: bool,
}

/// The three MCP policy sections (agents, browser, sessions use).
pub fn mcp_policy_sections() -> [McpPolicySection; 3] {
    [
        McpPolicySection {
            id: "agents".into(),
            title: {t("settings.agents")}.into(),
            summary: "Which agent sessions may use MCP servers, and what each server may do.".into(),
            rows: vec![
                McpPolicyRow { id: "agents.enabled".into(), title: {t("settings.allow_agent_mcp_access")}.into(), summary: "Sessions can call tools on approved MCP servers.".into(), enabled: true },
                McpPolicyRow { id: "agents.askOnWrite".into(), title: {t("settings.ask_before_writes")}.into(), summary: "An agent must ask before a tool writes outside its own session.".into(), enabled: true },
            ],
        },
        McpPolicySection {
            id: "browser".into(),
            title: {t("settings.browser")}.into(),
            summary: "Agent sessions get an isolated real browser. Access prompts are cooperation controls, not a sandbox.".into(),
            rows: vec![
                McpPolicyRow { id: "browser.enabled".into(), title: {t("settings.allow_browser_use")}.into(), summary: "New sessions launch with the browser domain advertised.".into(), enabled: false },
                McpPolicyRow { id: "browser.persistLogin".into(), title: {t("settings.persist_logins")}.into(), summary: "Keep the session browser's login state between runs.".into(), enabled: false },
            ],
        },
        McpPolicySection {
            id: "sessions".into(),
            title: {t("settings.sessions_use")}.into(),
            summary: "Agent sessions can read other sessions and request write access to explicit targets.".into(),
            rows: vec![
                McpPolicyRow { id: "sessions.readAll".into(), title: {t("settings.read_other_sessions")}.into(), summary: "Sessions can read every other session.".into(), enabled: true },
                McpPolicyRow { id: "sessions.askOnWrite".into(), title: {t("settings.ask_before_writing_to_another_session")}.into(), summary: "Unless that pair was already approved.".into(), enabled: true },
            ],
        },
    ]
}

/// A plugin settings entry. Mirrors `PluginSettingsList`'s projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSettingsEntry {
    pub id: String,
    pub title: String,
    pub enabled: bool,
    /// Whether the row renders the detached drag card affordance.
    pub draggable_card: bool,
}

pub fn plugin_settings_list(entries: &[PluginSettingsEntry]) -> Vec<PluginSettingsEntry> {
    let mut out = entries.to_vec();
    out.sort_by_key(|a| a.title.to_lowercase());
    out
}

/// Debug/developer toggles. Mirrors `DevSettings` (persisted key kept).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DevSettings {
    /// Draw an outline around the terminal grid to inspect its real bounds.
    pub show_terminal_bounds: bool,
}

impl DevSettings {
    pub const BOUNDS_KEY: &'static str = "supercli.dev.showTerminalBounds";
}

/// Agents whose TUI owns wheel scrolling itself (its transcript scrolls in
/// the TUI), judged from any identity a session summary carries. Mirrors
/// `RemoteGhosttyRenderer.remoteMouseWheelProviders`.
pub const REMOTE_MOUSE_WHEEL_PROVIDERS: &[&str] = &["claude", "grok", "opencode"];

/// Whether the session's agent owns wheel scrolling: any of the three
/// identities qualifies — the legacy launch-derived provider, the command
/// head, or the Host-observed foreground runtime (the last is what
/// qualifies a Claude started by hand in a shell or through a wrapper).
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
    let Some(executable) = command
        .split(char::is_whitespace)
        .next()
        .map(|head| head.rsplit('/').next().unwrap_or(head).to_lowercase())
    else {
        return false;
    };
    if executable.is_empty() {
        return false;
    }
    REMOTE_MOUSE_WHEEL_PROVIDERS.contains(&executable.as_str())
}

/// The wheel-forwarding decision once (or before) the Host's mode snapshot
/// arrives. Mirrors `RemoteGhosttyRenderer.wheelForwarding`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelForwarding {
    /// Do not forward: the flick scrolls local scrollback, like any shell.
    None,
    /// SGR wheel reports: the program tracks the mouse.
    Mouse,
    /// Alternate scroll: the program owns the alternate screen but tracks
    /// no mouse, so each wheel step becomes a cursor Up/Down key.
    AlternateScroll,
}

pub fn wheel_forwarding(
    has_host_mode_snapshot: bool,
    mouse_tracking_enabled: bool,
    alternate_screen_enabled: bool,
    saw_mouse_or_alternate_disable: bool,
    provider_prefers_remote_mouse_wheel: bool,
) -> WheelForwarding {
    if mouse_tracking_enabled {
        return WheelForwarding::Mouse;
    }
    if alternate_screen_enabled {
        return WheelForwarding::AlternateScroll;
    }
    if has_host_mode_snapshot {
        return WheelForwarding::None;
    }
    if provider_prefers_remote_mouse_wheel && !saw_mouse_or_alternate_disable {
        WheelForwarding::Mouse
    } else {
        WheelForwarding::None
    }
}

/// A settings tab in the desktop settings surface. Mirrors `SettingsView`'s
/// tab list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    Features,
    Agents,
    Browser,
    Sessions,
    Plugins,
    Workspaces,
    Worktrees,
    Developer,
}

impl SettingsTab {
    pub fn title(&self) -> String {
        match self {
            SettingsTab::Features => t("settings.features"),
            SettingsTab::Agents => t("settings.agents"),
            SettingsTab::Browser => t("settings.browser"),
            SettingsTab::Sessions => t("settings.sessions"),
            SettingsTab::Plugins => t("settings.plugins"),
            SettingsTab::Workspaces => t("settings.workspaces"),
            SettingsTab::Worktrees => t("settings.worktrees"),
            SettingsTab::Developer => t("settings.developer"),
        }
    }

    pub fn all() -> [SettingsTab; 8] {
        [
            SettingsTab::Features,
            SettingsTab::Agents,
            SettingsTab::Browser,
            SettingsTab::Sessions,
            SettingsTab::Plugins,
            SettingsTab::Workspaces,
            SettingsTab::Worktrees,
            SettingsTab::Developer,
        ]
    }
}

/// Dioxus component: the desktop settings root.
pub mod component {
    use super::*;
    use dioxus::prelude::*;

    #[component]
    pub fn SettingsView(
        active_tab: SettingsTab,
        features: Vec<(AppFeature, bool)>,
        on_tab: EventHandler<SettingsTab>,
        on_toggle_feature: EventHandler<(&'static str, bool)>,
    ) -> Element {
        rsx! {
            div { class: "settings-view",
                nav { class: "settings-tabs",
                    for tab in SettingsTab::all() {
                        button {
                            key: "{tab.title()}",
                            class: if tab == active_tab { "settings-tab active" } else { "settings-tab" },
                            onclick: move |_| on_tab.call(tab),
                            "{tab.title()}"
                        }
                    }
                }
                div { class: "settings-body",
                    if active_tab == SettingsTab::Features {
                        for (feature, enabled) in features {
                            div {
                                key: "{feature.key}",
                                class: "feature-row",
                                div { class: "feature-text",
                                    div { class: "feature-title", "{feature.title}" }
                                    div { class: "feature-summary", "{feature.summary}" }
                                    if feature.experimental { span { class: "feature-badge", {t("settings.experimental")} } }
                                }
                                input {
                                    r#type: "checkbox",
                                    checked: enabled,
                                    onchange: move |e: Event<FormData>| {
                                        on_toggle_feature.call((feature.key, e.value() == "true"));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn feature_catalog_shape() {
        let all = all_features();
        assert_eq!(all.len(), 5);
        // Shipped keys are immutable: workspaces still persists as `profiles`.
        assert_eq!(FEATURE_WORKSPACES.key, "profiles");
        assert_eq!(
            FEATURE_WORKSPACES.defaults_key(),
            "supercli.experimental.profiles"
        );
        assert_eq!(
            FEATURE_WORKSPACES.env_overrides(),
            vec!["SUPERCLI_DEV_WORKSPACES", "SUPERCLI_DEV_PROFILES"]
        );
        assert_eq!(shipped_features().len(), 4);
        assert_eq!(experimental_features().len(), 1);
        assert_eq!(experimental_features()[0].key, "browserMcp");
    }

    #[test]
    fn flag_evaluation_order() {
        let stored = HashMap::new();
        // Built-in default wins when nothing overrides.
        assert!(is_enabled(&FEATURE_WORKTREES, &stored, &no_env));
        // Stored preference wins over the default.
        let mut stored = HashMap::new();
        stored.insert(FEATURE_WORKTREES.defaults_key(), false);
        assert!(!is_enabled(&FEATURE_WORKTREES, &stored, &no_env));
        // Env override wins over everything (dev escape hatch).
        let env = |name: &str| {
            if name == "SUPERCLI_DEV_WORKTREES" {
                Some("1".to_string())
            } else {
                None
            }
        };
        assert!(is_enabled(&FEATURE_WORKTREES, &stored, &env));
    }

    #[test]
    fn mcp_sections_shape() {
        let sections = mcp_policy_sections();
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].id, "agents");
        assert_eq!(sections[1].id, "browser");
        assert_eq!(sections[2].id, "sessions");
        assert!(sections.iter().all(|s| !s.rows.is_empty()));
    }

    #[test]
    fn plugin_list_sorted_case_insensitive() {
        let entries = vec![
            PluginSettingsEntry {
                id: "b".into(),
                title: "Zulu".into(),
                enabled: true,
                draggable_card: true,
            },
            PluginSettingsEntry {
                id: "a".into(),
                title: "alpha".into(),
                enabled: false,
                draggable_card: false,
            },
        ];
        let sorted = plugin_settings_list(&entries);
        assert_eq!(sorted[0].title, "alpha");
        assert_eq!(sorted[1].title, "Zulu");
    }

    #[test]
    fn dev_settings_key() {
        assert_eq!(DevSettings::BOUNDS_KEY, "supercli.dev.showTerminalBounds");
        assert!(!DevSettings::default().show_terminal_bounds);
    }

    #[test]
    fn wheel_pref_qualifies_any_identity() {
        // Host-observed runtime qualifies even with a wrapper command.
        assert!(prefers_remote_mouse_wheel(None, "/bin/zsh", Some("Claude")));
        assert!(prefers_remote_mouse_wheel(
            None,
            "~/bin/agent-wrapper --profile work",
            Some("claude")
        ));
        // Legacy provider and command head still qualify.
        assert!(prefers_remote_mouse_wheel(Some("claude"), "/bin/zsh", None));
        assert!(prefers_remote_mouse_wheel(
            None,
            "/opt/homebrew/bin/claude --resume",
            None
        ));
        // Plain shells do not.
        assert!(!prefers_remote_mouse_wheel(None, "/bin/zsh", None));
        assert!(!prefers_remote_mouse_wheel(None, "/bin/zsh", Some("vim")));
        assert!(!prefers_remote_mouse_wheel(None, "", None));
    }

    #[test]
    fn wheel_forwarding_decision() {
        // Tracked modes are authoritative once the snapshot arrived.
        assert_eq!(
            wheel_forwarding(true, true, false, false, false),
            WheelForwarding::Mouse
        );
        assert_eq!(
            wheel_forwarding(true, false, true, false, false),
            WheelForwarding::AlternateScroll
        );
        assert_eq!(
            wheel_forwarding(true, false, false, false, true),
            WheelForwarding::None
        );
        // Pre-snapshot: the provider heuristic is the fallback until a
        // disable is seen.
        assert_eq!(
            wheel_forwarding(false, false, false, false, true),
            WheelForwarding::Mouse
        );
        assert_eq!(
            wheel_forwarding(false, false, false, true, true),
            WheelForwarding::None
        );
        assert_eq!(
            wheel_forwarding(false, false, false, false, false),
            WheelForwarding::None
        );
    }

    #[test]
    fn settings_tabs_complete() {
        assert_eq!(SettingsTab::all().len(), 8);
        assert_eq!(SettingsTab::Developer.title(), "Developer");
    }
}
