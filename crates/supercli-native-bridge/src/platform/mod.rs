//! Platform-native glue for the Supercli desktop (macOS) and mobile (iOS) apps.
//!
//! This module is the Rust replacement for the 9 Swift "platform glue" files
//! identified in `docs/swift-to-rust-parity.md`:
//!
//! | Swift file | This module |
//! |---|---|
//! | `AppDelegate.swift` (macOS) | [`app_lifecycle`] |
//! | `UnpeelIOSApp.swift` (iOS) | [`app_lifecycle`] |
//! | `main.swift` (macOS entry) | [`app_lifecycle`] |
//! | `DesktopNotifier.swift` | [`notifications`] |
//! | `PushManager.swift` (iOS) | [`push`] |
//! | `DictationReflection.swift`, `VoiceDictationController.swift` | [`speech`] |
//! | `MenuBarController.swift` | [`menu_bar`] |
//! | `LicenseKeychain.swift` | **not here** — reuses `supercli-connector`'s keychain |
//!
//! # Platform gating
//!
//! Every submodule compiles on all targets:
//!
//! - On Apple targets (`target_vendor = "apple"`) the real objc2 bindings run.
//! - Everywhere else (Linux CI, etc.) stub implementations are compiled that
//!   return [`PlatformError::UnsupportedPlatform`]. They exist so the rest of
//!   the workspace — and Linux CI — keeps building and testing the
//!   platform-agnostic logic.
//!
//! macOS-only APIs (`menu_bar`, `file_picker`) are further gated on
//! `target_os = "macos"`; iOS-only APIs (`push`) on `target_os = "ios"`.
//!
//! # Keychain
//!
//! `LicenseKeychain.swift` is intentionally **not** reimplemented here.
//! `supercli-connector` already owns the keychain abstraction; callers should
//! use that instead of duplicating Security-framework bindings.

pub mod app_lifecycle;
pub mod file_picker;
pub mod license;
pub mod menu_bar;
pub mod notifications;
pub mod push;
pub mod speech;
pub mod updater;

use std::fmt;

/// Error type for all platform-glue operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformError {
    /// The operation is not available on this OS (e.g. called on Linux).
    UnsupportedPlatform(&'static str),
    /// The user denied the permission (notifications, speech, etc.).
    PermissionDenied(&'static str),
    /// The underlying platform API returned an error.
    Platform(String),
    /// Invalid input passed to a platform API.
    InvalidInput(String),
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform(op) => {
                write!(f, "{op} is not supported on this platform")
            }
            Self::PermissionDenied(what) => write!(f, "permission denied: {what}"),
            Self::Platform(msg) => write!(f, "platform error: {msg}"),
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
        }
    }
}

impl std::error::Error for PlatformError {}

/// Whether the current build has real platform bindings or stubs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformBackend {
    /// Real objc2 bindings (Apple targets).
    Native,
    /// Compile-only stubs (Linux and other non-Apple targets).
    Stub,
}

/// Which backend this build uses.
pub const PLATFORM_BACKEND: PlatformBackend = {
    #[cfg(target_vendor = "apple")]
    {
        PlatformBackend::Native
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        PlatformBackend::Stub
    }
};

/// `true` when real platform bindings are compiled in.
pub fn has_native_platform() -> bool {
    matches!(PLATFORM_BACKEND, PlatformBackend::Native)
}
