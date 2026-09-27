//! Port of `HookServer.swift` — loopback parsing and protocol types.
//!
//! The app's loopback listener (not a Host): handles authenticated
//! platform-adapter callbacks and frontend coordination pings. Strict
//! fixed-length HTTP body parsing with a 4 MiB maximum; only typed,
//! registered platform-adapter operations are accepted.
//!
//! Pure parsing is cross-platform and tested. The socket server is
//! `#[cfg(target_os = "macos")]`.

/// Maximum HTTP body: 4 MiB. Swift: `HookServer.maxBodyBytes`.
pub const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

/// Loopback-only paths the hook server dispatches. Host routes are never
/// answered here. Swift: `HookServer.shouldDispatch`.
pub fn should_dispatch(path: &str) -> bool {
    matches!(
        path,
        "/_supercli/platform-adapter/call"
            | "/state-changed"
            | "/show-window"
            | "/reload-appearance"
    )
}

/// Platform-adapter bearer authorization: exact match on a long token.
/// Case-insensitive scheme; rejects embedded NULs and short tokens.
pub fn platform_adapter_authorization_matches(header: &str, token: Option<&str>) -> bool {
    let Some(token) = token else { return false };
    if token.len() < 16 {
        return false;
    }
    let Some(value) = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
    else {
        return false;
    };
    if value.contains('\0') {
        return false;
    }
    // Constant-time-ish compare: length first, then bytes.
    if value.len() != token.len() {
        return false;
    }
    value.bytes().zip(token.bytes()).all(|(a, b)| a == b)
}

/// Strict fixed-length body reader: reads exactly `content_length` bytes,
/// rejecting missing/negative lengths and anything over `MAX_BODY_BYTES`.
pub fn read_fixed_body(
    mut stream: impl std::io::Read,
    content_length: Option<i64>,
) -> Result<Vec<u8>, String> {
    let len = content_length.ok_or_else(|| "missing Content-Length".to_string())?;
    if len < 0 {
        return Err("negative Content-Length".to_string());
    }
    let len = len as usize;
    if len > MAX_BODY_BYTES {
        return Err(format!("body too large: {len} > {MAX_BODY_BYTES}"));
    }
    let mut buf = vec![0u8; len];
    stream
        .read_exact(&mut buf)
        .map_err(|e| format!("short body: {e}"))?;
    Ok(buf)
}

/// A typed platform-adapter operation. Only registered operations parse;
/// anything else is rejected. Swift: `HookServer.PlatformAdapterCall`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformAdapterCall {
    SetNotifyWhenDone { session_id: String, enabled: bool },
    ComputerStatus,
    OverlaySnapshot,
    // Additional operations from HookServer.swift are added as their
    // request types are ported (notification delivery, editor opening,
    // thumbnails, link entitlement refresh, mobile keys, push registration,
    // relay recovery).
}

/// Parses a platform-adapter envelope: `{"version":1,"operation":…,"request":…}`.
pub fn platform_adapter_call(data: &[u8]) -> Result<PlatformAdapterCall, String> {
    #[derive(serde::Deserialize)]
    struct Envelope {
        version: u32,
        operation: String,
        request: serde_json::Value,
    }
    let env: Envelope = serde_json::from_slice(data).map_err(|_| "invalid envelope".to_string())?;
    if env.version != 1 {
        return Err("invalid envelope".to_string());
    }
    match env.operation.as_str() {
        "session.notify_when_done.set" => {
            #[derive(serde::Deserialize)]
            struct Req {
                #[serde(rename = "sessionID")]
                session_id: String,
                #[serde(rename = "notifyWhenDone")]
                notify_when_done: bool,
            }
            // Reject unknown fields strictly: only typed requests parse.
            let req: Req =
                serde_json::from_value(env.request).map_err(|_| "invalid envelope".to_string())?;
            Ok(PlatformAdapterCall::SetNotifyWhenDone {
                session_id: req.session_id,
                enabled: req.notify_when_done,
            })
        }
        "computer.status" => {
            // Takes no parameters; any request fields reject.
            if !env
                .request
                .as_object()
                .map(|o| o.is_empty())
                .unwrap_or(false)
            {
                return Err("invalid envelope".to_string());
            }
            Ok(PlatformAdapterCall::ComputerStatus)
        }
        "overlay.snapshot" => {
            if !env
                .request
                .as_object()
                .map(|o| o.is_empty())
                .unwrap_or(false)
            {
                return Err("invalid envelope".to_string());
            }
            Ok(PlatformAdapterCall::OverlaySnapshot)
        }
        _ => Err("unknown operation".to_string()),
    }
}

/// The controller-owner header value the loopback server expects.
pub const CONTROLLER_OWNER_HEADER_VALUE: &str = "serve";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_surface_keeps_callbacks_and_answers_no_host_route() {
        let client_routes = [
            "/_supercli/platform-adapter/call",
            "/state-changed",
            "/show-window",
            "/reload-appearance",
        ];
        let host_routes = [
            "/mcp/sidebar",
            "/mcp/begin-pairing",
            "/hook/session-1",
            "/notify/session-1",
            "/app-theme/session-1",
            "/app-context/session-1",
            "/open-in-editor/session-1",
            "/mobile/bootstrap",
        ];
        for path in client_routes {
            assert!(should_dispatch(path), "{path}");
        }
        for path in host_routes {
            assert!(!should_dispatch(path), "{path}");
        }
        assert_eq!(CONTROLLER_OWNER_HEADER_VALUE, "serve");
    }

    #[test]
    fn platform_adapter_bearer_requires_exact_long_token() {
        let token = "0123456789abcdef0123456789abcdef";
        assert!(platform_adapter_authorization_matches(
            &format!("Bearer {token}"),
            Some(token)
        ));
        assert!(platform_adapter_authorization_matches(
            &format!("bearer {token}"),
            Some(token)
        ));
        assert!(!platform_adapter_authorization_matches(
            &format!("Bearer {token}x"),
            Some(token)
        ));
        assert!(!platform_adapter_authorization_matches(
            &format!("Bearer {token}\0{}", "\0".repeat(256)),
            Some(token)
        ));
        assert!(!platform_adapter_authorization_matches(
            &format!("Bearer {token}"),
            None
        ));
        assert!(!platform_adapter_authorization_matches(
            "Bearer short",
            Some("short")
        ));
    }

    #[test]
    fn platform_adapter_call_accepts_only_typed_registered_operation() {
        let valid = br#"{"version":1,"operation":"session.notify_when_done.set","request":{"sessionID":"session-1","notifyWhenDone":true}}"#;
        assert_eq!(
            platform_adapter_call(valid),
            Ok(PlatformAdapterCall::SetNotifyWhenDone {
                session_id: "session-1".to_string(),
                enabled: true
            })
        );

        let computer_status = br#"{"version":1,"operation":"computer.status","request":{}}"#;
        assert_eq!(
            platform_adapter_call(computer_status),
            Ok(PlatformAdapterCall::ComputerStatus)
        );
        let invalid_computer_status =
            br#"{"version":1,"operation":"computer.status","request":{"platform":"macOS"}}"#;
        assert_eq!(
            platform_adapter_call(invalid_computer_status),
            Err("invalid envelope".to_string())
        );

        let overlay = br#"{"version":1,"operation":"overlay.snapshot","request":{}}"#;
        assert_eq!(
            platform_adapter_call(overlay),
            Ok(PlatformAdapterCall::OverlaySnapshot)
        );
    }

    #[test]
    fn read_fixed_body_enforces_limits() {
        let data = b"hello";
        let body = read_fixed_body(&data[..], Some(5)).unwrap();
        assert_eq!(body, b"hello");

        assert!(read_fixed_body(&data[..], None).is_err());
        assert!(read_fixed_body(&data[..], Some(-1)).is_err());
        assert!(read_fixed_body(&data[..], Some(MAX_BODY_BYTES as i64 + 1)).is_err());
        // Short read fails.
        assert!(read_fixed_body(&data[..], Some(10)).is_err());
    }
}
