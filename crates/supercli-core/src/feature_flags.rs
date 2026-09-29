//! Feature flag registry.
//!
//! Port of `clients/legacy/native/SupercliNative/Sources/SupercliNative/FeatureFlags.swift`.
//!
//! User-facing optional features, toggleable in Settings ▸ Features. A
//! feature is either shipped (the plain Features list) or `experimental`
//! (the tab's Experimental section: still being shaped, may change or
//! disappear between releases). Graduating one is flipping that flag; the
//! toggle, key, and gates stay.
//!
//! Resolution order mirrors Swift exactly:
//! 1. env override — any of the feature's env vars `== "1"` force-enables
//!    (dev escape hatch; it can never force-disable),
//! 2. this workspace's own stored preference,
//! 3. the default workspace's inherited value (only when this is not the
//!    default instance — Swift's `SupercliWorkspaceContext.isDefaultInstance`),
//! 4. the feature's built-in default.
//!
//! Persistence is injected through [`DefaultsStore`]: Swift uses a native
//! `UserDefaults` overlay (never `app-state.json`), keyed by
//! [`AppFeature::defaults_key`] — the `supercli.experimental.` prefix is the
//! shipped spelling for every feature, graduated or not. The native shell
//! must wire a UserDefaults-backed store; tests use [`MemoryDefaults`].
//!
//! Honest gaps (see sidecar notes):
//! - `WorkspaceFeature::pickerEnabled`'s remote half (`RemoteHostFeature.pickerEnabled`)
//!   is not ported yet; [`workspace_picker_enabled`] takes both booleans, matching
//!   Swift's pure, testable form (`nonisolated static func pickerEnabled(...)`).

use std::collections::HashMap;
use std::sync::Mutex;

/// A user-facing optional feature, toggleable in Settings ▸ Features.
///
/// Adding a feature is a single static below plus an entry in [`AppFeature::all`]:
/// it automatically gets a toggle row in the Features tab and an `is_enabled`
/// check to gate UI on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AppFeature {
    /// Stable id; also the UserDefaults key suffix. Never rename once shipped.
    pub key: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    /// Primary dev-escape-hatch env var; force-enables the feature when `== "1"`.
    pub env_override: Option<&'static str>,
    /// Older env spellings still honored (e.g. `SUPERCLI_DEV_PROFILES`).
    pub legacy_env_overrides: &'static [&'static str],
    pub default_on: bool,
    /// Still being shaped: listed under the Features tab's Experimental
    /// section instead of the shipped list.
    pub experimental: bool,
}

impl AppFeature {
    /// The `supercli.experimental.` prefix is the shipped spelling for every
    /// feature, graduated or not.
    pub fn defaults_key(&self) -> String {
        format!("supercli.experimental.{}", self.key)
    }

    /// All env vars that force-enable this feature when `== "1"`.
    pub fn env_overrides(&self) -> Vec<&'static str> {
        let mut vars: Vec<&'static str> = Vec::new();
        if let Some(primary) = self.env_override {
            vars.push(primary);
        }
        vars.extend(self.legacy_env_overrides.iter().copied());
        vars
    }

    /// Run sessions in isolated git worktrees so multiple agents can work the
    /// same repo in parallel. Gates the project-menu worktree controls, the
    /// inline worktree folder rows, and Settings ▸ Worktrees.
    pub const WORKTREES: AppFeature = AppFeature {
        key: "worktrees",
        title: "Git worktrees",
        summary: "Run sessions in an isolated git worktree of a project so multiple \
            agents can work the same repo in parallel without touching each other's \
            files. Adds worktree controls to the project menu, sidebar, and the \
            Worktrees settings tab.",
        env_override: Some("SUPERCLI_DEV_WORKTREES"),
        legacy_env_overrides: &[],
        default_on: true,
        experimental: false,
    };

    /// Sessions MCP: agent sessions can read other sessions and request write
    /// access to explicit targets. Gates the Settings ▸ Sessions use tab and
    /// whether new sessions launch with the MCP client injected.
    pub const SESSIONS_MCP: AppFeature = AppFeature {
        key: "sessionsMcp",
        title: "Sessions use",
        summary: "Let an agent session see your other sessions: it can read them all, \
            and asks before writing to another session unless you already approved \
            that pair. These are cooperation controls, not a sandbox \
            against commands running as your macOS user. Adds the Sessions settings \
            tab. Applies when a session starts, so already-running sessions pick it \
            up after a restart.",
        env_override: Some("SUPERCLI_DEV_SESSIONS_MCP"),
        legacy_env_overrides: &[],
        default_on: true,
        experimental: false,
    };

    /// Workspaces: use additional, fully isolated Supercli homes on this Mac
    /// (own sessions, projects, settings, and phone pairing identity).
    /// Gates the Settings ▸ Workspaces tab. The persisted key is deliberately
    /// still `profiles`: shipped experimental-feature keys are immutable.
    pub const WORKSPACES: AppFeature = AppFeature {
        key: "profiles",
        title: "Workspaces",
        summary: "Use extra, fully separate workspaces on this Mac — each \
            workspace has its own sessions, projects, presets, settings, and \
            pairs with your phone as its own workspace. Adds the Workspaces \
            settings tab.",
        env_override: Some("SUPERCLI_DEV_WORKSPACES"),
        legacy_env_overrides: &["SUPERCLI_DEV_PROFILES"],
        default_on: true,
        experimental: false,
    };

    /// Legacy preference identity retained for decoding saved settings.
    /// Retired in every build: [`is_available`] is always false for it.
    pub const COMPUTER_USE: AppFeature = AppFeature {
        key: "computerUse",
        title: "Computer use",
        summary: "Supercli computer use has been retired.",
        env_override: None,
        legacy_env_overrides: &[],
        default_on: false,
        experimental: true,
    };

    /// Browser MCP: agent sessions get an isolated real browser. Gates the
    /// Settings ▸ Browser tab and whether new sessions launch with the
    /// `browser` domain advertised. Still experimental (2026-09-08): the
    /// engine pin, login persistence, and takeover story are moving.
    pub const BROWSER_MCP: AppFeature = AppFeature {
        key: "browserMcp",
        title: "Browser use",
        summary: "Let agent sessions drive a real browser — open pages, click, \
            fill forms, and take screenshots. Each session gets its own \
            isolated browser with no access to your normal browser profile. Browser \
            access prompts are cooperation controls, not a sandbox against commands \
            running as your macOS user. Adds the Browser settings tab.",
        env_override: Some("SUPERCLI_DEV_BROWSER_MCP"),
        legacy_env_overrides: &[],
        default_on: true,
        experimental: true,
    };

    /// Remote workspaces in the released app (decided 2026-09-02): the Host
    /// picker, Share This Mac…, Add Workspace… ▸ Nearby/code and SSH. Direct is
    /// bearer-authenticated plaintext meant for LAN/VPN; Link carries the
    /// encrypted path off-network. Off hides the picker again at the next launch.
    pub const REMOTE_WORKSPACES: AppFeature = AppFeature {
        key: "remoteWorkspaces",
        title: "Remote workspaces",
        summary: "Add and control workspaces on other machines — pair another Mac, a \
            headless `supercli serve` box, or an SSH host — and share this Mac with \
            other devices. Direct connections are for your own network or VPN; \
            Supercli Link carries the encrypted path when you are away.",
        env_override: Some("SUPERCLI_DEV_REMOTE_WORKSPACES"),
        legacy_env_overrides: &[],
        default_on: true,
        experimental: false,
    };

    /// Everything shown in Settings ▸ Features, in display order (shipped
    /// features first; the panel then groups the experimental ones under
    /// their own section). Remote workspaces, Git worktrees, Sessions use,
    /// and Workspaces graduated on 2026-09-08; Browser use stays experimental.
    pub fn all() -> Vec<&'static AppFeature> {
        vec![
            &AppFeature::REMOTE_WORKSPACES,
            &AppFeature::WORKTREES,
            &AppFeature::SESSIONS_MCP,
            &AppFeature::WORKSPACES,
            &AppFeature::BROWSER_MCP,
        ]
    }
}

/// Whether this build offers a toggle for the feature. `computerUse` is
/// retired in every build (kept only for decoding old saved settings).
pub fn is_available(feature: &AppFeature) -> bool {
    feature.key != AppFeature::COMPUTER_USE.key
}

/// Every feature this build offers a toggle for, in display order.
pub fn available_features() -> Vec<&'static AppFeature> {
    AppFeature::all()
        .into_iter()
        .filter(|f| is_available(f))
        .collect()
}

/// The shipped features: the Features tab's plain list.
pub fn available_shipped_features() -> Vec<&'static AppFeature> {
    available_features()
        .into_iter()
        .filter(|f| !f.experimental)
        .collect()
}

/// The features still marked experimental: the tab's Experimental section.
pub fn available_experimental_features() -> Vec<&'static AppFeature> {
    available_features()
        .into_iter()
        .filter(|f| f.experimental)
        .collect()
}

/// Header copy for the Features tab's Experimental section, shared by the
/// local, per-workspace, and remote Host panels.
pub const EXPERIMENTAL_SECTION_DESCRIPTION: &str = "Early features that are still being shaped. \
    They can change or disappear between releases. Turn one off here if it gets in the way.";

/// Injectable persistence for feature-flag preferences.
///
/// Mirrors Swift's `AppDefaults.shared` (this workspace's own values) and
/// `UserDefaults.standard` (the default workspace's inherited values).
pub trait DefaultsStore {
    /// Returns the stored preference, or `None` when the workspace has no
    /// own value for the key.
    fn get_bool(&self, key: &str) -> Option<bool>;
    fn set_bool(&self, key: &str, value: bool);
    fn remove(&self, key: &str);
}

/// In-memory [`DefaultsStore`] for tests and non-native hosts.
#[derive(Debug, Default)]
pub struct MemoryDefaults {
    inner: Mutex<HashMap<String, bool>>,
}

impl MemoryDefaults {
    pub fn new() -> Self {
        MemoryDefaults {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl DefaultsStore for MemoryDefaults {
    fn get_bool(&self, key: &str) -> Option<bool> {
        self.inner.lock().ok()?.get(key).copied()
    }

    fn set_bool(&self, key: &str, value: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.insert(key.to_owned(), value);
        }
    }

    fn remove(&self, key: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.remove(key);
        }
    }
}

/// The resolution context for feature flags, mirroring Swift's
/// `SupercliFeatureFlags` namespace.
///
/// `own` is this workspace's preference store (`AppDefaults.shared`);
/// `standard` is the default workspace's store (`UserDefaults.standard`).
/// `env` maps an env var name to its value; production callers pass
/// `&|name| std::env::var(name).ok()`.
pub struct FeatureFlagContext<'a> {
    pub own: &'a dyn DefaultsStore,
    pub standard: &'a dyn DefaultsStore,
    /// Mirrors `SupercliWorkspaceContext.isDefaultInstance`.
    pub is_default_instance: bool,
    pub env: &'a dyn Fn(&str) -> Option<String>,
}

impl<'a> FeatureFlagContext<'a> {
    /// Whether a feature is currently enabled — env override first (dev
    /// escape hatch), then this workspace's own stored preference, then the
    /// default workspace's value, then the feature's built-in default.
    pub fn is_enabled(&self, feature: &AppFeature) -> bool {
        if !is_available(feature) {
            return false;
        }
        if feature
            .env_overrides()
            .iter()
            .any(|name| (self.env)(name).as_deref() == Some("1"))
        {
            return true;
        }
        let key = feature.defaults_key();
        if let Some(own_value) = self.own.get_bool(&key) {
            return own_value;
        }
        if !self.is_default_instance {
            if let Some(inherited) = self.standard.get_bool(&key) {
                return inherited;
            }
        }
        feature.default_on
    }

    /// Whether this workspace records its OWN value for the feature — the
    /// revert-to-default button's enablement.
    pub fn has_own_setting(&self, feature: &AppFeature) -> bool {
        self.own.get_bool(&feature.defaults_key()).is_some()
    }

    /// Decision 4's revert for feature flags: drop every own value so this
    /// workspace inherits the default workspace's flags again.
    pub fn revert_to_inherited_baseline(&self) {
        for feature in AppFeature::all() {
            self.own.remove(&feature.defaults_key());
        }
    }

    /// Persist a user preference for a feature. No-op for retired features.
    pub fn set_enabled(&self, enabled: bool, feature: &AppFeature) {
        if !is_available(feature) {
            return;
        }
        self.own.set_bool(&feature.defaults_key(), enabled);
    }

    /// Mobile remote control gate. Defaults to ON when nothing is stored.
    pub fn mobile_remote_control_enabled(&self) -> bool {
        if (self.env)("SUPERCLI_DEV_MOBILE_REMOTE").as_deref() == Some("1") {
            return true;
        }
        self.own
            .get_bool("supercli.dev.mobileRemoteControl")
            .unwrap_or(true)
    }

    /// Mac-as-client: connect this Supercli to another Supercli's remote
    /// server and attach to its sessions. Experimental; pairs with the
    /// Rust-side SUPERCLI_REMOTE_ATTACH=1 gate on the attach CLI.
    pub fn remote_supercli_client_enabled(&self) -> bool {
        if (self.env)("SUPERCLI_REMOTE_ATTACH").as_deref() == Some("1") {
            return true;
        }
        self.own
            .get_bool("supercli.dev.remoteSupercliClient")
            .unwrap_or(false)
    }
}

/// Desktop workspace switching is a local feature. Pure form keeps the
/// release boundary testable: Swift's computed `pickerEnabled` feeds
/// `SupercliFeatureFlags.isEnabled(.workspaces)` and
/// `RemoteHostFeature.pickerEnabled` into this.
pub fn workspace_picker_enabled(
    local_workspaces_enabled: bool,
    remote_host_picker_enabled: bool,
) -> bool {
    local_workspaces_enabled || remote_host_picker_enabled
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_with(vars: &[(&str, &str)]) -> HashMap<String, String> {
        vars.iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn empty_env() -> HashMap<String, String> {
        HashMap::new()
    }

    fn ctx<'a>(
        own: &'a dyn DefaultsStore,
        standard: &'a dyn DefaultsStore,
        is_default_instance: bool,
        env_fn: &'a dyn Fn(&str) -> Option<String>,
    ) -> FeatureFlagContext<'a> {
        FeatureFlagContext {
            own,
            standard,
            is_default_instance,
            env: env_fn,
        }
    }

    #[test]
    fn defaults_key_uses_shipped_prefix() {
        assert_eq!(
            AppFeature::WORKTREES.defaults_key(),
            "supercli.experimental.worktrees"
        );
        assert_eq!(
            AppFeature::SESSIONS_MCP.defaults_key(),
            "supercli.experimental.sessionsMcp"
        );
    }

    #[test]
    fn workspaces_persisted_key_is_immutable_profiles() {
        // The persisted key is deliberately still `profiles`: shipped
        // experimental-feature keys are immutable (see Swift source).
        assert_eq!(AppFeature::WORKSPACES.key, "profiles");
        assert_eq!(
            AppFeature::WORKSPACES.defaults_key(),
            "supercli.experimental.profiles"
        );
    }

    #[test]
    fn env_overrides_combine_primary_and_legacy() {
        assert_eq!(
            AppFeature::WORKSPACES.env_overrides(),
            vec!["SUPERCLI_DEV_WORKSPACES", "SUPERCLI_DEV_PROFILES"]
        );
        assert_eq!(
            AppFeature::WORKTREES.env_overrides(),
            vec!["SUPERCLI_DEV_WORKTREES"]
        );
        assert!(AppFeature::COMPUTER_USE.env_overrides().is_empty());
    }

    #[test]
    fn all_in_display_order() {
        let keys: Vec<&str> = AppFeature::all().iter().map(|f| f.key).collect();
        assert_eq!(
            keys,
            vec![
                "remoteWorkspaces",
                "worktrees",
                "sessionsMcp",
                "profiles",
                "browserMcp"
            ]
        );
    }

    #[test]
    fn computer_use_never_available() {
        assert!(!is_available(&AppFeature::COMPUTER_USE));
        for feature in AppFeature::all() {
            assert!(is_available(feature), "{}", feature.key);
        }
    }

    #[test]
    fn available_features_excludes_retired() {
        assert_eq!(available_features().len(), 5);
        assert!(!available_features().iter().any(|f| f.key == "computerUse"));
    }

    #[test]
    fn shipped_and_experimental_split() {
        let shipped: Vec<&str> = available_shipped_features().iter().map(|f| f.key).collect();
        let experimental: Vec<&str> = available_experimental_features()
            .iter()
            .map(|f| f.key)
            .collect();
        assert_eq!(
            shipped,
            vec!["remoteWorkspaces", "worktrees", "sessionsMcp", "profiles"]
        );
        assert_eq!(experimental, vec!["browserMcp"]);
    }

    #[test]
    fn env_override_force_enables() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = env_with(&[("SUPERCLI_DEV_WORKTREES", "1")]);
        // Even with an explicit stored OFF, the env escape hatch wins.
        own.set_bool(&AppFeature::WORKTREES.defaults_key(), false);
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        assert!(c.is_enabled(&AppFeature::WORKTREES));
    }

    #[test]
    fn legacy_env_override_force_enables() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = env_with(&[("SUPERCLI_DEV_PROFILES", "1")]);
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        assert!(c.is_enabled(&AppFeature::WORKSPACES));
    }

    #[test]
    fn env_value_other_than_one_does_not_force() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = env_with(&[("SUPERCLI_DEV_WORKTREES", "0")]);
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        // defaultOn is true, but the point is the env value "0" is not an override.
        own.set_bool(&AppFeature::WORKTREES.defaults_key(), false);
        assert!(!c.is_enabled(&AppFeature::WORKTREES));
    }

    #[test]
    fn own_setting_wins_over_default() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = empty_env();
        own.set_bool(&AppFeature::WORKTREES.defaults_key(), false);
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        assert!(!c.is_enabled(&AppFeature::WORKTREES));
    }

    #[test]
    fn non_default_instance_inherits_standard() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = empty_env();
        standard.set_bool(&AppFeature::WORKTREES.defaults_key(), false);
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, false, &env_lookup);
        assert!(!c.is_enabled(&AppFeature::WORKTREES));
    }

    #[test]
    fn own_setting_wins_over_inherited() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = empty_env();
        standard.set_bool(&AppFeature::WORKTREES.defaults_key(), false);
        own.set_bool(&AppFeature::WORKTREES.defaults_key(), true);
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, false, &env_lookup);
        assert!(c.is_enabled(&AppFeature::WORKTREES));
    }

    #[test]
    fn default_instance_ignores_standard_store() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = empty_env();
        standard.set_bool(&AppFeature::WORKTREES.defaults_key(), false);
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        assert!(c.is_enabled(&AppFeature::WORKTREES));
    }

    #[test]
    fn falls_back_to_builtin_default() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = empty_env();
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        // All shipped features default on; the retired one is never available.
        for feature in AppFeature::all() {
            assert!(c.is_enabled(feature), "{}", feature.key);
        }
        assert!(!c.is_enabled(&AppFeature::COMPUTER_USE));
    }

    #[test]
    fn retired_feature_never_enabled_even_with_env() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = env_with(&[("SUPERCLI_DEV_ANYTHING", "1")]);
        own.set_bool(&AppFeature::COMPUTER_USE.defaults_key(), true);
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        assert!(!c.is_enabled(&AppFeature::COMPUTER_USE));
    }

    #[test]
    fn has_own_setting_tracks_stored_values() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = empty_env();
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        assert!(!c.has_own_setting(&AppFeature::WORKTREES));
        own.set_bool(&AppFeature::WORKTREES.defaults_key(), true);
        assert!(c.has_own_setting(&AppFeature::WORKTREES));
    }

    #[test]
    fn set_enabled_persists_and_reverts() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = empty_env();
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        c.set_enabled(false, &AppFeature::WORKTREES);
        assert!(c.has_own_setting(&AppFeature::WORKTREES));
        assert!(!c.is_enabled(&AppFeature::WORKTREES));
        c.revert_to_inherited_baseline();
        assert!(!c.has_own_setting(&AppFeature::WORKTREES));
        // Back to the built-in default after revert.
        assert!(c.is_enabled(&AppFeature::WORKTREES));
    }

    #[test]
    fn set_enabled_ignores_retired_feature() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = empty_env();
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        c.set_enabled(true, &AppFeature::COMPUTER_USE);
        assert!(!c.has_own_setting(&AppFeature::COMPUTER_USE));
    }

    #[test]
    fn mobile_remote_control_defaults_on_when_unset() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = empty_env();
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        assert!(c.mobile_remote_control_enabled());
    }

    #[test]
    fn mobile_remote_control_env_forces_on() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        own.set_bool("supercli.dev.mobileRemoteControl", false);
        let env = env_with(&[("SUPERCLI_DEV_MOBILE_REMOTE", "1")]);
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        assert!(c.mobile_remote_control_enabled());
    }

    #[test]
    fn mobile_remote_control_respects_stored_off() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        own.set_bool("supercli.dev.mobileRemoteControl", false);
        let env = empty_env();
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        assert!(!c.mobile_remote_control_enabled());
    }

    #[test]
    fn remote_supercli_client_defaults_off() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = empty_env();
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        assert!(!c.remote_supercli_client_enabled());
        own.set_bool("supercli.dev.remoteSupercliClient", true);
        assert!(c.remote_supercli_client_enabled());
    }

    #[test]
    fn remote_supercli_client_env_forces_on() {
        let own = MemoryDefaults::new();
        let standard = MemoryDefaults::new();
        let env = env_with(&[("SUPERCLI_REMOTE_ATTACH", "1")]);
        let env_lookup = |name: &str| env.get(name).cloned();
        let c = ctx(&own, &standard, true, &env_lookup);
        assert!(c.remote_supercli_client_enabled());
    }

    #[test]
    fn workspace_picker_enabled_is_either_surface() {
        assert!(workspace_picker_enabled(true, false));
        assert!(workspace_picker_enabled(false, true));
        assert!(workspace_picker_enabled(true, true));
        assert!(!workspace_picker_enabled(false, false));
    }

    #[test]
    fn experimental_section_copy_present() {
        assert!(EXPERIMENTAL_SECTION_DESCRIPTION.contains("still being shaped"));
    }
}
