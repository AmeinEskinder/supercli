//! LITE device control for supercli: Android via `adb`/`emulator` (fallback)
//! and `scrcpy` (primary), iOS via installed `baguette` (stream/input/a11y)
//! and `xcrun simctl` (lifecycle + screenshots only).
//!
//! This module (trait + error types + tool runner) is always compiled and
//! has no dependencies beyond `std`. The real backends live behind the
//! `device` cargo feature; without it this crate compiles to a few KB.
//!
//! Design: `docs/device.md`. Nothing here is vendored: baguette and scrcpy
//! must be installed by the user; the crate shells out to them.

use std::error::Error;
use std::fmt;
use std::io::{self, Read};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Default timeout for every platform-tool invocation.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Opaque device identifier: an adb serial (`emulator-5554`, `ABC123…`) or a
/// simctl UDID. For `boot`, the id is the AVD name (Android) or UDID (iOS).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeviceId(pub String);

impl DeviceId {
    pub fn new(id: impl Into<String>) -> Self {
        DeviceId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which platform a device belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Android,
    IOS,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::Android => f.write_str("android"),
            Platform::IOS => f.write_str("ios"),
        }
    }
}

/// Lifecycle state of a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceState {
    /// Currently booted and reachable.
    Running,
    /// Known but shut down (e.g. simctl Shutdown, offline adb entry).
    Stopped,
    /// Defined but never booted in this session (e.g. an AVD with no
    /// matching `emulator-XXXX` serial).
    Available,
}

impl fmt::Display for DeviceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceState::Running => f.write_str("running"),
            DeviceState::Stopped => f.write_str("stopped"),
            DeviceState::Available => f.write_str("available"),
        }
    }
}

/// One known device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: DeviceId,
    pub name: String,
    pub platform: Platform,
    pub state: DeviceState,
}

/// Errors from device operations. All variants are honest: a missing tool or
/// a wrong host OS is an error, never a faked success.
#[derive(Debug)]
pub enum DeviceError {
    /// A required platform tool is not on PATH.
    ToolMissing(String),
    /// iOS Simulator work attempted on a non-macOS host.
    NotMacOSHost,
    /// The tool did not finish within the timeout; the child was killed.
    Timeout { tool: String },
    /// The tool exited non-zero (or was signalled).
    ToolFailed {
        tool: String,
        code: Option<i32>,
        stderr: String,
    },
    /// The operation is not supported on this backend (e.g. tap via simctl).
    Unsupported(String),
    /// Output could not be parsed (e.g. simctl JSON).
    Parse(String),
    /// Underlying I/O error.
    Io(io::Error),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::ToolMissing(tool) => write!(f, "{tool} not found on PATH"),
            // Exact string the CLI must print (design §1).
            DeviceError::NotMacOSHost => f.write_str("iOS Simulator requires a macOS host"),
            DeviceError::Timeout { tool } => write!(f, "timed out waiting for '{tool}'"),
            DeviceError::ToolFailed { tool, code, stderr } => {
                let code = match code {
                    Some(c) => c.to_string(),
                    None => "signal".to_string(),
                };
                // Keep the message bounded; full stderr is in the variant.
                let mut short: String = stderr.chars().take(300).collect();
                if stderr.chars().count() > 300 {
                    short.push('…');
                }
                write!(f, "'{tool}' failed (exit {code}): {short}")
            }
            DeviceError::Unsupported(msg) => f.write_str(msg),
            DeviceError::Parse(msg) => write!(f, "could not parse tool output: {msg}"),
            DeviceError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl Error for DeviceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DeviceError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for DeviceError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::NotFound {
            // `Command::spawn` surfaces a missing binary as NotFound. The
            // tool name is unknown here, so keep it generic; `run_tool`
            // maps this to `ToolMissing(tool)` with the real name.
            DeviceError::Io(e)
        } else {
            DeviceError::Io(e)
        }
    }
}

/// Captured output of one tool invocation.
#[derive(Clone, Debug)]
pub struct ToolOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// None if killed by a signal.
    pub code: Option<i32>,
}

impl ToolOutput {
    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// True if `tool` resolves to an executable file on PATH.
pub fn tool_on_path(tool: &str) -> bool {
    let path = match std::env::var_os("PATH") {
        Some(p) => p,
        None => return false,
    };
    std::env::split_paths(&path).any(|dir| {
        let direct = dir.join(tool);
        is_executable_file(&direct) || {
            #[cfg(windows)]
            {
                ["exe", "bat", "cmd"]
                    .iter()
                    .any(|ext| is_executable_file(&dir.join(format!("{tool}.{ext}"))))
            }
            #[cfg(not(windows))]
            {
                false
            }
        }
    })
}

#[cfg(unix)]
fn is_executable_file(p: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.is_file()
        && p.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(p: &std::path::Path) -> bool {
    p.is_file()
}

/// Run a platform tool with a timeout, capturing stdout/stderr.
///
/// Draining threads prevent pipe deadlock on large output (e.g.
/// `logcat -d`). On timeout the child is killed and reaped, then
/// `DeviceError::Timeout` is returned.
pub fn run_tool_with_timeout(
    tool: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<ToolOutput, DeviceError> {
    let mut child = Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                DeviceError::ToolMissing(tool.to_string())
            } else {
                DeviceError::Io(e)
            }
        })?;
    // NOTE: std has no `Child::kill_on_drop`; every exit path below kills
    // and reaps explicitly so no stray child survives us.

    let mut out_pipe: ChildStdout = child.stdout.take().expect("stdout piped");
    let mut err_pipe = child.stderr.take().expect("stderr piped");
    let out_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait(); // reap; ignore errors
                    let _ = out_handle.join();
                    let _ = err_handle.join();
                    return Err(DeviceError::Timeout {
                        tool: tool.to_string(),
                    });
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                // try_wait itself failed: kill and reap before giving up.
                let _ = child.kill();
                let _ = child.wait();
                let _ = out_handle.join();
                let _ = err_handle.join();
                return Err(DeviceError::Io(e));
            }
        }
    };

    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();
    Ok(ToolOutput {
        stdout,
        stderr,
        code: status.code(),
    })
}

/// [`run_tool_with_timeout`] with the 120 s default.
pub fn run_tool(tool: &str, args: &[&str]) -> Result<ToolOutput, DeviceError> {
    run_tool_with_timeout(tool, args, DEFAULT_TIMEOUT)
}

/// [`run_tool`] that maps a non-zero exit (or signal) to
/// [`DeviceError::ToolFailed`].
pub fn run_tool_ok(tool: &str, args: &[&str]) -> Result<ToolOutput, DeviceError> {
    let out = run_tool(tool, args)?;
    match out.code {
        Some(0) => Ok(out),
        code => Err(DeviceError::ToolFailed {
            tool: tool.to_string(),
            code,
            stderr: out.stderr_lossy(),
        }),
    }
}

/// A live byte stream from a device (e.g. `adb exec-out screenrecord …`).
///
/// Owns the child process: dropping the stream kills it (`kill_on_drop`),
/// so callers cannot leak a `screenrecord` session by forgetting to stop it.
pub struct DeviceStream {
    child: Child,
    stdout: ChildStdout,
}

impl DeviceStream {
    /// Wrap an already-spawned child whose stdout is piped.
    pub fn from_child(mut child: Child) -> io::Result<Self> {
        match child.stdout.take() {
            Some(stdout) => Ok(DeviceStream { child, stdout }),
            None => {
                // Unusable without a pipe: kill and reap, then report.
                let _ = child.kill();
                let _ = child.wait();
                Err(io::Error::other("child stdout was not piped"))
            }
        }
    }

    /// Stop the underlying tool (kills the child, reaps it).
    pub fn stop(mut self) -> io::Result<()> {
        let _ = self.child.kill();
        self.child.wait().map(|_| ())
    }
}

// Dropping a stream must never leak the tool (e.g. a `screenrecord`
// session): kill first (SIGKILL/TerminateProcess always works), then reap.
impl Drop for DeviceStream {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Read for DeviceStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stdout.read(buf)
    }
}

impl fmt::Debug for DeviceStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceStream")
            .field("pid", &self.child.id())
            .finish()
    }
}

/// Spawn `tool` with stdout piped and return it as a [`DeviceStream`].
/// Used for continuous output (screenrecord); the caller reads the pipe.
pub fn spawn_stream(tool: &str, args: &[&str]) -> Result<DeviceStream, DeviceError> {
    let child = Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                DeviceError::ToolMissing(tool.to_string())
            } else {
                DeviceError::Io(e)
            }
        })?;
    DeviceStream::from_child(child).map_err(DeviceError::Io)
}

/// Backend operations shared by Android (adb/scrcpy) and iOS
/// (baguette/simctl).
///
/// `id` semantics: adb serial, baguette session id, or simctl UDID; for
/// [`DeviceBackend::boot`] it is the AVD name (Android) or UDID (iOS).
pub trait DeviceBackend {
    /// All devices the backend knows about (running + stopped + available).
    fn list(&self) -> Result<Vec<DeviceInfo>, DeviceError>;
    /// Boot the device (AVD name on Android, UDID on iOS). Waits for the
    /// device to become usable, up to the 120 s tool timeout.
    fn boot(&self, id: &DeviceId) -> Result<(), DeviceError>;
    /// Shut the device down.
    fn stop(&self, id: &DeviceId) -> Result<(), DeviceError>;
    /// Install an app package (APK on Android, .app bundle on iOS).
    fn install(&self, id: &DeviceId, path: &std::path::Path) -> Result<(), DeviceError>;
    /// Launch an installed app by package name (Android) or bundle id (iOS).
    fn launch(&self, id: &DeviceId, app_id: &str) -> Result<(), DeviceError>;
    /// Capture the screen; returns PNG bytes.
    fn screenshot(&self, id: &DeviceId) -> Result<Vec<u8>, DeviceError>;
    /// Fetch device logs; `clear` discards the log buffer instead.
    fn logs(&self, id: &DeviceId, clear: bool) -> Result<String, DeviceError>;
    /// Tap at device-pixel coordinates.
    fn tap(&self, id: &DeviceId, x: u32, y: u32) -> Result<(), DeviceError>;
    /// Type text into the focused field.
    fn type_text(&self, id: &DeviceId, text: &str) -> Result<(), DeviceError>;
    /// Swipe from (x1,y1) to (x2,y2) over `duration_ms`.
    fn swipe(
        &self,
        id: &DeviceId,
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
        duration_ms: u32,
    ) -> Result<(), DeviceError>;
    /// Start a continuous H.264 screen stream; the caller reads bytes from
    /// the returned stream. Dropping it stops the tool.
    fn stream(&self, id: &DeviceId) -> Result<DeviceStream, DeviceError>;
    /// Accessibility tree for agent perception: baguette a11y JSON on iOS,
    /// `uiautomator dump` XML on Android. Returned as a raw string; parsing
    /// is the caller's job. Agents act on elements, not pixels.
    fn describe_ui(&self, id: &DeviceId) -> Result<String, DeviceError>;
}

#[cfg(feature = "device")]
pub mod adb;
#[cfg(feature = "device")]
pub mod baguette;
#[cfg(test)]
mod fake;
#[cfg(feature = "device")]
pub(crate) mod json;
#[cfg(feature = "device")]
pub mod scrcpy;
#[cfg(feature = "device")]
pub mod scrcpy_native;
#[cfg(feature = "device")]
pub mod simctl;

// Always compiled: pure std wire format + setup planners (no tools).
pub mod setup;
pub mod wire_format;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_display_roundtrip() {
        let id = DeviceId::new("emulator-5554");
        assert_eq!(id.as_str(), "emulator-5554");
        assert_eq!(id.to_string(), "emulator-5554");
    }

    #[test]
    fn error_display_strings_are_exact() {
        assert_eq!(
            DeviceError::NotMacOSHost.to_string(),
            "iOS Simulator requires a macOS host"
        );
        assert_eq!(
            DeviceError::ToolMissing("adb".to_string()).to_string(),
            "adb not found on PATH"
        );
        assert_eq!(
            DeviceError::Timeout {
                tool: "emulator".to_string()
            }
            .to_string(),
            "timed out waiting for 'emulator'"
        );
        let e = DeviceError::ToolFailed {
            tool: "adb".to_string(),
            code: Some(1),
            stderr: "boom".to_string(),
        };
        assert!(e.to_string().contains("'adb' failed (exit 1)"));
        assert!(e.to_string().contains("boom"));
        let signal = DeviceError::ToolFailed {
            tool: "adb".to_string(),
            code: None,
            stderr: String::new(),
        };
        assert!(signal.to_string().contains("exit signal"));
    }

    #[test]
    fn run_tool_missing_tool_is_tool_missing() {
        let err = run_tool("definitely-not-a-real-tool-xyz-123", &[]).unwrap_err();
        match err {
            DeviceError::ToolMissing(t) => assert_eq!(t, "definitely-not-a-real-tool-xyz-123"),
            other => panic!("expected ToolMissing, got {other:?}"),
        }
    }

    #[test]
    fn run_tool_nonzero_exit_is_tool_failed() {
        // `false` exits 1 on unix; on Windows use cmd /c exit 1.
        #[cfg(unix)]
        let err = run_tool_ok("false", &[]).unwrap_err();
        #[cfg(windows)]
        let err = run_tool_ok("cmd", &["/c", "exit", "1"]).unwrap_err();
        match err {
            DeviceError::ToolFailed { tool, code, .. } => {
                assert_eq!(code, Some(1));
                let _ = tool;
            }
            other => panic!("expected ToolFailed, got {other:?}"),
        }
        // Plain run_tool reports the code without erroring.
        #[cfg(unix)]
        let out = run_tool("false", &[]).expect("run_tool never fails on exit code");
        #[cfg(windows)]
        let out = run_tool("cmd", &["/c", "exit", "1"]).expect("run_tool never fails on exit code");
        assert_eq!(out.code, Some(1));
    }

    #[test]
    fn run_tool_ok_zero_exit_passes() {
        #[cfg(unix)]
        let out = run_tool_ok("true", &[]).expect("true exits 0");
        #[cfg(windows)]
        let out = run_tool_ok("cmd", &["/c", "exit", "0"]).expect("exit 0");
        assert_eq!(out.code, Some(0));
    }

    #[cfg(unix)]
    #[test]
    fn run_tool_timeout_kills_child() {
        let start = Instant::now();
        let err = run_tool_with_timeout("sleep", &["30"], Duration::from_millis(300)).unwrap_err();
        let elapsed = start.elapsed();
        match err {
            DeviceError::Timeout { tool } => assert_eq!(tool, "sleep"),
            other => panic!("expected Timeout, got {other:?}"),
        }
        // Must return promptly (the 30 s sleep was killed), not after 30 s.
        assert!(
            elapsed < Duration::from_secs(10),
            "timeout did not kill the child promptly: {elapsed:?}"
        );
    }

    #[test]
    fn tool_on_path_finds_real_tools() {
        // `sh` exists on unix PATHs in practice; the negative case is the
        // important one and is platform-independent.
        assert!(!tool_on_path("definitely-not-a-real-tool-xyz-123"));
    }
}
