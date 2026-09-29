//! Rust client for the Supercli Host protocol.
//!
//! This crate is the portable replacement for the Swift `SupercliShared`
//! Host-protocol client (`RemoteControlProtocol.swift`,
//! `RelayProtocol.swift`). Every cross-platform client — Dioxus desktop,
//! Dioxus mobile, the chat TUI — speaks to a Host through this crate, so
//! the wire contract is implemented exactly once, in Rust.
//!
//! Layout:
//!
//! Web-safe (also compiled for `wasm32-unknown-unknown` by the Dioxus web
//! target): [`protocol`], [`dto`], [`types`]. Everything else — transport,
//! relay, TLS, credentials, pairing I/O, the Host registry — is native
//! only, behind `#[cfg(not(target_arch = "wasm32"))]`:
//!
//! - [`protocol`] — version constants, capability ids, the protocol descriptor
//! - [`dto`] — serde data-transfer objects (bootstrap, sessions, transcripts…)
//! - [`types`] — pure shared types: [`TransportKind`], [`ArtifactMeta`],
//!   [`PairedHostRecord`]
//! - [`crypto`] — relay end-to-end crypto, byte-compatible with the Swift and
//!   JS implementations (pinned by `protocol/relay-kat-vectors-v2.json`)
//! - [`transport`] — blocking HTTP client for the Host's `/mobile` API
//! - [`relay`] — relay wire protocol: handshake keys, frame envelopes, DTOs
//! - [`relay_conn`] — blocking WebSocket relay transport (E2E tunnel)
//! - [`pairing`] — one-time sealed pairing: QR codes, pairing crypto, exchange
//! - [`tls`] — certificate-pinned TLS for the Direct `https://` endpoint
//! - [`credentials`] — platform keychain storage for pairing secrets
//! - [`direct_transport`] — Direct `/mobile` transport decisions: pinned-HTTPS
//!   vs plaintext policy, bootstrap deadlines, push-token routes
//! - [`terminal_stream`] — terminal output WebSocket wire contract: hello/error
//!   frame decoding, binary frame parsing, WS transport selection

#[cfg(not(target_arch = "wasm32"))]
pub mod credentials;
#[cfg(not(target_arch = "wasm32"))]
pub mod crypto;
pub mod dev_settings;
#[cfg(not(target_arch = "wasm32"))]
pub mod direct_transport;
pub mod dto;
pub mod events;
#[cfg(not(target_arch = "wasm32"))]
pub mod host_routing;
#[cfg(not(target_arch = "wasm32"))]
pub mod host_store;
#[cfg(not(target_arch = "wasm32"))]
pub mod hosts;
pub mod launch_config;
#[cfg(not(target_arch = "wasm32"))]
pub mod link_authority;
#[cfg(not(target_arch = "wasm32"))]
pub mod local_host_client;
pub mod mouse_mode;
#[cfg(not(target_arch = "wasm32"))]
pub mod nearby;
#[cfg(not(target_arch = "wasm32"))]
pub mod pairing;
#[cfg(not(target_arch = "wasm32"))]
pub mod pairing_proxy;
pub mod protocol;
#[cfg(not(target_arch = "wasm32"))]
pub mod relay;
#[cfg(not(target_arch = "wasm32"))]
pub mod relay_conn;
#[cfg(not(target_arch = "wasm32"))]
pub mod relay_transport;
#[cfg(not(target_arch = "wasm32"))]
pub mod remote_connection;
pub mod remote_dto_adapters;
#[cfg(not(target_arch = "wasm32"))]
pub mod remote_runtime;
pub mod remote_session_cache;
#[cfg(not(target_arch = "wasm32"))]
pub mod remote_supercli_peer;
pub mod remote_terminal_websocket;
#[cfg(not(target_arch = "wasm32"))]
pub mod remote_ws_transport;
pub mod resume_command;
#[cfg(not(target_arch = "wasm32"))]
pub mod scope;
pub mod stream_frame_reconciler;
pub mod terminal_query_filter;
#[cfg(not(target_arch = "wasm32"))]
pub mod terminal_stream;
#[cfg(not(target_arch = "wasm32"))]
pub mod tls;
#[cfg(not(target_arch = "wasm32"))]
pub mod transport;
pub mod types;
#[cfg(not(target_arch = "wasm32"))]
pub mod viewer_presence;
#[cfg(not(target_arch = "wasm32"))]
pub mod workspace_pool;

pub use types::{ArtifactMeta, PairedHostRecord, TransportKind};

#[cfg(not(target_arch = "wasm32"))]
pub use credentials::{
    delete_host_secrets, load_host_secrets, open_controller_store, relay_credentials_for_host,
    store_host_secrets, CredentialError, CredentialStore, HostSecrets, KeyringStore, MemoryStore,
    PairedHost, KEYCHAIN_SERVICE,
};

#[cfg(not(target_arch = "wasm32"))]
pub use host_store::{
    HostCredentialStore, HostStoreError, OsKeychainCredentialStore, RemoteHostStore, SshHostRecord,
};
// MemoryHostCredentialStore is test-only: gated so production code cannot
// import it by mistake. Available under `cargo test` or with the
// `test-util` feature.
#[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test-util")))]
pub use host_store::MemoryHostCredentialStore;
#[cfg(not(target_arch = "wasm32"))]
pub use hosts::{HostRegistry, LiveHost};
#[cfg(not(target_arch = "wasm32"))]
pub use pairing::{
    client_for_paired_host, decode_pairing_code, device_identity, encode_pairing_code,
    load_paired_host_records, pair, remove_paired_host, save_paired_host_records,
    upsert_paired_host, PairingError, RemoteDeviceIdentity, RemotePairingPayload,
    RemotePairingResponse,
};
#[cfg(not(target_arch = "wasm32"))]
pub use relay_conn::{
    DeliveryState, PerformParams, RelayConnection, RelayError, RelayTransportResponse,
};
#[cfg(not(target_arch = "wasm32"))]
pub use remote_connection::{
    ConnectOutcome, ReconnectPolicy, RemoteHostConnection, RemoteHostConnector,
};
#[cfg(not(target_arch = "wasm32"))]
pub use remote_runtime::{
    PairedHostConnectionPlan, PairedTransport, RemoteHostConnectionRoute, RemoteHostConnectionState,
};
#[cfg(not(target_arch = "wasm32"))]
pub use transport::{
    connect_direct_classified, AnswerReport, AnswerUiState, DirectFailure, HostClient,
    HostClientError, OutputChunk, UploadChunkParams, UploadProgress,
};
