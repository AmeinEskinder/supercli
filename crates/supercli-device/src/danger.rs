//! Approval-gated dangerous device operations: install, uninstall, erase.
//!
//! These operations are never exposed over MCP ([`crate::mcp`] omits them
//! deliberately) and never run without an explicit approval decision.
//! [`GuardedBackend`] wraps any [`DeviceBackend`]: every dangerous op goes
//! through an [`ApprovalGate`] first, and every decision — allow or deny —
//! is appended to a JSONL audit log.
//!
//! Wiring into the product approval flow: the production [`ApprovalGate`]
//! is `HubGate` in the `supercli-cli` crate (`device_cli` module) — it
//! cannot live here because this crate is `std`-only and must not depend
//! on `supercli-serve`. `HubGate` calls the Host's `ApprovalHub::request`
//! with kind `"device-danger"`, title from [`DangerousOp::title`], body from
//! [`DangerousOp::body`]; a denial (or timeout, or no-human-present) maps
//! to `approved: false`, which surfaces as [`DeviceError::Denied`] with
//! zero backend calls.
//!
//! Compiled only with the `device` cargo feature.

use super::{run_tool_ok, tool_on_path, DeviceBackend, DeviceError, DeviceId, Platform};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Which dangerous operation is being requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DangerousOpKind {
    Install,
    Uninstall,
    Erase,
}

impl DangerousOpKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DangerousOpKind::Install => "install",
            DangerousOpKind::Uninstall => "uninstall",
            DangerousOpKind::Erase => "erase",
        }
    }
}

impl std::fmt::Display for DangerousOpKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A dangerous operation awaiting (or holding) an approval decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DangerousOp {
    pub kind: DangerousOpKind,
    pub device: DeviceId,
    /// Package path (install), package/bundle id (uninstall), "" (erase).
    pub target: String,
    /// Human-readable detail, e.g. the APK file name.
    pub detail: String,
}

impl DangerousOp {
    /// Short title for an approval card.
    pub fn title(&self) -> String {
        match self.kind {
            DangerousOpKind::Install => {
                format!("Install app on {}?", self.device)
            }
            DangerousOpKind::Uninstall => {
                format!("Uninstall {} from {}?", self.target, self.device)
            }
            DangerousOpKind::Erase => {
                format!("Erase all data on {}?", self.device)
            }
        }
    }

    /// Body for an approval card: what, where, and the blast radius.
    pub fn body(&self) -> String {
        match self.kind {
            DangerousOpKind::Install => format!(
                "Install package '{}' ({}) on device '{}'.\nThis grants the app its requested permissions on the device.",
                self.target, self.detail, self.device
            ),
            DangerousOpKind::Uninstall => format!(
                "Uninstall '{}' from device '{}'.\nApp data for this package will be removed.",
                self.target, self.device
            ),
            DangerousOpKind::Erase => format!(
                "WIPE device '{}': erase all user data and settings.\nThis is irreversible. The device will reboot.",
                self.device
            ),
        }
    }
}

/// The verdict of an [`ApprovalGate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalDecision {
    /// True only on an explicit human allow. Timeouts, errors, and
    /// non-interactive contexts must produce `approved: false`.
    pub approved: bool,
    /// Who approved (paired-device id, user name…), when known.
    pub approved_by: Option<String>,
    /// Why: policy name, prompt id, or gate-specific note.
    pub note: String,
}

impl ApprovalDecision {
    pub fn allow(approved_by: Option<String>, note: impl Into<String>) -> Self {
        ApprovalDecision {
            approved: true,
            approved_by,
            note: note.into(),
        }
    }

    pub fn deny(note: impl Into<String>) -> Self {
        ApprovalDecision {
            approved: false,
            approved_by: None,
            note: note.into(),
        }
    }
}

/// Decides whether a [`DangerousOp`] may run. Implementations must fail
/// closed: any uncertainty (timeout, missing human, error) → deny.
pub trait ApprovalGate {
    fn decide(&self, op: &DangerousOp) -> ApprovalDecision;
}

/// [`ApprovalGate`] from a closure. Handy for tests and for embedding.
pub struct FnGate<F>(pub F)
where
    F: Fn(&DangerousOp) -> ApprovalDecision;

impl<F> ApprovalGate for FnGate<F>
where
    F: Fn(&DangerousOp) -> ApprovalDecision,
{
    fn decide(&self, op: &DangerousOp) -> ApprovalDecision {
        (self.0)(op)
    }
}

/// One audit record per dangerous-op decision. Appended as JSONL; every
/// decision is recorded, allows and denials alike.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DangerAudit {
    pub timestamp_ms: u64,
    pub op: String,
    pub device: String,
    pub target: String,
    pub approved: bool,
    pub approved_by: Option<String>,
    pub note: String,
}

impl DangerAudit {
    pub fn new(op: &DangerousOp, decision: &ApprovalDecision) -> Self {
        DangerAudit {
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            op: op.kind.as_str().to_string(),
            device: op.device.to_string(),
            target: op.target.clone(),
            approved: decision.approved,
            approved_by: decision.approved_by.clone(),
            note: decision.note.clone(),
        }
    }

    pub fn to_json(&self) -> String {
        let by = match &self.approved_by {
            Some(b) => format!("\"{}\"", super::ui::json_escape(b)),
            None => "null".to_string(),
        };
        format!(
            "{{\"ts\":{},\"op\":\"{}\",\"device\":\"{}\",\"target\":\"{}\",\
             \"approved\":{},\"approved_by\":{},\"note\":\"{}\"}}",
            self.timestamp_ms,
            super::ui::json_escape(&self.op),
            super::ui::json_escape(&self.device),
            super::ui::json_escape(&self.target),
            self.approved,
            by,
            super::ui::json_escape(&self.note),
        )
    }
}

/// Append one audit record (JSONL) to `path`, creating parent directories.
/// Best-effort: callers log the error but must not fail the gate on it —
/// the denial/allow decision itself is already made.
pub fn append_danger_audit(path: &Path, record: &DangerAudit) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", record.to_json())?;
    f.sync_all()?;
    Ok(())
}

/// [`DeviceBackend`] wrapper that approval-gates install/uninstall/erase.
///
/// Safe operations (tap, swipe, type, screenshot, describe-ui, logs, list,
/// boot, launch, stream) delegate directly. Dangerous ones ask the
/// [`ApprovalGate`] first; a denial returns [`DeviceError::Denied`] and the
/// inner backend is never touched. Every decision is audited when
/// `audit_path` is set.
pub struct GuardedBackend<'a, B: DeviceBackend> {
    inner: B,
    gate: &'a dyn ApprovalGate,
    platform: Platform,
    audit_path: Option<PathBuf>,
}

impl<'a, B: DeviceBackend> GuardedBackend<'a, B> {
    pub fn new(
        inner: B,
        gate: &'a dyn ApprovalGate,
        platform: Platform,
        audit_path: Option<PathBuf>,
    ) -> Self {
        GuardedBackend {
            inner,
            gate,
            platform,
            audit_path,
        }
    }

    /// Access the wrapped backend (for safe operations).
    pub fn inner(&self) -> &B {
        &self.inner
    }

    fn check(&self, op: DangerousOp) -> Result<(), DeviceError> {
        let decision = self.gate.decide(&op);
        if let Some(path) = &self.audit_path {
            let record = DangerAudit::new(&op, &decision);
            if let Err(e) = append_danger_audit(path, &record) {
                // Audit failure must not silently pass a dangerous op, and
                // must not mask a denial either: report it honestly.
                return Err(DeviceError::Io(e));
            }
        }
        if decision.approved {
            Ok(())
        } else {
            Err(DeviceError::Denied {
                op: format!("{} {}", op.kind, op.target).trim_end().to_string(),
            })
        }
    }

    /// Approval-gated install. Delegates to the inner backend on allow.
    pub fn install(&self, id: &DeviceId, package: &Path) -> Result<(), DeviceError> {
        self.check(DangerousOp {
            kind: DangerousOpKind::Install,
            device: id.clone(),
            target: package.display().to_string(),
            detail: package
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        })?;
        self.inner.install(id, package)
    }

    /// Approval-gated uninstall.
    /// Android: `adb -s <serial> uninstall <package>`.
    /// iOS: `xcrun simctl uninstall <udid> <bundle-id>`.
    pub fn uninstall(&self, id: &DeviceId, package: &str) -> Result<(), DeviceError> {
        self.check(DangerousOp {
            kind: DangerousOpKind::Uninstall,
            device: id.clone(),
            target: package.to_string(),
            detail: String::new(),
        })?;
        match self.platform {
            Platform::Android => {
                if !tool_on_path("adb") {
                    return Err(DeviceError::ToolMissing("adb".to_string()));
                }
                run_tool_ok("adb", &["-s", id.as_str(), "uninstall", package])?;
                Ok(())
            }
            Platform::IOS => {
                #[cfg(not(target_os = "macos"))]
                {
                    Err(DeviceError::NotMacOSHost)
                }
                #[cfg(target_os = "macos")]
                {
                    if !tool_on_path("xcrun") {
                        return Err(DeviceError::ToolMissing("xcrun".to_string()));
                    }
                    run_tool_ok("xcrun", &["simctl", "uninstall", id.as_str(), package])?;
                    Ok(())
                }
            }
        }
    }

    /// Approval-gated erase (factory wipe of user data).
    /// Android: `adb -s <serial> reboot bootloader` then `fastboot -s
    /// <serial> -w` (requires an unlocked bootloader; the device reboots).
    /// iOS: `xcrun simctl erase <udid>`.
    pub fn erase(&self, id: &DeviceId) -> Result<(), DeviceError> {
        self.check(DangerousOp {
            kind: DangerousOpKind::Erase,
            device: id.clone(),
            target: String::new(),
            detail: "factory wipe of user data".to_string(),
        })?;
        match self.platform {
            Platform::Android => {
                if !tool_on_path("adb") {
                    return Err(DeviceError::ToolMissing("adb".to_string()));
                }
                if !tool_on_path("fastboot") {
                    return Err(DeviceError::ToolMissing("fastboot".to_string()));
                }
                run_tool_ok("adb", &["-s", id.as_str(), "reboot", "bootloader"])?;
                run_tool_ok("fastboot", &["-s", id.as_str(), "-w"])?;
                Ok(())
            }
            Platform::IOS => {
                #[cfg(not(target_os = "macos"))]
                {
                    Err(DeviceError::NotMacOSHost)
                }
                #[cfg(target_os = "macos")]
                {
                    if !tool_on_path("xcrun") {
                        return Err(DeviceError::ToolMissing("xcrun".to_string()));
                    }
                    run_tool_ok("xcrun", &["simctl", "erase", id.as_str()])?;
                    Ok(())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeBackend;
    use std::path::PathBuf;

    fn allow_gate() -> FnGate<impl Fn(&DangerousOp) -> ApprovalDecision> {
        FnGate(|op: &DangerousOp| {
            ApprovalDecision::allow(
                Some("test-human".to_string()),
                format!("allowed {}", op.kind),
            )
        })
    }

    fn deny_gate() -> FnGate<impl Fn(&DangerousOp) -> ApprovalDecision> {
        FnGate(|_: &DangerousOp| ApprovalDecision::deny("policy: no dangerous ops in tests"))
    }

    fn android_guarded<'a>(
        gate: &'a dyn ApprovalGate,
        audit: Option<PathBuf>,
    ) -> GuardedBackend<'a, FakeBackend> {
        GuardedBackend::new(
            FakeBackend::new(vec![FakeBackend::android_running()]),
            gate,
            Platform::Android,
            audit,
        )
    }

    #[test]
    fn denied_install_never_touches_inner_backend() {
        let gate = deny_gate();
        let g = android_guarded(&gate, None);
        let id = DeviceId::new("emulator-5554");
        let err = g.install(&id, Path::new("/tmp/app.apk")).unwrap_err();
        match &err {
            DeviceError::Denied { op } => assert!(op.contains("install"), "got {op}"),
            other => panic!("expected Denied, got {other:?}"),
        }
        assert!(g.inner().calls().is_empty(), "inner backend untouched");
        assert!(err.to_string().contains("denied by approval gate"));
    }

    #[test]
    fn allowed_install_delegates_and_audits() {
        let dir = std::env::temp_dir().join(format!("danger-audit-{}", std::process::id()));
        let audit_path = dir.join("audit.jsonl");
        let gate = allow_gate();
        let g = android_guarded(&gate, Some(audit_path.clone()));
        let id = DeviceId::new("emulator-5554");
        g.install(&id, Path::new("/tmp/app.apk")).expect("allowed");
        // Inner backend saw exactly one install call.
        let calls = g.inner().calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, "install");

        // Audit log has the allow record.
        let text = std::fs::read_to_string(&audit_path).expect("audit written");
        assert!(text.contains("\"op\":\"install\""), "got {text}");
        assert!(text.contains("\"approved\":true"), "got {text}");
        assert!(text.contains("test-human"), "got {text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn denied_uninstall_is_audited_too() {
        let dir = std::env::temp_dir().join(format!("danger-deny-{}", std::process::id()));
        let audit_path = dir.join("audit.jsonl");
        let gate = deny_gate();
        let g = android_guarded(&gate, Some(audit_path.clone()));
        let id = DeviceId::new("emulator-5554");
        let err = g.uninstall(&id, "com.example.app").unwrap_err();
        assert!(matches!(err, DeviceError::Denied { .. }));
        let text = std::fs::read_to_string(&audit_path).expect("audit written");
        assert!(text.contains("\"op\":\"uninstall\""), "got {text}");
        assert!(text.contains("\"approved\":false"), "got {text}");
        assert!(text.contains("com.example.app"), "got {text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn op_title_and_body_are_human_readable() {
        let op = DangerousOp {
            kind: DangerousOpKind::Erase,
            device: DeviceId::new("emulator-5554"),
            target: String::new(),
            detail: String::new(),
        };
        assert!(op.title().contains("Erase"));
        assert!(op.body().contains("irreversible"));
        let inst = DangerousOp {
            kind: DangerousOpKind::Install,
            device: DeviceId::new("emulator-5554"),
            target: "/tmp/app.apk".to_string(),
            detail: "app.apk".to_string(),
        };
        assert!(inst.title().contains("Install"));
    }

    #[test]
    fn audit_json_is_well_formed() {
        let op = DangerousOp {
            kind: DangerousOpKind::Uninstall,
            device: DeviceId::new("ABC\"123"),
            target: "com.example".to_string(),
            detail: String::new(),
        };
        let d = ApprovalDecision::deny("no\"pe");
        let rec = DangerAudit::new(&op, &d);
        let j = rec.to_json();
        // Escaped quotes keep the JSON valid per the crate parser.
        assert!(crate::json::parse_json(&j).is_ok(), "got {j}");
        assert!(j.contains("\"approved\":false"));
    }
}
