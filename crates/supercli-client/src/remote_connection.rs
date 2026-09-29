//! Live remote-Host connection management.
//!
//! This is the wiring layer for the ported `RemoteHostRuntime` state
//! machine (`crates/supercli-client/src/remote_runtime.rs`, from
//! `clients/legacy/native/SupercliNative/Sources/SupercliNative/RemoteHostRuntime.swift`).
//! It connects a paired host's record through the **authenticated** Host
//! API ([`HostClient`]), with Direct→Link fallback per
//! [`PairedHostConnectionPlan`], reconnection backoff, and the live-client
//! registry ([`HostRegistry`]).
//!
//! Credentials come from the secure store behind [`HostCredentialStore`]
//! (OS keychain in production via
//! [`RemoteHostStore::with_os_keychain`](crate::RemoteHostStore::with_os_keychain),
//! in-memory only in tests). They are never logged and never written to
//! plaintext here — the Debug impl of [`HostClient`] already redacts the
//! bearer token.
//!
//! The Link (relay) leg is transport-pluggable through
//! [`RemoteHostConnector`]: the default connector implements the Direct
//! leg against a real Host and reports Link as unavailable without a live
//! relay connection (the app wires the real relay leg; the ordering and
//! gating policy — Direct first, Link only on classified reachability
//! failure, never on hard failures — live here).

use std::collections::HashMap;
use std::time::Duration;

use crate::credentials::{relay_credentials_for_host, HostSecrets};
use crate::host_store::{HostCredentialStore, RemoteHostStore};
use crate::hosts::HostRegistry;
use crate::relay::RelayCredentials;
use crate::remote_runtime::{
    ConnectionPlanError, PairedHostConnectionPlan, RemoteHostConnectionRoute,
    RemoteHostConnectionState,
};
use crate::transport::{connect_direct_classified, DirectFailure, HostClient, HostClientError};
use crate::types::PairedHostRecord;

/// Terminal result of one [`RemoteHostConnection::connect`] attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectOutcome {
    Connected { route: RemoteHostConnectionRoute },
    Failed { message: String },
    RepairRequired { message: String },
    Incompatible { message: String },
}

/// Backoff policy for [`RemoteHostConnection::reconnect`].
#[derive(Debug, Clone, Copy)]
pub struct ReconnectPolicy {
    /// How many connect attempts before giving up (the first attempt
    /// counts, so 1 means "try once, no retry").
    pub max_attempts: u32,
    /// Delay before the first retry; doubles each attempt.
    pub base_delay: Duration,
    /// Cap on the doubled delay.
    pub max_delay: Duration,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
        }
    }
}

/// Transport leg behind [`RemoteHostConnection`].
///
/// The default implementation ([`DefaultRemoteHostConnector`]) performs the
/// real Direct leg against a live Host; the Link leg is left to the app,
/// which owns the live [`RelayConnection`](crate::relay_conn::RelayConnection).
/// Tests inject a stub to exercise the fallback ordering without a relay.
pub trait RemoteHostConnector: Send + Sync {
    /// Open the Direct leg: build the client and verify it with a
    /// bootstrap. Failures are classified for the fallback policy.
    fn connect_direct(
        &self,
        record: &PairedHostRecord,
        secrets: &HostSecrets,
    ) -> Result<(HostClient, crate::dto::BootstrapSnapshot), DirectFailure>;

    /// Open the Link leg. The default implementation returns an error
    /// naming the missing relay connection; the app provides the real one.
    fn connect_link(
        &self,
        creds: &RelayCredentials,
        auth_token: &str,
        controller_device_id: &str,
    ) -> Result<HostClient, String>;
}

/// Production connector: real Direct leg, Link leg owned by the app.
pub struct DefaultRemoteHostConnector;

impl RemoteHostConnector for DefaultRemoteHostConnector {
    fn connect_direct(
        &self,
        record: &PairedHostRecord,
        secrets: &HostSecrets,
    ) -> Result<(HostClient, crate::dto::BootstrapSnapshot), DirectFailure> {
        connect_direct_classified(record, secrets)
    }

    fn connect_link(
        &self,
        _creds: &RelayCredentials,
        _auth_token: &str,
        _controller_device_id: &str,
    ) -> Result<HostClient, String> {
        Err(
            "link requires a live relay connection: the app must provide a RemoteHostConnector"
                .to_string(),
        )
    }
}

/// One paired Host's live connection lifecycle.
///
/// Owns the [`RemoteHostStore`] (records + secure credentials) and the
/// [`HostRegistry`] (live clients), and drives
/// [`RemoteHostConnectionState`] through connect / reconnect / disconnect
/// against a real Host's authenticated API.
pub struct RemoteHostConnection<S: HostCredentialStore> {
    store: RemoteHostStore<S>,
    registry: HostRegistry,
    connector: Box<dyn RemoteHostConnector>,
    states: HashMap<String, RemoteHostConnectionState>,
    attempts: HashMap<String, u32>,
    reconnect_policy: ReconnectPolicy,
}

impl<S: HostCredentialStore> RemoteHostConnection<S> {
    /// Build around a host store (records + credential store).
    pub fn new(store: RemoteHostStore<S>) -> Self {
        Self::with_connector(store, Box::new(DefaultRemoteHostConnector))
    }

    /// Build with a custom transport connector (tests, or the app's
    /// relay-backed Link leg).
    pub fn with_connector(
        store: RemoteHostStore<S>,
        connector: Box<dyn RemoteHostConnector>,
    ) -> Self {
        Self {
            store,
            registry: HostRegistry::new(),
            connector,
            states: HashMap::new(),
            attempts: HashMap::new(),
            reconnect_policy: ReconnectPolicy::default(),
        }
    }

    /// Current lifecycle state for a host (`Idle` when never touched).
    pub fn state(&self, host_id: &str) -> RemoteHostConnectionState {
        self.states
            .get(host_id)
            .cloned()
            .unwrap_or(RemoteHostConnectionState::Idle)
    }

    /// Tune the reconnect backoff (tests use zero delays).
    pub fn set_reconnect_policy(&mut self, policy: ReconnectPolicy) {
        self.reconnect_policy = policy;
    }

    /// Borrow the underlying store (records + credentials).
    pub fn store(&self) -> &RemoteHostStore<S> {
        &self.store
    }

    /// Borrow the live-client registry.
    pub fn registry(&self) -> &HostRegistry {
        &self.registry
    }

    fn set_state(&mut self, host_id: &str, state: RemoteHostConnectionState) {
        self.states.insert(host_id.to_string(), state);
    }

    fn record(&self, host_id: &str) -> Option<PairedHostRecord> {
        self.store
            .records
            .iter()
            .find(|r| r.host_id == host_id)
            .cloned()
    }

    /// Connect a paired Host: Direct first, Link fallback only on a
    /// classified reachability failure and only when the record's plan
    /// allows it. On success the live client is registered and the state
    /// becomes `Connected`.
    pub fn connect(&mut self, host_id: &str) -> ConnectOutcome {
        let record = match self.record(host_id) {
            Some(record) => record,
            None => {
                let message = format!("unknown paired host: {host_id}");
                self.set_state(
                    host_id,
                    RemoteHostConnectionState::Failed {
                        message: message.clone(),
                    },
                );
                return ConnectOutcome::Failed { message };
            }
        };

        let plan = match PairedHostConnectionPlan::build(
            record.host_id.clone(),
            record.endpoint.clone(),
            record.controller_device_id.clone(),
            record.is_link_enabled(),
            record
                .remote_server_certificate_fingerprint
                .clone()
                .or_else(|| record.certificate_fingerprint.clone()),
        ) {
            Ok(plan) => plan,
            Err(ConnectionPlanError::PairingRepairRequired { message }) => {
                self.set_state(
                    host_id,
                    RemoteHostConnectionState::RepairRequired {
                        message: message.clone(),
                    },
                );
                return ConnectOutcome::RepairRequired { message };
            }
        };

        let secrets = match self.store.secrets_for(host_id) {
            Some(secrets) => secrets,
            None => {
                let message =
                    "no stored credential for this Host — re-pair it to connect".to_string();
                self.set_state(
                    host_id,
                    RemoteHostConnectionState::RepairRequired {
                        message: message.clone(),
                    },
                );
                return ConnectOutcome::RepairRequired { message };
            }
        };

        self.set_state(host_id, RemoteHostConnectionState::Connecting);

        match self.connector.connect_direct(&record, &secrets) {
            Ok((client, snapshot)) => {
                let name = record.name.clone();
                self.registry.connect(record, client, Some(snapshot));
                self.attempts.remove(host_id);
                self.set_state(host_id, RemoteHostConnectionState::Connected { name });
                ConnectOutcome::Connected {
                    route: RemoteHostConnectionRoute::Direct,
                }
            }
            Err(failure) => self.on_direct_failure(host_id, &record, &secrets, &plan, failure),
        }
    }

    /// Handle a failed Direct leg: Link fallback only when the failure is
    /// reachability-classified *and* the plan carries a Link transport.
    /// Hard failures (bad pin, 401, decode) never touch the relay — falling
    /// back there would silently route around a security decision.
    fn on_direct_failure(
        &mut self,
        host_id: &str,
        record: &PairedHostRecord,
        secrets: &HostSecrets,
        plan: &PairedHostConnectionPlan,
        failure: DirectFailure,
    ) -> ConnectOutcome {
        let link_allowed = plan.link.is_some() && failure.relay_eligible();
        if !link_allowed {
            return self.hard_failure(host_id, failure);
        }

        self.set_state(
            host_id,
            RemoteHostConnectionState::Reconnecting {
                message: "Direct unreachable — trying Via Link".to_string(),
            },
        );

        let creds = match relay_credentials_for_host(record, secrets) {
            Some(creds) => creds,
            None => {
                let message =
                    "Direct unreachable and this Host has no Link credentials — re-pair to enable Via Link"
                        .to_string();
                self.set_state(
                    host_id,
                    RemoteHostConnectionState::Failed {
                        message: message.clone(),
                    },
                );
                return ConnectOutcome::Failed { message };
            }
        };

        match self
            .connector
            .connect_link(&creds, &secrets.auth_token, &record.controller_device_id)
        {
            Ok(client) => match client.bootstrap() {
                Ok(snapshot) => {
                    let name = record.name.clone();
                    self.registry
                        .connect(record.clone(), client, Some(snapshot));
                    self.attempts.remove(host_id);
                    self.set_state(host_id, RemoteHostConnectionState::Connected { name });
                    ConnectOutcome::Connected {
                        route: RemoteHostConnectionRoute::Link,
                    }
                }
                Err(e) => {
                    let message = format!("Via Link connected but bootstrap failed: {e}");
                    self.set_state(
                        host_id,
                        RemoteHostConnectionState::Failed {
                            message: message.clone(),
                        },
                    );
                    ConnectOutcome::Failed { message }
                }
            },
            Err(e) => {
                let message = format!("Direct unreachable; Via Link failed: {e}");
                self.set_state(
                    host_id,
                    RemoteHostConnectionState::Failed {
                        message: message.clone(),
                    },
                );
                ConnectOutcome::Failed { message }
            }
        }
    }

    /// Classify a hard (non-reachability) Direct failure into the right
    /// terminal state. A 401 means our stored credential is wrong — the
    /// user must re-pair, not retry.
    fn hard_failure(&mut self, host_id: &str, failure: DirectFailure) -> ConnectOutcome {
        let message = failure.to_string();
        match &failure {
            DirectFailure::Hard(HostClientError::Status(401, _)) => {
                self.set_state(
                    host_id,
                    RemoteHostConnectionState::RepairRequired {
                        message: message.clone(),
                    },
                );
                ConnectOutcome::RepairRequired { message }
            }
            DirectFailure::Setup(_) => {
                self.set_state(
                    host_id,
                    RemoteHostConnectionState::RepairRequired {
                        message: message.clone(),
                    },
                );
                ConnectOutcome::RepairRequired { message }
            }
            _ => {
                self.set_state(
                    host_id,
                    RemoteHostConnectionState::Failed {
                        message: message.clone(),
                    },
                );
                ConnectOutcome::Failed { message }
            }
        }
    }

    /// Reconnect with backoff: retries [`ReconnectPolicy::max_attempts`]
    /// times, holding `Reconnecting` between attempts. The attempt counter
    /// resets on success. Mirrors the Swift runtime keeping the last valid
    /// sidebar/terminal visible while reconnection runs.
    pub fn reconnect(&mut self, host_id: &str) -> ConnectOutcome {
        let policy = self.reconnect_policy;
        let mut delay = policy.base_delay;
        let mut last = ConnectOutcome::Failed {
            message: "reconnect attempted 0 times".to_string(),
        };
        for attempt in 1..=policy.max_attempts.max(1) {
            self.set_state(
                host_id,
                RemoteHostConnectionState::Reconnecting {
                    message: format!(
                        "reconnecting (attempt {attempt} of {})",
                        policy.max_attempts.max(1)
                    ),
                },
            );
            last = self.connect(host_id);
            if matches!(last, ConnectOutcome::Connected { .. }) {
                return last;
            }
            self.attempts.insert(host_id.to_string(), attempt);
            if attempt < policy.max_attempts.max(1) && !delay.is_zero() {
                std::thread::sleep(delay);
                delay = (delay * 2).min(policy.max_delay);
            }
        }
        last
    }

    /// Tear down one Host's connection and return it to `Idle`.
    pub fn disconnect(&mut self, host_id: &str) {
        self.registry.disconnect(host_id);
        self.attempts.remove(host_id);
        self.set_state(host_id, RemoteHostConnectionState::Idle);
    }

    /// Tear down every connection.
    pub fn disconnect_all(&mut self) {
        self.registry.disconnect_all();
        self.attempts.clear();
        for state in self.states.values_mut() {
            *state = RemoteHostConnectionState::Idle;
        }
    }

    /// Re-run bootstrap on the live client: the lightweight health check
    /// the runtime uses to notice a dead connection. Returns the fresh
    /// snapshot and caches it in the registry.
    pub fn health_check(&mut self, host_id: &str) -> Result<crate::dto::BootstrapSnapshot, String> {
        let client = self
            .registry
            .get(host_id)
            .map(|live| live.client.clone())
            .ok_or_else(|| format!("{host_id} is not connected"))?;
        match client.bootstrap() {
            Ok(snapshot) => {
                self.registry.set_snapshot(host_id, snapshot.clone());
                Ok(snapshot)
            }
            Err(e) => {
                self.registry.set_error(host_id, e.to_string());
                Err(format!("health check failed: {e}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::HostSecrets;
    use crate::host_store::MemoryHostCredentialStore;
    use crate::types::PairedHostRecord;

    fn test_record(host_id: &str) -> PairedHostRecord {
        PairedHostRecord {
            host_id: host_id.to_string(),
            name: "Test Mac".to_string(),
            endpoint: "https://127.0.0.1:1/mobile".to_string(),
            controller_device_id: "ctrl-1".to_string(),
            paired_at_unix_ms: 1,
            certificate_fingerprint: Some("ab".repeat(32)),
            remote_server_port: None,
            remote_server_certificate_fingerprint: None,
            link_enabled: None,
        }
    }

    fn test_secrets() -> HostSecrets {
        HostSecrets {
            auth_token: "tok".to_string(),
            relay_token: "relay-tok".to_string(),
            e2e_key_b64: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                [7u8; 32],
            ),
            relay_url: Some("wss://relay.example/link".to_string()),
        }
    }

    fn store_with(
        host_id: &str,
        secrets: HostSecrets,
    ) -> RemoteHostStore<MemoryHostCredentialStore> {
        let mut store = RemoteHostStore::new(
            MemoryHostCredentialStore::default(),
            "ctrl-1".to_string(),
            None,
        );
        store.records.push(test_record(host_id));
        store
            .seed_secrets_for_test(host_id, secrets)
            .expect("seed secrets");
        store
    }

    struct UnreachableConnector;
    impl RemoteHostConnector for UnreachableConnector {
        fn connect_direct(
            &self,
            _record: &PairedHostRecord,
            _secrets: &HostSecrets,
        ) -> Result<(HostClient, crate::dto::BootstrapSnapshot), DirectFailure> {
            Err(DirectFailure::Unreachable(HostClientError::Transport(
                "connection refused".to_string(),
            )))
        }
        fn connect_link(
            &self,
            _creds: &RelayCredentials,
            _auth_token: &str,
            _controller_device_id: &str,
        ) -> Result<HostClient, String> {
            Err("no relay in unit tests".to_string())
        }
    }

    #[test]
    fn unknown_host_is_a_hard_failure() {
        let store = RemoteHostStore::new(
            MemoryHostCredentialStore::default(),
            "ctrl-1".to_string(),
            None,
        );
        let mut conn = RemoteHostConnection::new(store);
        let outcome = conn.connect("nope");
        assert!(matches!(outcome, ConnectOutcome::Failed { .. }));
        assert!(matches!(
            conn.state("nope"),
            RemoteHostConnectionState::Failed { .. }
        ));
    }

    #[test]
    fn missing_credential_requires_repair() {
        let mut store = RemoteHostStore::new(
            MemoryHostCredentialStore::default(),
            "ctrl-1".to_string(),
            None,
        );
        store.records.push(test_record("h1"));
        let mut conn = RemoteHostConnection::new(store);
        let outcome = conn.connect("h1");
        assert!(
            matches!(outcome, ConnectOutcome::RepairRequired { .. }),
            "unexpected: {outcome:?}"
        );
        assert!(matches!(
            conn.state("h1"),
            RemoteHostConnectionState::RepairRequired { .. }
        ));
    }

    #[test]
    fn direct_only_unreachable_fails_without_touching_link() {
        let mut store = store_with("h1", test_secrets());
        store.records[0].link_enabled = Some(false); // Direct-only scoping
        let mut conn = RemoteHostConnection::with_connector(store, Box::new(UnreachableConnector));
        let outcome = conn.connect("h1");
        assert!(
            matches!(outcome, ConnectOutcome::Failed { .. }),
            "unexpected: {outcome:?}"
        );
        // Link must not have been attempted for a Direct-only host: the
        // failure message names the Direct route, not the relay.
        if let ConnectOutcome::Failed { message } = outcome {
            assert!(message.contains("direct unreachable"), "{message}");
        }
        assert!(!conn.registry().contains("h1"));
    }

    #[test]
    fn disconnect_returns_to_idle() {
        let store = RemoteHostStore::new(
            MemoryHostCredentialStore::default(),
            "ctrl-1".to_string(),
            None,
        );
        let mut conn = RemoteHostConnection::new(store);
        conn.disconnect("h1");
        assert_eq!(conn.state("h1"), RemoteHostConnectionState::Idle);
    }

    #[test]
    fn reconnect_retries_then_reports_last_failure() {
        let store = store_with("h1", test_secrets());
        let mut conn = RemoteHostConnection::with_connector(store, Box::new(UnreachableConnector));
        conn.set_reconnect_policy(ReconnectPolicy {
            max_attempts: 3,
            base_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        });
        let outcome = conn.reconnect("h1");
        assert!(matches!(outcome, ConnectOutcome::Failed { .. }));
        assert_eq!(conn.attempts.get("h1"), Some(&3));
    }
}
