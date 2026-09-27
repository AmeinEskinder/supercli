//! Controller-side paired-Host record management.
//!
//! Ported from `clients/legacy/native/SupercliNative/Sources/SupercliNative/RemoteHosts.swift`
//! (`RemoteHostStore`). This is the UI-free record layer: adopting pairing
//! responses, forgetting hosts, renaming, selecting, and Link scoping.
//! Credential bytes stay behind the [`CredentialStore`] trait (Keychain on
//! device, memory in tests); public metadata is plain data.
//!
//! The live-connection registry ([`crate::hosts::HostRegistry`]) sits above
//! this: it owns the [`crate::transport::HostClient`] per Host, while this
//! module owns the persisted records those clients are built from.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::pairing::{RemotePairingResponse, PAIRING_PROTOCOL_VERSION};
use crate::types::PairedHostRecord;

/// Failures of Host record management. Mirrors Swift `RemoteHostPairingError`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HostStoreError {
    #[error("that is not a Supercli pairing code")]
    InvalidCode,
    #[error("this Host uses an incompatible pairing protocol")]
    IncompatibleProtocol,
    #[error("that code belongs to a different Host")]
    CandidateMismatch,
    #[error("this is this machine; choose Local instead")]
    SelfPairing,
    #[error("that pairing code expired")]
    Expired,
    #[error("the Host rejected the pairing request: {0}")]
    HostRejected(String),
    #[error("the Host pairing response could not be authenticated")]
    Authentication,
    #[error("credential storage failed: {0}")]
    Credential(String),
}

/// Minimal credential-store surface needed by record management.
/// The production Keychain implementation lives behind `credentials::`.
pub trait HostCredentialStore {
    fn save(&mut self, account: &str, secret: &str) -> Result<(), String>;
    fn load(&self, account: &str) -> Option<String>;
    fn delete(&mut self, account: &str);
}

/// In-memory credential store for tests and non-Keychain platforms.
#[derive(Default)]
pub struct MemoryHostCredentialStore {
    secrets: HashMap<String, String>,
}

impl HostCredentialStore for MemoryHostCredentialStore {
    fn save(&mut self, account: &str, secret: &str) -> Result<(), String> {
        self.secrets.insert(account.to_string(), secret.to_string());
        Ok(())
    }

    fn load(&self, account: &str) -> Option<String> {
        self.secrets.get(account).cloned()
    }

    fn delete(&mut self, account: &str) {
        self.secrets.remove(account);
    }
}

/// One SSH host record. Mirrors Swift `SSHHostRecord`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshHostRecord {
    pub id: String,
    pub name: String,
    pub target: String,
    pub host_id: String,
    pub uses_stored_secret: bool,
}

impl SshHostRecord {
    /// Everything after the `ssh://` prefix.
    pub fn destination(&self) -> &str {
        self.target.strip_prefix("ssh://").unwrap_or(&self.target)
    }
}

/// Controller-side store of paired and SSH host records.
///
/// Mirrors Swift `RemoteHostStore` minus the `@Published` UI bindings:
/// `records`, `ssh_records`, and `selected_host_id` are plain fields the
/// launcher reads after each mutation.
pub struct RemoteHostStore<S: HostCredentialStore> {
    /// Paired (Direct/Link) host records, never including this machine.
    pub records: Vec<PairedHostRecord>,
    /// SSH host records.
    pub ssh_records: Vec<SshHostRecord>,
    /// Selected host id; `None` means Local.
    pub selected_host_id: Option<String>,
    credential_store: S,
    controller_id: String,
    local_host_id: Option<String>,
}

impl<S: HostCredentialStore> RemoteHostStore<S> {
    pub fn new(credential_store: S, controller_id: String, local_host_id: Option<String>) -> Self {
        Self {
            records: Vec::new(),
            ssh_records: Vec::new(),
            selected_host_id: None,
            credential_store,
            controller_id,
            local_host_id,
        }
    }

    fn credential_account(&self, host_id: &str) -> String {
        format!("pairing.{}.{}", self.controller_id, host_id)
    }

    fn is_local_host(&self, host_id: &str) -> bool {
        match &self.local_host_id {
            Some(local) if !local.is_empty() => local.eq_ignore_ascii_case(host_id),
            _ => false,
        }
    }

    /// Credentials for a paired host, if stored.
    pub fn credentials_for(&self, host_id: &str) -> Option<String> {
        self.credential_store
            .load(&self.credential_account(host_id))
    }

    /// Persist a successfully authenticated pairing response.
    ///
    /// Metadata is committed only after the credential store accepts the
    /// secret, so a crash cannot leave a picker row that can never connect.
    /// Mirrors Swift `RemoteHostStore.adopt`.
    pub fn adopt(
        &mut self,
        response: &RemotePairingResponse,
        certificate_fingerprint: Option<String>,
        select: bool,
    ) -> Result<PairedHostRecord, HostStoreError> {
        if response.protocol_version != PAIRING_PROTOCOL_VERSION {
            return Err(HostStoreError::IncompatibleProtocol);
        }
        if self.is_local_host(&response.mac_id) {
            return Err(HostStoreError::SelfPairing);
        }
        let relay = &response.relay_credentials;
        if response.mac_id.is_empty()
            || response.device_id != self.controller_id
            || response.auth_token.is_empty()
            || relay.mac_id != response.mac_id
            || relay.relay_token.is_empty()
            || !relay.relay_url.to_ascii_lowercase().starts_with("wss://")
            || relay.e2e_key().is_none()
        {
            return Err(HostStoreError::Authentication);
        }
        self.credential_store
            .save(
                &self.credential_account(&response.mac_id),
                &response.auth_token,
            )
            .map_err(HostStoreError::Credential)?;
        let record = PairedHostRecord::from_pairing_response(response, certificate_fingerprint);
        upsert_record(&mut self.records, record.clone());
        if select {
            self.select_host(Some(record.host_id.clone()));
        }
        Ok(record)
    }

    /// Adopt an SSH host record, upserting by target. Mirrors Swift
    /// `RemoteHostStore.adoptSSH` (minus the live SSH probe, which the
    /// launcher performs separately).
    pub fn adopt_ssh(
        &mut self,
        target: String,
        name: String,
        host_id: String,
        secret: Option<String>,
        select: bool,
    ) -> Result<SshHostRecord, HostStoreError> {
        if self.is_local_host(&host_id) {
            return Err(HostStoreError::SelfPairing);
        }
        let existing_id = self
            .ssh_records
            .iter()
            .find(|r| r.target == target)
            .map(|r| r.id.clone());
        let record_id = existing_id.unwrap_or_else(|| format!("ssh.{}", uuid_simple()));
        let account = format!("ssh.{}.{}", self.controller_id, record_id);
        let has_secret = secret.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
        match secret.as_ref().filter(|s| !s.is_empty()) {
            Some(s) => self
                .credential_store
                .save(&account, s)
                .map_err(HostStoreError::Credential)?,
            None => self.credential_store.delete(&account),
        }
        let display_name = if name.trim().is_empty() {
            target.strip_prefix("ssh://").unwrap_or(&target).to_string()
        } else {
            name
        };
        let record = SshHostRecord {
            id: record_id.clone(),
            name: display_name,
            target,
            host_id,
            uses_stored_secret: has_secret,
        };
        match self.ssh_records.iter_mut().find(|r| r.id == record_id) {
            Some(slot) => *slot = record.clone(),
            None => self.ssh_records.push(record.clone()),
        }
        if select {
            self.select_host(Some(record.id.clone()));
        }
        Ok(record)
    }

    /// Select a host. A paired host is usable only with stored credentials;
    /// an SSH host only when its required secret is present. Unusable or
    /// unknown ids are ignored (selection stays put). `None` selects Local.
    /// Mirrors Swift `RemoteHostStore.selectHost`.
    pub fn select_host(&mut self, host_id: Option<String>) {
        let Some(id) = host_id else {
            self.selected_host_id = None;
            return;
        };
        let paired_usable =
            self.records.iter().any(|r| r.host_id == id) && self.credentials_for(&id).is_some();
        let ssh_usable = self
            .ssh_records
            .iter()
            .find(|r| r.id == id)
            .map(|r| {
                !r.uses_stored_secret
                    || self
                        .credential_store
                        .load(&format!("ssh.{}.{}", self.controller_id, r.id))
                        .is_some()
            })
            .unwrap_or(false);
        if paired_usable || ssh_usable {
            self.selected_host_id = Some(id);
        }
    }

    /// Scope a paired host to Direct-only (`false`) or restore its Link
    /// fallback (`true`). Narrows-only storage: allowed is the default, so
    /// only `false` is ever persisted. Mirrors Swift
    /// `RemoteHostStore.setLinkEnabled`.
    pub fn set_link_enabled(&mut self, enabled: bool, host_id: &str) {
        if let Some(record) = self.records.iter_mut().find(|r| r.host_id == host_id) {
            if record.is_link_enabled() != enabled {
                record.link_enabled = if enabled { None } else { Some(false) };
            }
        }
    }

    /// Rename a controller-side host alias. The explicit alias wins over
    /// whatever the host advertises until changed again. Returns false when
    /// the name is blank or the host is unknown. Mirrors Swift
    /// `RemoteHostStore.renameHost`.
    pub fn rename_host(&mut self, host_id: &str, raw_name: &str) -> bool {
        let name = raw_name.trim();
        if name.is_empty() {
            return false;
        }
        if let Some(record) = self.records.iter_mut().find(|r| r.host_id == host_id) {
            if record.name == name {
                return true;
            }
            record.name = name.to_string();
            return true;
        }
        if let Some(record) = self.ssh_records.iter_mut().find(|r| r.id == host_id) {
            if record.name == name {
                return true;
            }
            record.name = name.to_string();
            return true;
        }
        false
    }

    /// Forget a host: drop its record and credentials. If it was selected,
    /// fall back to Local. Mirrors Swift `RemoteHostStore.forget`.
    pub fn forget(&mut self, host_id: &str) {
        if let Some(ssh) = self.ssh_records.iter().find(|r| r.id == host_id).cloned() {
            self.ssh_records.retain(|r| r.id != host_id);
            self.credential_store
                .delete(&format!("ssh.{}.{}", self.controller_id, ssh.id));
        } else {
            self.records.retain(|r| r.host_id != host_id);
            self.credential_store
                .delete(&self.credential_account(host_id));
        }
        if self.selected_host_id.as_deref() == Some(host_id) {
            self.selected_host_id = None;
        }
    }
}

fn upsert_record(records: &mut Vec<PairedHostRecord>, record: PairedHostRecord) {
    match records.iter_mut().find(|r| r.host_id == record.host_id) {
        Some(slot) => *slot = record,
        None => records.push(record),
    }
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}", nanos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_response() -> RemotePairingResponse {
        RemotePairingResponse {
            protocol_version: PAIRING_PROTOCOL_VERSION,
            mac_id: "MACID-UPPER".to_string(),
            mac_name: "Test Mac".to_string(),
            endpoint: "https://192.168.1.5:443".to_string(),
            direct_endpoint: None,
            device_id: "controller-1".to_string(),
            auth_token: "auth-token-abc".to_string(),
            relay_credentials: crate::relay::RelayCredentials {
                mac_id: "MACID-UPPER".to_string(),
                relay_token: "relay-token-xyz".to_string(),
                relay_url: "wss://relay.superc.li".to_string(),
                e2e_key_b64: {
                    use base64::Engine;
                    base64::engine::general_purpose::STANDARD.encode([7u8; 32])
                },
            },
            paired_at_unix_ms: 1_700_000_000_000,
            remote_server_port: None,
            remote_server_certificate_fingerprint: None,
            server_version: None,
        }
    }

    fn store() -> RemoteHostStore<MemoryHostCredentialStore> {
        RemoteHostStore::new(
            MemoryHostCredentialStore::default(),
            "controller-1".to_string(),
            Some("LOCAL-MACID".to_string()),
        )
    }

    #[test]
    fn pair_adopts_record() {
        let mut s = store();
        let record = s.adopt(&test_response(), None, true).unwrap();
        assert_eq!(record.host_id, "MACID-UPPER");
        assert_eq!(s.records.len(), 1);
        // Credentials committed before metadata: the picker row can connect.
        assert_eq!(
            s.credentials_for("MACID-UPPER"),
            Some("auth-token-abc".to_string())
        );
        // select=true selects the new host.
        assert_eq!(s.selected_host_id, Some("MACID-UPPER".to_string()));
    }

    #[test]
    fn adopt_rejects_self_pairing() {
        let mut s = store();
        let mut response = test_response();
        response.mac_id = "LOCAL-MACID".to_string();
        // Relay mac_id must still match for the self-check to be reached first.
        response.relay_credentials.mac_id = "LOCAL-MACID".to_string();
        assert_eq!(
            s.adopt(&response, None, false).unwrap_err(),
            HostStoreError::SelfPairing
        );
        assert!(s.records.is_empty());
    }

    #[test]
    fn adopt_rejects_bad_protocol() {
        let mut s = store();
        let mut response = test_response();
        response.protocol_version = PAIRING_PROTOCOL_VERSION + 99;
        assert_eq!(
            s.adopt(&response, None, false).unwrap_err(),
            HostStoreError::IncompatibleProtocol
        );
    }

    #[test]
    fn adopt_rejects_unauthenticated_response() {
        let mut s = store();
        let mut response = test_response();
        response.auth_token = String::new();
        assert_eq!(
            s.adopt(&response, None, false).unwrap_err(),
            HostStoreError::Authentication
        );
        // No picker row left behind.
        assert!(s.records.is_empty());
        assert!(s.credentials_for("MACID-UPPER").is_none());
    }

    #[test]
    fn forget_removes_host() {
        let mut s = store();
        s.adopt(&test_response(), None, true).unwrap();
        s.forget("MACID-UPPER");
        assert!(s.records.is_empty());
        assert!(s.credentials_for("MACID-UPPER").is_none());
        // Was selected: falls back to Local.
        assert_eq!(s.selected_host_id, None);
    }

    #[test]
    fn rename_validates_name() {
        let mut s = store();
        s.adopt(&test_response(), None, false).unwrap();
        assert!(!s.rename_host("MACID-UPPER", "   "));
        assert!(!s.rename_host("unknown-id", "New Name"));
        assert!(s.rename_host("MACID-UPPER", "  Studio Mac  "));
        assert_eq!(s.records[0].name, "Studio Mac");
        // Same name is a no-op success.
        assert!(s.rename_host("MACID-UPPER", "Studio Mac"));
    }

    #[test]
    fn select_host_requires_credentials() {
        let mut s = store();
        s.adopt(&test_response(), None, false).unwrap();
        // Usable: credentials present.
        s.select_host(Some("MACID-UPPER".to_string()));
        assert_eq!(s.selected_host_id, Some("MACID-UPPER".to_string()));
        // Unknown id: selection stays put.
        s.select_host(Some("nope".to_string()));
        assert_eq!(s.selected_host_id, Some("MACID-UPPER".to_string()));
        // None selects Local.
        s.select_host(None);
        assert_eq!(s.selected_host_id, None);
    }

    #[test]
    fn set_link_enabled_narrows_only() {
        let mut s = store();
        s.adopt(&test_response(), None, false).unwrap();
        assert!(s.records[0].is_link_enabled());
        s.set_link_enabled(false, "MACID-UPPER");
        assert!(!s.records[0].is_link_enabled());
        assert_eq!(s.records[0].link_enabled, Some(false));
        s.set_link_enabled(true, "MACID-UPPER");
        assert!(s.records[0].is_link_enabled());
        // Allowed is the default: no key stored.
        assert_eq!(s.records[0].link_enabled, None);
    }

    #[test]
    fn adopt_ssh_upserts_by_target() {
        let mut s = store();
        let r1 = s
            .adopt_ssh(
                "ssh://pi@192.168.1.9".to_string(),
                "".to_string(),
                "SSH-HOST-1".to_string(),
                Some("s3cret".to_string()),
                false,
            )
            .unwrap();
        // Blank name falls back to the destination.
        assert_eq!(r1.name, "pi@192.168.1.9");
        assert!(r1.uses_stored_secret);
        // Same target: upserts, keeps the id.
        let r2 = s
            .adopt_ssh(
                "ssh://pi@192.168.1.9".to_string(),
                "Pi".to_string(),
                "SSH-HOST-1".to_string(),
                None,
                false,
            )
            .unwrap();
        assert_eq!(r1.id, r2.id);
        assert_eq!(r2.name, "Pi");
        assert_eq!(s.ssh_records.len(), 1);
    }
}
