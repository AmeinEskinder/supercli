//! iOS backend via installed `baguette` (https://github.com/tddworks/baguette,
//! Apache-2.0): 60 fps H.264 over its `serve` WebSocket, real tap/swipe/pinch
//! input injection, an a11y tree, and live `os_log`.
//!
//! Compiled only with the `device` cargo feature. Never vendored: `baguette`
//! must be installed by the user; this backend shells out to its CLI.
//!
//! Requirements (design §1): Apple Silicon, macOS 15+, Xcode 26. Every
//! method gates on macOS first (exact `NotMacOSHost` string), then Apple
//! Silicon, then `baguette` on PATH — no fake success on the wrong host.
//!
//! CLI shapes assumed below (`baguette list --json`, `baguette tap …`, …)
//! follow baguette's documented CLI; if upstream renames a subcommand the
//! `ToolFailed` stderr surfaces it honestly — adjust the constants, not the
//! error handling.

use super::{
    json::{parse_json, Json},
    run_tool_ok, spawn_stream, tool_on_path, DeviceBackend, DeviceError, DeviceId, DeviceInfo,
    DeviceState, DeviceStream, Platform,
};

/// PNG magic: `screenshot()` validates it so a non-PNG failure is reported
/// as an error instead of silent garbage bytes.
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// iOS backend via installed `baguette`.
pub struct BaguetteBackend;

impl BaguetteBackend {
    pub fn new() -> Self {
        BaguetteBackend
    }

    /// macOS gate: runs first in every method.
    fn require_macos() -> Result<(), DeviceError> {
        if cfg!(target_os = "macos") {
            Ok(())
        } else {
            Err(DeviceError::NotMacOSHost)
        }
    }

    /// baguette needs Apple Silicon (design §1).
    fn require_apple_silicon() -> Result<(), DeviceError> {
        if std::env::consts::ARCH == "aarch64" {
            Ok(())
        } else {
            Err(DeviceError::Unsupported(
                "baguette requires Apple Silicon (aarch64)".to_string(),
            ))
        }
    }

    fn check_baguette() -> Result<(), DeviceError> {
        if tool_on_path("baguette") {
            Ok(())
        } else {
            Err(DeviceError::ToolMissing("baguette".to_string()))
        }
    }

    /// All three gates, in order: host OS, CPU arch, tool on PATH.
    fn gates() -> Result<(), DeviceError> {
        Self::require_macos()?;
        Self::require_apple_silicon()?;
        Self::check_baguette()
    }

    /// Run `baguette <args>`, mapping non-zero exits to `ToolFailed`.
    fn baguette(args: &[&str]) -> Result<super::ToolOutput, DeviceError> {
        Self::gates()?;
        run_tool_ok("baguette", args)
    }

    /// Lifecycle operations baguette does not own go through simctl
    /// directly (design §1: simctl keeps boot/shutdown/install/launch).
    fn simctl(args: &[&str]) -> Result<super::ToolOutput, DeviceError> {
        Self::require_macos()?;
        if !tool_on_path("xcrun") {
            return Err(DeviceError::ToolMissing("xcrun".to_string()));
        }
        let mut full: Vec<&str> = Vec::with_capacity(args.len() + 1);
        full.push("simctl");
        full.extend_from_slice(args);
        run_tool_ok("xcrun", &full)
    }
}

impl Default for BaguetteBackend {
    fn default() -> Self {
        BaguetteBackend::new()
    }
}

/// Extract devices from `baguette list --json`.
///
/// Tolerant: accepts a top-level array of device objects or an object with
/// a `devices`/`sessions` array. Each entry reads `id` (or `udid`), `name`,
/// and `state` (or `status`). Unknown shapes are skipped rather than
/// failing the whole list — a partial honest list beats an error.
pub fn parse_baguette_devices(json: &str) -> Result<Vec<DeviceInfo>, DeviceError> {
    let root = parse_json(json).map_err(DeviceError::Parse)?;
    let arr: &[Json] = match &root {
        Json::Arr(a) => a,
        Json::Obj(_) => root
            .get("devices")
            .or_else(|| root.get("sessions"))
            .and_then(Json::as_arr)
            .ok_or_else(|| {
                DeviceError::Parse(
                    "baguette list: expected array or {\"devices\": [...]}".to_string(),
                )
            })?,
        _ => {
            return Err(DeviceError::Parse(
                "baguette list: expected array or object".to_string(),
            ))
        }
    };
    let mut infos = Vec::new();
    for d in arr {
        let id = match d
            .get("id")
            .or_else(|| d.get("udid"))
            .or_else(|| d.get("session_id"))
            .and_then(Json::as_str)
        {
            Some(id) => id,
            None => continue,
        };
        let name = d
            .get("name")
            .and_then(Json::as_str)
            .unwrap_or(id)
            .to_string();
        let state = match d
            .get("state")
            .or_else(|| d.get("status"))
            .and_then(Json::as_str)
            .map(str::to_lowercase)
            .as_deref()
        {
            Some("running") | Some("booted") | Some("active") | Some("connected") => {
                DeviceState::Running
            }
            Some("stopped") | Some("shutdown") | Some("inactive") | Some("disconnected") => {
                DeviceState::Stopped
            }
            _ => DeviceState::Available,
        };
        infos.push(DeviceInfo {
            id: DeviceId::new(id),
            name,
            platform: Platform::IOS,
            state,
        });
    }
    Ok(infos)
}

impl DeviceBackend for BaguetteBackend {
    fn list(&self) -> Result<Vec<DeviceInfo>, DeviceError> {
        let out = Self::baguette(&["list", "--json"])?;
        parse_baguette_devices(&out.stdout_lossy())
    }

    fn boot(&self, id: &DeviceId) -> Result<(), DeviceError> {
        Self::baguette(&["boot", id.as_str()])?;
        Ok(())
    }

    fn stop(&self, id: &DeviceId) -> Result<(), DeviceError> {
        // Lifecycle stays on simctl (design §1).
        Self::simctl(&["shutdown", id.as_str()])?;
        Ok(())
    }

    fn install(&self, id: &DeviceId, path: &std::path::Path) -> Result<(), DeviceError> {
        let path = path
            .to_str()
            .ok_or_else(|| DeviceError::Unsupported(".app path is not valid UTF-8".to_string()))?;
        Self::simctl(&["install", id.as_str(), path])?;
        Ok(())
    }

    fn launch(&self, id: &DeviceId, app_id: &str) -> Result<(), DeviceError> {
        Self::simctl(&["launch", id.as_str(), app_id])?;
        Ok(())
    }

    fn screenshot(&self, id: &DeviceId) -> Result<Vec<u8>, DeviceError> {
        let out = Self::baguette(&["screenshot", id.as_str()])?;
        if out.stdout.len() >= PNG_MAGIC.len() && out.stdout[..8] == PNG_MAGIC {
            Ok(out.stdout)
        } else {
            Err(DeviceError::ToolFailed {
                tool: "baguette".to_string(),
                code: out.code,
                stderr: format!(
                    "baguette screenshot did not return PNG data ({} bytes); stderr: {}",
                    out.stdout.len(),
                    out.stderr_lossy()
                ),
            })
        }
    }

    fn logs(&self, id: &DeviceId, clear: bool) -> Result<String, DeviceError> {
        if clear {
            return Err(DeviceError::Unsupported(
                "baguette cannot clear device logs; erase the simulator instead".to_string(),
            ));
        }
        // Live os_log comes over the `serve` WebSocket in the web UI; the
        // CLI surfaces the buffered tail.
        let out = Self::baguette(&["logs", id.as_str()])?;
        Ok(out.stdout_lossy())
    }

    fn tap(&self, id: &DeviceId, x: u32, y: u32) -> Result<(), DeviceError> {
        let (x, y) = (x.to_string(), y.to_string());
        Self::baguette(&["tap", id.as_str(), &x, &y])?;
        Ok(())
    }

    fn type_text(&self, id: &DeviceId, text: &str) -> Result<(), DeviceError> {
        Self::baguette(&["type", id.as_str(), text])?;
        Ok(())
    }

    fn swipe(
        &self,
        id: &DeviceId,
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
        duration_ms: u32,
    ) -> Result<(), DeviceError> {
        let (x1, y1, x2, y2, ms) = (
            x1.to_string(),
            y1.to_string(),
            x2.to_string(),
            y2.to_string(),
            duration_ms.to_string(),
        );
        Self::baguette(&[
            "swipe",
            id.as_str(),
            &x1,
            &y1,
            &x2,
            &y2,
            "--duration-ms",
            &ms,
        ])?;
        Ok(())
    }

    fn stream(&self, id: &DeviceId) -> Result<DeviceStream, DeviceError> {
        Self::gates()?;
        // Assumes `baguette stream <udid>` writes raw H.264 to stdout for
        // `supercli device stream > out.h264`. The web UI does not use this
        // path: it connects to baguette's `serve` WebSocket directly
        // (browser-native WebSocket → WebCodecs). If upstream names the
        // subcommand differently, ToolFailed surfaces the real stderr.
        spawn_stream("baguette", &["stream", id.as_str()])
    }

    fn describe_ui(&self, id: &DeviceId) -> Result<String, DeviceError> {
        // baguette a11y-tree JSON — the primary agent perception tool on
        // iOS (agents act on elements, not pixels).
        let out = Self::baguette(&["describe-ui", id.as_str(), "--json"])?;
        Ok(out.stdout_lossy())
    }
}

#[cfg(all(test, feature = "device"))]
mod tests {
    use super::*;

    const BAGUETTE_LIST_ARRAY: &str = r#"[
      {"id": "sess-1", "name": "iPhone 16 Pro", "state": "running"},
      {"udid": "A1B2-UDID", "name": "iPhone SE", "status": "shutdown"}
    ]"#;

    const BAGUETTE_LIST_WRAPPED: &str =
        r#"{"devices": [{"session_id": "sess-9", "name": "iPad", "state": "weird"}]}"#;

    #[test]
    fn parse_baguette_devices_array_fixture() {
        let devices = parse_baguette_devices(BAGUETTE_LIST_ARRAY).expect("fixture parses");
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].platform, Platform::IOS);
        assert_eq!(devices[0].id.as_str(), "sess-1");
        assert_eq!(devices[0].name, "iPhone 16 Pro");
        assert_eq!(devices[0].state, DeviceState::Running);
        assert_eq!(devices[1].id.as_str(), "A1B2-UDID");
        assert_eq!(devices[1].state, DeviceState::Stopped);
    }

    #[test]
    fn parse_baguette_devices_wrapped_and_unknown_state() {
        let devices = parse_baguette_devices(BAGUETTE_LIST_WRAPPED).expect("fixture parses");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id.as_str(), "sess-9");
        // Unknown state strings map to Available, never an error.
        assert_eq!(devices[0].state, DeviceState::Available);
    }

    #[test]
    fn parse_baguette_devices_skips_idless_entries() {
        let devices = parse_baguette_devices(r#"[{"name": "ghost"}]"#).expect("parses");
        assert!(devices.is_empty());
    }

    #[test]
    fn parse_baguette_devices_garbage_is_parse_error() {
        let err = parse_baguette_devices("not json").unwrap_err();
        assert!(matches!(err, DeviceError::Parse(_)));
    }

    #[test]
    fn gates_fail_honestly_without_baguette() {
        // baguette is not installed on CI/dev VMs: every baguette-touching
        // method must fail honestly. On non-macOS the macOS gate fires
        // first; on macOS/x86_64 the arch gate fires; only on Apple Silicon
        // macOS does the ToolMissing assertion apply.
        let backend = BaguetteBackend::new();
        let id = DeviceId::new("sess-1");
        let err = backend.list().unwrap_err();
        #[cfg(not(target_os = "macos"))]
        assert!(matches!(err, DeviceError::NotMacOSHost));
        #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
        assert!(matches!(err, DeviceError::Unsupported(_)));
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        match err {
            DeviceError::ToolMissing(t) => assert_eq!(t, "baguette"),
            other => panic!("expected ToolMissing(baguette), got {other:?}"),
        }
        let _ = id;
    }

    #[test]
    fn not_macos_host_gating_is_exact() {
        #[cfg(not(target_os = "macos"))]
        {
            let backend = BaguetteBackend::new();
            let id = DeviceId::new("sess-1");
            for result in [
                backend.list().map(|_| ()),
                backend.boot(&id),
                backend.stop(&id),
                backend.screenshot(&id).map(|_| ()),
                backend.logs(&id, false).map(|_| ()),
                backend.tap(&id, 10, 10),
                backend.type_text(&id, "hi"),
                backend.swipe(&id, 0, 0, 1, 1, 100),
                backend.stream(&id).map(|_| ()),
                backend.describe_ui(&id).map(|_| ()),
            ] {
                match result {
                    Err(DeviceError::NotMacOSHost) => {}
                    other => panic!("expected NotMacOSHost, got {other:?}"),
                }
            }
        }
        #[cfg(target_os = "macos")]
        {
            assert!(BaguetteBackend::require_macos().is_ok());
        }
    }
}
