//! SSH Host records and the SSH transport boundary, ported from
//! `clients/native/UnpeelNative/Sources/UnpeelNative/RemoteHosts.swift` and
//! `clients/native/UnpeelNative/Sources/UnpeelNative/NativeRemoteBackend.swift`.
//!
//! The actual SSH connection machinery already lives in Rust:
//! `unpeel_core::{ssh_connection, remote_session_backend}` (the Swift
//! `NativeRemoteBackend` was only a thin ownership boundary over the Rust
//! `RemoteSessionBackend` via the C bridge). This module ports the
//! Controller-side record layer — `SSHHostRecord`, target validation, the
//! secret-store abstraction, and the `RemoteHostStore` SSH half — and the
//! desktop launcher wires these records to the real backend on a background
//! thread.

use crate::i18n::t;
use serde::{Deserialize, Serialize};

/// How the Controller reaches the remote Host over SSH.
/// Mirrors `RemoteSSHConnectionMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoteSshConnectionMode {
    Command,
    InteractiveShell,
}

impl RemoteSshConnectionMode {
    pub fn display_name(&self) -> String {
        match self {
            RemoteSshConnectionMode::Command => t("ssh.standard_ssh"),
            RemoteSshConnectionMode::InteractiveShell => t("ssh.interactive_shell"),
        }
    }

    pub fn all() -> [RemoteSshConnectionMode; 2] {
        [
            RemoteSshConnectionMode::Command,
            RemoteSshConnectionMode::InteractiveShell,
        ]
    }
}

/// A Controller-side SSH Host record. Mirrors `SSHHostRecord`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshHostRecord {
    pub id: String,
    pub name: String,
    /// `ssh://` URI or bare `user@host` / config alias.
    pub target: String,
    /// The Host's stable identity, learned when Unpeel is reached over SSH.
    pub host_id: String,
    pub mode: RemoteSshConnectionMode,
    pub uses_stored_secret: bool,
}

impl SshHostRecord {
    /// The dial destination: the target without the `ssh://` scheme prefix.
    pub fn destination(&self) -> &str {
        self.target
            .strip_prefix("ssh://")
            .unwrap_or(self.target.as_str())
    }
}

/// Setup failures, mirroring `SSHHostSetupError` (messages kept verbatim —
/// they are user-facing copy).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshHostSetupError {
    InvalidTarget,
    MissingIdentity,
    Connection {
        standard: String,
        interactive: String,
    },
    Installation {
        standard: String,
        interactive: String,
    },
    SelfPairing,
}

impl SshHostSetupError {
    pub fn message(&self) -> String {
        match self {
            SshHostSetupError::InvalidTarget => "Enter an SSH config alias or user@host. Put ports, keys, and ProxyJump settings in ~/.ssh/config.".to_string(),
            SshHostSetupError::MissingIdentity => "The remote Unpeel Host did not provide a stable identity. Update Unpeel on the Host and try again.".to_string(),
            SshHostSetupError::Connection { standard, interactive } => format!(
                "Could not start Unpeel over SSH. Standard SSH: {standard} Interactive shell: {interactive}"
            ),
            SshHostSetupError::Installation { standard, interactive } => format!(
                "Could not install Unpeel over SSH. Standard SSH: {standard} Interactive shell: {interactive}"
            ),
            SshHostSetupError::SelfPairing => {
                "This is this machine. Use the local workspace instead.".to_string()
            }
        }
    }
}

/// Validate an SSH target the way the Swift setup sheet does: either an
/// `ssh://` URI or a single `user@host` / config-alias token. Ports, keys
/// and ProxyJump live in `~/.ssh/config`, never in the target field.
pub fn validate_ssh_target(target: &str) -> Result<(), SshHostSetupError> {
    let target = target.trim();
    if target.is_empty() {
        return Err(SshHostSetupError::InvalidTarget);
    }
    if let Some(rest) = target.strip_prefix("ssh://") {
        if rest.trim().is_empty() || rest.contains(char::is_whitespace) {
            return Err(SshHostSetupError::InvalidTarget);
        }
        return Ok(());
    }
    // Bare token: no scheme, no whitespace, no slashes — an alias or
    // user@host as understood by ~/.ssh/config.
    if target.contains(char::is_whitespace) || target.contains('/') || target.contains("://") {
        return Err(SshHostSetupError::InvalidTarget);
    }
    Ok(())
}

/// Secret storage for SSH Host credentials. The launchers back this with the
/// OS credential store; tests use [`MemorySshSecretStore`].
pub trait SshSecretStore {
    type Error: std::fmt::Display;
    fn save(&mut self, secret: &str, account: &str) -> Result<(), Self::Error>;
    fn load(&self, account: &str) -> Option<String>;
    fn delete(&mut self, account: &str);
}

#[derive(Default)]
pub struct MemorySshSecretStore {
    inner: std::collections::HashMap<String, String>,
}

impl SshSecretStore for MemorySshSecretStore {
    type Error = std::convert::Infallible;
    fn save(&mut self, secret: &str, account: &str) -> Result<(), Self::Error> {
        self.inner.insert(account.to_string(), secret.to_string());
        Ok(())
    }
    fn load(&self, account: &str) -> Option<String> {
        self.inner.get(account).cloned()
    }
    fn delete(&mut self, account: &str) {
        self.inner.remove(account);
    }
}

/// The SSH half of `RemoteHostStore`: adopted records, selection, and
/// credential hygiene. Persistence of the record list is the launcher's job
/// (JSON via serde); secrets never touch the record file.
pub struct SshHostStore<S: SshSecretStore> {
    records: Vec<SshHostRecord>,
    selected_id: Option<String>,
    secrets: S,
    controller_id: String,
    local_host_id: Option<String>,
}

impl<S: SshSecretStore> SshHostStore<S> {
    pub fn new(secrets: S, controller_id: String, local_host_id: Option<String>) -> Self {
        Self {
            records: Vec::new(),
            selected_id: None,
            secrets,
            controller_id,
            local_host_id,
        }
    }

    pub fn records(&self) -> &[SshHostRecord] {
        &self.records
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.selected_id.as_deref()
    }

    pub fn selected_record(&self) -> Option<&SshHostRecord> {
        self.selected_id
            .as_deref()
            .and_then(|id| self.records.iter().find(|r| r.id == id))
    }

    fn ssh_secret_account(&self, record_id: &str) -> String {
        format!("ssh.{}.{}", self.controller_id, record_id)
    }

    fn is_local_host(&self, host_id: &str) -> bool {
        match &self.local_host_id {
            Some(local) if !local.is_empty() => local.eq_ignore_ascii_case(host_id),
            _ => false,
        }
    }

    /// Adopt (or re-adopt) an SSH Host. Mirrors `RemoteHostStore.adoptSSH`:
    /// self-pairing is refused, an existing record for the same target keeps
    /// its id, and the secret is stored only when non-empty.
    pub fn adopt_ssh(
        &mut self,
        target: &str,
        name: &str,
        host_id: &str,
        mode: RemoteSshConnectionMode,
        secret: Option<&str>,
        select: bool,
    ) -> Result<SshHostRecord, SshHostSetupError> {
        validate_ssh_target(target)?;
        if self.is_local_host(host_id) {
            return Err(SshHostSetupError::SelfPairing);
        }
        let existing = self.records.iter().find(|r| r.target == target);
        let record_id = existing
            .map(|r| r.id.clone())
            .unwrap_or_else(|| format!("ssh.{}", uuid_simple()));
        let normalized_secret = secret.filter(|s| !s.is_empty());
        let account = self.ssh_secret_account(&record_id);
        if let Some(secret) = normalized_secret {
            self.secrets
                .save(secret, &account)
                .map_err(|_| SshHostSetupError::InvalidTarget)?;
        } else {
            self.secrets.delete(&account);
        }
        let record = SshHostRecord {
            id: record_id.clone(),
            name: if name.trim().is_empty() {
                target.strip_prefix("ssh://").unwrap_or(target).to_string()
            } else {
                name.trim().to_string()
            },
            target: target.trim().to_string(),
            host_id: host_id.to_string(),
            mode,
            uses_stored_secret: normalized_secret.is_some(),
        };
        if let Some(index) = self.records.iter().position(|r| r.id == record_id) {
            self.records[index] = record.clone();
        } else {
            self.records.push(record.clone());
        }
        if select {
            self.select(&record_id);
        }
        Ok(record)
    }

    pub fn ssh_secret(&self, record_id: &str) -> Option<String> {
        self.secrets.load(&self.ssh_secret_account(record_id))
    }

    /// Select a Host only when it is usable (a stored secret exists when the
    /// record says one is needed). Mirrors `RemoteHostStore.selectHost`.
    pub fn select(&mut self, id: &str) {
        let usable = self
            .records
            .iter()
            .find(|r| r.id == id)
            .is_some_and(|r| !r.uses_stored_secret || self.ssh_secret(&r.id).is_some());
        if usable {
            self.selected_id = Some(id.to_string());
        }
    }

    pub fn deselect(&mut self) {
        self.selected_id = None;
    }

    /// Rename a Controller-side Host alias without changing its stable identity.
    pub fn rename(&mut self, id: &str, raw_name: &str) -> bool {
        let name = raw_name.trim();
        if name.is_empty() {
            return false;
        }
        if let Some(record) = self.records.iter_mut().find(|r| r.id == id) {
            if record.name == name {
                return true;
            }
            record.name = name.to_string();
            return true;
        }
        false
    }

    /// Forget a record and delete its secret. Mirrors `RemoteHostStore.forget`.
    pub fn forget(&mut self, id: &str) {
        if let Some(record) = self.records.iter().find(|r| r.id == id) {
            self.secrets.delete(&self.ssh_secret_account(&record.id));
        }
        self.records.retain(|r| r.id != id);
        if self.selected_id.as_deref() == Some(id) {
            self.selected_id = None;
        }
    }

    /// Restore persisted records (dedupe by id, drop empties) — mirrors the
    /// Swift load path's filtering.
    pub fn restore(&mut self, records: Vec<SshHostRecord>) {
        let mut seen = std::collections::HashSet::new();
        self.records = records
            .into_iter()
            .filter(|r| {
                r.id.starts_with("ssh.")
                    && !r.target.is_empty()
                    && !r.host_id.is_empty()
                    && seen.insert(r.id.clone())
            })
            .collect();
        if let Some(selected) = self.selected_id.clone() {
            if !self.is_usable(&selected) {
                self.selected_id = None;
            }
        }
    }

    fn is_usable(&self, id: &str) -> bool {
        self.records
            .iter()
            .find(|r| r.id == id)
            .is_some_and(|r| !r.uses_stored_secret || self.ssh_secret(&r.id).is_some())
    }
}

/// Minimal UUID-ish id without a dependency (not a v4 UUID; only used for
/// local record ids, never on the wire).
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    format!("{nanos:032x}-{pid:08x}")
}

/// The SSH transport runner contract the desktop launcher implements on a
/// background thread against `unpeel_core::remote_session_backend`.
/// Kept as a trait so the UI layer never touches the backend directly.
pub trait SshTransportRunner {
    type Error: std::fmt::Display;
    /// Start Unpeel on the remote Host over SSH (installing first when
    /// needed) and return the Host's stable identity.
    fn ensure_remote_host(
        &self,
        target: &str,
        mode: RemoteSshConnectionMode,
    ) -> Result<String, Self::Error>;
}

/// Dioxus component: the SSH Host picker rows (paired/SSH switcher).
pub mod component {
    use super::*;
    use dioxus::prelude::*;

    #[component]
    pub fn SshHostRows(
        records: Vec<SshHostRecord>,
        selected_id: Option<String>,
        on_select: EventHandler<String>,
        on_forget: EventHandler<String>,
    ) -> Element {
        rsx! {
            ul { class: "ssh-host-rows",
                for r in records {
                    li {
                        key: "{r.id}",
                        button {
                            class: if Some(r.id.as_str()) == selected_id.as_deref() { "ssh-row selected" } else { "ssh-row" },
                            onclick: {
                                let id = r.id.clone();
                                move |_| on_select.call(id.clone())
                            },
                            span { class: "ssh-row-name", "{r.name}" }
                            span { class: "ssh-row-target", "{r.destination()} · {r.mode.display_name()}" }
                        }
                        button {
                            class: "ssh-row-forget",
                            onclick: {
                                let id = r.id.clone();
                                move |_| on_forget.call(id.clone())
                            },
                            {t("ssh.forget")}
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SshHostStore<MemorySshSecretStore> {
        SshHostStore::new(
            MemorySshSecretStore::default(),
            "controller-1".to_string(),
            Some("local-host".to_string()),
        )
    }

    #[test]
    fn target_validation() {
        assert!(validate_ssh_target("ssh://user@example.com").is_ok());
        assert!(validate_ssh_target("my-alias").is_ok());
        assert!(validate_ssh_target("user@example.com").is_ok());
        assert!(validate_ssh_target("  user@example.com  ").is_ok());
        assert!(validate_ssh_target("").is_err());
        assert!(validate_ssh_target("   ").is_err());
        assert!(validate_ssh_target("ssh://").is_err());
        assert!(validate_ssh_target("ssh://user@host extra").is_err());
        assert!(validate_ssh_target("user@host extra").is_err());
        assert!(validate_ssh_target("/etc/passwd").is_err());
        assert!(validate_ssh_target("https://example.com").is_err());
        let err = validate_ssh_target("").unwrap_err();
        assert!(err.message().contains("~/.ssh/config"));
    }

    #[test]
    fn destination_strips_scheme() {
        let r = SshHostRecord {
            id: "ssh.1".into(),
            name: "n".into(),
            target: "ssh://user@host".into(),
            host_id: "h".into(),
            mode: RemoteSshConnectionMode::Command,
            uses_stored_secret: false,
        };
        assert_eq!(r.destination(), "user@host");
    }

    #[test]
    fn adopt_reject_self_pairing() {
        let mut s = store();
        let err = s
            .adopt_ssh(
                "my-alias",
                "Me",
                "LOCAL-HOST",
                RemoteSshConnectionMode::Command,
                None,
                true,
            )
            .unwrap_err();
        assert_eq!(err, SshHostSetupError::SelfPairing);
        assert!(s.records().is_empty());
    }

    #[test]
    fn adopt_stores_secret_and_selects() {
        let mut s = store();
        let r = s
            .adopt_ssh(
                "ssh://deploy@prod",
                "",
                "host-9",
                RemoteSshConnectionMode::InteractiveShell,
                Some("s3cret"),
                true,
            )
            .unwrap();
        assert!(r.id.starts_with("ssh."));
        assert_eq!(r.name, "deploy@prod"); // empty name → destination
        assert!(r.uses_stored_secret);
        assert_eq!(r.mode.display_name(), "Interactive shell");
        assert_eq!(s.selected_id(), Some(r.id.as_str()));
        assert_eq!(s.ssh_secret(&r.id).as_deref(), Some("s3cret"));

        // Re-adopt the same target: id is stable, secret cleared when empty.
        let r2 = s
            .adopt_ssh(
                "ssh://deploy@prod",
                "Prod",
                "host-9",
                RemoteSshConnectionMode::Command,
                Some(""),
                true,
            )
            .unwrap();
        assert_eq!(r2.id, r.id);
        assert!(!r2.uses_stored_secret);
        assert_eq!(s.ssh_secret(&r.id), None);
    }

    #[test]
    fn select_requires_usable_secret() {
        let mut s = store();
        let r = s
            .adopt_ssh(
                "a",
                "A",
                "h1",
                RemoteSshConnectionMode::Command,
                Some("pw"),
                false,
            )
            .unwrap();
        s.deselect();
        // Secret present → selectable.
        s.select(&r.id);
        assert_eq!(s.selected_id(), Some(r.id.as_str()));
        // Delete the secret behind the store's back → no longer selectable.
        s.secrets.delete(&s.ssh_secret_account(&r.id));
        s.deselect();
        s.select(&r.id);
        assert_eq!(s.selected_id(), None);
    }

    #[test]
    fn rename_and_forget() {
        let mut s = store();
        let r = s
            .adopt_ssh(
                "a",
                "A",
                "h1",
                RemoteSshConnectionMode::Command,
                Some("pw"),
                true,
            )
            .unwrap();
        assert!(!s.rename(&r.id, "   "));
        assert!(s.rename(&r.id, "Renamed"));
        assert_eq!(s.records()[0].name, "Renamed");
        s.forget(&r.id);
        assert!(s.records().is_empty());
        assert_eq!(s.selected_id(), None);
        assert_eq!(s.ssh_secret(&r.id), None);
    }

    #[test]
    fn restore_filters_and_dedups() {
        let mut s = store();
        let good = SshHostRecord {
            id: "ssh.1".into(),
            name: "n".into(),
            target: "a".into(),
            host_id: "h".into(),
            mode: RemoteSshConnectionMode::Command,
            uses_stored_secret: false,
        };
        let mut bad = good.clone();
        bad.id = "nope".into();
        let mut dup = good.clone();
        dup.name = "dup".into();
        s.restore(vec![good.clone(), bad, dup, good.clone()]);
        assert_eq!(s.records().len(), 1);
        assert_eq!(s.records()[0].name, "n");
    }

    #[test]
    fn setup_error_messages_verbatim() {
        assert!(SshHostSetupError::InvalidTarget
            .message()
            .starts_with("Enter an SSH config alias"));
        let e = SshHostSetupError::Connection {
            standard: "s".into(),
            interactive: "i".into(),
        };
        assert!(e.message().contains("Standard SSH: s"));
    }
}
