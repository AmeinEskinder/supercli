//! iOS backend: shells out to `xcrun simctl` — lifecycle and screenshots
//! ONLY (boot/shutdown/install/launch/screenshot/logs).
//!
//! Compiled only with the `device` cargo feature. Every public method first
//! checks for a macOS host and returns [`DeviceError::NotMacOSHost`] on any
//! other OS — no fake success. JSON parsing is shared from `super::json`
//! (hand-rolled recursive descent; this crate has zero mandatory
//! dependencies).
//!
//! Known limitation (design §1): `simctl` has **no input injection** — no
//! tap, swipe, type — and no streaming or a11y tree. Those go through
//! installed `baguette` (`super::baguette::BaguetteBackend`). The simctl
//! methods below return [`DeviceError::Unsupported`] honestly instead of
//! faking success.

use super::{
    json::{parse_json, Json},
    run_tool_ok, tool_on_path, DeviceBackend, DeviceError, DeviceId, DeviceInfo, DeviceState,
    DeviceStream, Platform,
};
use std::path::PathBuf;

/// iOS backend via `xcrun simctl`.
pub struct SimctlBackend;

impl SimctlBackend {
    pub fn new() -> Self {
        SimctlBackend
    }

    /// macOS gate: every iOS operation calls this first.
    fn require_macos() -> Result<(), DeviceError> {
        if cfg!(target_os = "macos") {
            Ok(())
        } else {
            Err(DeviceError::NotMacOSHost)
        }
    }

    fn check_xcrun() -> Result<(), DeviceError> {
        if tool_on_path("xcrun") {
            Ok(())
        } else {
            Err(DeviceError::ToolMissing("xcrun".to_string()))
        }
    }

    /// Run `xcrun simctl <args>`, mapping non-zero exits to `ToolFailed`.
    fn simctl(args: &[&str]) -> Result<super::ToolOutput, DeviceError> {
        Self::require_macos()?;
        Self::check_xcrun()?;
        let mut full: Vec<&str> = Vec::with_capacity(args.len() + 1);
        full.push("simctl");
        full.extend_from_slice(args);
        run_tool_ok("xcrun", &full)
    }
}

impl Default for SimctlBackend {
    fn default() -> Self {
        SimctlBackend::new()
    }
}

/// Extract devices from `xcrun simctl list devices available --json`.
///
/// Shape: `{ "devices": { "<runtime-id>": [ {udid, name, state, …}, … ], … } }`.
/// State mapping: "Booted" → Running, "Shutdown" → Stopped, anything else →
/// Available.
pub fn parse_simctl_devices(json: &str) -> Result<Vec<DeviceInfo>, DeviceError> {
    let root = parse_json(json).map_err(DeviceError::Parse)?;
    let devices_obj = root
        .get("devices")
        .ok_or_else(|| DeviceError::Parse("missing \"devices\" key".to_string()))?;
    let Json::Obj(pairs) = devices_obj else {
        return Err(DeviceError::Parse(
            "\"devices\" is not an object".to_string(),
        ));
    };
    let mut infos = Vec::new();
    for (_, runtime_devices) in pairs {
        let arr = runtime_devices
            .as_arr()
            .ok_or_else(|| DeviceError::Parse("runtime device list is not an array".to_string()))?;
        for d in arr {
            let udid = d
                .get("udid")
                .and_then(Json::as_str)
                .ok_or_else(|| DeviceError::Parse("device missing udid".to_string()))?;
            let name = d
                .get("name")
                .and_then(Json::as_str)
                .unwrap_or(udid)
                .to_string();
            let state = match d.get("state").and_then(Json::as_str) {
                Some("Booted") => DeviceState::Running,
                Some("Shutdown") => DeviceState::Stopped,
                _ => DeviceState::Available,
            };
            infos.push(DeviceInfo {
                id: DeviceId::new(udid),
                name,
                platform: Platform::IOS,
                state,
            });
        }
    }
    Ok(infos)
}

impl DeviceBackend for SimctlBackend {
    fn list(&self) -> Result<Vec<DeviceInfo>, DeviceError> {
        let out = Self::simctl(&["list", "devices", "available", "--json"])?;
        parse_simctl_devices(&out.stdout_lossy())
    }

    fn boot(&self, id: &DeviceId) -> Result<(), DeviceError> {
        Self::simctl(&["boot", id.as_str()])?;
        Ok(())
    }

    fn stop(&self, id: &DeviceId) -> Result<(), DeviceError> {
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
        // `simctl io screenshot` needs a file path (no stdout mode), so go
        // through a temp file and read it back. Extension selects PNG.
        let tmp: PathBuf = std::env::temp_dir().join(format!(
            "supercli-screenshot-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let tmp_str = tmp.to_str().ok_or_else(|| {
            DeviceError::Unsupported("temp dir path is not valid UTF-8".to_string())
        })?;
        let result = Self::simctl(&["io", id.as_str(), "screenshot", tmp_str]);
        // Always try to clean up, even if simctl failed.
        let bytes = std::fs::read(&tmp);
        let _ = std::fs::remove_file(&tmp);
        result?;
        bytes.map_err(DeviceError::Io)
    }

    fn logs(&self, id: &DeviceId, clear: bool) -> Result<String, DeviceError> {
        if clear {
            return Err(DeviceError::Unsupported(
                "simctl cannot clear device logs; erase the simulator instead".to_string(),
            ));
        }
        let out = Self::simctl(&[
            "spawn",
            id.as_str(),
            "log",
            "show",
            "--style",
            "compact",
            "--last",
            "5m",
        ])?;
        Ok(out.stdout_lossy())
    }

    fn tap(&self, _id: &DeviceId, _x: u32, _y: u32) -> Result<(), DeviceError> {
        Self::require_macos()?;
        Err(DeviceError::Unsupported(
            "simctl has no input injection; use baguette".to_string(),
        ))
    }

    fn type_text(&self, _id: &DeviceId, _text: &str) -> Result<(), DeviceError> {
        Self::require_macos()?;
        Err(DeviceError::Unsupported(
            "simctl has no input injection; use baguette".to_string(),
        ))
    }

    fn swipe(
        &self,
        _id: &DeviceId,
        _x1: u32,
        _y1: u32,
        _x2: u32,
        _y2: u32,
        _duration_ms: u32,
    ) -> Result<(), DeviceError> {
        Self::require_macos()?;
        Err(DeviceError::Unsupported(
            "simctl has no input injection; use baguette".to_string(),
        ))
    }

    fn stream(&self, id: &DeviceId) -> Result<DeviceStream, DeviceError> {
        Self::require_macos()?;
        // `simctl io recordVideo` is file-based (no stdout streaming), so a
        // live pipe is not available. Report honestly instead of faking it.
        let _ = id;
        Err(DeviceError::Unsupported(
            "simctl has no streaming; use baguette".to_string(),
        ))
    }

    fn describe_ui(&self, _id: &DeviceId) -> Result<String, DeviceError> {
        Self::require_macos()?;
        Err(DeviceError::Unsupported(
            "simctl has no input injection; use baguette".to_string(),
        ))
    }
}

#[cfg(all(test, feature = "device"))]
mod tests {
    use super::*;

    const SIMCTL_FIXTURE: &str = r#"{
  "devices" : {
    "com.apple.CoreSimulator.SimRuntime.iOS-18-0" : [
      {
        "udid" : "A1B2C3D4-E5F6-7890-ABCD-EF1234567890",
        "name" : "iPhone 16 Pro",
        "state" : "Booted",
        "isAvailable" : true
      },
      {
        "udid" : "12345678-1234-1234-1234-123456789ABC",
        "name" : "iPhone SE (3rd generation)",
        "state" : "Shutdown",
        "isAvailable" : true
      }
    ],
    "com.apple.CoreSimulator.SimRuntime.iOS-17-5" : [
      {
        "udid" : "DEADBEEF-0000-1111-2222-333344445555",
        "name" : "iPad Air",
        "state" : "Creating",
        "isAvailable" : true
      }
    ]
  }
}"#;

    #[test]
    fn parse_simctl_devices_fixture() {
        let devices = parse_simctl_devices(SIMCTL_FIXTURE).expect("fixture parses");
        assert_eq!(devices.len(), 3);
        assert_eq!(devices[0].platform, Platform::IOS);
        assert_eq!(devices[0].name, "iPhone 16 Pro");
        assert_eq!(devices[0].state, DeviceState::Running);
        assert_eq!(
            devices[0].id.as_str(),
            "A1B2C3D4-E5F6-7890-ABCD-EF1234567890"
        );
        assert_eq!(devices[1].state, DeviceState::Stopped);
        assert_eq!(devices[2].state, DeviceState::Available);
    }

    #[test]
    fn parse_simctl_devices_missing_key_is_parse_error() {
        let err = parse_simctl_devices(r#"{"nope": 1}"#).unwrap_err();
        assert!(matches!(err, DeviceError::Parse(_)));
    }

    #[test]
    fn parse_simctl_devices_garbage_is_parse_error() {
        let err = parse_simctl_devices("not json at all").unwrap_err();
        assert!(matches!(err, DeviceError::Parse(_)));
    }

    #[test]
    fn not_macos_host_gating_is_exact() {
        // Every method must fail with NotMacOSHost before touching xcrun.
        // This test only runs on non-macOS hosts (it is the gate itself).
        #[cfg(not(target_os = "macos"))]
        {
            let backend = SimctlBackend::new();
            let id = DeviceId::new("some-udid");
            for result in [
                backend.list().map(|_| ()),
                backend.boot(&id),
                backend.stop(&id),
                backend.launch(&id, "com.example.app"),
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
            assert_eq!(
                DeviceError::NotMacOSHost.to_string(),
                "iOS Simulator requires a macOS host"
            );
        }
        // On macOS the gate passes; nothing to assert without simctl.
        #[cfg(target_os = "macos")]
        {
            assert!(SimctlBackend::require_macos().is_ok());
        }
    }

    #[test]
    fn simctl_tap_limitation_documented() {
        // On macOS, tap must report Unsupported (never fake success);
        // off macOS it reports NotMacOSHost (gate runs first).
        let backend = SimctlBackend::new();
        let err = backend.tap(&DeviceId::new("udid"), 1, 1).unwrap_err();
        #[cfg(target_os = "macos")]
        assert!(matches!(err, DeviceError::Unsupported(_)));
        #[cfg(not(target_os = "macos"))]
        assert!(matches!(err, DeviceError::NotMacOSHost));
    }
}
