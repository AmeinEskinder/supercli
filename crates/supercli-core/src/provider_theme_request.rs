//! Port of `ProviderThemeReadRequest.swift` (SupercliNative).
//!
//! A single provider-theme read for a session: the session identity, the
//! sessions directory, the launch command, and the optional working
//! directory. `read()` resolves the provider background (OpenCode / Grok)
//! and samples the canvas's dominant background from the session's
//! `output.bin` tail; `matches()` is the pure identity check the caller
//! uses to decide whether a cached read is still valid.
//!
//! Ported from
//! `clients/legacy/native/SupercliNative/Sources/SupercliNative/ProviderThemeReadRequest.swift`.

use std::path::{Path, PathBuf};

use super::provider_theme::{
    dominant_background_in_data, provider_background, resolve_frame_background,
    ResolvedFrameBackground, ThemeBackground, SAMPLER_SAMPLE_BYTES,
};

/// One provider-theme read request for a session.
/// Mirrors `ProviderThemeReadRequest`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderThemeReadRequest {
    pub session_id: String,
    /// Stable session identity (a UUID in the Swift source), used by `matches`.
    pub identity: String,
    pub sessions_dir: PathBuf,
    pub command: String,
    pub working_directory: Option<String>,
}

/// The outcome of `ProviderThemeReadRequest.read`.
/// Mirrors `ProviderThemeReadRequest.Result`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderThemeReadResult {
    pub background: Option<ThemeBackground>,
    /// Dominant canvas color as 0xRRGGBB.
    pub canvas: Option<u32>,
}

impl ProviderThemeReadRequest {
    /// Resolve the provider background and the canvas dominant color.
    /// Mirrors `ProviderThemeReadRequest.read()`.
    pub fn read(&self) -> ProviderThemeReadResult {
        ProviderThemeReadResult {
            background: provider_background(&self.command, self.working_directory.as_deref()),
            canvas: dominant_canvas_background(&self.sessions_dir, &self.session_id),
        }
    }

    /// Whether this request still describes the given session parameters.
    /// Mirrors
    /// `ProviderThemeReadRequest.matches(identity:sessionsDir:command:workingDirectory:)`.
    pub fn matches(
        &self,
        identity: &str,
        sessions_dir: &Path,
        command: &str,
        working_directory: Option<&str>,
    ) -> bool {
        self.identity == identity
            && self.sessions_dir == sessions_dir
            && self.command == command
            && self.working_directory.as_deref() == working_directory
    }
}

impl ProviderThemeReadResult {
    /// Resolve the terminal frame background for this read, with the live
    /// canvas sample (if any) winning over the config-derived background.
    /// Mirrors `TerminalFrameStyle.resolved(command:workingDirectory:canvasOverride:)`
    /// as called from `SurfaceCache` with `result.canvas`.
    pub fn resolve_frame(
        &self,
        command: &str,
        working_directory: Option<&str>,
    ) -> ResolvedFrameBackground {
        resolve_frame_background(command, working_directory, self.canvas)
    }
}

/// Mirrors `ProviderCanvasSampler.dominantBackground(sessionID:sessionsDir:)`:
/// sample the tail of `<sessionsDir>/<sessionID>/output.bin`.
///
/// The journal is append-only and can be hundreds of MB, so only the last
/// [`SAMPLER_SAMPLE_BYTES`] are read (seek from the end, no full read).
fn dominant_canvas_background(sessions_dir: &Path, session_id: &str) -> Option<u32> {
    use std::io::{Read, Seek, SeekFrom};

    let path = sessions_dir.join(session_id).join("output.bin");
    let mut f = std::fs::File::open(&path).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(SAMPLER_SAMPLE_BYTES as u64);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut tail = Vec::new();
    f.read_to_end(&mut tail).ok()?;
    if tail.is_empty() {
        return None;
    }
    dominant_background_in_data(&tail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn request(sessions_dir: &Path) -> ProviderThemeReadRequest {
        ProviderThemeReadRequest {
            session_id: "s1".to_string(),
            identity: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            sessions_dir: sessions_dir.to_path_buf(),
            command: "grok --light".to_string(),
            working_directory: None,
        }
    }

    #[test]
    fn matches_accepts_identical_parameters() {
        let r = request(Path::new("/tmp/sessions"));
        assert!(r.matches(
            "550e8400-e29b-41d4-a716-446655440000",
            Path::new("/tmp/sessions"),
            "grok --light",
            None,
        ));
    }

    #[test]
    fn matches_rejects_any_difference() {
        let r = request(Path::new("/tmp/sessions"));
        let id = "550e8400-e29b-41d4-a716-446655440000";
        assert!(!r.matches(
            "other-identity",
            Path::new("/tmp/sessions"),
            "grok --light",
            None
        ));
        assert!(!r.matches(id, Path::new("/other"), "grok --light", None));
        assert!(!r.matches(id, Path::new("/tmp/sessions"), "opencode", None));
        assert!(!r.matches(
            id,
            Path::new("/tmp/sessions"),
            "grok --light",
            Some("/work")
        ));
    }

    #[test]
    fn read_returns_grok_light_background_for_light_flag() {
        // `grok --light` resolves the grokday built-in without touching config.
        let r = request(Path::new("/tmp/ptr-definitely-missing"));
        let result = r.read();
        assert_eq!(
            result.background,
            Some(ThemeBackground::new(Some(0xFAFAFA), Some(0xFAFAFA)))
        );
        // No output.bin under a missing dir → no canvas.
        assert_eq!(result.canvas, None);
    }

    #[test]
    fn resolve_frame_prefers_canvas_over_config_background() {
        // `ProviderThemeReadResult.resolve_frame` wires the live canvas sample
        // through `resolve_frame_background` (port of
        // `TerminalFrameStyle.resolved(command:workingDirectory:canvasOverride:)`
        // as called from SurfaceCache with `result.canvas`).
        let result = ProviderThemeReadResult {
            background: Some(ThemeBackground::new(Some(0xFAFAFA), Some(0xFAFAFA))),
            canvas: Some(0x0A0A12),
        };
        let frame = result.resolve_frame("grok --light", None);
        // Canvas override wins over the config-derived background.
        assert_eq!(
            frame.background,
            Some(ThemeBackground::new(Some(0x0A0A12), Some(0x0A0A12)))
        );
        assert_eq!(frame.pane_dark_hex.as_deref(), Some("#0A0A12"));
        assert_eq!(frame.pane_light_hex.as_deref(), Some("#0A0A12"));
    }

    #[test]
    fn resolve_frame_falls_back_to_config_without_canvas() {
        let result = ProviderThemeReadResult {
            background: Some(ThemeBackground::new(Some(0xFAFAFA), Some(0xFAFAFA))),
            canvas: None,
        };
        let frame = result.resolve_frame("grok --light", None);
        assert_eq!(
            frame.background,
            Some(ThemeBackground::new(Some(0xFAFAFA), Some(0xFAFAFA)))
        );
        assert_eq!(frame.pane_dark_hex.as_deref(), Some("#FAFAFA"));
    }

    #[test]
    fn read_returns_no_provider_background_for_plain_command() {
        let mut r = request(&std::env::temp_dir());
        r.command = "bash".to_string();
        assert_eq!(r.read().background, None);
    }

    #[test]
    fn read_samples_canvas_from_output_bin_tail() {
        let dir = std::env::temp_dir().join(format!("ptr-canvas-{}", std::process::id()));
        let session_dir = dir.join("s1");
        std::fs::create_dir_all(&session_dir).unwrap();
        let mut payload = Vec::new();
        for _ in 0..80 {
            payload.extend_from_slice(b"\x1b[48;2;20;20;20m ");
        }
        let mut f = std::fs::File::create(session_dir.join("output.bin")).unwrap();
        f.write_all(&payload).unwrap();
        let result = request(&dir).read();
        assert_eq!(result.canvas, Some(0x141414));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_ignores_empty_output_bin() {
        let dir = std::env::temp_dir().join(format!("ptr-empty-{}", std::process::id()));
        let session_dir = dir.join("s1");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::File::create(session_dir.join("output.bin")).unwrap();
        assert_eq!(request(&dir).read().canvas, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_samples_only_the_tail_for_large_output_bin() {
        // The journal is append-only and can be hundreds of MB; read() must
        // only touch the last SAMPLER_SAMPLE_BYTES. The head of the file is
        // painted a dominant color (far more hits than the tail); if the
        // whole file were read, the head color would win.
        let dir = std::env::temp_dir().join(format!("ptr-large-{}", std::process::id()));
        let session_dir = dir.join("s1");
        std::fs::create_dir_all(&session_dir).unwrap();
        let head_frame = b"\x1b[48;2;200;10;10m ";
        let tail_frame = b"\x1b[48;2;20;20;20m ";
        // Scoped so the file handle closes before the tail-only read below.
        {
            let mut f = std::fs::File::create(session_dir.join("output.bin")).unwrap();
            let head_frames = (SAMPLER_SAMPLE_BYTES * 2) / head_frame.len();
            for _ in 0..head_frames {
                f.write_all(head_frame).unwrap();
            }
            // The tail region alone fills the whole sample window, so a correct
            // tail-only read sees nothing but the tail color. If the whole file
            // were read, the head color would win (ratio 2.0 > 1.6 dominance).
            let tail_frames = SAMPLER_SAMPLE_BYTES / tail_frame.len();
            for _ in 0..tail_frames {
                f.write_all(tail_frame).unwrap();
            }
        }
        let len = std::fs::metadata(session_dir.join("output.bin"))
            .unwrap()
            .len();
        assert!(len > SAMPLER_SAMPLE_BYTES as u64);
        assert_eq!(request(&dir).read().canvas, Some(0x141414));
        std::fs::remove_dir_all(&dir).ok();
    }
}
