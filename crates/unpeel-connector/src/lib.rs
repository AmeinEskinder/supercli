//! Connector (plugin) runtime for the Unpeel harness host.
//!
//! Implements `docs/connectors.md`: declarative `connector.toml` v1
//! manifests, discovery, approval policy, keychain token storage, the
//! MCP stdio process lifecycle, MCP over Streamable HTTP, the OAuth2
//! browser dance with token refresh, Ed25519 bundle signing, a local
//! registry, config-schema forms, and per-session attachment records.
//! The manifest is the trust boundary — `tools.provides` is a closed
//! list and `policy` defaults are ceilings.

pub mod bundle;
pub mod discovery;
pub mod form;
pub mod http;
pub mod keychain;
pub mod link;
pub mod manifest;
pub mod oauth;
pub mod policy;
pub mod process;
pub mod registry;
pub mod scaffold;
pub mod session;
pub mod signing;

pub use bundle::{
    bundle_file_name, bundle_manifest, bundle_sha256, pack, unpack_verified, BundleError,
};
pub use discovery::{default_roots, discover, DiscoveredConnector, DiscoveryError};
pub use form::{merge_config, run_form, validate_pairs, FormError};
pub use http::{ConnectorHttp, HttpConnectorError};
pub use keychain::{
    delete_connector_token, keychain_store, load_connector_token, open_connector_store,
    store_connector_token, CONNECTORS_KEYCHAIN_ENV, CONNECTOR_KEYCHAIN_SERVICE,
};
pub use link::{config_string, mcp_url_from_config, ConnectorLink, LinkError};
pub use manifest::{
    parse_manifest, ApprovalPolicy, AuthFlow, ConnectorKind, ConnectorManifest, ManifestError,
    OAuthEndpoints,
};
pub use oauth::{
    ensure_fresh, now_unix, refresh_access_token, resolve_connector_token, run_oauth_dance,
    OAuthError, TokenSet,
};
pub use policy::{effective_policy, silently_allowed};
pub use process::{ConnectorError, ConnectorProcess, McpTool, CONNECTOR_TOKEN_ENV};
pub use registry::{
    default_registry_dir, fetch, publish, read_index, resolve, RegistryError, RegistryIndex,
};
pub use scaffold::{
    auth_flow_from_str, connector_kind_from_str, scaffold, ScaffoldError, ScaffoldOptions,
    ScaffoldedConnector,
};
pub use session::{
    attachments_path, disable_attachment, enable_attachment, read_attachments, write_attachments,
    SessionAttachment, SessionAttachmentError, SessionAttachments, ATTACHMENTS_FILE,
};
pub use signing::{
    keygen, keys_dir, load_public_key, load_secret_key, parse_public_key, public_key_base64,
    read_signature, sign_bundle, verify_bundle, BundleSignature, SigningError,
};
/// Re-exported so Host-side callers can name the token-store type.
pub use unpeel_client::CredentialStore;
