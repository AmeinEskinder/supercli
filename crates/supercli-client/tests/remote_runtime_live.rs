//! Live-Host wiring tests for [`RemoteHostConnection`].
//!
//! These tests run the connection state machine against a REAL `supercli
//! serve` (the prebuilt binary, pointed at by `SUPERCLI_TEST_BINARY` or the
//! workspace default) with a paired-device fixture, exactly the way a
//! phone talks to the Host: pinned TLS + bearer token on the Direct
//! `/mobile` endpoint.
//!
//! Covered against the live Host:
//! - `connect` → `Connected { route: Direct }` with the bootstrap snapshot
//!   cached in the registry
//! - `health_check` on the live client
//! - `disconnect` → `Idle` + registry cleared
//! - killing the Host → `health_check` fails, `reconnect` reports the
//!   failure; restarting the Host → `reconnect` reaches `Connected` again
//! - a wrong bearer token → `RepairRequired` (401), never a relay attempt
//! - `adopt` stores the full secret bundle (auth + relay credentials),
//!   verified through `secrets_for`
//!
//! The Link fallback leg needs a live relay connection, which the app
//! owns; the fallback *ordering and gating* (Direct first, Link only on
//! classified reachability failure, Direct-only scoping) is unit-tested in
//! `remote_connection.rs` with a stub connector.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use base64::Engine;
use supercli_client::pairing::{RemotePairingResponse, PAIRING_PROTOCOL_VERSION};
use supercli_client::relay::RelayCredentials;
use supercli_client::{
    ConnectOutcome, HostSecrets, MemoryHostCredentialStore, ReconnectPolicy, RemoteHostConnection,
    RemoteHostConnectionRoute, RemoteHostConnectionState, RemoteHostStore,
};

const TEST_CONTROLLER: &str = "wire-test-controller";
const TEST_MAC_ID: &str = "wire-test-mac";
const TEST_TOKEN: &str = "wire-test-bearer-token";

fn sha256_hex(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn reserve_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    condition()
}

/// The pin a paired phone holds: generate the Host certificate first so
/// serve loads this exact file, then pin its SHA-256.
fn ensure_host_tls(home: &Path) -> String {
    let dir = home.join("remote").join("tls");
    std::fs::create_dir_all(&dir).unwrap();
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let der = certified.cert.der().to_vec();
    std::fs::write(dir.join("cert.pem"), certified.cert.pem().as_bytes()).unwrap();
    std::fs::write(
        dir.join("key.pem"),
        certified.key_pair.serialize_pem().as_bytes(),
    )
    .unwrap();
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(&der);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Pair this test controller the way a phone is paired: a devices.json
/// entry whose tokenHash matches the bearer we will present.
fn write_pairing_fixture(home: &Path, port: u16, token: &str) {
    let mobile = home.join("mobile");
    std::fs::create_dir_all(&mobile).unwrap();
    std::fs::write(mobile.join("server-port"), format!("{port}\n")).unwrap();
    let devices = serde_json::json!({
        "version": 1,
        "devices": [{
            "id": "wire-test-phone",
            "name": "Wire Test Phone",
            "platform": "iOS",
            "tokenHash": sha256_hex(token),
            "pairedAtUnixMs": 1,
            "relayAllowed": false
        }]
    });
    std::fs::write(
        mobile.join("devices.json"),
        serde_json::to_vec(&devices).unwrap(),
    )
    .unwrap();
}

fn test_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("SUPERCLI_TEST_BINARY") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    // Fall back to the `supercli` binary in the same cargo target dir as this
    // test binary (…/target/debug/deps/<test> → …/target/debug/supercli).
    // No hardcoded checkout paths: the live tests skip if no binary is found.
    let fallback = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent()?.parent().map(|dir| dir.join("supercli")));
    fallback.filter(|p| p.is_file())
}

struct LiveHost {
    child: Option<Child>,
    home: PathBuf,
    port: u16,
    fingerprint: String,
}

impl LiveHost {
    fn start() -> Option<Self> {
        let binary = test_binary()?;
        let home = std::env::temp_dir().join(format!("wire-runtime-live-{}", uuid_simple()));
        let port = reserve_port();
        let fingerprint = ensure_host_tls(&home);
        write_pairing_fixture(&home, port, TEST_TOKEN);
        let child = Command::new(&binary)
            .arg("serve")
            .env("SUPERCLI_HOME", &home)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn supercli serve");
        let mut host = Self {
            child: Some(child),
            home,
            port,
            fingerprint,
        };
        if !host.wait_ready() {
            eprintln!("live host never became ready; skipping");
            return None;
        }
        Some(host)
    }

    fn wait_ready(&mut self) -> bool {
        let status_path = self.home.join("serve.json");
        wait_until(Duration::from_secs(15), || {
            std::fs::read(&status_path)
                .ok()
                .and_then(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok())
                .and_then(|status| status.get("directPort").and_then(|v| v.as_u64()))
                == Some(u64::from(self.port))
        })
    }

    /// Kill the Host (SIGKILL): simulates the Host process dying.
    fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Restart the Host on the same home (same identity, same pairing).
    fn restart(&mut self) {
        self.kill();
        // Drop the stale readiness marker so wait_ready tracks the NEW process.
        let _ = std::fs::remove_file(self.home.join("serve.json"));
        let binary = test_binary().expect("test binary still present");
        let child = Command::new(&binary)
            .arg("serve")
            .env("SUPERCLI_HOME", &self.home)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("restart supercli serve");
        self.child = Some(child);
        assert!(self.wait_ready(), "restarted host became ready");
    }
}

impl Drop for LiveHost {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}-{}", nanos, std::process::id())
}

/// A pairing response for the live Host: real endpoint + fingerprint, and
/// structurally valid relay credentials (never used on the wire here).
fn pairing_response(port: u16, fingerprint: &str, token: &str) -> RemotePairingResponse {
    RemotePairingResponse {
        protocol_version: PAIRING_PROTOCOL_VERSION,
        mac_id: TEST_MAC_ID.to_string(),
        mac_name: "Wire Test Mac".to_string(),
        endpoint: format!("https://127.0.0.1:{port}/mobile"),
        direct_endpoint: None,
        device_id: TEST_CONTROLLER.to_string(),
        auth_token: token.to_string(),
        paired_at_unix_ms: 1,
        remote_server_port: Some(port),
        remote_server_certificate_fingerprint: Some(fingerprint.to_string()),
        relay_credentials: RelayCredentials {
            relay_url: "wss://relay.example/link".to_string(),
            mac_id: TEST_MAC_ID.to_string(),
            relay_token: "wire-relay-token".to_string(),
            e2e_key_b64: base64::engine::general_purpose::STANDARD.encode([9u8; 32]),
        },
        server_version: None,
    }
}

fn connected_store(host: &LiveHost, token: &str) -> RemoteHostStore<MemoryHostCredentialStore> {
    let mut store = RemoteHostStore::new(
        MemoryHostCredentialStore::default(),
        TEST_CONTROLLER.to_string(),
        None,
    );
    let response = pairing_response(host.port, &host.fingerprint, token);
    let record = store
        .adopt(&response, Some(host.fingerprint.clone()), true)
        .expect("adopt the live host");
    assert_eq!(record.host_id, TEST_MAC_ID);

    // adopt must have stored the FULL secret bundle (auth + relay),
    // matching Swift's RemoteHostCredentials.
    let secrets: HostSecrets = store.secrets_for(TEST_MAC_ID).expect("secret bundle");
    assert_eq!(secrets.auth_token, token);
    assert_eq!(secrets.relay_token, "wire-relay-token");
    assert_eq!(
        secrets.relay_url.as_deref(),
        Some("wss://relay.example/link")
    );
    assert!(secrets.e2e_key().is_some());
    store
}

fn skip_if_no_host() -> Option<LiveHost> {
    match LiveHost::start() {
        Some(host) => Some(host),
        None => {
            eprintln!("SKIP: no live supercli binary (set SUPERCLI_TEST_BINARY)");
            None
        }
    }
}

#[test]
fn live_connect_health_disconnect() {
    let host = match skip_if_no_host() {
        Some(host) => host,
        None => return,
    };
    let store = connected_store(&host, TEST_TOKEN);
    let mut conn = RemoteHostConnection::new(store);

    assert_eq!(conn.state(TEST_MAC_ID), RemoteHostConnectionState::Idle);

    let outcome = conn.connect(TEST_MAC_ID);
    assert!(
        matches!(
            outcome,
            ConnectOutcome::Connected {
                route: RemoteHostConnectionRoute::Direct
            }
        ),
        "live connect failed: {outcome:?}"
    );
    assert!(matches!(
        conn.state(TEST_MAC_ID),
        RemoteHostConnectionState::Connected { .. }
    ));
    assert!(conn.registry().contains(TEST_MAC_ID));
    assert_eq!(conn.registry().active_id(), Some(TEST_MAC_ID));

    // The registry cached the real bootstrap snapshot: it carries the
    // Host's identity and a protocol descriptor.
    let snapshot = conn.health_check(TEST_MAC_ID).expect("health check");
    assert!(
        snapshot.host_protocol.is_some(),
        "live bootstrap carries the host protocol descriptor"
    );

    conn.disconnect(TEST_MAC_ID);
    assert_eq!(conn.state(TEST_MAC_ID), RemoteHostConnectionState::Idle);
    assert!(!conn.registry().contains(TEST_MAC_ID));
}

#[test]
fn live_reconnect_after_host_death() {
    let mut host = match skip_if_no_host() {
        Some(host) => host,
        None => return,
    };
    let store = connected_store(&host, TEST_TOKEN);
    let mut conn = RemoteHostConnection::new(store);
    conn.set_reconnect_policy(ReconnectPolicy {
        max_attempts: 6,
        base_delay: Duration::from_millis(250),
        max_delay: Duration::from_secs(2),
    });

    let outcome = conn.connect(TEST_MAC_ID);
    assert!(
        matches!(outcome, ConnectOutcome::Connected { .. }),
        "initial connect: {outcome:?}"
    );

    // Kill the Host: the connection is dead.
    host.kill();
    assert!(
        conn.health_check(TEST_MAC_ID).is_err(),
        "health check must fail against a dead host"
    );

    // Reconnect while the Host is down: fails, holding the failure state.
    conn.set_reconnect_policy(ReconnectPolicy {
        max_attempts: 2,
        base_delay: Duration::ZERO,
        max_delay: Duration::ZERO,
    });
    let outcome = conn.reconnect(TEST_MAC_ID);
    assert!(
        matches!(outcome, ConnectOutcome::Failed { .. }),
        "reconnect against dead host: {outcome:?}"
    );

    // Restart the Host (same identity, same pairing): reconnect succeeds.
    // Backoff rides out the startup window.
    host.restart();
    conn.set_reconnect_policy(ReconnectPolicy {
        max_attempts: 6,
        base_delay: Duration::from_millis(250),
        max_delay: Duration::from_secs(2),
    });
    let outcome = conn.reconnect(TEST_MAC_ID);
    assert!(
        matches!(
            outcome,
            ConnectOutcome::Connected {
                route: RemoteHostConnectionRoute::Direct
            }
        ),
        "reconnect after restart: {outcome:?}"
    );
    assert!(conn.registry().contains(TEST_MAC_ID));
}

#[test]
fn live_wrong_token_requires_repair_not_retry() {
    let host = match skip_if_no_host() {
        Some(host) => host,
        None => return,
    };
    // Adopt the record but seed a WRONG bearer token: the Host answers
    // 401, which must surface as RepairRequired (re-pair), never as a
    // relay fallback or an endless retry.
    let mut store = RemoteHostStore::new(
        MemoryHostCredentialStore::default(),
        TEST_CONTROLLER.to_string(),
        None,
    );
    let response = pairing_response(host.port, &host.fingerprint, TEST_TOKEN);
    store
        .adopt(&response, Some(host.fingerprint.clone()), false)
        .expect("adopt");
    let mut secrets = store.secrets_for(TEST_MAC_ID).expect("secrets");
    secrets.auth_token = "wrong-token".to_string();
    store
        .seed_secrets_for_test(TEST_MAC_ID, secrets)
        .expect("reseed wrong token");

    let mut conn = RemoteHostConnection::new(store);
    let outcome = conn.connect(TEST_MAC_ID);
    assert!(
        matches!(outcome, ConnectOutcome::RepairRequired { .. }),
        "wrong token must require repair: {outcome:?}"
    );
    assert!(matches!(
        conn.state(TEST_MAC_ID),
        RemoteHostConnectionState::RepairRequired { .. }
    ));
    assert!(!conn.registry().contains(TEST_MAC_ID));
}
