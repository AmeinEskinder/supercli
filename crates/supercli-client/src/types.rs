//! Pure, transport-neutral client types.
//!
//! This module is `wasm32`-safe: no sockets, no threads, no filesystem, no
//! OS keychain. It is the subset of `unpeel-client` the Dioxus web target
//! builds against. Transport, pairing I/O, relay, TLS, credentials, and
//! the multi-Host registry stay behind
//! `#[cfg(not(target_arch = "wasm32"))]` in `lib.rs`.

use serde::{Deserialize, Serialize};

/// Which path a client talks to its Host over.
///
/// Mirrors the native clients' connection display: the UI shows only
/// **Direct** or **Via Link**, never relay internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransportKind {
    /// Plain LAN route: `http://` or pinned-`https://` to the Host's
    /// `/mobile` endpoint.
    #[default]
    Direct,
    /// Tunneled over the relay (`Link`) when the Direct route is
    /// unreachable. Same `/mobile` semantics, E2E-sealed per request.
    Relay,
}

impl std::fmt::Display for TransportKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportKind::Direct => write!(f, "Direct"),
            TransportKind::Relay => write!(f, "Via Link"),
        }
    }
}

/// One row of the session browser gallery, mirroring
/// `unpeel_core::session_artifacts::SessionArtifactMetadata` (snake_case
/// field names — the Host serializes the struct as-is).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ArtifactMeta {
    /// Artifact kind: `image`, `screenshot`, or `file`.
    pub kind: String,
    /// Display/storage name within the kind.
    pub name: String,
    /// Byte size.
    pub size: u64,
    /// Last-modified time, unix milliseconds.
    pub modified_at_unix_ms: u64,
}

/// Persisted, transport-neutral identity for a paired Host. The shipped
/// mobile-v1 wire still calls the identity `macID`; the DTO boundary keeps
/// that name while Controller state uses `host_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedHostRecord {
    pub host_id: String,
    pub name: String,
    pub endpoint: String,
    pub controller_device_id: String,
    pub paired_at_unix_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_server_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_server_certificate_fingerprint: Option<String>,
    /// Per-Host Link scope, narrows-only: nil means allowed, only `false`
    /// is ever stored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_enabled: Option<bool>,
}

impl PairedHostRecord {
    pub fn is_link_enabled(&self) -> bool {
        self.link_enabled.unwrap_or(true)
    }
}
