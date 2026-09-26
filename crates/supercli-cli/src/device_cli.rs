//! `supercli device` — headless Android/iOS device control, setup verbs.
//!
//! Only the setup verbs are wired here; the backend surface
//! (list/boot/tap/stream/…) arrives with the native scrcpy client + web
//! panel work. Setup is wired first because developers and CI need it to
//! provision devices.
//!
//! Exit codes: 0 success · 1 tool failure · 2 missing tool / bad usage /
//! declined-or-non-interactive download (mirrors the platform-gating
//! convention in docs/device.md §1).

use std::io::{self, IsTerminal, Write};

use supercli_device::setup::{self, RealRunner, SetupError};

pub const HELP: &str = "\
supercli device — headless Android/iOS device control

  supercli device setup android [--yes] [--dry-run]
      install the Android SDK pieces (platform-tools, emulator, one system
      image) via sdkmanager and create the `supercli` AVD with avdmanager.
      The downloads go through an install prompt unless --yes is given.
      --dry-run prints the plan without running anything.

  supercli device setup ios
      check for baguette on PATH and verify it runs; prints
      `brew install baguette` and exits 2 when it is missing.

Backend verbs (list/boot/tap/stream/describe-ui/…) require the `device`
cargo feature and arrive with the native scrcpy client + web panel work.";

/// `args` are the raw words after `device`.
pub fn run(args: &[String]) -> i32 {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["setup", "android", flags @ ..] => setup_android(flags),
        ["setup", "ios"] => setup_ios(),
        ["setup", other, ..] => {
            eprintln!("unknown device setup target {other:?} (expected `android` or `ios`)");
            2
        }
        ["setup"] | [] => {
            println!("{HELP}");
            0
        }
        [other, ..] => {
            eprintln!("unknown device subcommand {other:?}\n{HELP}");
            2
        }
    }
}

fn setup_android(flags: &[&str]) -> i32 {
    let mut yes = false;
    let mut dry_run = false;
    for flag in flags {
        match *flag {
            "--yes" => yes = true,
            "--dry-run" => dry_run = true,
            other => {
                eprintln!("unknown flag {other:?} for `supercli device setup android`");
                return 2;
            }
        }
    }

    let plan = setup::android_plan();
    if dry_run {
        println!(
            "sdkmanager --install {}",
            plan.sdkmanager_packages.join(" ")
        );
        println!(
            "avdmanager create avd -n {} -k {} --device {}",
            plan.avd_name, plan.system_image, plan.device_profile
        );
        return 0;
    }

    if let Err(error) = setup::check_sdkmanager() {
        eprintln!("supercli device: {error}");
        return 2;
    }

    // Downloads are network fetches: approval-gated like `device install`
    // (docs/device.md §9.5), same prompt shape as apps install confirm.
    if !yes && !confirm_downloads(&plan) {
        return 2;
    }

    let mut progress = |s: &str| println!("supercli device: {s}");
    match setup::setup_android(&RealRunner, &mut progress) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("supercli device: setup android failed: {error}");
            1
        }
    }
}

fn setup_ios() -> i32 {
    match setup::setup_ios() {
        Ok(version) => {
            println!("baguette {version}");
            0
        }
        Err(SetupError::BaguetteMissing) => {
            // Exact string (docs/device.md §9.5).
            println!("brew install baguette");
            2
        }
        Err(error) => {
            eprintln!("supercli device: setup ios failed: {error}");
            1
        }
    }
}

/// Approval prompt for the SDK downloads. Non-interactive stdin refuses
/// rather than hanging (same rule as app installs).
fn confirm_downloads(plan: &supercli_device::setup::AndroidSetupPlan) -> bool {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        eprintln!(
            "supercli device: refusing to download Android SDK packages non-interactively. \
             Re-run in a terminal, or pass --yes from user-owned automation."
        );
        return false;
    }
    eprint!(
        "Download and install Android SDK packages ({})? [y/N] ",
        plan.sdkmanager_packages.join(", ")
    );
    let _ = io::stderr().flush();
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

// ---------------------------------------------------------------------------
// `device` cargo feature: MCP server entry + routed backend + HubGate
// ---------------------------------------------------------------------------

#[cfg(feature = "device")]
use std::sync::Arc;
#[cfg(feature = "device")]
use std::time::Duration;
#[cfg(feature = "device")]
use supercli_device::danger::{ApprovalDecision, ApprovalGate, DangerousOp};
#[cfg(feature = "device")]
use supercli_device::{
    adb::AdbBackend, baguette::BaguetteBackend, simctl::SimctlBackend, DeviceBackend, DeviceError,
    DeviceId, DeviceInfo, DeviceStream, Platform,
};
#[cfg(feature = "device")]
use supercli_serve::approvals::ApprovalHub;

/// [`DeviceBackend`] that routes each device id to the backend that lists it:
/// Android → adb, iOS → baguette (physical) then simctl (simulator).
/// A backend whose tooling is missing simply contributes no devices.
#[cfg(feature = "device")]
pub struct RoutedBackend {
    android: AdbBackend,
    ios_hw: BaguetteBackend,
    ios_sim: SimctlBackend,
}

#[cfg(feature = "device")]
impl RoutedBackend {
    pub fn new() -> Self {
        RoutedBackend {
            android: AdbBackend,
            ios_hw: BaguetteBackend,
            ios_sim: SimctlBackend,
        }
    }

    fn owner(&self, id: &DeviceId) -> Result<Platform, DeviceError> {
        if let Ok(devices) = self.android.list() {
            if devices.iter().any(|d| d.id == *id) {
                return Ok(Platform::Android);
            }
        }
        if let Ok(devices) = self.ios_hw.list() {
            if devices.iter().any(|d| d.id == *id) {
                return Ok(Platform::IOS);
            }
        }
        if let Ok(devices) = self.ios_sim.list() {
            if devices.iter().any(|d| d.id == *id) {
                return Ok(Platform::IOS);
            }
        }
        Err(DeviceError::Unsupported(format!(
            "unknown device '{id}': not listed by adb, baguette, or simctl"
        )))
    }

    fn backend_for(&self, platform: Platform) -> &dyn DeviceBackend {
        match platform {
            Platform::Android => &self.android,
            // Physical devices first; simulators via simctl when baguette
            // does not own the id (owner() already disambiguated).
            Platform::IOS => &self.ios_hw,
        }
    }
}

#[cfg(feature = "device")]
impl Default for RoutedBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "device")]
impl DeviceBackend for RoutedBackend {
    fn list(&self) -> Result<Vec<DeviceInfo>, DeviceError> {
        let mut out = Vec::new();
        for devices in [self.android.list(), self.ios_hw.list(), self.ios_sim.list()] {
            // A missing tool means "no devices here", not a fatal error.
            if let Ok(mut devices) = devices {
                out.append(&mut devices);
            }
        }
        out.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        out.dedup_by(|a, b| a.id == b.id);
        Ok(out)
    }

    fn boot(&self, id: &DeviceId) -> Result<(), DeviceError> {
        self.backend_for(self.owner(id)?).boot(id)
    }

    fn stop(&self, id: &DeviceId) -> Result<(), DeviceError> {
        self.backend_for(self.owner(id)?).stop(id)
    }

    fn install(&self, id: &DeviceId, path: &std::path::Path) -> Result<(), DeviceError> {
        self.backend_for(self.owner(id)?).install(id, path)
    }

    fn launch(&self, id: &DeviceId, app_id: &str) -> Result<(), DeviceError> {
        self.backend_for(self.owner(id)?).launch(id, app_id)
    }

    fn screenshot(&self, id: &DeviceId) -> Result<Vec<u8>, DeviceError> {
        self.backend_for(self.owner(id)?).screenshot(id)
    }

    fn logs(&self, id: &DeviceId, clear: bool) -> Result<String, DeviceError> {
        self.backend_for(self.owner(id)?).logs(id, clear)
    }

    fn tap(&self, id: &DeviceId, x: u32, y: u32) -> Result<(), DeviceError> {
        self.backend_for(self.owner(id)?).tap(id, x, y)
    }

    fn type_text(&self, id: &DeviceId, text: &str) -> Result<(), DeviceError> {
        self.backend_for(self.owner(id)?).type_text(id, text)
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
        self.backend_for(self.owner(id)?)
            .swipe(id, x1, y1, x2, y2, duration_ms)
    }

    fn stream(&self, id: &DeviceId) -> Result<DeviceStream, DeviceError> {
        self.backend_for(self.owner(id)?).stream(id)
    }

    fn describe_ui(&self, id: &DeviceId) -> Result<String, DeviceError> {
        // iOS simulators cannot do a11y via simctl; prefer the baguette
        // backend for any iOS id it lists (owner() guarantees this).
        self.backend_for(self.owner(id)?).describe_ui(id)
    }
}

/// Production [`ApprovalGate`] over the Host's [`ApprovalHub`] — the same
/// path `session_host` uses for tool approvals. The approval card appears on
/// the human's paired device (or the TUI); approve → the op runs, deny or
/// timeout or no-human-present → [`DeviceError::Denied`] and the backend is
/// never touched. Fail-closed by construction.
///
/// This is the real implementation the [`supercli_device::danger`] docs point
/// at. Construct it wherever an `Arc<ApprovalHub>` is available (today:
/// Host-side device endpoints); the [`GuardedBackend`] audit trail records
/// every decision.
#[cfg(feature = "device")]
pub struct HubGate {
    hub: Arc<ApprovalHub>,
    session: String,
    timeout: Duration,
}

#[cfg(feature = "device")]
impl HubGate {
    pub fn new(hub: Arc<ApprovalHub>, session: String, timeout: Duration) -> Self {
        HubGate {
            hub,
            session,
            timeout,
        }
    }
}

#[cfg(feature = "device")]
impl ApprovalGate for HubGate {
    fn decide(&self, op: &DangerousOp) -> ApprovalDecision {
        let (approved, answered_by) = self.hub.request(
            "device-danger",
            op.title(),
            op.body(),
            self.session.clone(),
            None,
            self.timeout,
        );
        if approved {
            ApprovalDecision::allow(answered_by, "approval-hub")
        } else {
            ApprovalDecision::deny("approval-hub: denied, timed out, or no human present")
        }
    }
}

/// Hidden `__device_mcp__` entry: speak MCP (JSON-RPC 2.0) on stdio, like
/// `__browser_mcp__`. Launched by the Host or an agent session, never by
/// hand. Only the 6 safe tools are exposed; dangerous ops are not on the
/// wire at all.
#[cfg(feature = "device")]
pub fn run_device_mcp() -> i32 {
    let backend = RoutedBackend::new();
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    match supercli_device::mcp::run_stdio_mcp(&backend, &mut input, &mut output) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("supercli __device_mcp__: {error}");
            1
        }
    }
}

/// Test seam: run the MCP server over caller-supplied streams.
#[cfg(feature = "device")]
pub fn run_device_mcp_with(
    backend: &dyn DeviceBackend,
    input: &mut dyn io::BufRead,
    output: &mut dyn io::Write,
) -> Result<(), String> {
    supercli_device::mcp::run_stdio_mcp(backend, input, output)
}

#[cfg(all(test, feature = "device"))]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Mutex;
    use supercli_device::danger::GuardedBackend;
    use supercli_device::DeviceState;

    /// Recording stub: every backend method logs its name; install logs the
    /// package path too. Proves gate-bypass attempts make zero backend calls.
    struct StubBackend {
        calls: Mutex<Vec<String>>,
    }

    impl StubBackend {
        fn new() -> Self {
            StubBackend {
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }

        fn log(&self, name: &str) {
            self.calls.lock().unwrap().push(name.to_string());
        }
    }

    impl DeviceBackend for StubBackend {
        fn list(&self) -> Result<Vec<DeviceInfo>, DeviceError> {
            self.log("list");
            Ok(vec![DeviceInfo {
                id: DeviceId::new("stub-1"),
                name: "stub".into(),
                platform: Platform::Android,
                state: DeviceState::Running,
            }])
        }

        fn boot(&self, _id: &DeviceId) -> Result<(), DeviceError> {
            self.log("boot");
            Ok(())
        }

        fn stop(&self, _id: &DeviceId) -> Result<(), DeviceError> {
            self.log("stop");
            Ok(())
        }

        fn install(&self, _id: &DeviceId, path: &Path) -> Result<(), DeviceError> {
            self.log(&format!("install {}", path.display()));
            Ok(())
        }

        fn launch(&self, _id: &DeviceId, _app_id: &str) -> Result<(), DeviceError> {
            self.log("launch");
            Ok(())
        }

        fn screenshot(&self, _id: &DeviceId) -> Result<Vec<u8>, DeviceError> {
            self.log("screenshot");
            Ok(vec![])
        }

        fn logs(&self, _id: &DeviceId, _clear: bool) -> Result<String, DeviceError> {
            self.log("logs");
            Ok(String::new())
        }

        fn tap(&self, _id: &DeviceId, _x: u32, _y: u32) -> Result<(), DeviceError> {
            self.log("tap");
            Ok(())
        }

        fn type_text(&self, _id: &DeviceId, _text: &str) -> Result<(), DeviceError> {
            self.log("type_text");
            Ok(())
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
            self.log("swipe");
            Ok(())
        }

        fn stream(&self, _id: &DeviceId) -> Result<DeviceStream, DeviceError> {
            self.log("stream");
            Err(DeviceError::Unsupported("stub".into()))
        }

        fn describe_ui(&self, _id: &DeviceId) -> Result<String, DeviceError> {
            self.log("describe_ui");
            Ok("{}".into())
        }
    }

    /// Drive the MCP server entry over in-memory streams.
    fn drive_mcp(backend: &dyn DeviceBackend, requests: &[&str]) -> Vec<serde_json::Value> {
        let input = requests.join("\n") + "\n";
        let mut reader = io::BufReader::new(input.as_bytes());
        let mut out = Vec::new();
        run_device_mcp_with(backend, &mut reader, &mut out).expect("mcp server runs");
        out.split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).expect("valid json response"))
            .collect()
    }

    #[test]
    fn mcp_entry_lists_exactly_the_six_safe_tools() {
        let stub = StubBackend::new();
        let responses = drive_mcp(
            &stub,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
                r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
            ],
        );
        assert_eq!(responses.len(), 2);
        let names: Vec<&str> = responses[1]["result"]["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .map(|t| t["name"].as_str().expect("tool name"))
            .collect();
        assert_eq!(
            names,
            vec![
                "device_tap",
                "device_swipe",
                "device_type",
                "device_describe_ui",
                "device_screenshot",
                "device_logs",
            ],
            "exactly the six safe tools, no dangerous ones"
        );
        assert!(
            !names
                .iter()
                .any(|n| n.contains("install") || n.contains("erase")),
            "install/uninstall/erase are not on the wire"
        );
    }

    #[test]
    fn mcp_entry_rejects_smuggled_install_with_zero_backend_calls() {
        let stub = StubBackend::new();
        let responses = drive_mcp(
            &stub,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
                r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"device_install","arguments":{"id":"stub-1","path":"/tmp/evil.apk"}}}"#,
            ],
        );
        assert_eq!(responses.len(), 2);
        let call = &responses[1];
        assert_eq!(
            call["result"]["isError"], true,
            "smuggled install is an error"
        );
        assert!(
            call["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("unknown tool"),
            "rejected as unknown tool before any backend use"
        );
        assert!(
            stub.calls().is_empty(),
            "zero backend calls for a gate-bypass attempt, got {:?}",
            stub.calls()
        );
    }

    /// Answer a real [`ApprovalHub`] request from a helper thread, like a
    /// human tapping approve/deny on their paired device.
    fn answer_next(hub: &Arc<ApprovalHub>, approved: bool) -> std::thread::JoinHandle<()> {
        let hub = hub.clone();
        std::thread::spawn(move || {
            let mut id = None;
            for _ in 0..500 {
                if let Some((pending_id, _)) = hub.front() {
                    id = Some(pending_id);
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let id = id.expect("the approval request must appear on the hub");
            assert!(
                hub.answer_legacy(&id, approved, Some("paired-device-1".to_string())),
                "answering the pending request must work"
            );
        })
    }

    #[test]
    fn hub_gate_approve_delegates_exactly_once() {
        let hub = Arc::new(ApprovalHub::default());
        let answerer = answer_next(&hub, true);
        let gate = HubGate::new(hub, "test-session".into(), Duration::from_secs(10));
        let stub = StubBackend::new();
        let guarded = GuardedBackend::new(stub, &gate, Platform::Android, None);

        let id = DeviceId::new("stub-1");
        guarded
            .install(&id, Path::new("/tmp/app.apk"))
            .expect("hub approve must run the op");

        answerer.join().expect("answerer thread");
        assert_eq!(
            guarded.inner().calls(),
            vec!["install /tmp/app.apk"],
            "approved op delegates to the real backend exactly once"
        );
    }

    #[test]
    fn hub_gate_deny_returns_denied_with_zero_backend_calls() {
        let hub = Arc::new(ApprovalHub::default());
        let answerer = answer_next(&hub, false);
        let gate = HubGate::new(hub, "test-session".into(), Duration::from_secs(10));
        let stub = StubBackend::new();
        let guarded = GuardedBackend::new(stub, &gate, Platform::Android, None);

        let id = DeviceId::new("stub-1");
        let error = guarded
            .install(&id, Path::new("/tmp/app.apk"))
            .expect_err("hub deny must fail the op");

        answerer.join().expect("answerer thread");
        assert!(
            matches!(error, DeviceError::Denied { .. }),
            "denial surfaces as DeviceError::Denied, got {error:?}"
        );
        assert!(
            guarded.inner().calls().is_empty(),
            "denied op makes zero backend calls, got {:?}",
            guarded.inner().calls()
        );
    }

    #[test]
    fn hub_gate_timeout_is_fail_closed_deny() {
        // No answerer at all: no human present must deny, never allow.
        let hub = Arc::new(ApprovalHub::default());
        let gate = HubGate::new(hub, "test-session".into(), Duration::from_millis(200));
        let stub = StubBackend::new();
        let guarded = GuardedBackend::new(stub, &gate, Platform::Android, None);

        let id = DeviceId::new("stub-1");
        let error = guarded
            .install(&id, Path::new("/tmp/app.apk"))
            .expect_err("timeout must deny");

        assert!(
            matches!(error, DeviceError::Denied { .. }),
            "timeout surfaces as DeviceError::Denied, got {error:?}"
        );
        assert!(
            guarded.inner().calls().is_empty(),
            "timed-out op makes zero backend calls"
        );
    }
}
