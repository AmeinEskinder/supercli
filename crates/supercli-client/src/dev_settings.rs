//! Port of `DevSettings.swift` (SupercliIOS).
//!
//! Debug/developer toggles, persisted to the platform defaults store. The
//! Swift source surfaces these in the "Your Mac" sheet's Developer section
//! (DEBUG builds only); the logic ported here is the toggle state plus its
//! persistence key, behind a tiny key-value store trait so it stays
//! unit-testable without UserDefaults.
//!
//! Ported from
//! `clients/legacy/ios/SupercliIOS/Sources/SupercliIOS/DevSettings.swift`.

/// Defaults key for the terminal-bounds debug outline.
/// Mirrors `DevSettings.boundsKey`.
pub const SHOW_TERMINAL_BOUNDS_KEY: &str = "supercli.dev.showTerminalBounds";

/// Minimal bool key-value store (UserDefaults on Apple platforms).
/// Mirrors the `UserDefaults.standard.bool(forKey:)` / `set(_:forKey:)` use
/// in `DevSettings`.
pub trait DevSettingsStore {
    fn get_bool(&self, key: &str) -> bool;
    fn set_bool(&mut self, key: &str, value: bool);
}

/// Developer toggles. Mirrors `DevSettings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DevSettings {
    /// Draw a red outline around the terminal grid to inspect its real bounds.
    /// Mirrors `DevSettings.showTerminalBounds`.
    pub show_terminal_bounds: bool,
}

impl DevSettings {
    /// Load the toggles from the store. Mirrors `DevSettings.init()`.
    pub fn load(store: &impl DevSettingsStore) -> Self {
        Self {
            show_terminal_bounds: store.get_bool(SHOW_TERMINAL_BOUNDS_KEY),
        }
    }

    /// Apply one toggle change and persist it, like the `didSet` observer on
    /// `showTerminalBounds`.
    pub fn set_show_terminal_bounds(&mut self, value: bool, store: &mut impl DevSettingsStore) {
        self.show_terminal_bounds = value;
        store.set_bool(SHOW_TERMINAL_BOUNDS_KEY, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemStore(HashMap<String, bool>);

    impl DevSettingsStore for MemStore {
        fn get_bool(&self, key: &str) -> bool {
            self.0.get(key).copied().unwrap_or(false)
        }

        fn set_bool(&mut self, key: &str, value: bool) {
            self.0.insert(key.to_string(), value);
        }
    }

    #[test]
    fn defaults_to_off() {
        let store = MemStore::default();
        assert_eq!(DevSettings::load(&store), DevSettings::default());
        assert!(!DevSettings::default().show_terminal_bounds);
    }

    #[test]
    fn toggle_persists_to_store() {
        let mut store = MemStore::default();
        let mut settings = DevSettings::load(&store);
        settings.set_show_terminal_bounds(true, &mut store);
        assert!(settings.show_terminal_bounds);
        assert!(store.get_bool(SHOW_TERMINAL_BOUNDS_KEY));
        // A reload sees the persisted value.
        assert!(DevSettings::load(&store).show_terminal_bounds);
    }

    #[test]
    fn toggle_off_clears_persisted_value() {
        let mut store = MemStore::default();
        let mut settings = DevSettings::load(&store);
        settings.set_show_terminal_bounds(true, &mut store);
        settings.set_show_terminal_bounds(false, &mut store);
        assert!(!DevSettings::load(&store).show_terminal_bounds);
    }

    #[test]
    fn uses_stable_defaults_key() {
        assert_eq!(SHOW_TERMINAL_BOUNDS_KEY, "supercli.dev.showTerminalBounds");
    }
}
