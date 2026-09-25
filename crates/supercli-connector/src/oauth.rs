//! OAuth2 for connectors: the harness-side browser dance, token storage,
//! and refresh.
//!
//! The developer declares the provider's endpoints in the manifest:
//! ```toml
//! [oauth]
//! authorize_url = "https://accounts.example.com/o/oauth2/auth"
//! token_url = "https://accounts.example.com/o/oauth2/token"
//! ```
//! and the (public) `client_id` in the connector's `config.json`
//! (`install --form` asks for it). `connect` then:
//! 1. builds an authorize URL (PKCE S256, random `state`),
//! 2. binds a loopback listener for the redirect,
//! 3. opens the system browser (best-effort; the URL is always printed),
//! 4. waits for the `?code=…&state=…` callback,
//! 5. exchanges the code for tokens and stores the resulting
//!    [`TokenSet`] in the keychain.
//!
//! The connector process never sees any of this — it only ever gets the
//! current access token via `SUPERCLI_CONNECTOR_TOKEN` (stdio) or the
//! `Authorization: Bearer` header (HTTP). [`ensure_fresh`] refreshes an
//! expiring token before a spawn/call, so `doctor`'s "token fresh"
//! check and every tool call see a live token.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

use crate::keychain::{load_connector_token, store_connector_token};
use crate::link::config_string;
use crate::manifest::{AuthFlow, ConnectorManifest};
use crate::CredentialStore;

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("I/O: {0}")]
    Io(String),
    #[error("HTTP: {0}")]
    Http(String),
    #[error("credential store: {0}")]
    Store(String),
    #[error("the manifest has no [oauth] endpoints (authorize_url, token_url)")]
    NoEndpoints,
    #[error("no client_id: put it in the connector's config.json (install --form asks for it)")]
    NoClientId,
    #[error("OAuth dance failed: {0}")]
    Dance(String),
    #[error("token endpoint: {0}")]
    Token(String),
    #[error("no stored token (run `supercli connector connect <name>` first)")]
    NotConnected,
    #[error("stored token is corrupt: {0}")]
    Corrupt(String),
}

/// Provider endpoints from the manifest's `[oauth]` table (re-exported
/// from the manifest module for convenience).
pub use crate::manifest::OAuthEndpoints;

/// The stored credential for an OAuth2 connector. Serialized as JSON in
/// the keychain under the same `connector:{name}` account the API-key
/// flow uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Unix seconds when the access token expires, if the provider said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix: Option<i64>,
    /// Kept so refresh works even if the manifest later changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,
    pub obtained_at_unix: i64,
}

impl TokenSet {
    /// Parse what `connect` stored. Accepts the current JSON form and the
    /// legacy plain-string form (a bare access token).
    pub fn from_stored(text: &str) -> Result<Self, OAuthError> {
        if let Ok(set) = serde_json::from_str::<TokenSet>(text) {
            if !set.access_token.is_empty() {
                return Ok(set);
            }
        }
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(OAuthError::Corrupt("empty token".to_string()));
        }
        Ok(TokenSet {
            access_token: trimmed.to_string(),
            refresh_token: None,
            expires_at_unix: None,
            token_url: None,
            obtained_at_unix: now_unix(),
        })
    }

    pub fn to_stored(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// True when the token is expired or expires within `within_secs`.
    /// A token with no expiry is treated as live (some providers don't
    /// say); it is never "refreshed" speculatively.
    pub fn needs_refresh(&self, within_secs: i64) -> bool {
        match self.expires_at_unix {
            Some(exp) => now_unix() >= exp - within_secs,
            None => false,
        }
    }
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn random_b64url(nbytes: usize) -> String {
    let mut buf = vec![0u8; nbytes];
    getrandom::fill(&mut buf).expect("OS randomness");
    B64URL.encode(&buf)
}

/// Refresh the access token when it is expired (or expires within two
/// minutes) and a refresh token is stored. Returns the live set,
/// persisting the rotated tokens back through `persist`.
pub fn ensure_fresh<F>(
    set: &TokenSet,
    client_id: &str,
    timeout: Duration,
    mut persist: F,
) -> Result<TokenSet, OAuthError>
where
    F: FnMut(&TokenSet) -> Result<(), OAuthError>,
{
    if !set.needs_refresh(120) {
        return Ok(set.clone());
    }
    let (refresh_token, token_url) = match (&set.refresh_token, &set.token_url) {
        (Some(r), Some(u)) => (r.clone(), u.clone()),
        _ => {
            return Err(OAuthError::Token(
                "access token expired and no refresh token was stored; reconnect".to_string(),
            ))
        }
    };
    let refreshed = refresh_access_token(&token_url, client_id, &refresh_token, timeout)?;
    persist(&refreshed)?;
    Ok(refreshed)
}

/// Exchange a refresh token for a new access token (RFC 6749 §6).
pub fn refresh_access_token(
    token_url: &str,
    client_id: &str,
    refresh_token: &str,
    timeout: Duration,
) -> Result<TokenSet, OAuthError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent
        .post(token_url)
        .send_form([
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
        ])
        .map_err(|e| OAuthError::Http(e.to_string()))?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| OAuthError::Http(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(OAuthError::Token(format!(
            "HTTP {status}: {}",
            body.chars().take(200).collect::<String>()
        )));
    }
    token_set_from_response(&body, Some(token_url))
}

fn token_set_from_response(body: &str, token_url: Option<&str>) -> Result<TokenSet, OAuthError> {
    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: Option<String>,
        refresh_token: Option<String>,
        expires_in: Option<i64>,
        #[allow(dead_code)]
        error: Option<String>,
        #[allow(dead_code)]
        error_description: Option<String>,
    }
    let parsed: TokenResponse = serde_json::from_str(body)
        .map_err(|e| OAuthError::Token(format!("bad token JSON: {e}")))?;
    let access_token = parsed
        .access_token
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            OAuthError::Token(format!(
                "no access_token in response: {}",
                body.chars().take(200).collect::<String>()
            ))
        })?;
    let now = now_unix();
    Ok(TokenSet {
        access_token,
        refresh_token: parsed.refresh_token,
        expires_at_unix: parsed.expires_in.map(|s| now + s),
        token_url: token_url.map(str::to_string),
        obtained_at_unix: now,
    })
}

/// Resolve the token to inject for one connector, shared by the Host
/// session layer and the CLI:
/// - `none`: empty (the connector needs nothing);
/// - `api-key`: the stored secret, [`OAuthError::NotConnected`] when
///   never connected;
/// - `oauth2`: the stored [`TokenSet`], refreshed first when it is
///   expired or about to expire (the rotated set is persisted back to
///   the store, so refresh is invisible to the caller).
pub fn resolve_connector_token(
    manifest: &ConnectorManifest,
    dir: &Path,
    store: &dyn CredentialStore,
    name: &str,
    timeout: Duration,
) -> Result<String, OAuthError> {
    match manifest.auth_flow {
        AuthFlow::None => Ok(String::new()),
        AuthFlow::ApiKey => {
            let token = load_connector_token(store, name)
                .map_err(|e| OAuthError::Store(e.to_string()))?
                .unwrap_or_default();
            if token.trim().is_empty() {
                return Err(OAuthError::NotConnected);
            }
            Ok(token)
        }
        AuthFlow::OAuth2 => {
            let stored = load_connector_token(store, name)
                .map_err(|e| OAuthError::Store(e.to_string()))?
                .unwrap_or_default();
            if stored.trim().is_empty() {
                return Err(OAuthError::NotConnected);
            }
            let set = TokenSet::from_stored(&stored)?;
            let client_id = config_string(dir, "client_id").unwrap_or_default();
            let fresh = ensure_fresh(&set, &client_id, timeout, |rotated| {
                store_connector_token(store, name, &rotated.to_stored())
                    .map_err(|e| OAuthError::Store(e.to_string()))
            })?;
            Ok(fresh.access_token)
        }
    }
}

/// An OAuth2 dance in progress: the loopback listener is up, the
/// authorization URL has been built, and `finish` waits for the browser
/// callback then exchanges the code. Split out of [`run_oauth_dance`] so
/// tests can drive the callback without a real browser.
pub struct PendingDance {
    listener: TcpListener,
    state: String,
    verifier: String,
    endpoints: OAuthEndpoints,
    client_id: String,
    redirect_uri: String,
    deadline: std::time::Instant,
    exchange_timeout: Duration,
}

/// Build the PKCE authorization URL and start the loopback callback
/// listener. Returns the URL (for the browser, real or simulated) and the
/// pending dance.
pub fn begin_oauth_dance(
    endpoints: &OAuthEndpoints,
    client_id: &str,
    scopes: &[String],
    timeout: Duration,
) -> Result<(String, PendingDance), OAuthError> {
    let verifier = random_b64url(32);
    let challenge = B64URL.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_b64url(16);

    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| OAuthError::Io(e.to_string()))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| OAuthError::Io(e.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|e| OAuthError::Io(e.to_string()))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    let mut url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&state={}&code_challenge={}&code_challenge_method=S256",
        endpoints.authorize_url,
        url_encode(client_id),
        url_encode(&redirect_uri),
        url_encode(&state),
        url_encode(&challenge),
    );
    if !scopes.is_empty() {
        url.push_str(&format!("&scope={}", url_encode(&scopes.join(" "))));
    }

    Ok((
        url,
        PendingDance {
            listener,
            state,
            verifier,
            endpoints: endpoints.clone(),
            client_id: client_id.to_string(),
            redirect_uri,
            deadline: std::time::Instant::now() + timeout,
            exchange_timeout: timeout,
        },
    ))
}

/// Wait for the loopback callback, then exchange the authorization code
/// for a [`TokenSet`].
pub fn finish_oauth_dance(dance: PendingDance) -> Result<TokenSet, OAuthError> {
    let PendingDance {
        listener,
        state,
        verifier,
        endpoints,
        client_id,
        redirect_uri,
        deadline,
        exchange_timeout,
    } = dance;
    let code = loop {
        if std::time::Instant::now() > deadline {
            return Err(OAuthError::Dance(
                "timed out waiting for the browser callback".to_string(),
            ));
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let mut buf = vec![0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let request_line = req.lines().next().unwrap_or("");
                let path = request_line.split_whitespace().nth(1).unwrap_or("");
                let (path_only, query) = match path.split_once('?') {
                    Some((p, q)) => (p, q),
                    None => (path, ""),
                };
                let params: std::collections::HashMap<&str, &str> = query
                    .split('&')
                    .filter_map(|kv| kv.split_once('='))
                    .collect();
                let body = if path_only == "/callback" {
                    match (params.get("code"), params.get("state")) {
                        (Some(code), Some(got_state)) if *got_state == state => {
                            let page = "<html><body><h1>Connected.</h1><p>You can close this tab and return to the terminal.</p></body></html>";
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                page.len(), page
                            );
                            let _ = stream.write_all(response.as_bytes());
                            break url_decode(code).map_err(OAuthError::Dance)?;
                        }
                        _ => {
                            let page = "<html><body><h1>Authorization failed.</h1><p>Missing code or state mismatch; try again.</p></body></html>";
                            let response = format!(
                                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                page.len(), page
                            );
                            let _ = stream.write_all(response.as_bytes());
                            return Err(OAuthError::Dance(
                                "callback missing code or state mismatch".to_string(),
                            ));
                        }
                    }
                } else {
                    let page = "<html><body><p>Not found.</p></body></html>";
                    format!(
                        "HTTP/1.1 404 Not Found\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        page.len(), page
                    )
                };
                let _ = stream.write_all(body.as_bytes());
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(OAuthError::Io(e.to_string())),
        }
    };

    // Exchange the code for tokens.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(exchange_timeout))
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent
        .post(&endpoints.token_url)
        .send_form([
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("client_id", client_id.as_str()),
            ("code_verifier", verifier.as_str()),
        ])
        .map_err(|e| OAuthError::Http(e.to_string()))?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| OAuthError::Http(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(OAuthError::Token(format!(
            "HTTP {status}: {}",
            body.chars().take(300).collect::<String>()
        )));
    }
    token_set_from_response(&body, Some(&endpoints.token_url))
}

/// The interactive OAuth2 dance: print the authorization URL, open the
/// browser, wait for the loopback callback, exchange the code.
pub fn run_oauth_dance(
    endpoints: &OAuthEndpoints,
    client_id: &str,
    scopes: &[String],
    timeout: Duration,
) -> Result<TokenSet, OAuthError> {
    let (url, dance) = begin_oauth_dance(endpoints, client_id, scopes, timeout)?;
    println!("Opening the browser for OAuth authorization…");
    println!("{url}");
    println!("If the browser did not open, visit the URL above.");
    open_browser(&url);
    finish_oauth_dance(dance)
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(not(target_os = "macos"))]
    let opener = "xdg-open";
    let _ = std::process::Command::new(opener).arg(url).spawn();
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn url_decode(s: &str) -> Result<String, String> {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = &s[i + 1..i + 3];
                let b = u8::from_str_radix(hex, 16).map_err(|_| format!("bad escape %{hex}"))?;
                out.push(b);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    fn find_header_end(raw: &[u8]) -> Option<usize> {
        raw.windows(4).position(|w| w == b"\r\n\r\n")
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut raw = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = stream.read(&mut buf).unwrap_or(0);
            if n == 0 {
                break;
            }
            raw.extend_from_slice(&buf[..n]);
            if let Some(end) = find_header_end(&raw) {
                let headers = String::from_utf8_lossy(&raw[..end]).to_string();
                let content_length = headers
                    .lines()
                    .find(|l| l.to_lowercase().starts_with("content-length:"))
                    .and_then(|l| l.split(':').nth(1))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                let want = end + 4 + content_length;
                while raw.len() < want {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    raw.extend_from_slice(&buf[..n]);
                }
                break;
            }
        }
        String::from_utf8_lossy(&raw).into_owned()
    }

    #[test]
    fn token_set_roundtrip_and_legacy() {
        let set = TokenSet {
            access_token: "a".to_string(),
            refresh_token: Some("r".to_string()),
            expires_at_unix: Some(now_unix() + 3600),
            token_url: Some("https://x/token".to_string()),
            obtained_at_unix: now_unix(),
        };
        let stored = set.to_stored();
        let back = TokenSet::from_stored(&stored).unwrap();
        assert_eq!(back.access_token, "a");
        assert!(!back.needs_refresh(120));
        // Legacy plain-string token.
        let legacy = TokenSet::from_stored("plain-token").unwrap();
        assert_eq!(legacy.access_token, "plain-token");
        assert!(!legacy.needs_refresh(120));
    }

    #[test]
    fn needs_refresh_logic() {
        let expired = TokenSet {
            access_token: "a".into(),
            refresh_token: None,
            expires_at_unix: Some(now_unix() - 10),
            token_url: None,
            obtained_at_unix: 0,
        };
        assert!(expired.needs_refresh(120));
        let soon = TokenSet {
            expires_at_unix: Some(now_unix() + 60),
            ..expired.clone()
        };
        assert!(soon.needs_refresh(120));
        assert!(!soon.needs_refresh(30));
    }

    #[test]
    fn refresh_hits_token_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/token", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let req = read_request(&mut stream);
            assert!(req.contains("grant_type=refresh_token"));
            assert!(req.contains("refresh_token=old-refresh"));
            let payload =
                r#"{"access_token":"new-access","refresh_token":"new-refresh","expires_in":3600}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(), payload
            );
            let _ = stream.write_all(response.as_bytes());
        });
        let fresh =
            refresh_access_token(&url, "cid", "old-refresh", Duration::from_secs(10)).unwrap();
        assert_eq!(fresh.access_token, "new-access");
        assert_eq!(fresh.refresh_token.as_deref(), Some("new-refresh"));
        assert_eq!(fresh.token_url.as_deref(), Some(url.as_str()));
        handle.join().unwrap();
    }

    #[test]
    fn dance_begin_finish_headless() {
        // Stub token endpoint.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/token", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let req = read_request(&mut stream);
            assert!(req.contains("grant_type=authorization_code"));
            assert!(req.contains("code_verifier="));
            let payload = r#"{"access_token":"dance-access","refresh_token":"dance-refresh","expires_in":3600}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(response.as_bytes());
        });

        let endpoints = OAuthEndpoints {
            authorize_url: "https://example.com/auth".to_string(),
            token_url: url,
        };
        let (auth_url, dance) = begin_oauth_dance(
            &endpoints,
            "test-client",
            &["read".to_string()],
            Duration::from_secs(15),
        )
        .unwrap();
        // The auth URL carries PKCE, state, and our loopback redirect.
        assert!(auth_url.contains("code_challenge="));
        assert!(auth_url.contains("code_challenge_method=S256"));
        assert!(auth_url.contains("redirect_uri=http%3A%2F%2F127.0.0.1"));

        // Simulate the browser: pull redirect_uri and state out of the
        // auth URL and hit the loopback callback ourselves.
        let query = auth_url.split_once('?').unwrap().1;
        let param = |k: &str| {
            query
                .split('&')
                .filter_map(|kv| kv.split_once('='))
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
                .unwrap()
        };
        let redirect = url_decode(&param("redirect_uri")).unwrap();
        let state = param("state");

        // The callback response only arrives once finish starts accepting,
        // so the dance runs on its own thread while this thread plays the
        // browser.
        let dance_handle = std::thread::spawn(move || finish_oauth_dance(dance));
        let callback = format!("{redirect}?code=headless-code&state={state}");
        let agent: ureq::Agent = ureq::Agent::config_builder().build().into();
        let status = agent.get(&callback).call().unwrap().status().as_u16();
        assert_eq!(status, 200);

        let set = dance_handle.join().unwrap().unwrap();
        assert_eq!(set.access_token, "dance-access");
        assert_eq!(set.refresh_token.as_deref(), Some("dance-refresh"));
        handle.join().unwrap();
    }

    #[test]
    fn ensure_fresh_skips_live_tokens() {
        let set = TokenSet {
            access_token: "live".into(),
            refresh_token: None,
            expires_at_unix: Some(now_unix() + 3600),
            token_url: None,
            obtained_at_unix: 0,
        };
        let mut persisted = false;
        let out = ensure_fresh(&set, "cid", Duration::from_secs(1), |_| {
            persisted = true;
            Ok(())
        })
        .unwrap();
        assert_eq!(out.access_token, "live");
        assert!(!persisted);
    }
}
