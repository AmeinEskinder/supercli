//! Relay wire protocol: handshake keys, frame envelopes, and the JSON
//! DTOs tunneled inside the encrypted channel.
//!
//! Byte-compatible with `RelayProtocol.swift`. The transport itself lives
//! in [`crate::relay_conn`]; this module is the pure protocol half and has
//! no I/O.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::MAX_PLAINTEXT_BYTES;

/// Relay protocol version, pinned on the wire (`v` fields) and in the
/// HKDF/HMAC info strings.
pub const RELAY_PROTOCOL_VERSION: u32 = 1;

/// First byte of every relay frame (host side of the DO protocol).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RelayFrameType {
    Hello = 0x01,
    Data = 0x02,
    ClientClosed = 0x03,
    ClientData = 0x04,
}

#[derive(Debug, Error)]
pub enum RelayProtocolError {
    #[error("malformed frame")]
    MalformedFrame,
    #[error("bad key/salt length")]
    BadLength,
    #[error("OS RNG failure")]
    Rng,
    #[error("request exceeds maximum frame size")]
    RequestTooLarge,
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<getrandom::Error> for RelayProtocolError {
    fn from(_: getrandom::Error) -> Self {
        RelayProtocolError::Rng
    }
}

/// Fill `buf` from the OS RNG. Fails closed — callers abort the handshake
/// rather than proceeding with weak material.
pub fn random_bytes(buf: &mut [u8]) -> Result<(), RelayProtocolError> {
    getrandom::getrandom(buf)?;
    Ok(())
}

/// Ephemeral X25519 keypair for one handshake. The private half is
/// consumed by [`shared_secret`], enforcing at compile time that an
/// ephemeral key is never reused.
pub struct EphemeralKeyPair {
    private: x25519_dalek::EphemeralSecret,
    /// Raw 32-byte public key.
    pub public: [u8; 32],
}

impl EphemeralKeyPair {
    pub fn generate() -> Self {
        let private = x25519_dalek::EphemeralSecret::random();
        let public = x25519_dalek::PublicKey::from(&private).to_bytes();
        Self { private, public }
    }

    /// X25519 shared secret with the peer's ephemeral public key.
    /// Consumes the keypair. 32 bytes; errors on a malformed peer key.
    pub fn shared_secret(self, peer_public_key: &[u8]) -> Result<[u8; 32], RelayProtocolError> {
        if peer_public_key.len() != 32 {
            return Err(RelayProtocolError::BadLength);
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(peer_public_key);
        let peer = x25519_dalek::PublicKey::from(bytes);
        Ok(self.private.diffie_hellman(&peer).to_bytes())
    }
}

/// Host-side frame envelopes (the Mac uplink's view of the DO protocol).
/// The Controller (phone) side sends bare opaque bytes; the DO wraps them
/// with the connection id before piping to the Host.
pub mod envelope {
    use super::{RelayFrameType, RelayProtocolError};

    /// `[0x02][connID u32 BE][opaque]` — data to/from one phone connection.
    pub fn encode_data(conn_id: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(5 + payload.len());
        out.push(RelayFrameType::Data as u8);
        out.extend_from_slice(&conn_id.to_be_bytes());
        out.extend_from_slice(payload);
        out
    }

    /// `[0x01][JSON]` — registers paired device IDs + relayToken hashes.
    pub fn encode_hello(devices: &[super::RelayDeviceTokenRegistration]) -> Vec<u8> {
        #[derive(serde::Serialize)]
        struct Hello<'a> {
            v: u32,
            devices: &'a [super::RelayDeviceTokenRegistration],
        }
        let mut out = vec![RelayFrameType::Hello as u8];
        if let Ok(json) = serde_json::to_vec(&Hello {
            v: super::RELAY_PROTOCOL_VERSION,
            devices,
        }) {
            out.extend_from_slice(&json);
        }
        out
    }

    /// Frames a Host uplink receives from the relay.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Incoming {
        ClientClosed {
            conn_id: u32,
        },
        ClientData {
            conn_id: u32,
            device_id: String,
            payload: Vec<u8>,
        },
    }

    /// Decode one host-side frame. Mirrors `RelayHostFrame.decode`.
    pub fn decode(frame: &[u8]) -> Result<Option<Incoming>, RelayProtocolError> {
        if frame.len() < 5 {
            return Err(RelayProtocolError::MalformedFrame);
        }
        let frame_type = frame[0];
        let conn_id = u32::from_be_bytes([frame[1], frame[2], frame[3], frame[4]]);
        match frame_type {
            x if x == RelayFrameType::Data as u8 => Ok(None),
            x if x == RelayFrameType::Hello as u8 => Ok(None),
            x if x == RelayFrameType::ClientClosed as u8 => {
                Ok(Some(Incoming::ClientClosed { conn_id }))
            }
            x if x == RelayFrameType::ClientData as u8 => {
                if frame.len() < 7 {
                    return Err(RelayProtocolError::MalformedFrame);
                }
                let id_len = frame[5] as usize;
                if id_len == 0 || id_len > 128 || frame.len() < 6 + id_len {
                    return Err(RelayProtocolError::MalformedFrame);
                }
                let device_id = std::str::from_utf8(&frame[6..6 + id_len])
                    .map_err(|_| RelayProtocolError::MalformedFrame)?;
                if device_id.is_empty() {
                    return Err(RelayProtocolError::MalformedFrame);
                }
                Ok(Some(Incoming::ClientData {
                    conn_id,
                    device_id: device_id.to_string(),
                    payload: frame[6 + id_len..].to_vec(),
                }))
            }
            _ => Err(RelayProtocolError::MalformedFrame),
        }
    }
}

/// Registration of a paired device on the host uplink hello.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayDeviceTokenRegistration {
    #[serde(rename = "deviceID")]
    pub device_id: String,
    #[serde(rename = "tokenHash")]
    pub token_hash: String,
}

/// Client's first plaintext payload: which device key to use, its salt,
/// and its ephemeral public key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayClientHello {
    pub v: u32,
    #[serde(rename = "deviceID")]
    pub device_id: String,
    #[serde(rename = "saltB64")]
    pub salt_b64: String,
    #[serde(rename = "ephemeralPublicKeyB64")]
    pub ephemeral_public_key_b64: String,
}

/// Host's reply: its salt, its ephemeral public key, and the transcript
/// MAC proving it holds the device key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayHostHello {
    pub v: u32,
    #[serde(rename = "saltB64")]
    pub salt_b64: String,
    #[serde(rename = "ephemeralPublicKeyB64")]
    pub ephemeral_public_key_b64: String,
    #[serde(rename = "macB64")]
    pub mac_b64: String,
}

/// One `/mobile/*` request tunneled through the relay. Ids correlate
/// concurrent requests over one connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayTunnelRequest {
    pub id: u64,
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub query: std::collections::HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    #[serde(
        default,
        rename = "contentType",
        skip_serializing_if = "Option::is_none"
    )]
    pub content_type: Option<String>,
    #[serde(default, rename = "bodyB64", skip_serializing_if = "Option::is_none")]
    pub body_b64: Option<String>,
}

impl RelayTunnelRequest {
    pub fn body(&self) -> Vec<u8> {
        use base64::Engine;
        self.body_b64
            .as_deref()
            .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayTunnelResponse {
    pub id: u64,
    pub status: i32,
    #[serde(default, rename = "bodyB64")]
    pub body_b64: Option<String>,
}

impl RelayTunnelResponse {
    pub fn body(&self) -> Vec<u8> {
        use base64::Engine;
        self.body_b64
            .as_deref()
            .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
            .unwrap_or_default()
    }
}

/// Canonical request encoding at the relay's encrypted-frame boundary.
/// Measures the complete JSON/base64 envelope — callers check size BEFORE
/// sealing so an oversized request never burns a crypto counter.
pub fn encode_tunnel_request(request: &RelayTunnelRequest) -> Result<Vec<u8>, RelayProtocolError> {
    let plaintext = serde_json::to_vec(request)?;
    if plaintext.len() > MAX_PLAINTEXT_BYTES {
        return Err(RelayProtocolError::RequestTooLarge);
    }
    Ok(plaintext)
}

/// Relay-only output-stream control paths (intercepted by the Mac uplink
/// before the tunneled `/mobile/*` pipeline).
pub mod stream_paths {
    pub const SUBSCRIBE: &str = "/relay/output-stream";
    pub const CREDIT: &str = "/relay/output-credit";
    pub const UNSUBSCRIBE: &str = "/relay/output-stream-stop";
    pub const SUBSCRIPTION_ID_QUERY: &str = "subscription_id";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelayStreamEvent {
    Ended,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelayBootstrapEncoding {
    Identity,
    Lzfse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayStreamBootstrapPart {
    pub index: i32,
    #[serde(rename = "final")]
    pub is_final: bool,
    pub encoding: RelayBootstrapEncoding,
    #[serde(rename = "uncompressedBytes")]
    pub uncompressed_bytes: i32,
    #[serde(rename = "endOffset")]
    pub end_offset: u64,
}

/// One pushed terminal-output frame. Rides the same per-connection crypto
/// session as tunnel responses. Distinguished from `RelayTunnelResponse`
/// by shape: responses always carry `id` + `status`, pushes always carry
/// `stream` + `offset`.
///
/// `stream` and `offset` are required (no serde default) so that decoding
/// a response as a push fails cleanly, mirroring Swift's strict Decodable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayStreamPush {
    pub stream: String,
    #[serde(
        default,
        rename = "subscriptionID",
        skip_serializing_if = "Option::is_none"
    )]
    pub subscription_id: Option<String>,
    pub offset: u64,
    #[serde(default, rename = "dataB64", skip_serializing_if = "Option::is_none")]
    pub data_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rebased: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cols: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<RelayStreamEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bootstrap: Option<RelayStreamBootstrapPart>,
}

impl RelayStreamPush {
    pub fn data(&self) -> Vec<u8> {
        use base64::Engine;
        self.data_b64
            .as_deref()
            .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
            .unwrap_or_default()
    }
}

/// What a Controller needs to reach its Host through the relay. `e2e_key`
/// and `relay_token` are keychain-only; the Host stores the raw e2eKey and
/// only the SHA-256 of the relayToken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayCredentials {
    #[serde(rename = "relayURL")]
    pub relay_url: String,
    #[serde(rename = "macID")]
    pub mac_id: String,
    #[serde(rename = "relayToken")]
    pub relay_token: String,
    #[serde(rename = "e2eKeyB64")]
    pub e2e_key_b64: String,
}

impl RelayCredentials {
    pub fn e2e_key(&self) -> Option<[u8; 32]> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&self.e2e_key_b64)
            .ok()?;
        if bytes.len() != 32 {
            return None;
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Some(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_envelope_roundtrip() {
        let payload = b"opaque".to_vec();
        let frame = envelope::encode_data(0x01020304, &payload);
        assert_eq!(frame[0], RelayFrameType::Data as u8);
        // Host-side decode of a data frame yields None (it's the DO's own type).
        assert_eq!(envelope::decode(&frame).unwrap(), None);

        // clientData: [0x04][connID u32 BE][idLen u8][deviceID][payload]
        let mut f = vec![RelayFrameType::ClientData as u8];
        f.extend_from_slice(&7u32.to_be_bytes());
        f.push(3);
        f.extend_from_slice(b"dev");
        f.extend_from_slice(b"PAYLOAD");
        match envelope::decode(&f).unwrap() {
            Some(envelope::Incoming::ClientData {
                conn_id,
                device_id,
                payload,
            }) => {
                assert_eq!(conn_id, 7);
                assert_eq!(device_id, "dev");
                assert_eq!(payload, b"PAYLOAD");
            }
            other => panic!("unexpected {other:?}"),
        }

        let mut f = vec![RelayFrameType::ClientClosed as u8];
        f.extend_from_slice(&9u32.to_be_bytes());
        assert_eq!(
            envelope::decode(&f).unwrap(),
            Some(envelope::Incoming::ClientClosed { conn_id: 9 })
        );

        assert!(envelope::decode(&[]).is_err());
        assert!(envelope::decode(&[0x04, 0, 0]).is_err());
    }

    #[test]
    fn tunnel_request_size_checked_before_seal() {
        let big = RelayTunnelRequest {
            id: 1,
            method: "POST".into(),
            path: "/mobile/upload".into(),
            query: Default::default(),
            auth: None,
            content_type: None,
            body_b64: Some("A".repeat(MAX_PLAINTEXT_BYTES)),
        };
        assert!(matches!(
            encode_tunnel_request(&big),
            Err(RelayProtocolError::RequestTooLarge)
        ));
    }

    #[test]
    fn response_and_push_decode_by_shape() {
        // A response decodes as a response…
        let resp = br#"{"id":3,"status":200,"bodyB64":"aGk="}"#;
        let r: RelayTunnelResponse = serde_json::from_slice(resp).unwrap();
        assert_eq!(r.id, 3);
        assert_eq!(r.body(), b"hi");
        // …but not as a push (missing stream/offset).
        assert!(serde_json::from_slice::<RelayStreamPush>(resp).is_err());

        // A push decodes as a push…
        let push = br#"{"stream":"sess-1","offset":128,"dataB64":"eA=="}"#;
        let p: RelayStreamPush = serde_json::from_slice(push).unwrap();
        assert_eq!(p.stream, "sess-1");
        assert_eq!(p.data(), b"x");
        // …but not as a response (missing id/status).
        assert!(serde_json::from_slice::<RelayTunnelResponse>(push).is_err());
    }

    #[test]
    fn hello_json_uses_wire_names() {
        let hello = RelayClientHello {
            v: RELAY_PROTOCOL_VERSION,
            device_id: "dev-1".into(),
            salt_b64: "c2FsdA==".into(),
            ephemeral_public_key_b64: "a2V5".into(),
        };
        let json = serde_json::to_string(&hello).unwrap();
        assert!(json.contains("\"deviceID\""));
        assert!(json.contains("\"saltB64\""));
        assert!(json.contains("\"ephemeralPublicKeyB64\""));
    }
}
