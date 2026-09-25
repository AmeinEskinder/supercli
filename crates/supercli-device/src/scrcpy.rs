//! Android backend via installed `scrcpy` (scrcpy-server, Apache-2.0):
//! 60 fps H.264 + input injection — the primary Android tier.
//!
//! Compiled only with the `device` cargo feature. Never vendored: `scrcpy`
//! must be installed by the user; this backend shells out to it. Device
//! enumeration and input fall back to `adb` primitives (shared from
//! `super::adb`); `super::adb::AdbBackend` remains the fallback tier when
//! `scrcpy` is not on PATH.
//!
//! `stream()` attempts `scrcpy --no-window --record -` (stdout recording).
//! scrcpy builds vary in stdout support — a `ToolFailed` here surfaces the
//! real stderr honestly; the fallback is `AdbBackend::stream`
//! (`adb exec-out screenrecord`).

use super::{
    adb::{escape_input_text, parse_adb_devices, uiautomator_dump},
    run_tool_ok, spawn_stream, tool_on_path, DeviceBackend, DeviceError, DeviceId, DeviceInfo,
    DeviceState, DeviceStream, Platform,
};
use std::collections::HashSet;

/// Android backend via installed `scrcpy`.
pub struct ScrcpyBackend;

impl ScrcpyBackend {
    pub fn new() -> Self {
        ScrcpyBackend
    }

    fn check_scrcpy() -> Result<(), DeviceError> {
        if tool_on_path("scrcpy") {
            Ok(())
        } else {
            Err(DeviceError::ToolMissing("scrcpy".to_string()))
        }
    }

    fn check_adb() -> Result<(), DeviceError> {
        if tool_on_path("adb") {
            Ok(())
        } else {
            Err(DeviceError::ToolMissing("adb".to_string()))
        }
    }

    /// Tier gate: `scrcpy` must be installed. Callers that shell to `adb`
    /// additionally check for it (scrcpy itself needs adb anyway).
    fn gates() -> Result<(), DeviceError> {
        Self::check_scrcpy()
    }

    /// Run `adb [-s serial] <args>` (input + uiautomator go through adb).
    fn adb(serial: Option<&DeviceId>, args: &[&str]) -> Result<super::ToolOutput, DeviceError> {
        Self::check_adb()?;
        let mut full: Vec<&str> = Vec::with_capacity(args.len() + 2);
        if let Some(s) = serial {
            full.push("-s");
            full.push(s.as_str());
        }
        full.extend_from_slice(args);
        run_tool_ok("adb", &full)
    }
}

impl Default for ScrcpyBackend {
    fn default() -> Self {
        ScrcpyBackend::new()
    }
}

impl DeviceBackend for ScrcpyBackend {
    fn list(&self) -> Result<Vec<DeviceInfo>, DeviceError> {
        Self::gates()?;
        Self::check_adb()?;
        // scrcpy uses adb for device enumeration.
        let out = run_tool_ok("adb", &["devices", "-l"])?;
        let attached = parse_adb_devices(&out.stdout_lossy());
        let mut seen: HashSet<String> = HashSet::new();
        let mut infos = Vec::new();
        for (serial, state) in attached {
            if !seen.insert(serial.clone()) {
                continue;
            }
            infos.push(DeviceInfo {
                id: DeviceId::new(serial.clone()),
                name: serial,
                platform: Platform::Android,
                state: if state == "device" {
                    DeviceState::Running
                } else {
                    DeviceState::Stopped
                },
            });
        }
        Ok(infos)
    }

    fn boot(&self, id: &DeviceId) -> Result<(), DeviceError> {
        // Booting is an emulator-binary concern, not scrcpy's: delegate to
        // the same daemonized `emulator -avd` path the adb tier uses by
        // shelling out directly (kept here so the primary tier is complete).
        Self::gates()?;
        if !tool_on_path("emulator") {
            return Err(DeviceError::ToolMissing("emulator".to_string()));
        }
        Self::check_adb()?;
        let before: HashSet<String> = {
            let out = run_tool_ok("adb", &["devices"])?;
            parse_adb_devices(&out.stdout_lossy())
                .into_iter()
                .map(|(s, _)| s)
                .collect()
        };
        let child = std::process::Command::new("emulator")
            .args(["-avd", id.as_str(), "-no-window", "-no-audio"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    DeviceError::ToolMissing("emulator".to_string())
                } else {
                    DeviceError::Io(e)
                }
            })?;
        drop(child);
        let deadline = std::time::Instant::now() + super::DEFAULT_TIMEOUT;
        loop {
            if std::time::Instant::now() >= deadline {
                return Err(DeviceError::Timeout {
                    tool: "emulator".to_string(),
                });
            }
            let out = run_tool_ok("adb", &["devices"])?;
            let new_serial =
                parse_adb_devices(&out.stdout_lossy())
                    .into_iter()
                    .find_map(|(serial, state)| {
                        if state == "device"
                            && serial.starts_with("emulator-")
                            && !before.contains(&serial)
                        {
                            Some(serial)
                        } else {
                            None
                        }
                    });
            if let Some(serial) = new_serial {
                let prop = Self::adb(
                    Some(&DeviceId::new(serial)),
                    &["shell", "getprop", "sys.boot_completed"],
                )?;
                if prop.stdout_lossy().trim() == "1" {
                    return Ok(());
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }

    fn stop(&self, id: &DeviceId) -> Result<(), DeviceError> {
        Self::gates()?;
        Self::adb(Some(id), &["emu", "kill"])?;
        Ok(())
    }

    fn install(&self, id: &DeviceId, path: &std::path::Path) -> Result<(), DeviceError> {
        Self::gates()?;
        let path = path
            .to_str()
            .ok_or_else(|| DeviceError::Unsupported("APK path is not valid UTF-8".to_string()))?;
        Self::adb(Some(id), &["install", "-r", path])?;
        Ok(())
    }

    fn launch(&self, id: &DeviceId, app_id: &str) -> Result<(), DeviceError> {
        Self::gates()?;
        Self::adb(Some(id), &["shell", "monkey", "-p", app_id, "1"])?;
        Ok(())
    }

    fn screenshot(&self, id: &DeviceId) -> Result<Vec<u8>, DeviceError> {
        Self::gates()?;
        // screencap is instant and reliable; scrcpy's value is streaming
        // and input, not stills.
        let out = Self::adb(Some(id), &["exec-out", "screencap", "-p"])?;
        const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        if out.stdout.len() >= PNG_MAGIC.len() && out.stdout[..8] == PNG_MAGIC {
            Ok(out.stdout)
        } else {
            Err(DeviceError::ToolFailed {
                tool: "adb".to_string(),
                code: out.code,
                stderr: format!(
                    "screencap did not return PNG data ({} bytes)",
                    out.stdout.len()
                ),
            })
        }
    }

    fn logs(&self, id: &DeviceId, clear: bool) -> Result<String, DeviceError> {
        Self::gates()?;
        if clear {
            Self::adb(Some(id), &["logcat", "-c"])?;
            Ok(String::new())
        } else {
            let out = Self::adb(Some(id), &["logcat", "-d"])?;
            Ok(out.stdout_lossy())
        }
    }

    fn tap(&self, id: &DeviceId, x: u32, y: u32) -> Result<(), DeviceError> {
        Self::gates()?;
        // scrcpy injects input through its control channel while its client
        // runs; for one-shot CLI taps the adb input path is equivalent and
        // needs no running client.
        let (x, y) = (x.to_string(), y.to_string());
        Self::adb(Some(id), &["shell", "input", "tap", &x, &y])?;
        Ok(())
    }

    fn type_text(&self, id: &DeviceId, text: &str) -> Result<(), DeviceError> {
        Self::gates()?;
        let escaped = escape_input_text(text);
        Self::adb(Some(id), &["shell", "input", "text", &escaped])?;
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
        Self::gates()?;
        let (x1, y1, x2, y2, ms) = (
            x1.to_string(),
            y1.to_string(),
            x2.to_string(),
            y2.to_string(),
            duration_ms.to_string(),
        );
        Self::adb(
            Some(id),
            &["shell", "input", "swipe", &x1, &y1, &x2, &y2, &ms],
        )?;
        Ok(())
    }

    fn stream(&self, id: &DeviceId) -> Result<DeviceStream, DeviceError> {
        Self::gates()?;
        // Attempt stdout recording: `scrcpy --no-window --record -`.
        // scrcpy builds vary in stdout support — ToolFailed surfaces the
        // real stderr honestly; fallback is AdbBackend::stream.
        let _ = id;
        spawn_stream("scrcpy", &["--no-window", "--record", "-"])
    }

    fn describe_ui(&self, id: &DeviceId) -> Result<String, DeviceError> {
        Self::gates()?;
        // Same `uiautomator dump` as the adb fallback tier.
        Self::check_adb()?;
        uiautomator_dump(id)
    }
}

#[cfg(all(test, feature = "device"))]
mod tests {
    use super::*;

    #[test]
    fn tier_gate_requires_scrcpy() {
        // scrcpy is not installed here: every method must report
        // ToolMissing("scrcpy") honestly — never fake success.
        let backend = ScrcpyBackend::new();
        let id = DeviceId::new("emulator-5554");
        for result in [
            backend.list().map(|_| ()),
            backend.stop(&id),
            backend.screenshot(&id).map(|_| ()),
            backend.logs(&id, false).map(|_| ()),
            backend.tap(&id, 1, 1),
            backend.type_text(&id, "hi"),
            backend.swipe(&id, 0, 0, 1, 1, 100),
            backend.stream(&id).map(|_| ()),
            backend.describe_ui(&id).map(|_| ()),
        ] {
            match result {
                Err(DeviceError::ToolMissing(tool)) => assert_eq!(tool, "scrcpy"),
                other if tool_on_path("scrcpy") => {
                    // scrcpy actually exists here; any error shape is fine.
                    let _ = other;
                }
                other => panic!("expected ToolMissing(scrcpy), got {other:?}"),
            }
        }
    }

    #[test]
    fn uiautomator_dump_needs_adb() {
        // Helper shares the adb gate: without adb it fails honestly.
        if tool_on_path("adb") {
            return; // adb exists; nothing honest to assert without a device.
        }
        let err = uiautomator_dump(&DeviceId::new("emulator-5554")).unwrap_err();
        assert!(matches!(err, DeviceError::ToolMissing(_)));
    }
}
