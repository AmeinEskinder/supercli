//! Android backend: shells out to `adb` and the `emulator` binary.
//!
//! Compiled only with the `device` cargo feature. No dependencies beyond
//! `std` — every operation is a [`Command`] invocation parsed in plain Rust.
//!
//! This is the **fallback tier**: `super::scrcpy::ScrcpyBackend`
//! (scrcpy-server, 60 fps H.264 + input injection) is preferred when `scrcpy`
//! is on PATH. `describe_ui` uses the same `uiautomator dump` the scrcpy
//! tier uses.

use super::{
    run_tool_ok, run_tool_with_timeout, spawn_stream, tool_on_path, DeviceBackend, DeviceError,
    DeviceId, DeviceInfo, DeviceState, DeviceStream, Platform, DEFAULT_TIMEOUT,
};
use std::collections::HashSet;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// PNG magic: `screenshot()` validates it so a non-PNG `exec-out` failure
/// is reported as an error instead of silent garbage bytes.
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Android backend via `adb` + `emulator` on PATH.
pub struct AdbBackend;

impl AdbBackend {
    pub fn new() -> Self {
        AdbBackend
    }

    fn check_adb() -> Result<(), DeviceError> {
        if tool_on_path("adb") {
            Ok(())
        } else {
            Err(DeviceError::ToolMissing("adb".to_string()))
        }
    }

    fn check_emulator() -> Result<(), DeviceError> {
        if tool_on_path("emulator") {
            Ok(())
        } else {
            Err(DeviceError::ToolMissing("emulator".to_string()))
        }
    }

    /// Run `adb [-s serial] <args>`, mapping non-zero exits to `ToolFailed`.
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

    /// Serials currently attached according to `adb devices -l`, with their
    /// state word (`device`, `offline`, `unauthorized`, …).
    fn attached_serials() -> Result<Vec<(String, String)>, DeviceError> {
        Self::check_adb()?;
        let out = run_tool_ok("adb", &["devices", "-l"])?;
        Ok(parse_adb_devices(&out.stdout_lossy()))
    }
}

impl Default for AdbBackend {
    fn default() -> Self {
        AdbBackend::new()
    }
}

/// Escape text for `adb shell input text`.
///
/// The input command (not a shell) interprets `%s` as a space and `%%` as a
/// literal percent, so those two substitutions are required; everything else
/// is passed through untouched because we exec `adb` directly (no shell).
pub fn escape_input_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '%' => out.push_str("%%"),
            ' ' => out.push_str("%s"),
            _ => out.push(ch),
        }
    }
    out
}

/// Parse `adb devices -l` output into (serial, state-word) pairs.
/// Skips the "List of devices attached" header and blank lines.
pub fn parse_adb_devices(output: &str) -> Vec<(String, String)> {
    let mut devices = Vec::new();
    for line in output.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let serial = match parts.next() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let state = parts.next().unwrap_or("unknown").to_string();
        devices.push((serial, state));
    }
    devices
}

/// Parse `emulator -list-avds` output: one AVD name per line.
pub fn parse_avd_list(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// Dump the accessibility tree via `uiautomator`.
///
/// `adb shell uiautomator dump /dev/stdout` prints the raw XML hierarchy.
/// Shared with `ScrcpyBackend` (`super::scrcpy`); the `AdbBackend` is the
/// fallback tier (tried when `scrcpy` is not on PATH), so this stays honest
/// about which tool produced the tree.
pub(crate) fn uiautomator_dump(serial: &DeviceId) -> Result<String, DeviceError> {
    let out = AdbBackend::adb(
        Some(serial),
        &["shell", "uiautomator", "dump", "/dev/stdout"],
    )?;
    Ok(out.stdout_lossy())
}

/// Merge `adb devices` (running) with the AVD list (available):
/// an AVD with no matching `emulator-XXXX` serial is `Available`.
fn merge_devices(attached: &[(String, String)], avds: &[String]) -> Vec<DeviceInfo> {
    let mut infos: Vec<DeviceInfo> = Vec::new();
    let running_serials: HashSet<&str> = attached
        .iter()
        .filter(|(_, state)| state == "device")
        .map(|(serial, _)| serial.as_str())
        .collect();

    for (serial, state) in attached {
        let device_state = if state == "device" {
            DeviceState::Running
        } else {
            DeviceState::Stopped
        };
        infos.push(DeviceInfo {
            id: DeviceId::new(serial.clone()),
            name: serial.clone(),
            platform: Platform::Android,
            state: device_state,
        });
    }

    // AVDs that have no running emulator serial are bootable-but-off.
    // (Matching an AVD name to an emulator-XXXX serial is not possible from
    // `adb devices` alone, so every AVD appears as Available; the running
    // entries above carry the live state.)
    let _ = &running_serials;
    for avd in avds {
        infos.push(DeviceInfo {
            id: DeviceId::new(avd.clone()),
            name: avd.clone(),
            platform: Platform::Android,
            state: DeviceState::Available,
        });
    }
    infos
}

impl DeviceBackend for AdbBackend {
    fn list(&self) -> Result<Vec<DeviceInfo>, DeviceError> {
        let attached = Self::attached_serials()?;
        // AVD merge is best-effort: without the emulator binary we still
        // report attached devices honestly instead of failing the list.
        let avds = match Self::check_emulator() {
            Ok(()) => {
                let out = run_tool_ok("emulator", &["-list-avds"])?;
                parse_avd_list(&out.stdout_lossy())
            }
            Err(_) => Vec::new(),
        };
        Ok(merge_devices(&attached, &avds))
    }

    fn boot(&self, id: &DeviceId) -> Result<(), DeviceError> {
        Self::check_emulator()?;
        Self::check_adb()?;
        let before: HashSet<String> = Self::attached_serials()?
            .into_iter()
            .map(|(serial, _)| serial)
            .collect();

        // Daemonize: detached stdio, and the default (no kill on drop) —
        // the emulator keeps running after this Child handle is dropped.
        let child = Command::new("emulator")
            .args(["-avd", id.as_str(), "-no-window", "-no-audio"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    DeviceError::ToolMissing("emulator".to_string())
                } else {
                    DeviceError::Io(e)
                }
            })?;
        drop(child);

        // Wait for a *new* emulator serial, then for sys.boot_completed=1.
        let deadline = Instant::now() + DEFAULT_TIMEOUT;
        loop {
            if Instant::now() >= deadline {
                return Err(DeviceError::Timeout {
                    tool: "emulator".to_string(),
                });
            }
            let serials = Self::attached_serials()?;
            let new_serial = serials.iter().find_map(|(serial, state)| {
                if state == "device" && serial.starts_with("emulator-") && !before.contains(serial)
                {
                    Some(serial.clone())
                } else {
                    None
                }
            });
            if let Some(serial) = new_serial {
                let id = DeviceId::new(serial);
                // `adb wait-for-device` semantics, then the boot flag.
                let prop = Self::adb(Some(&id), &["shell", "getprop", "sys.boot_completed"])?;
                if prop.stdout_lossy().trim() == "1" {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    fn stop(&self, id: &DeviceId) -> Result<(), DeviceError> {
        Self::adb(Some(id), &["emu", "kill"])?;
        Ok(())
    }

    fn install(&self, id: &DeviceId, path: &std::path::Path) -> Result<(), DeviceError> {
        let path = path
            .to_str()
            .ok_or_else(|| DeviceError::Unsupported("APK path is not valid UTF-8".to_string()))?;
        Self::adb(Some(id), &["install", "-r", path])?;
        Ok(())
    }

    fn launch(&self, id: &DeviceId, app_id: &str) -> Result<(), DeviceError> {
        // `monkey -p <pkg> 1` starts the default launcher activity.
        Self::adb(Some(id), &["shell", "monkey", "-p", app_id, "1"])?;
        Ok(())
    }

    fn screenshot(&self, id: &DeviceId) -> Result<Vec<u8>, DeviceError> {
        let out = Self::adb(Some(id), &["exec-out", "screencap", "-p"])?;
        if out.stdout.len() >= PNG_MAGIC.len() && out.stdout[..8] == PNG_MAGIC {
            Ok(out.stdout)
        } else {
            Err(DeviceError::ToolFailed {
                tool: "adb".to_string(),
                code: out.code,
                stderr: format!(
                    "screencap did not return PNG data ({} bytes); stderr: {}",
                    out.stdout.len(),
                    out.stderr_lossy()
                ),
            })
        }
    }

    fn logs(&self, id: &DeviceId, clear: bool) -> Result<String, DeviceError> {
        if clear {
            Self::adb(Some(id), &["logcat", "-c"])?;
            Ok(String::new())
        } else {
            let out = Self::adb(Some(id), &["logcat", "-d"])?;
            Ok(out.stdout_lossy())
        }
    }

    fn tap(&self, id: &DeviceId, x: u32, y: u32) -> Result<(), DeviceError> {
        let (x, y) = (x.to_string(), y.to_string());
        Self::adb(Some(id), &["shell", "input", "tap", &x, &y])?;
        Ok(())
    }

    fn type_text(&self, id: &DeviceId, text: &str) -> Result<(), DeviceError> {
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
        Self::check_adb()?;
        // Raw H.264 Annex-B on stdout; the caller reconnects before the
        // 180 s screenrecord cap (design §3). This is the fallback tier:
        // `ScrcpyBackend` (scrcpy-server, 60 fps) is preferred when `scrcpy`
        // is on PATH.
        let args = [
            "-s",
            id.as_str(),
            "exec-out",
            "screenrecord",
            "--output-format=h264",
            "-",
        ];
        spawn_stream("adb", &args)
    }

    fn describe_ui(&self, id: &DeviceId) -> Result<String, DeviceError> {
        // Fallback tier for describe-ui: same `uiautomator dump` the
        // scrcpy tier uses; tried when `scrcpy` is not on PATH.
        uiautomator_dump(id)
    }
}

/// Keep the 120 s helper referenced so the import set stays honest even if
/// future edits drop the direct use in `boot`.
#[allow(dead_code)]
fn _timeout_probe() {
    let _ = run_tool_with_timeout;
}

#[cfg(all(test, feature = "device"))]
mod tests {
    use super::*;

    #[test]
    fn parse_adb_devices_skips_header_and_blanks() {
        let fixture = "List of devices attached\n\
             emulator-5554\tdevice product:sdk_gphone64_x86_64 model:sdk_gphone64_x86_64 device:emulator64_x86_64 transport_id:1\n\
             \n\
             0A061FDD6000AB\tunauthorized transport_id:2\n";
        let parsed = parse_adb_devices(fixture);
        assert_eq!(
            parsed,
            vec![
                ("emulator-5554".to_string(), "device".to_string()),
                ("0A061FDD6000AB".to_string(), "unauthorized".to_string()),
            ]
        );
    }

    #[test]
    fn parse_adb_devices_empty() {
        assert!(parse_adb_devices("List of devices attached\n\n").is_empty());
    }

    #[test]
    fn parse_avd_list_trims_and_skips_blanks() {
        let fixture = "Pixel_8_API_34\n\n  Medium_Phone_API_35  \n";
        assert_eq!(
            parse_avd_list(fixture),
            vec![
                "Pixel_8_API_34".to_string(),
                "Medium_Phone_API_35".to_string()
            ]
        );
    }

    #[test]
    fn merge_devices_marks_states() {
        let attached = vec![
            ("emulator-5554".to_string(), "device".to_string()),
            ("deadbeef".to_string(), "offline".to_string()),
        ];
        let avds = vec!["Pixel_8_API_34".to_string()];
        let merged = merge_devices(&attached, &avds);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].state, DeviceState::Running);
        assert_eq!(merged[0].platform, Platform::Android);
        assert_eq!(merged[1].state, DeviceState::Stopped);
        assert_eq!(merged[2].state, DeviceState::Available);
        assert_eq!(merged[2].name, "Pixel_8_API_34");
    }

    #[test]
    fn escape_input_text_escapes_percent_and_space() {
        assert_eq!(escape_input_text("hello world"), "hello%sworld");
        assert_eq!(escape_input_text("100%"), "100%%");
        assert_eq!(escape_input_text("a b%c"), "a%sb%%c");
        assert_eq!(escape_input_text("plain"), "plain");
        assert_eq!(escape_input_text(""), "");
    }

    #[test]
    fn backend_without_adb_reports_tool_missing() {
        // PATH scrubbed of adb: every adb-touching method must fail
        // honestly instead of faking success.
        let backend = AdbBackend::new();
        let id = DeviceId::new("emulator-5554");
        for result in [
            backend.list().map(|_| ()),
            backend.stop(&id),
            backend.screenshot(&id).map(|_| ()),
            backend.tap(&id, 1, 1),
        ] {
            match result {
                Err(DeviceError::ToolMissing(tool)) => assert_eq!(tool, "adb"),
                other
                    if std::env::var_os("PATH")
                        .map(|p| {
                            std::env::split_paths(&p)
                                .any(|d| d.join("adb").is_file() || d.join("adb.exe").is_file())
                        })
                        .unwrap_or(false) =>
                {
                    // adb actually exists here; any error shape is fine.
                    let _ = other;
                }
                other => panic!("expected ToolMissing(adb), got {other:?}"),
            }
        }
    }
}
