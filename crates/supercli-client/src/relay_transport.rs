//! [`DirectTransport`] over the relay: tunnels `/mobile` requests through
//! an E2E-sealed [`RelayConnection`] instead of a Direct LAN route.
//!
//! The [`HostClient`](crate::transport::HostClient) API is unchanged —
//! `roundtrip` maps its `(method, url, auth, body)` calls onto
//! [`RelayConnection::perform`]. The URL's authority is ignored (the relay
//! routes by the connection's mac id); only the `/mobile/...` path and the
//! query pairs cross the tunnel boundary, matching what the Host's relay
//! uplink feeds into its `/mobile` pipeline.

use std::collections::HashMap;
use std::time::Duration;

use super::transport::{DirectTransport, HostClientError};
use crate::relay_conn::{PerformParams, RelayConnection, RelayError};

/// Tunnels [`HostClient`](crate::transport::HostClient) requests through a
/// connected relay, for the Direct→relay fallback.
///
/// Construct via
/// [`HostClient::via_relay`](crate::transport::HostClient::via_relay),
/// not directly.
pub struct RelayTransport {
    conn: RelayConnection,
    timeout: Duration,
}

impl RelayTransport {
    pub(crate) fn new(conn: RelayConnection, timeout: Duration) -> Self {
        Self { conn, timeout }
    }

    /// For tests: which relay connection this transport tunnels through.
    #[cfg(test)]
    pub fn connection(&self) -> &RelayConnection {
        &self.conn
    }
}

/// Split a client-built URL into the tunnel path and its query map.
///
/// The URL looks like `relay://link/mobile/output?session_id=…&limit=…`.
/// Only the `/mobile/...` suffix and the decoded query pairs are
/// tunneled; the scheme and authority never leave the device.
fn split_tunnel_target(url: &str) -> Result<(String, HashMap<String, String>), HostClientError> {
    let invalid =
        |detail: &str| HostClientError::InvalidEndpoint(format!("bad relay URL: {detail}"));
    let after_scheme = url
        .split("://")
        .nth(1)
        .ok_or_else(|| invalid("missing scheme"))?;
    let slash = after_scheme
        .find('/')
        .ok_or_else(|| invalid("missing path"))?;
    let path_and_query = &after_scheme[slash..];
    let (path, raw_query) = match path_and_query.find('?') {
        Some(i) => (&path_and_query[..i], &path_and_query[i + 1..]),
        None => (path_and_query, ""),
    };
    if !path.starts_with("/mobile/") && path != "/mobile" {
        return Err(invalid("path is not under /mobile"));
    }
    let mut query = HashMap::new();
    if !raw_query.is_empty() {
        for pair in raw_query.split('&') {
            let (name, value) = match pair.find('=') {
                Some(i) => (&pair[..i], &pair[i + 1..]),
                None => (pair, ""),
            };
            query.insert(percent_decode(name)?, percent_decode(value)?);
        }
    }
    Ok((path.to_string(), query))
}

/// Decode `%XX` escapes in one query component. `+` is left alone: the
/// client never form-encodes spaces as `+`, so a literal plus must
/// survive the round trip.
fn percent_decode(s: &str) -> Result<String, HostClientError> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(HostClientError::InvalidEndpoint(
                    "truncated percent-escape in relay URL".to_string(),
                ));
            }
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).map_err(|_| {
                HostClientError::InvalidEndpoint("non-UTF8 percent-escape in relay URL".to_string())
            })?;
            let byte = u8::from_str_radix(hex, 16).map_err(|_| {
                HostClientError::InvalidEndpoint("bad percent-escape in relay URL".to_string())
            })?;
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| {
        HostClientError::InvalidEndpoint("non-UTF8 query component in relay URL".to_string())
    })
}

fn relay_error(e: RelayError) -> HostClientError {
    HostClientError::Transport(format!("relay: {e}"))
}

impl DirectTransport for RelayTransport {
    fn roundtrip(
        &self,
        method: &str,
        url: &str,
        auth: &str,
        body: Option<(&str, &[u8])>,
    ) -> Result<(u16, String), HostClientError> {
        if method != "GET" && method != "POST" {
            return Err(HostClientError::Transport(format!(
                "unsupported relay method {method}"
            )));
        }
        if method == "POST" && body.is_none() {
            return Err(HostClientError::Transport(
                "POST requires a body".to_string(),
            ));
        }
        let (path, query) = split_tunnel_target(url)?;
        let (content_type, body_bytes) = match body {
            Some((ct, bytes)) => (Some(ct), Some(bytes)),
            None => (None, None),
        };
        let response = self
            .conn
            .perform(PerformParams {
                method,
                path: &path,
                query,
                auth: Some(auth),
                content_type,
                body: body_bytes,
                timeout: self.timeout,
            })
            .map_err(relay_error)?;
        if response.status < 0 || response.status > 65535 {
            return Err(HostClientError::Decode(format!(
                "relay returned out-of-range status {}",
                response.status
            )));
        }
        let text = String::from_utf8(response.body())
            .map_err(|e| HostClientError::Decode(format!("relay body not UTF-8: {e}")))?;
        Ok((response.status as u16, text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_path_and_query() {
        let (path, query) =
            split_tunnel_target("relay://link/mobile/output?session_id=abc&limit=10&wait_ms=1500")
                .unwrap();
        assert_eq!(path, "/mobile/output");
        assert_eq!(query.get("session_id").map(String::as_str), Some("abc"));
        assert_eq!(query.get("limit").map(String::as_str), Some("10"));
        assert_eq!(query.get("wait_ms").map(String::as_str), Some("1500"));
    }

    #[test]
    fn path_without_query_gives_empty_map() {
        let (path, query) = split_tunnel_target("relay://link/mobile/bootstrap").unwrap();
        assert_eq!(path, "/mobile/bootstrap");
        assert!(query.is_empty());
    }

    #[test]
    fn percent_escapes_decode() {
        let (path, query) = split_tunnel_target("relay://link/mobile/x?q=a%20b%2Fc").unwrap();
        assert_eq!(path, "/mobile/x");
        assert_eq!(query.get("q").map(String::as_str), Some("a b/c"));
    }

    #[test]
    fn plus_survives_decoding() {
        let (_, query) = split_tunnel_target("relay://link/mobile/x?q=a+b").unwrap();
        assert_eq!(query.get("q").map(String::as_str), Some("a+b"));
    }

    #[test]
    fn rejects_non_mobile_paths() {
        assert!(split_tunnel_target("relay://link/admin/secret").is_err());
    }

    #[test]
    fn rejects_malformed_urls() {
        assert!(split_tunnel_target("not-a-url").is_err());
        assert!(split_tunnel_target("relay://link").is_err());
        assert!(split_tunnel_target("relay://link/mobile/x?q=%zz").is_err());
        assert!(split_tunnel_target("relay://link/mobile/x?q=abc%2").is_err());
    }
}
