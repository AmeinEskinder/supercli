//! macOS services ported from `clients/legacy/native/SupercliNative`.
//!
//! Each module ports one Swift file's behavior (not its UI):
//!
//! | Module | Swift source |
//! |---|---|
//! | `launchd` | `HostServiceAgent.swift` — launchd plist + launchctl |
//! | `service_identity` | `HostServiceIdentity.swift` — version skew detection |
//! | `service_manager` | `HostServiceManager.swift` — service lifecycle |
//! | `keychain` | `Licensing/LicenseKeychain.swift` — Keychain storage |
//! | `license` | `Licensing/LicenseManager.swift` — license verify/activate |
//! | `notifications` | `DesktopNotifier.swift` — notification payloads |
//! | `menu_bar` | `MenuBarController.swift` — menu-bar state machine |
//! | `hook_server` | `HookServer.swift` — parsing + protocol types |
//! | `updater` | `AppDelegate.swift` (Sparkle) — signed updater |
//! | `local_host_control` | `LocalHostControl.swift` — bridge client types |
//!
//! Pure logic is cross-platform and tested everywhere. Actual macOS API
//! calls (objc2) are `#[cfg(target_os = "macos")]` gated.

pub mod hook_server;
pub mod keychain;
pub mod launchd;
pub mod license;
pub mod local_host_control;
pub mod menu_bar;
pub mod notifications;
pub mod service_identity;
pub mod service_manager;
pub mod updater;
