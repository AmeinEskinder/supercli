//! One-command device setup: `supercli device setup android|ios`.
//!
//! Design: docs/device.md §9.5. Android setup downloads SDK pieces through
//! `sdkmanager` (approval-gated in the CLI: they are network fetches) and
//! creates the `supercli` AVD with `avdmanager`. iOS setup only checks for
//! baguette on PATH — no downloads, no approval.
//!
//! No feature gate: pure `std` + the always-compiled helpers in
//! `crate::{run_tool_ok, tool_on_path, ...}`. The [`ToolRunner`] trait keeps
//! every tool invocation mockable so tests never need real SDK tools.

use std::error::Error;
use std::fmt;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{tool_on_path, DeviceError, ToolOutput};

/// How long `sdkmanager` may take: it downloads hundreds of MB.
const SDKMANAGER_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// `avdmanager create avd` is local and fast.
const AVDMANAGER_TIMEOUT: Duration = Duration::from_secs(120);

/// A tool invocation, abstracted so tests can mock `sdkmanager` /
/// `avdmanager` / `baguette` without touching PATH or the network.
pub trait ToolRunner {
    /// Run `tool` with `args`, feeding `stdin` (if any) to its standard
    /// input. Non-zero exit becomes [`DeviceError::ToolFailed`].
    fn run(
        &self,
        tool: &str,
        args: &[&str],
        stdin: Option<&[u8]>,
    ) -> Result<ToolOutput, DeviceError>;
}

/// [`ToolRunner`] that really spawns processes.
pub struct RealRunner;

impl ToolRunner for RealRunner {
    fn run(
        &self,
        tool: &str,
        args: &[&str],
        stdin: Option<&[u8]>,
    ) -> Result<ToolOutput, DeviceError> {
        let timeout = if tool == "sdkmanager" {
            SDKMANAGER_TIMEOUT
        } else {
            AVDMANAGER_TIMEOUT
        };
        run_tool_with_stdin(tool, args, stdin, timeout)
    }
}

/// Like `crate::run_tool_ok` but with optional piped stdin and an explicit
/// timeout. Needed because `avdmanager create avd` asks an interactive
/// "custom hardware profile?" question — we answer "no".
fn run_tool_with_stdin(
    tool: &str,
    args: &[&str],
    stdin: Option<&[u8]>,
    timeout: Duration,
) -> Result<ToolOutput, DeviceError> {
    let mut child = Command::new(tool)
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DeviceError::ToolMissing(tool.to_string())
            } else {
                DeviceError::Io(e)
            }
        })?;

    if let Some(input) = stdin {
        if let Some(mut pipe) = child.stdin.take() {
            // Best effort: a closed/eagerly-exited stdin is fine.
            let _ = pipe.write_all(input);
            let _ = pipe.flush();
        }
        // `pipe` drops here → EOF for the child.
    }

    // Same drain-threads + poll-loop shape as `crate::run_tool_with_timeout`
    // so large output (sdkmanager progress) cannot deadlock the pipes.
    let mut out_pipe = child.stdout.take().expect("stdout piped");
    let mut err_pipe = child.stderr.take().expect("stderr piped");
    let out_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut out_pipe, &mut buf);
        buf
    });
    let err_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut err_pipe, &mut buf);
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_handle.join();
                    let _ = err_handle.join();
                    return Err(DeviceError::Timeout {
                        tool: tool.to_string(),
                    });
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
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
    let output = ToolOutput {
        stdout,
        stderr,
        code: status.code(),
    };
    match output.code {
        Some(0) => Ok(output),
        code => Err(DeviceError::ToolFailed {
            tool: tool.to_string(),
            code,
            stderr: output.stderr_lossy(),
        }),
    }
}

/// Errors from `setup android|ios`, with exact user-facing messages.
#[derive(Debug)]
pub enum SetupError {
    /// `sdkmanager` is not on PATH.
    SdkManagerMissing,
    /// `baguette` is not on PATH (iOS).
    BaguetteMissing,
    /// A platform tool failed.
    Device(DeviceError),
    /// The user declined the download prompt.
    Declined,
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Exact install guidance; the CLI exits 2 on this.
            SetupError::SdkManagerMissing => f.write_str(
                "sdkmanager not found on PATH (install the Android SDK command-line tools \
                 from https://developer.android.com/studio#command-line-tools-only, then \
                 make sure `sdkmanager` is on PATH)",
            ),
            // Exact string the iOS setup must print (docs/device.md §9.5).
            SetupError::BaguetteMissing => f.write_str("brew install baguette"),
            SetupError::Device(e) => write!(f, "{e}"),
            SetupError::Declined => f.write_str("setup declined by the user"),
        }
    }
}

impl Error for SetupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SetupError::Device(e) => Some(e),
            _ => None,
        }
    }
}

impl From<DeviceError> for SetupError {
    fn from(e: DeviceError) -> Self {
        SetupError::Device(e)
    }
}

/// What `setup android` installs and creates. Built by [`android_plan`];
/// pure data, no I/O.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AndroidSetupPlan {
    /// Packages passed to `sdkmanager --install`, in order.
    pub sdkmanager_packages: Vec<String>,
    /// AVD name passed to `avdmanager create avd -n`.
    pub avd_name: String,
    /// Full system-image package, e.g.
    /// `system-images;android-34;google_apis;x86_64`.
    pub system_image: String,
    /// `--device` profile for `avdmanager create avd`.
    pub device_profile: String,
}

/// Build the Android setup plan. Pure: no PATH lookups, no I/O — the
/// system image is chosen for this host's CPU (arm64 on Apple Silicon,
/// x86_64 elsewhere).
pub fn android_plan() -> AndroidSetupPlan {
    // aarch64 covers Apple Silicon macs and ARM Linux hosts.
    let image = if std::env::consts::ARCH == "aarch64" {
        "system-images;android-34;google_apis;arm64-v8a"
    } else {
        "system-images;android-34;google_apis;x86_64"
    };
    AndroidSetupPlan {
        sdkmanager_packages: vec![
            "platform-tools".to_string(),
            "emulator".to_string(),
            image.to_string(),
        ],
        avd_name: "supercli".to_string(),
        system_image: image.to_string(),
        device_profile: "pixel_7".to_string(),
    }
}

/// Check that `sdkmanager` resolves on PATH (injectable for tests).
pub fn check_sdkmanager_with(tool_exists: &dyn Fn(&str) -> bool) -> Result<(), SetupError> {
    if tool_exists("sdkmanager") {
        Ok(())
    } else {
        Err(SetupError::SdkManagerMissing)
    }
}

/// [`check_sdkmanager_with`] against the real PATH.
pub fn check_sdkmanager() -> Result<(), SetupError> {
    check_sdkmanager_with(&tool_on_path)
}

/// Install the SDK packages from the plan: accept licenses, then
/// `sdkmanager --install <packages...>`. Progress is reported through
/// `progress` (the CLI prints; tests record).
pub fn install_android_packages(
    runner: &dyn ToolRunner,
    plan: &AndroidSetupPlan,
    progress: &mut dyn FnMut(&str),
) -> Result<(), SetupError> {
    progress("Accepting Android SDK licenses (sdkmanager --licenses)...");
    // Licenses prompt repeatedly; answer "y" generously.
    let yes: Vec<u8> = "y\n".repeat(32).into_bytes();
    runner.run("sdkmanager", &["--licenses"], Some(&yes))?;

    let pkg_list = plan.sdkmanager_packages.join(" ");
    progress(&format!("Installing Android SDK packages: {pkg_list} ..."));
    let args: Vec<&str> = std::iter::once("--install")
        .chain(plan.sdkmanager_packages.iter().map(String::as_str))
        .collect();
    runner.run("sdkmanager", &args, None)?;
    progress("Android SDK packages installed.");
    Ok(())
}

/// Create the AVD from the plan via `avdmanager`. Answers "no" to the
/// interactive custom-hardware-profile prompt.
pub fn create_avd(
    runner: &dyn ToolRunner,
    plan: &AndroidSetupPlan,
    progress: &mut dyn FnMut(&str),
) -> Result<(), SetupError> {
    progress(&format!(
        "Creating AVD '{}' ({} on {})...",
        plan.avd_name, plan.system_image, plan.device_profile
    ));
    runner.run(
        "avdmanager",
        &[
            "create",
            "avd",
            "-n",
            &plan.avd_name,
            "-k",
            &plan.system_image,
            "--device",
            &plan.device_profile,
        ],
        Some(b"no\n"),
    )?;
    progress(&format!("AVD '{}' created.", plan.avd_name));
    Ok(())
}

/// Full `setup android`: licenses + packages + AVD. The caller (CLI)
/// owns the download approval prompt; this only runs tools.
pub fn setup_android(
    runner: &dyn ToolRunner,
    progress: &mut dyn FnMut(&str),
) -> Result<(), SetupError> {
    let plan = android_plan();
    install_android_packages(runner, &plan, progress)?;
    create_avd(runner, &plan, progress)?;
    progress("Android setup complete. Boot it with: supercli device boot supercli");
    Ok(())
}

/// `setup ios` with injectable tool lookup and runner (tests).
/// Returns the baguette version string on success.
pub fn setup_ios_with(
    tool_exists: &dyn Fn(&str) -> bool,
    runner: &dyn ToolRunner,
) -> Result<String, SetupError> {
    if !tool_exists("baguette") {
        return Err(SetupError::BaguetteMissing);
    }
    let out = runner.run("baguette", &["--version"], None)?;
    Ok(out.stdout_lossy().trim().to_string())
}

/// [`setup_ios_with`] against the real PATH and processes.
pub fn setup_ios() -> Result<String, SetupError> {
    setup_ios_with(&tool_on_path, &RealRunner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Mock tool runner: records every invocation, returns canned success.
    struct MockRunner {
        calls: RefCell<Vec<(String, Vec<String>, Option<Vec<u8>>)>>,
    }

    impl MockRunner {
        fn new() -> Self {
            MockRunner {
                calls: RefCell::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<(String, Vec<String>, Option<Vec<u8>>)> {
            self.calls.borrow().clone()
        }
    }

    impl ToolRunner for MockRunner {
        fn run(
            &self,
            tool: &str,
            args: &[&str],
            stdin: Option<&[u8]>,
        ) -> Result<ToolOutput, DeviceError> {
            self.calls.borrow_mut().push((
                tool.to_string(),
                args.iter().map(|s| s.to_string()).collect(),
                stdin.map(|b| b.to_vec()),
            ));
            Ok(ToolOutput {
                stdout: b"mock ok\n".to_vec(),
                stderr: Vec::new(),
                code: Some(0),
            })
        }
    }

    #[test]
    fn android_plan_picks_image_for_host_arch() {
        let plan = android_plan();
        assert_eq!(
            plan.sdkmanager_packages,
            vec![
                "platform-tools".to_string(),
                "emulator".to_string(),
                plan.system_image.clone(),
            ]
        );
        assert!(plan.system_image.contains("android-34"));
        if std::env::consts::ARCH == "aarch64" {
            assert!(plan.system_image.contains("arm64-v8a"));
        } else {
            assert!(plan.system_image.contains("x86_64"));
        }
        assert_eq!(plan.avd_name, "supercli");
        assert_eq!(plan.device_profile, "pixel_7");
    }

    #[test]
    fn check_sdkmanager_missing_gives_install_guidance() {
        let err = check_sdkmanager_with(&|_| false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("sdkmanager not found on PATH"), "{msg}");
        assert!(msg.contains("developer.android.com"), "{msg}");
        check_sdkmanager_with(&|_| true).unwrap();
    }

    #[test]
    fn install_android_packages_requests_exact_packages() {
        let plan = android_plan();
        let runner = MockRunner::new();
        let mut progress = vec![];
        install_android_packages(&runner, &plan, &mut |s: &str| progress.push(s.to_string()))
            .unwrap();

        let calls = runner.calls();
        assert_eq!(calls.len(), 2, "licenses + install, got {calls:?}");

        // 1. licenses accepted first.
        assert_eq!(calls[0].0, "sdkmanager");
        assert_eq!(calls[0].1, vec!["--licenses".to_string()]);
        let yes = calls[0].2.as_ref().expect("licenses need piped yes");
        assert_eq!(yes.len(), 64, "generous run of y answers");
        assert!(
            yes.chunks(2).all(|c| c == b"y\n"),
            "must answer y to licenses"
        );

        // 2. exact package set requested via --install.
        assert_eq!(calls[1].0, "sdkmanager");
        let mut expected = vec!["--install".to_string()];
        expected.extend(plan.sdkmanager_packages.clone());
        assert_eq!(calls[1].1, expected, "wrong sdkmanager packages");
        assert!(calls[1].2.is_none());

        assert!(progress.iter().any(|p| p.contains("Accepting")));
        assert!(progress.iter().any(|p| p.contains("Installing")));
    }

    #[test]
    fn create_avd_uses_plan_and_answers_no() {
        let plan = android_plan();
        let runner = MockRunner::new();
        let mut progress = vec![];
        create_avd(&runner, &plan, &mut |s: &str| progress.push(s.to_string())).unwrap();

        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "avdmanager");
        assert_eq!(
            calls[0].1,
            vec![
                "create".to_string(),
                "avd".to_string(),
                "-n".to_string(),
                "supercli".to_string(),
                "-k".to_string(),
                plan.system_image.clone(),
                "--device".to_string(),
                "pixel_7".to_string(),
            ]
        );
        assert_eq!(calls[0].2.as_deref(), Some(b"no\n".as_slice()));
        assert!(progress.iter().any(|p| p.contains("supercli")));
    }

    #[test]
    fn setup_android_runs_licenses_packages_avd_in_order() {
        let runner = MockRunner::new();
        let mut progress = vec![];
        setup_android(&runner, &mut |s: &str| progress.push(s.to_string())).unwrap();
        let tools: Vec<String> = runner.calls().iter().map(|c| c.0.clone()).collect();
        assert_eq!(tools, vec!["sdkmanager", "sdkmanager", "avdmanager"]);
        assert!(progress
            .last()
            .unwrap()
            .contains("supercli device boot supercli"));
    }

    #[test]
    fn setup_ios_missing_baguette_prints_brew_line() {
        let runner = MockRunner::new();
        let err = setup_ios_with(&|_| false, &runner).unwrap_err();
        // Exact string the CLI must print (docs/device.md §9.5).
        assert_eq!(err.to_string(), "brew install baguette");
        assert!(
            runner.calls().is_empty(),
            "no tools run when baguette is missing"
        );
    }

    #[test]
    fn setup_ios_present_verifies_version() {
        struct VersionRunner;
        impl ToolRunner for VersionRunner {
            fn run(
                &self,
                tool: &str,
                args: &[&str],
                _stdin: Option<&[u8]>,
            ) -> Result<ToolOutput, DeviceError> {
                assert_eq!(tool, "baguette");
                assert_eq!(args, &["--version"]);
                Ok(ToolOutput {
                    stdout: b"baguette 1.2.3\n".to_vec(),
                    stderr: Vec::new(),
                    code: Some(0),
                })
            }
        }
        let version = setup_ios_with(&|_| true, &VersionRunner).unwrap();
        assert_eq!(version, "baguette 1.2.3");
    }

    #[test]
    fn setup_error_display_strings_are_exact() {
        assert_eq!(
            SetupError::BaguetteMissing.to_string(),
            "brew install baguette"
        );
        assert_eq!(
            SetupError::Declined.to_string(),
            "setup declined by the user"
        );
        let tool_err: SetupError = DeviceError::ToolMissing("sdkmanager".to_string()).into();
        assert!(tool_err
            .to_string()
            .contains("sdkmanager not found on PATH"));
    }

    #[test]
    fn tool_failure_propagates_as_setup_error() {
        struct FailRunner;
        impl ToolRunner for FailRunner {
            fn run(
                &self,
                _tool: &str,
                _args: &[&str],
                _stdin: Option<&[u8]>,
            ) -> Result<ToolOutput, DeviceError> {
                Err(DeviceError::ToolFailed {
                    tool: "sdkmanager".to_string(),
                    code: Some(1),
                    stderr: "boom".to_string(),
                })
            }
        }
        let plan = android_plan();
        let mut progress = vec![];
        let err = install_android_packages(&FailRunner, &plan, &mut |s: &str| {
            progress.push(s.to_string())
        })
        .unwrap_err();
        assert!(err.to_string().contains("'sdkmanager' failed (exit 1)"));
    }
}
