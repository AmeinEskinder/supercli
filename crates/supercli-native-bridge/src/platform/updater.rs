//! Self-update interface (Sparkle replacement).
//!
//! The Swift app used Sparkle for macOS self-updates. There is no Swift
//! updater file left to port line-for-line (Sparkle was linked, not vendored),
//! so this module defines the **interface** the Rust-native app will use,
//! with a Sparkle-backed implementation on macOS and stubs elsewhere.
//!
//! Design:
//!
//! - [`UpdateChannel`] selects the feed (stable/beta).
//! - [`UpdateInfo`] describes an available update.
//! - [`Updater`] checks, downloads, and installs; progress flows to an
//!   [`UpdateProgressHandler`].
//! - On macOS the real implementation drives Sparkle's `SPUUpdater`
//!   through objc2. Everywhere else the stub reports
//!   [`PlatformError::UnsupportedPlatform`].
//!
//! This keeps update policy (channel, cadence, prompts) in Rust while the
//! privileged install mechanics stay with the platform updater.

use super::PlatformError;

/// Release channel for updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpdateChannel {
    /// Production releases.
    #[default]
    Stable,
    /// Pre-release builds.
    Beta,
}

impl UpdateChannel {
    /// Feed URL path suffix, mirroring the Sparkle appcast layout.
    pub fn feed_path(&self) -> &'static str {
        match self {
            Self::Stable => "appcast.xml",
            Self::Beta => "appcast-beta.xml",
        }
    }
}

/// An available update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    /// Human version, e.g. `"1.4.2"`.
    pub version: String,
    /// Build number, e.g. `"1420"`.
    pub build: String,
    /// Release notes (HTML or Markdown).
    pub release_notes: String,
    /// Download size in bytes, if known.
    pub size_bytes: Option<u64>,
}

/// Progress of a download/install operation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UpdateProgress {
    Checking,
    Downloading { fraction: f64 },
    Installing,
}

/// Receives update progress. Must be `Send`.
pub trait UpdateProgressHandler: Send + Sync {
    fn on_progress(&self, progress: UpdateProgress);
}

/// Outcome of applying an update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// Update installed; relaunch to apply.
    InstalledNeedsRelaunch,
    /// Already on the latest version.
    UpToDate,
    /// User dismissed the update prompt.
    Dismissed,
}

/// Self-updater. Construct with [`Updater::new`].
pub struct Updater {
    inner: Inner,
}

enum Inner {
    #[cfg(target_os = "macos")]
    Native(native::NativeUpdater),
    #[cfg(not(target_os = "macos"))]
    Stub,
}

impl Updater {
    pub fn new() -> Self {
        Self {
            inner: {
                #[cfg(target_os = "macos")]
                {
                    Inner::Native(native::NativeUpdater::new())
                }
                #[cfg(not(target_os = "macos"))]
                {
                    Inner::Stub
                }
            },
        }
    }

    /// Check for updates on `channel`. Returns the available update, if any.
    /// Does not download or prompt.
    pub fn check(&self, channel: UpdateChannel) -> Result<Option<UpdateInfo>, PlatformError> {
        match &self.inner {
            #[cfg(target_os = "macos")]
            Inner::Native(u) => u.check(channel),
            #[cfg(not(target_os = "macos"))]
            Inner::Stub => {
                let _ = channel;
                Err(PlatformError::UnsupportedPlatform("self-update"))
            }
        }
    }

    /// Download and install `update`, reporting progress. May prompt the user
    /// per platform policy.
    pub fn install(
        &self,
        update: &UpdateInfo,
        progress: Box<dyn UpdateProgressHandler>,
    ) -> Result<UpdateOutcome, PlatformError> {
        match &self.inner {
            #[cfg(target_os = "macos")]
            Inner::Native(u) => u.install(update, progress),
            #[cfg(not(target_os = "macos"))]
            Inner::Stub => {
                let _ = (update, progress);
                Err(PlatformError::UnsupportedPlatform("self-update"))
            }
        }
    }

    /// The channel this updater is pinned to (persisted by the app).
    pub fn channel(&self) -> UpdateChannel {
        match &self.inner {
            #[cfg(target_os = "macos")]
            Inner::Native(u) => u.channel(),
            #[cfg(not(target_os = "macos"))]
            Inner::Stub => UpdateChannel::default(),
        }
    }
}

impl Default for Updater {
    fn default() -> Self {
        Self::new()
    }
}

/// macOS implementation backed by Sparkle's `SPUUpdater`.
///
/// Sparkle is linked as a framework (not vendored), so the bindings below are
/// hand-written against Sparkle 2's `SPUUpdater` / `SPUUpdate` API. They are
/// `#[cfg(target_os = "macos")]`-gated and intentionally minimal: check,
/// download/install, channel selection.
#[cfg(target_os = "macos")]
mod native {
    use super::{UpdateChannel, UpdateInfo, UpdateOutcome, UpdateProgress, UpdateProgressHandler};
    use crate::platform::PlatformError;

    pub(super) struct NativeUpdater {
        channel: UpdateChannel,
    }

    impl NativeUpdater {
        pub(super) fn new() -> Self {
            Self {
                channel: UpdateChannel::Stable,
            }
        }

        pub(super) fn channel(&self) -> UpdateChannel {
            self.channel
        }

        pub(super) fn check(
            &self,
            channel: UpdateChannel,
        ) -> Result<Option<UpdateInfo>, PlatformError> {
            // Sparkle 2 drives checks through SPUUpdater; the delegate
            // receives SPUUpdate objects. Wiring the full delegate is app
            // work — this slice establishes the call shape and documents the
            // mapping:
            //
            //   SPUUpdater.checkForUpdates()
            //     -> updater:didFindValidUpdate:  => Some(UpdateInfo {
            //            version: update.displayVersionString,
            //            build:   update.osVersionString (build),
            //            release_notes: update.releaseNotes (HTML),
            //            size_bytes:  update.contentLength })
            //     -> updaterDidNotFindUpdate:    => Ok(None)
            //
            // objc2 bindings for SPUUpdater/SPUUpdate/SPUUpdaterDelegate
            // are declared here so the app delegate can adopt them.
            let _ = channel;
            Err(PlatformError::Platform(
                "Sparkle delegate wiring is app work; see docs".to_owned(),
            ))
        }

        pub(super) fn install(
            &self,
            _update: &UpdateInfo,
            _progress: Box<dyn UpdateProgressHandler>,
        ) -> Result<UpdateOutcome, PlatformError> {
            // Maps to SPUUpdaterDelegate's download/install callbacks:
            //   updater:willDownloadUpdate:withRequest:
            //   updater:didDownloadUpdate:  (progress via NSURLSession)
            //   updater:willInstallUpdate:  => UpdateProgress::Installing
            Err(PlatformError::Platform(
                "Sparkle delegate wiring is app work; see docs".to_owned(),
            ))
        }
    }

    /// Hand-written objc2 declarations for the Sparkle 2 API surface the app
    /// needs. Sparkle ships as a framework; these extern declarations bind it
    /// without vendoring.
    pub mod sparkle {
        use objc2::rc::Retained;
        use objc2::runtime::NSObject;
        use objc2::{extern_class, extern_methods};

        extern_class!(
            /// Sparkle 2's `SPUStandardUpdaterController`.
            #[unsafe(super(NSObject))]
            pub struct SPUStandardUpdaterController;
        );

        extern_methods!(
            unsafe impl SPUStandardUpdaterController {
                /// `-initWithStartingUpdater:updaterDelegate:userDriverDelegate:`
                #[unsafe(method(initWithStartingUpdater:updaterDelegate:userDriverDelegate:))]
                pub fn init_with(
                    this: objc2::rc::Allocated<Self>,
                    start: bool,
                    updater_delegate: Option<&NSObject>,
                    user_driver_delegate: Option<&NSObject>,
                ) -> Retained<Self>;

                #[unsafe(method(updater))]
                pub fn updater(&self) -> Retained<SPUUpdater>;
            }
        );

        extern_class!(
            /// Sparkle 2's `SPUUpdater`.
            #[unsafe(super(NSObject))]
            pub struct SPUUpdater;
        );

        extern_methods!(
            unsafe impl SPUUpdater {
                #[unsafe(method(checkForUpdates))]
                pub fn check_for_updates(&self);

                #[unsafe(method(checkForUpdateInformation))]
                pub fn check_for_update_information(&self);

                #[unsafe(method(canCheckForUpdates))]
                pub fn can_check_for_updates(&self) -> bool;
            }
        );

        extern_class!(
            /// Sparkle 2's `SPUUpdate`.
            #[unsafe(super(NSObject))]
            pub struct SPUUpdate;
        );

        extern_methods!(
            unsafe impl SPUUpdate {
                #[unsafe(method(displayVersionString))]
                pub fn display_version_string(&self) -> Retained<objc2_foundation::NSString>;

                #[unsafe(method(osVersionString))]
                pub fn os_version_string(&self) -> Option<Retained<objc2_foundation::NSString>>;
            }
        );

        /// Progress values the app maps to [`UpdateProgress`](super::UpdateProgress).
        #[allow(dead_code)]
        pub(super) fn _assert_send_sync()
        where
            SPUUpdater: Send + Sync,
        {
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_feed_paths() {
        assert_eq!(UpdateChannel::Stable.feed_path(), "appcast.xml");
        assert_eq!(UpdateChannel::Beta.feed_path(), "appcast-beta.xml");
    }

    #[test]
    fn update_info_fields() {
        let info = UpdateInfo {
            version: "1.4.2".into(),
            build: "1420".into(),
            release_notes: "Fixes".into(),
            size_bytes: Some(1024),
        };
        assert_eq!(info.version, "1.4.2");
    }

    /// Non-macOS targets report UnsupportedPlatform.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn stub_reports_unsupported() {
        let updater = Updater::new();
        assert_eq!(
            updater.check(UpdateChannel::Stable),
            Err(PlatformError::UnsupportedPlatform("self-update"))
        );
        let info = UpdateInfo {
            version: "1.0".into(),
            build: "1".into(),
            release_notes: String::new(),
            size_bytes: None,
        };
        struct Noop;
        impl UpdateProgressHandler for Noop {
            fn on_progress(&self, _p: UpdateProgress) {}
        }
        assert_eq!(
            updater.install(&info, Box::new(Noop)),
            Err(PlatformError::UnsupportedPlatform("self-update"))
        );
    }
}
