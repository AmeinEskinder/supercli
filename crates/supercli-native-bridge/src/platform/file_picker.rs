//! File pickers via `NSOpenPanel` / `NSSavePanel`.
//!
//! macOS-only. The Swift app used `NSOpenPanel` for workspace selection,
//! attachment picks, and export destinations. This module exposes a
//! platform-agnostic synchronous picker; on non-macOS targets it reports
//! [`PlatformError::UnsupportedPlatform`].
//!
//! Panels run app-modal (`runModal`), matching the Swift usage.

use super::PlatformError;
use std::path::PathBuf;

/// What the picker should select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PickMode {
    /// Pick a single file.
    #[default]
    File,
    /// Pick a single directory.
    Directory,
    /// Pick multiple files.
    MultipleFiles,
}

/// Options for the open panel.
#[derive(Debug, Clone, Default)]
pub struct PickOptions {
    pub mode: PickMode,
    /// Dialog title.
    pub title: Option<String>,
    /// Prompt button label (e.g. `"Choose"`).
    pub prompt: Option<String>,
    /// Allowed file extensions without the dot (e.g. `["png", "jpg"]`).
    /// Empty means all files.
    pub allowed_extensions: Vec<String>,
    /// Initial directory.
    pub directory: Option<PathBuf>,
}

/// Outcome of a pick operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickOutcome {
    /// The user picked these paths.
    Picked(Vec<PathBuf>),
    /// The user cancelled.
    Cancelled,
}

/// Show an open panel and return the outcome.
pub fn pick(options: &PickOptions) -> Result<PickOutcome, PlatformError> {
    #[cfg(target_os = "macos")]
    {
        native::pick(options)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = options;
        Err(PlatformError::UnsupportedPlatform("file picker"))
    }
}

/// Show a save panel and return the chosen path, if any.
pub fn pick_save(
    title: Option<&str>,
    suggested_name: Option<&str>,
    allowed_extensions: &[String],
) -> Result<Option<PathBuf>, PlatformError> {
    #[cfg(target_os = "macos")]
    {
        native::pick_save(title, suggested_name, allowed_extensions)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (title, suggested_name, allowed_extensions);
        Err(PlatformError::UnsupportedPlatform("file picker"))
    }
}

#[cfg(target_os = "macos")]
mod native {
    //! objc2 bindings for `NSOpenPanel` / `NSSavePanel`.

    use super::{PickMode, PickOptions, PickOutcome};
    use crate::platform::PlatformError;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSModalResponse, NSOpenPanel, NSSavePanel};
    use objc2_foundation::{NSString, NSURL};
    use std::path::PathBuf;

    fn ns(s: &str) -> Retained<NSString> {
        NSString::from_str(s)
    }

    fn url_to_path(url: &NSURL) -> Option<PathBuf> {
        url.path().map(|p| PathBuf::from(p.to_string()))
    }

    pub(super) fn pick(options: &PickOptions) -> Result<PickOutcome, PlatformError> {
        // NSOpenPanel must run on the main thread; callers are responsible
        // (the Swift original was @MainActor for the same reason).
        let panel = NSOpenPanel::openPanel();
        panel.setCanChooseFiles(matches!(
            options.mode,
            PickMode::File | PickMode::MultipleFiles
        ));
        panel.setCanChooseDirectories(matches!(options.mode, PickMode::Directory));
        panel.setAllowsMultipleSelection(matches!(options.mode, PickMode::MultipleFiles));
        if let Some(title) = &options.title {
            panel.setTitle(&ns(title));
        }
        if let Some(prompt) = &options.prompt {
            panel.setPrompt(&ns(prompt));
        }
        if !options.allowed_extensions.is_empty() {
            let types: Vec<Retained<NSString>> =
                options.allowed_extensions.iter().map(|e| ns(e)).collect();
            let array = objc2_foundation::NSArray::from_slice(&types);
            panel.setAllowedFileTypes(Some(&array));
        }
        if let Some(dir) = &options.directory {
            if let Some(url) = NSURL::fileURLWithPath(&ns(&dir.to_string_lossy())) {
                panel.setDirectoryURL(Some(&url));
            }
        }
        let response = panel.runModal();
        if response != NSModalResponse::OK {
            return Ok(PickOutcome::Cancelled);
        }
        let urls = panel.URLs();
        let mut paths = Vec::new();
        for url in urls.iter() {
            if let Some(path) = url_to_path(url) {
                paths.push(path);
            }
        }
        Ok(PickOutcome::Picked(paths))
    }

    pub(super) fn pick_save(
        title: Option<&str>,
        suggested_name: Option<&str>,
        allowed_extensions: &[String],
    ) -> Result<Option<PathBuf>, PlatformError> {
        let panel = NSSavePanel::savePanel();
        if let Some(title) = title {
            panel.setTitle(&ns(title));
        }
        if let Some(name) = suggested_name {
            panel.setNameFieldStringValue(&ns(name));
        }
        if !allowed_extensions.is_empty() {
            let types: Vec<Retained<NSString>> = allowed_extensions.iter().map(|e| ns(e)).collect();
            let array = objc2_foundation::NSArray::from_slice(&types);
            panel.setAllowedFileTypes(Some(&array));
        }
        let response = panel.runModal();
        if response != NSModalResponse::OK {
            return Ok(None);
        }
        Ok(panel.URL().and_then(|url| url_to_path(&url)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_options_default() {
        let opts = PickOptions::default();
        assert_eq!(opts.mode, PickMode::File);
        assert!(opts.allowed_extensions.is_empty());
    }

    /// Non-macOS targets report UnsupportedPlatform.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn stub_reports_unsupported() {
        let opts = PickOptions::default();
        assert_eq!(
            pick(&opts),
            Err(PlatformError::UnsupportedPlatform("file picker"))
        );
        assert_eq!(
            pick_save(None, None, &[]),
            Err(PlatformError::UnsupportedPlatform("file picker"))
        );
    }
}
