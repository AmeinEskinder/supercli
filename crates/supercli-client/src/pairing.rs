//! One-time sealed pairing: QR/paste-code parsing, pairing crypto, and the
//! pairing HTTP exchange. Rust port of Swift `RemotePairingClient`,
//! `RemotePairingCrypto`, `RemotePairingCode`, and `PairedHostRecord`.
//!
//! Pairing is always a direct HTTPS/HTTP request to the endpoint in the
//! scanned code. Confidentiality and Host identity come from the sealed
//! request/response exchange, not from trusting the HTTP bootstrap: the QR
//! secret derives an independent AES-GCM key per direction, and associated
//! data binds each message to the scanned Mac identity and endpoint.

use std::collections::HashMap;
use std::time::Duration;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;

use crate::credentials::{CredentialError, CredentialStore, HostSecrets};
use crate::relay::{random_bytes, RelayCredentials, RelayProtocolError};
use crate::transport::{HostClient, HostClientError};

/// Pairing wire-protocol version (`RemoteControlProtocol.version`).
pub const PAIRING_PROTOCOL_VERSION: u32 = 1;
const PAIRING_ENVELOPE_VERSION: u32 = 1;
/// `POST {endpoint}/pair` timeout, unless the endpoint is a pairing-proxy
/// path (controller-assisted pairing relays through another device).
const PAIR_TIMEOUT: Duration = Duration::from_secs(10);
const PAIR_PROXY_TIMEOUT: Duration = Duration::from_secs(25);
const PAIRING_PROXY_PATH_PREFIX: &str = "/mobile/pairing-proxy/";

/// Failures of the sealed, one-time Controller-to-Host pairing exchange.
/// Mirrors Swift `RemotePairingClientError`; HTTP failures retain the
/// Host's structured `{"error": ...}` message.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PairingError {
    #[error("pairing code expired")]
    Expired,
    #[error("pairing code has no host identity")]
    InvalidHostIdentity,
    #[error("host speaks an incompatible pairing protocol")]
    IncompatibleProtocol,
    #[error("invalid HTTP response from host")]
    InvalidHttpResponse,
    #[error("host returned status {0}: {1}")]
    HttpStatus(u16, String),
    #[error("response is not bound to the scanned host identity")]
    ResponseHostIdentityMismatch,
    #[error("response is not bound to the scanned endpoint")]
    ResponseEndpointMismatch,
    #[error("response is not bound to this device")]
    ResponseDeviceIdentityMismatch,
    #[error("host returned unusable credentials")]
    InvalidCredentials,
    #[error("pairing crypto failed: {0}")]
    Crypto(String),
    #[error("transport error: {0}")]
    Transport(String),
}

impl From<RelayProtocolError> for PairingError {
    fn from(e: RelayProtocolError) -> Self {
        PairingError::Crypto(e.to_string())
    }
}

/// The Controller's own identity, presented to the Host during pairing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteDeviceIdentity {
    pub id: String,
    pub name: String,
    pub platform: String,
    #[serde(rename = "appVersion", skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
}

/// What the QR / paste code carries: endpoint + one-time token + expiry.
/// Mac identity (id, name) arrives authoritatively in the pairing response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePairingPayload {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u32,
    #[serde(rename = "macID")]
    pub mac_id: String,
    #[serde(rename = "macName")]
    pub mac_name: String,
    pub endpoint: String,
    pub token: String,
    #[serde(
        rename = "certificateFingerprint",
        skip_serializing_if = "Option::is_none"
    )]
    pub certificate_fingerprint: Option<String>,
    #[serde(rename = "expiresAtUnixMs")]
    pub expires_at_unix_ms: i64,
}

/// Authenticated envelope for the otherwise-plaintext LAN pairing endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePairingEnvelope {
    pub v: u32,
    #[serde(rename = "saltB64")]
    pub salt_b64: String,
    #[serde(rename = "sealedB64")]
    pub sealed_b64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RemotePairingRequest {
    pub token: String,
    pub device: RemoteDeviceIdentity,
}

/// The Host's sealed answer to a pairing request. `endpoint` remains the
/// cryptographically bound bootstrap/proxy URL; `direct_endpoint` (when the
/// request was relayed through an authorized Controller) is what the
/// Controller persists for steady-state routing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePairingResponse {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u32,
    #[serde(rename = "macID")]
    pub mac_id: String,
    #[serde(rename = "macName")]
    pub mac_name: String,
    pub endpoint: String,
    #[serde(rename = "directEndpoint", skip_serializing_if = "Option::is_none")]
    pub direct_endpoint: Option<String>,
    #[serde(rename = "deviceID")]
    pub device_id: String,
    #[serde(rename = "authToken")]
    pub auth_token: String,
    #[serde(rename = "pairedAtUnixMs")]
    pub paired_at_unix_ms: i64,
    #[serde(rename = "remoteServerPort", skip_serializing_if = "Option::is_none")]
    pub remote_server_port: Option<u16>,
    #[serde(
        rename = "remoteServerCertificateFingerprint",
        skip_serializing_if = "Option::is_none"
    )]
    pub remote_server_certificate_fingerprint: Option<String>,
    #[serde(rename = "relayCredentials")]
    pub relay_credentials: RelayCredentials,
    #[serde(rename = "serverVersion", skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
}

/// Persisted, transport-neutral identity for a paired Host: see
/// [`crate::types::PairedHostRecord`]. Re-exported here so existing
/// `crate::pairing::PairedHostRecord` paths keep working.
pub use crate::types::PairedHostRecord;

impl PairedHostRecord {
    /// Translate the compatibility DTO once, at the wire boundary.
    pub fn from_pairing_response(
        response: &RemotePairingResponse,
        certificate_fingerprint: Option<String>,
    ) -> Self {
        Self {
            host_id: response.mac_id.clone(),
            name: response.mac_name.clone(),
            endpoint: response
                .direct_endpoint
                .clone()
                .unwrap_or_else(|| response.endpoint.clone()),
            controller_device_id: response.device_id.clone(),
            paired_at_unix_ms: response.paired_at_unix_ms,
            certificate_fingerprint,
            remote_server_port: response.remote_server_port,
            remote_server_certificate_fingerprint: response
                .remote_server_certificate_fingerprint
                .clone(),
            link_enabled: None,
        }
    }
}

/// Replace a Host in place so picker ordering is stable, or append a new
/// Host in pairing order.
pub fn upsert_paired_host(
    mut records: Vec<PairedHostRecord>,
    record: PairedHostRecord,
) -> Vec<PairedHostRecord> {
    if let Some(existing) = records.iter_mut().find(|r| r.host_id == record.host_id) {
        *existing = record;
    } else {
        records.push(record);
    }
    records
}

pub fn remove_paired_host(records: Vec<PairedHostRecord>, host_id: &str) -> Vec<PairedHostRecord> {
    records
        .into_iter()
        .filter(|r| r.host_id != host_id)
        .collect()
}

// ---------------------------------------------------------------------------
// QR / paste-code wire form
// ---------------------------------------------------------------------------

/// Decode a pairing QR / paste code. The compact form is
/// `SUPERCLI:<version>:<host>:<port>:<macID>:<token>:<expiresUnixSeconds>`
/// with an optional eighth `<proxyID>` field for controller-assisted
/// pairing — kept to the QR alphanumeric charset so codes stay small.
pub fn decode_pairing_code(raw: &str) -> Option<RemotePairingPayload> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Only the compact form exists on the wire today.
    let parts: Vec<&str> = trimmed.split(':').collect();
    if parts.len() != 7 && parts.len() != 8 {
        return None;
    }
    if !parts[0].eq_ignore_ascii_case("SUPERCLI") {
        return None;
    }
    let version: u32 = parts[1].parse().ok()?;
    let port: u16 = parts[3].parse().ok()?;
    let expires_seconds: i64 = parts[6].parse().ok()?;
    let host = parts[2];
    // The token is compared verbatim server-side — never case-fold it.
    let token = parts[5];
    let mac_id = parts[4].to_lowercase();
    let path = if parts.len() == 8 {
        let proxy_id = parts[7];
        if proxy_id.is_empty() || !is_code_charset(proxy_id) {
            return None;
        }
        format!("{PAIRING_PROXY_PATH_PREFIX}{proxy_id}")
    } else {
        "/mobile".to_string()
    };
    if host.is_empty() || mac_id.is_empty() || token.is_empty() {
        return None;
    }
    Some(RemotePairingPayload {
        protocol_version: version,
        mac_id,
        mac_name: String::new(),
        endpoint: format!("http://{host}:{port}{path}"),
        token: token.to_string(),
        certificate_fingerprint: None,
        expires_at_unix_ms: expires_seconds * 1000,
    })
}

/// Encode a payload in the compact QR form. Returns `None` when the
/// endpoint is not expressible in the compact charset (no host/port, or a
/// field containing `:`) — callers fall back to the JSON form.
pub fn encode_pairing_code(payload: &RemotePairingPayload) -> Option<String> {
    let (host, port, path) = split_endpoint(&payload.endpoint)?;
    if host.contains(':')
        || payload.mac_id.is_empty()
        || !is_code_charset(&payload.mac_id)
        || payload.token.is_empty()
        || payload.token.contains(':')
    {
        return None;
    }
    let mut fields = vec![
        "SUPERCLI".to_string(),
        payload.protocol_version.to_string(),
        host,
        port.to_string(),
        payload.mac_id.to_uppercase(),
        payload.token.clone(),
        (payload.expires_at_unix_ms / 1000).to_string(),
    ];
    if path != "/mobile" {
        let proxy_id = path.strip_prefix(PAIRING_PROXY_PATH_PREFIX)?;
        if proxy_id.is_empty() || !is_code_charset(proxy_id) {
            return None;
        }
        fields.push(proxy_id.to_string());
    }
    Some(fields.join(":"))
}

fn is_code_charset(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Split an `http(s)://host:port/path` endpoint. Returns `None` for
/// anything the compact QR form cannot express.
fn split_endpoint(endpoint: &str) -> Option<(String, u16, String)> {
    let rest = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    // IPv6 hosts contain ':' and are not QR-encodable; last ':' splits port.
    let colon = authority.rfind(':')?;
    let host = &authority[..colon];
    if host.is_empty() || host.contains(':') {
        return None;
    }
    let port: u16 = authority[colon + 1..].parse().ok()?;
    Some((host.to_string(), port, path.to_string()))
}

// ---------------------------------------------------------------------------
// Pairing crypto
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PairingDirection {
    /// Controller → Host.
    Request,
    /// Host → Controller.
    Response,
}

impl PairingDirection {
    fn as_str(self) -> &'static str {
        match self {
            PairingDirection::Request => "phone-to-mac",
            PairingDirection::Response => "mac-to-phone",
        }
    }
}

fn pairing_derive_key(token: &str, salt: &[u8], direction: PairingDirection) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(salt), token.as_bytes());
    let mut okm = [0u8; 32];
    hk.expand(
        format!("supercli-pairing-v1:{}", direction.as_str()).as_bytes(),
        &mut okm,
    )
    .expect("HKDF expand with fixed length cannot fail");
    okm
}

fn pairing_aad(mac_id: &str, endpoint: &str, direction: PairingDirection) -> Vec<u8> {
    format!(
        "supercli-pairing-v1\0{}\0{mac_id}\0{endpoint}",
        direction.as_str()
    )
    .into_bytes()
}

/// Seal a pairing message. The QR secret derives an independent AES-GCM key
/// per direction; associated data binds the message to the scanned Mac
/// identity and endpoint.
fn seal_pairing(
    plaintext: &[u8],
    token: &str,
    mac_id: &str,
    endpoint: &str,
    direction: PairingDirection,
) -> Result<RemotePairingEnvelope, PairingError> {
    if token.is_empty() || mac_id.is_empty() {
        return Err(PairingError::Crypto("invalid pairing context".to_string()));
    }
    let mut salt = [0u8; 16];
    random_bytes(&mut salt)?;
    let key = pairing_derive_key(token, &salt, direction);
    let mut nonce_bytes = [0u8; 12];
    random_bytes(&mut nonce_bytes)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: plaintext,
                aad: &pairing_aad(mac_id, endpoint, direction),
            },
        )
        .map_err(|e| PairingError::Crypto(format!("seal failed: {e}")))?;
    // CryptoKit `combined` form: nonce || ciphertext || tag.
    let mut sealed = Vec::with_capacity(12 + ciphertext.len());
    sealed.extend_from_slice(&nonce_bytes);
    sealed.extend_from_slice(&ciphertext);
    Ok(RemotePairingEnvelope {
        v: PAIRING_ENVELOPE_VERSION,
        salt_b64: base64::engine::general_purpose::STANDARD.encode(salt),
        sealed_b64: base64::engine::general_purpose::STANDARD.encode(sealed),
    })
}

fn open_pairing(
    envelope: &RemotePairingEnvelope,
    token: &str,
    mac_id: &str,
    endpoint: &str,
    direction: PairingDirection,
) -> Result<Vec<u8>, PairingError> {
    if envelope.v != PAIRING_ENVELOPE_VERSION || token.is_empty() || mac_id.is_empty() {
        return Err(PairingError::Crypto("invalid pairing envelope".to_string()));
    }
    let salt = base64::engine::general_purpose::STANDARD
        .decode(&envelope.salt_b64)
        .map_err(|_| PairingError::Crypto("invalid pairing envelope".to_string()))?;
    let sealed = base64::engine::general_purpose::STANDARD
        .decode(&envelope.sealed_b64)
        .map_err(|_| PairingError::Crypto("invalid pairing envelope".to_string()))?;
    if salt.len() != 16 || sealed.len() < 12 + 16 {
        return Err(PairingError::Crypto("invalid pairing envelope".to_string()));
    }
    let key = pairing_derive_key(token, &salt, direction);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    cipher
        .decrypt(
            Nonce::from_slice(&sealed[..12]),
            Payload {
                msg: &sealed[12..],
                aad: &pairing_aad(mac_id, endpoint, direction),
            },
        )
        .map_err(|_| PairingError::Crypto("pairing authentication failed".to_string()))
}

// ---------------------------------------------------------------------------
// Pairing exchange
// ---------------------------------------------------------------------------

fn server_error_message(body: &str) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let value: HashMap<String, serde_json::Value> = serde_json::from_str(body).ok()?;
    match value.get("error") {
        Some(serde_json::Value::String(message)) if !message.is_empty() => Some(message.clone()),
        _ => None,
    }
}

/// Run the sealed, one-time Controller-to-Host pairing exchange against
/// the endpoint in the scanned payload. Returns the Host's credentials on
/// success; the caller persists the auth token + relay credentials to the
/// platform keychain and the [`PairedHostRecord`] to Controller state.
pub fn pair(
    payload: &RemotePairingPayload,
    device: &RemoteDeviceIdentity,
    now_unix_ms: u64,
) -> Result<RemotePairingResponse, PairingError> {
    if (payload.expires_at_unix_ms as u64) <= now_unix_ms {
        return Err(PairingError::Expired);
    }
    if payload.mac_id.is_empty() {
        return Err(PairingError::InvalidHostIdentity);
    }

    let request_body = serde_json::to_vec(&RemotePairingRequest {
        token: payload.token.clone(),
        device: device.clone(),
    })
    .map_err(|e| PairingError::Crypto(e.to_string()))?;
    let envelope = seal_pairing(
        &request_body,
        &payload.token,
        &payload.mac_id,
        &payload.endpoint,
        PairingDirection::Request,
    )?;
    let envelope_body =
        serde_json::to_vec(&envelope).map_err(|e| PairingError::Crypto(e.to_string()))?;

    let url = format!("{}/pair", payload.endpoint.trim_end_matches('/'));
    let timeout = if payload.endpoint.contains(PAIRING_PROXY_PATH_PREFIX) {
        PAIR_PROXY_TIMEOUT
    } else {
        PAIR_TIMEOUT
    };
    // ureq configures timeouts per-Agent, not per-request. Statuses are
    // handled below (with the Host's structured {"error": ...} body), so
    // ureq must not convert non-2xx into an error first.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent
        .post(&url)
        .header("Content-Type", "application/json")
        .send(envelope_body)
        .map_err(|e| PairingError::Transport(e.to_string()))?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|_| PairingError::InvalidHttpResponse)?;
    if !(200..300).contains(&status) {
        return Err(PairingError::HttpStatus(
            status,
            server_error_message(&body).unwrap_or_default(),
        ));
    }

    let response_envelope: RemotePairingEnvelope =
        serde_json::from_str(&body).map_err(|_| PairingError::InvalidHttpResponse)?;
    let plaintext = open_pairing(
        &response_envelope,
        &payload.token,
        &payload.mac_id,
        &payload.endpoint,
        PairingDirection::Response,
    )?;
    let paired: RemotePairingResponse =
        serde_json::from_slice(&plaintext).map_err(|_| PairingError::InvalidHttpResponse)?;
    validate_pairing_response(&paired, payload, device)?;
    Ok(paired)
}

/// Every binding the Swift client enforces on the sealed response.
fn validate_pairing_response(
    paired: &RemotePairingResponse,
    payload: &RemotePairingPayload,
    device: &RemoteDeviceIdentity,
) -> Result<(), PairingError> {
    if paired.protocol_version != PAIRING_PROTOCOL_VERSION {
        return Err(PairingError::IncompatibleProtocol);
    }
    if paired.mac_id != payload.mac_id {
        return Err(PairingError::ResponseHostIdentityMismatch);
    }
    if paired.endpoint != payload.endpoint {
        return Err(PairingError::ResponseEndpointMismatch);
    }
    if let Some(direct) = &paired.direct_endpoint {
        // A TLS-capable Host may advertise its Direct endpoint as https;
        // the Controller decides the scheme from the certificate pin, so
        // both spellings name the same `/mobile` port.
        if !is_valid_direct_endpoint(direct) {
            return Err(PairingError::InvalidCredentials);
        }
    }
    if paired.device_id != device.id {
        return Err(PairingError::ResponseDeviceIdentityMismatch);
    }
    let relay = &paired.relay_credentials;
    let e2e_ok = relay.e2e_key().is_some();
    let relay_url_ok = relay.relay_url.to_lowercase().starts_with("wss://");
    if paired.auth_token.is_empty()
        || relay.mac_id != payload.mac_id
        || relay.relay_token.is_empty()
        || !relay_url_ok
        || !e2e_ok
    {
        return Err(PairingError::InvalidCredentials);
    }
    Ok(())
}

fn is_valid_direct_endpoint(endpoint: &str) -> bool {
    let rest = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"));
    let rest = match rest {
        Some(rest) => rest,
        None => return false,
    };
    // No query or fragment.
    if rest.contains(['?', '#']) {
        return false;
    }
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if path != "/mobile" {
        return false;
    }
    // Host and port must both be present.
    let colon = match authority.rfind(':') {
        Some(i) => i,
        None => return false,
    };
    !authority[..colon].is_empty() && authority[colon + 1..].parse::<u16>().is_ok()
}

// ---------------------------------------------------------------------------
// Controller state: paired-Host records, device identity, client construction
// ---------------------------------------------------------------------------

/// Keychain account holding the JSON list of [`PairedHostRecord`].
const PAIRED_HOSTS_ACCOUNT: &str = "paired-hosts";
/// Keychain account holding the Controller's [`RemoteDeviceIdentity`].
const DEVICE_IDENTITY_ACCOUNT: &str = "device-identity";

/// Load the paired-Host records. `Ok(vec![])` means this device has never
/// paired (or every Host was unpaired).
///
/// The records are non-secret, but they live in the keychain next to the
/// secrets rather than in a config file: one storage seam, no new home-dir
/// convention, and it works inside the iOS/Android app sandboxes where a
/// shared dotfile is awkward.
pub fn load_paired_host_records(
    store: &dyn CredentialStore,
) -> Result<Vec<PairedHostRecord>, CredentialError> {
    match store.get_secret(PAIRED_HOSTS_ACCOUNT)? {
        None => Ok(Vec::new()),
        Some(blob) => {
            serde_json::from_slice(&blob).map_err(|e| CredentialError::Corrupt(e.to_string()))
        }
    }
}

/// Persist the paired-Host records, replacing the previous list.
pub fn save_paired_host_records(
    store: &dyn CredentialStore,
    records: &[PairedHostRecord],
) -> Result<(), CredentialError> {
    let blob = serde_json::to_vec(records).map_err(|e| CredentialError::Store(e.to_string()))?;
    store.set_secret(PAIRED_HOSTS_ACCOUNT, &blob)
}

/// The Controller's stable device identity, created once and persisted.
/// The Host binds pairing responses to `device.id`, so it must survive app
/// restarts — hence keychain storage, not a fresh random id per launch.
pub fn device_identity(
    store: &dyn CredentialStore,
) -> Result<RemoteDeviceIdentity, CredentialError> {
    if let Some(blob) = store.get_secret(DEVICE_IDENTITY_ACCOUNT)? {
        let identity: RemoteDeviceIdentity =
            serde_json::from_slice(&blob).map_err(|e| CredentialError::Corrupt(e.to_string()))?;
        return Ok(identity);
    }
    let mut id_bytes = [0u8; 16];
    random_bytes(&mut id_bytes).map_err(|e| CredentialError::Store(e.to_string()))?;
    let identity = RemoteDeviceIdentity {
        id: id_bytes.iter().map(|b| format!("{b:02x}")).collect(),
        name: std::env::var("SUPERCLI_DEVICE_NAME")
            .unwrap_or_else(|_| "Supercli Controller".to_string()),
        platform: std::env::consts::OS.to_string(),
        app_version: None,
    };
    let blob = serde_json::to_vec(&identity).map_err(|e| CredentialError::Store(e.to_string()))?;
    store.set_secret(DEVICE_IDENTITY_ACCOUNT, &blob)?;
    Ok(identity)
}

/// Build the steady-state [`HostClient`] for a paired Host: the record's
/// endpoint with the keychain auth token. `https://` endpoints require a
/// certificate fingerprint and fail closed without one; anything that is
/// not `http(s)://` is rejected rather than panicking.
pub fn client_for_paired_host(
    record: &PairedHostRecord,
    secrets: &HostSecrets,
) -> Result<HostClient, HostClientError> {
    if record.endpoint.starts_with("https://") {
        let fingerprint = record
            .remote_server_certificate_fingerprint
            .as_deref()
            .or(record.certificate_fingerprint.as_deref())
            .ok_or_else(|| {
                HostClientError::InvalidEndpoint(
                    "https endpoint without a certificate fingerprint".to_string(),
                )
            })?;
        HostClient::with_pinned_tls(&record.endpoint, &secrets.auth_token, fingerprint)
    } else if record.endpoint.starts_with("http://") {
        HostClient::new(&record.endpoint, &secrets.auth_token)
    } else {
        Err(HostClientError::InvalidEndpoint(format!(
            "unsupported endpoint scheme: {}",
            record.endpoint
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_payload() -> RemotePairingPayload {
        RemotePairingPayload {
            protocol_version: PAIRING_PROTOCOL_VERSION,
            mac_id: "host-123".to_string(),
            mac_name: "Test Mac".to_string(),
            endpoint: "http://192.168.1.10:8321/mobile".to_string(),
            token: "one-time-secret".to_string(),
            certificate_fingerprint: None,
            expires_at_unix_ms: 1_800_000_000_000,
        }
    }

    fn test_device() -> RemoteDeviceIdentity {
        RemoteDeviceIdentity {
            id: "device-1".to_string(),
            name: "Test Phone".to_string(),
            platform: "ios".to_string(),
            app_version: Some("0.7.1".to_string()),
        }
    }

    #[test]
    fn qr_code_roundtrip() {
        let payload = test_payload();
        let code = encode_pairing_code(&payload).expect("encodable");
        assert_eq!(
            code,
            "SUPERCLI:1:192.168.1.10:8321:HOST-123:one-time-secret:1800000000"
        );
        let decoded = decode_pairing_code(&code).expect("decodable");
        assert_eq!(decoded.mac_id, payload.mac_id);
        assert_eq!(decoded.endpoint, payload.endpoint);
        assert_eq!(decoded.token, payload.token);
        assert_eq!(decoded.expires_at_unix_ms, payload.expires_at_unix_ms);
    }

    #[test]
    fn qr_code_with_proxy_id() {
        let mut payload = test_payload();
        payload.endpoint = "http://192.168.1.10:8321/mobile/pairing-proxy/abc-123".to_string();
        let code = encode_pairing_code(&payload).expect("encodable");
        assert!(code.ends_with(":abc-123"), "{code}");
        let decoded = decode_pairing_code(&code).expect("decodable");
        assert_eq!(decoded.endpoint, payload.endpoint);
    }

    #[test]
    fn qr_code_rejects_garbage() {
        assert!(decode_pairing_code("").is_none());
        assert!(decode_pairing_code("SUPERCLI:1:host").is_none());
        assert!(decode_pairing_code("OTHER:1:h:1:m:t:2").is_none());
        // Token case is preserved verbatim.
        let decoded = decode_pairing_code("SUPERCLI:1:example.com:8321:MACID:ToKeN:1800000000")
            .expect("decodable");
        assert_eq!(decoded.token, "ToKeN");
        assert_eq!(decoded.mac_id, "macid");
    }

    #[test]
    fn pairing_crypto_roundtrip() {
        let plaintext = br#"{"token":"abc","device":{"id":"d"}}"#;
        let envelope = seal_pairing(
            plaintext,
            "secret",
            "mac-1",
            "http://h:1/mobile",
            PairingDirection::Request,
        )
        .expect("seal");
        let opened = open_pairing(
            &envelope,
            "secret",
            "mac-1",
            "http://h:1/mobile",
            PairingDirection::Request,
        )
        .expect("open");
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn pairing_crypto_rejects_tampering() {
        let envelope = seal_pairing(
            b"hello",
            "secret",
            "mac-1",
            "http://h:1/mobile",
            PairingDirection::Request,
        )
        .expect("seal");
        // Wrong direction key.
        assert!(open_pairing(
            &envelope,
            "secret",
            "mac-1",
            "http://h:1/mobile",
            PairingDirection::Response,
        )
        .is_err());
        // Wrong endpoint binding.
        assert!(open_pairing(
            &envelope,
            "secret",
            "mac-1",
            "http://evil:1/mobile",
            PairingDirection::Request,
        )
        .is_err());
        // Wrong token.
        assert!(open_pairing(
            &envelope,
            "wrong",
            "mac-1",
            "http://h:1/mobile",
            PairingDirection::Request,
        )
        .is_err());
        // Bit-flipped ciphertext.
        let mut tampered = envelope.clone();
        let mut sealed = base64::engine::general_purpose::STANDARD
            .decode(&tampered.sealed_b64)
            .unwrap();
        sealed[20] ^= 1;
        tampered.sealed_b64 = base64::engine::general_purpose::STANDARD.encode(sealed);
        assert!(open_pairing(
            &tampered,
            "secret",
            "mac-1",
            "http://h:1/mobile",
            PairingDirection::Request,
        )
        .is_err());
    }

    #[test]
    fn validate_response_catches_mismatches() {
        let payload = test_payload();
        let device = test_device();
        let mut response = RemotePairingResponse {
            protocol_version: PAIRING_PROTOCOL_VERSION,
            mac_id: payload.mac_id.clone(),
            mac_name: "Test Mac".to_string(),
            endpoint: payload.endpoint.clone(),
            direct_endpoint: None,
            device_id: device.id.clone(),
            auth_token: "auth-1".to_string(),
            paired_at_unix_ms: 1_700_000_000_000,
            remote_server_port: None,
            remote_server_certificate_fingerprint: None,
            relay_credentials: RelayCredentials {
                relay_url: "wss://relay.example.com".to_string(),
                mac_id: payload.mac_id.clone(),
                relay_token: "relay-token".to_string(),
                e2e_key_b64: base64::engine::general_purpose::STANDARD.encode([9u8; 32]),
            },
            server_version: None,
        };
        assert!(validate_pairing_response(&response, &payload, &device).is_ok());

        let mut bad = response.clone();
        bad.mac_id = "other".to_string();
        assert_eq!(
            validate_pairing_response(&bad, &payload, &device),
            Err(PairingError::ResponseHostIdentityMismatch)
        );

        let mut bad = response.clone();
        bad.device_id = "other".to_string();
        assert_eq!(
            validate_pairing_response(&bad, &payload, &device),
            Err(PairingError::ResponseDeviceIdentityMismatch)
        );

        let mut bad = response.clone();
        bad.relay_credentials.relay_url = "ws://insecure.example.com".to_string();
        assert_eq!(
            validate_pairing_response(&bad, &payload, &device),
            Err(PairingError::InvalidCredentials)
        );

        // Direct endpoint shape checks.
        let mut bad = response.clone();
        bad.direct_endpoint = Some("http://h:8321/other".to_string());
        assert_eq!(
            validate_pairing_response(&bad, &payload, &device),
            Err(PairingError::InvalidCredentials)
        );
        response.direct_endpoint = Some("https://192.168.1.10:8321/mobile".to_string());
        assert!(validate_pairing_response(&response, &payload, &device).is_ok());
    }

    #[test]
    fn paired_host_upsert_is_stable() {
        let make = |id: &str| PairedHostRecord {
            host_id: id.to_string(),
            name: id.to_string(),
            endpoint: "http://h:1/mobile".to_string(),
            controller_device_id: "d".to_string(),
            paired_at_unix_ms: 1,
            certificate_fingerprint: None,
            remote_server_port: None,
            remote_server_certificate_fingerprint: None,
            link_enabled: None,
        };
        let records = upsert_paired_host(vec![make("a"), make("b")], make("c"));
        assert_eq!(records.len(), 3);
        let records = upsert_paired_host(records, make("b"));
        assert_eq!(records.len(), 3);
        assert_eq!(records[1].host_id, "b");
        let records = remove_paired_host(records, "a");
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|r| r.host_id != "a"));
    }

    // ------------------------------------------------------------------
    // Controller state: records persistence + device identity
    // ------------------------------------------------------------------

    fn test_record(id: &str) -> PairedHostRecord {
        PairedHostRecord {
            host_id: id.to_string(),
            name: format!("Host {id}"),
            endpoint: "http://10.0.0.2:8321/mobile".to_string(),
            controller_device_id: "d1".to_string(),
            paired_at_unix_ms: 1_700_000_000_000,
            certificate_fingerprint: None,
            remote_server_port: None,
            remote_server_certificate_fingerprint: None,
            link_enabled: None,
        }
    }

    #[test]
    fn paired_host_records_roundtrip() {
        use crate::credentials::MemoryStore;
        let store = MemoryStore::default();
        assert_eq!(load_paired_host_records(&store).unwrap(), vec![]);

        let records = vec![test_record("h1"), test_record("h2")];
        save_paired_host_records(&store, &records).unwrap();
        assert_eq!(load_paired_host_records(&store).unwrap(), records);

        // Overwrite replaces, and empty round-trips.
        save_paired_host_records(&store, &records[..1]).unwrap();
        assert_eq!(load_paired_host_records(&store).unwrap(), records[..1]);
        save_paired_host_records(&store, &[]).unwrap();
        assert_eq!(load_paired_host_records(&store).unwrap(), vec![]);
    }

    #[test]
    fn device_identity_is_stable() {
        use crate::credentials::MemoryStore;
        let store = MemoryStore::default();
        let first = device_identity(&store).unwrap();
        let second = device_identity(&store).unwrap();
        assert_eq!(first.id, second.id, "identity must survive reload");
        assert!(!first.id.is_empty());
        assert_eq!(first.platform, std::env::consts::OS);
    }

    #[test]
    fn client_for_paired_host_routes_by_scheme() {
        let secrets = HostSecrets {
            auth_token: "tok".to_string(),
            relay_token: "relay".to_string(),
            e2e_key_b64: base64::engine::general_purpose::STANDARD.encode([1u8; 32]),
            relay_url: None,
        };
        let mut record = test_record("h1");
        assert!(client_for_paired_host(&record, &secrets).is_ok());

        // https without a fingerprint fails closed instead of panicking.
        record.endpoint = "https://10.0.0.2:8321/mobile".to_string();
        assert!(client_for_paired_host(&record, &secrets).is_err());
        record.remote_server_certificate_fingerprint = Some("ab".repeat(32));
        assert!(client_for_paired_host(&record, &secrets).is_ok());

        record.endpoint = "ftp://10.0.0.2:8321/mobile".to_string();
        assert!(client_for_paired_host(&record, &secrets).is_err());
    }

    // ------------------------------------------------------------------
    // Mock-Host pairing exchange
    // ------------------------------------------------------------------

    /// Serve exactly one `POST /pair`, computing the response from the raw
    /// request body. Returns the `/mobile` endpoint URL and the server
    /// thread (join it to propagate mock-side assertion failures).
    fn serve_one_pair<F>(respond: F) -> (String, std::thread::JoinHandle<()>)
    where
        F: FnOnce(String, Vec<u8>) -> (u16, Vec<u8>) + Send + 'static,
    {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock host");
        let port = listener.local_addr().unwrap().port();
        let endpoint = format!("http://127.0.0.1:{port}/mobile");
        let thread_endpoint = endpoint.clone();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("mock host accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            assert!(
                request_line.starts_with("POST /mobile/pair "),
                "unexpected request: {request_line}"
            );
            let mut content_length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let header = line.trim_end().to_ascii_lowercase();
                if header.is_empty() {
                    break;
                }
                if let Some(value) = header.strip_prefix("content-length:") {
                    content_length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body).unwrap();
            drop(reader);

            let (status, resp_body) = respond(thread_endpoint, body);
            let reason = if (200..300).contains(&status) {
                "OK"
            } else {
                "Error"
            };
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                resp_body.len()
            )
            .unwrap();
            stream.write_all(&resp_body).unwrap();
            stream.flush().unwrap();
        });
        (endpoint, handle)
    }

    const MOCK_TOKEN: &str = "one-time-secret";
    const MOCK_MAC_ID: &str = "host-123";

    fn mock_pair_response(endpoint: &str, device_id: &str) -> RemotePairingResponse {
        RemotePairingResponse {
            protocol_version: PAIRING_PROTOCOL_VERSION,
            mac_id: MOCK_MAC_ID.to_string(),
            mac_name: "Test Mac".to_string(),
            endpoint: endpoint.to_string(),
            direct_endpoint: None,
            device_id: device_id.to_string(),
            auth_token: "auth-token-abc".to_string(),
            paired_at_unix_ms: 1_700_000_000_000,
            remote_server_port: None,
            remote_server_certificate_fingerprint: None,
            relay_credentials: RelayCredentials {
                relay_url: "wss://relay.example".to_string(),
                mac_id: MOCK_MAC_ID.to_string(),
                relay_token: "relay-token-xyz".to_string(),
                e2e_key_b64: base64::engine::general_purpose::STANDARD.encode([7u8; 32]),
            },
            server_version: None,
        }
    }

    #[test]
    fn pair_happy_path_against_mock_host() {
        let (endpoint, server) = serve_one_pair(|endpoint, body| {
            let envelope: RemotePairingEnvelope =
                serde_json::from_slice(&body).expect("request is an envelope");
            let plaintext = open_pairing(
                &envelope,
                MOCK_TOKEN,
                MOCK_MAC_ID,
                &endpoint,
                PairingDirection::Request,
            )
            .expect("request envelope opens");
            let request: RemotePairingRequest =
                serde_json::from_slice(&plaintext).expect("request is JSON");
            assert_eq!(request.token, MOCK_TOKEN);

            let response = mock_pair_response(&endpoint, &request.device.id);
            let sealed = seal_pairing(
                &serde_json::to_vec(&response).unwrap(),
                MOCK_TOKEN,
                MOCK_MAC_ID,
                &endpoint,
                PairingDirection::Response,
            )
            .unwrap();
            (200, serde_json::to_vec(&sealed).unwrap())
        });

        let payload = RemotePairingPayload {
            protocol_version: PAIRING_PROTOCOL_VERSION,
            mac_id: MOCK_MAC_ID.to_string(),
            mac_name: String::new(),
            endpoint: endpoint.clone(),
            token: MOCK_TOKEN.to_string(),
            certificate_fingerprint: None,
            expires_at_unix_ms: 1_900_000_000_000,
        };
        let device = test_device();
        let response = pair(&payload, &device, 1_800_000_000_000).expect("pair succeeds");
        assert_eq!(response.auth_token, "auth-token-abc");
        assert_eq!(response.relay_credentials.relay_token, "relay-token-xyz");
        assert_eq!(response.device_id, device.id);

        // The full persist path works off the response.
        let record = PairedHostRecord::from_pairing_response(&response, None);
        assert_eq!(record.host_id, MOCK_MAC_ID);
        assert_eq!(record.endpoint, endpoint);
        server.join().expect("mock host thread");
    }

    #[test]
    fn pair_propagates_structured_server_error() {
        let (endpoint, server) =
            serve_one_pair(|_, _| (410, br#"{"error":"pairing code already used"}"#.to_vec()));
        let mut payload = test_payload();
        payload.endpoint = endpoint;
        let err = pair(&payload, &test_device(), 1_700_000_000_000).unwrap_err();
        assert_eq!(
            err,
            PairingError::HttpStatus(410, "pairing code already used".to_string())
        );
        server.join().expect("mock host thread");
    }
}
