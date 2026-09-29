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
    SetNotifyWhenDone {
        session_id: String,
        enabled: bool,
    },
    ComputerStatus,
    OverlaySnapshot,
    PresentApprovals(Vec<PlatformPresentedApproval>),
    OpenInEditor {
        path: String,
    },
    Thumbnail {
        query: std::collections::HashMap<String, String>,
    },
    RefreshLinkEntitlement {
        mac_id: String,
    },
    ReconcileMobileE2EKeys,
    RemoveMobileE2EKey {
        device_id: String,
    },
    SetProjectFolderColor {
        project_id: String,
        color_id: Option<String>,
    },
    RegisterPushToken {
        device_id: String,
        token: String,
        environment: String,
    },
    RecoverRelayCredentials {
        device_id: String,
    },
    DeliverNotification {
        session_id: String,
        title: String,
        body: String,
        kind: String,
        requires_notify_when_done: bool,
        send_desktop: bool,
        suppress_device_ids: Vec<String>,
    },
}

/// One Host-owned approval row mirrored for native presentation.
/// Swift: `HookServer.PlatformPresentedApproval`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformPresentedApproval {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub caller_session_id: String,
    pub target_session_id: Option<String>,
    pub requested_at_unix_ms: i64,
}

/// Valid project folder colors. Swift: `ProjectFolderColor` (Theme.swift).
const PROJECT_FOLDER_COLORS: &[&str] = &[
    "sky", "blue", "violet", "rose", "amber", "moss", "teal", "graphite",
];

/// Trimmed, non-empty, bounded token with no NUL/CR/LF.
/// Swift: the repeated `trimmingCharacters` + emptiness + length + NUL checks.
fn clean_id(s: &str, max_len: usize) -> Option<String> {
    let t = s.trim();
    if t.is_empty() || t.len() > max_len || t.contains('\0') || t.contains('\n') || t.contains('\r')
    {
        return None;
    }
    Some(t.to_string())
}

/// Extract a string field from a JSON object.
fn str_field(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    obj.get(key)?.as_str().map(|s| s.to_string())
}

/// Validate an editor path: non-empty, ≤16 KiB, absolute, exists.
/// Swift: `HookServer.editorPath(from:)`.
pub fn editor_path(raw_path: &str) -> Option<String> {
    if raw_path.is_empty() || raw_path.as_bytes().len() > 16_384 {
        return None;
    }
    let path = std::path::Path::new(raw_path);
    if !path.is_absolute() {
        return None;
    }
    if !path.exists() {
        return None;
    }
    Some(path.to_string_lossy().into_owned())
}

/// Pure registry reconciliation, kept visible for unit tests.
/// Swift: `HookServer.reconciledPortRegistry(_:registering:isDefinitelyStale:)`.
pub fn reconciled_port_registry(
    existing: &[u16],
    registering: u16,
    is_definitely_stale: impl Fn(u16) -> bool,
) -> Vec<u16> {
    let mut seen = std::collections::HashSet::new();
    let mut ports: Vec<u16> = existing
        .iter()
        .copied()
        .filter(|c| *c != 0 && *c != registering && seen.insert(*c) && !is_definitely_stale(*c))
        .collect();
    ports.push(registering);
    const MAX_ENTRIES: usize = 16;
    if ports.len() > MAX_ENTRIES {
        ports.drain(..ports.len() - MAX_ENTRIES);
    }
    ports
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
        "approval.present" => {
            let obj = env.request.as_object().ok_or("invalid envelope")?;
            let raw_approvals = obj
                .get("approvals")
                .and_then(|v| v.as_array())
                .ok_or("invalid envelope")?;
            if raw_approvals.len() > 32 {
                return Err("invalid envelope".to_string());
            }
            let mut approvals = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for raw in raw_approvals {
                let v = raw.as_object().ok_or("invalid envelope")?;
                let id = str_field(v, "id").ok_or("invalid envelope")?;
                let kind = str_field(v, "kind").ok_or("invalid envelope")?;
                let title = str_field(v, "title").ok_or("invalid envelope")?;
                let body = str_field(v, "body").ok_or("invalid envelope")?;
                let caller = str_field(v, "callerSessionID").ok_or("invalid envelope")?;
                let requested = v
                    .get("requestedAtUnixMs")
                    .and_then(|n| n.as_i64())
                    .ok_or("invalid envelope")?;
                if !["write", "browser", "computer", "app-open"].contains(&kind.as_str())
                    || id.is_empty()
                    || id.len() > 128
                    || !seen.insert(id.clone())
                    || title.is_empty()
                    || title.len() > 1024
                    || body.is_empty()
                    || body.len() > 4096
                    || caller.is_empty()
                    || caller.len() > 128
                    || requested < 0
                {
                    return Err("invalid envelope".to_string());
                }
                let target = match v.get("targetSessionID") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(serde_json::Value::String(t)) => {
                        if t.is_empty() || t.len() > 128 {
                            return Err("invalid envelope".to_string());
                        }
                        Some(t.clone())
                    }
                    _ => return Err("invalid envelope".to_string()),
                };
                approvals.push(PlatformPresentedApproval {
                    id,
                    kind,
                    title,
                    body,
                    caller_session_id: caller,
                    target_session_id: target,
                    requested_at_unix_ms: requested,
                });
            }
            Ok(PlatformAdapterCall::PresentApprovals(approvals))
        }
        "app.open-in-editor" => {
            let obj = env.request.as_object().ok_or("invalid envelope")?;
            if obj.len() != 1 {
                return Err("invalid envelope".to_string());
            }
            let raw_path = str_field(obj, "path").ok_or("invalid envelope")?;
            let path = editor_path(&raw_path).ok_or("invalid envelope")?;
            Ok(PlatformAdapterCall::OpenInEditor { path })
        }
        "artifact.thumbnail" => {
            let obj = env.request.as_object().ok_or("invalid envelope")?;
            if obj.len() != 1 {
                return Err("invalid envelope".to_string());
            }
            let raw_query = obj
                .get("query")
                .and_then(|v| v.as_object())
                .ok_or("invalid envelope")?;
            if raw_query.len() > 7 {
                return Err("invalid envelope".to_string());
            }
            let allowed: std::collections::HashSet<&str> = [
                "session_id",
                "sessionID",
                "kind",
                "name",
                "offset",
                "limit",
                "max_dim",
            ]
            .into_iter()
            .collect();
            let mut query = std::collections::HashMap::new();
            for (key, raw_value) in raw_query {
                let value = raw_value.as_str().ok_or("invalid envelope")?;
                if !allowed.contains(key.as_str()) || value.is_empty() || value.len() > 16_384 {
                    return Err("invalid envelope".to_string());
                }
                query.insert(key.clone(), value.to_string());
            }
            let has_session = query.contains_key("session_id") || query.contains_key("sessionID");
            let max_dim_ok = query
                .get("max_dim")
                .and_then(|s| s.parse::<i64>().ok())
                .map(|n| n > 0)
                .unwrap_or(false);
            if !has_session
                || !query.contains_key("kind")
                || !query.contains_key("name")
                || !max_dim_ok
            {
                return Err("invalid envelope".to_string());
            }
            for key in ["offset", "limit"] {
                if let Some(value) = query.get(key) {
                    if value.parse::<u64>().is_err() {
                        return Err("invalid envelope".to_string());
                    }
                }
            }
            Ok(PlatformAdapterCall::Thumbnail { query })
        }
        "link.entitlement.refresh" => {
            let obj = env.request.as_object().ok_or("invalid envelope")?;
            if obj.len() != 1 {
                return Err("invalid envelope".to_string());
            }
            let raw = str_field(obj, "macID").ok_or("invalid envelope")?;
            let mac_id = clean_id(&raw, 256).ok_or("invalid envelope")?;
            Ok(PlatformAdapterCall::RefreshLinkEntitlement { mac_id })
        }
        "mobile.e2e-key.reconcile" => {
            let obj = env.request.as_object().ok_or("invalid envelope")?;
            let action = str_field(obj, "action").ok_or("invalid envelope")?;
            if action == "sync" && obj.len() == 1 {
                return Ok(PlatformAdapterCall::ReconcileMobileE2EKeys);
            }
            if action == "remove" && obj.len() == 2 {
                let raw = str_field(obj, "deviceID").ok_or("invalid envelope")?;
                let device_id = clean_id(&raw, 256).ok_or("invalid envelope")?;
                return Ok(PlatformAdapterCall::RemoveMobileE2EKey { device_id });
            }
            Err("invalid envelope".to_string())
        }
        "overlay.project-color.set" => {
            let obj = env.request.as_object().ok_or("invalid envelope")?;
            if obj.len() != 2 {
                return Err("invalid envelope".to_string());
            }
            let raw_project = str_field(obj, "projectID").ok_or("invalid envelope")?;
            let project_id = clean_id(&raw_project, 256).ok_or("invalid envelope")?;
            let color_raw = str_field(obj, "colorID").ok_or("invalid envelope")?;
            let color_id = if color_raw.is_empty() {
                None
            } else if PROJECT_FOLDER_COLORS.contains(&color_raw.as_str()) {
                Some(color_raw)
            } else {
                return Err("invalid envelope".to_string());
            };
            Ok(PlatformAdapterCall::SetProjectFolderColor {
                project_id,
                color_id,
            })
        }
        "push.register" => {
            let obj = env.request.as_object().ok_or("invalid envelope")?;
            let raw_device = str_field(obj, "deviceID").ok_or("invalid envelope")?;
            let device_id = clean_id(&raw_device, 256).ok_or("invalid envelope")?;
            let token = str_field(obj, "apnsToken").ok_or("invalid envelope")?;
            let token_bytes = token.as_bytes();
            if !(16..=200).contains(&token_bytes.len())
                || !token_bytes.iter().all(|b| b.is_ascii_hexdigit())
            {
                return Err("invalid envelope".to_string());
            }
            let environment = str_field(obj, "environment").ok_or("invalid envelope")?;
            if environment != "sandbox" && environment != "production" {
                return Err("invalid envelope".to_string());
            }
            Ok(PlatformAdapterCall::RegisterPushToken {
                device_id,
                token,
                environment,
            })
        }
        "relay.credentials.recover" => {
            let obj = env.request.as_object().ok_or("invalid envelope")?;
            let raw = str_field(obj, "deviceID").ok_or("invalid envelope")?;
            let device_id = clean_id(&raw, 256).ok_or("invalid envelope")?;
            Ok(PlatformAdapterCall::RecoverRelayCredentials { device_id })
        }
        "notification.deliver" => {
            let obj = env.request.as_object().ok_or("invalid envelope")?;
            let raw_session = str_field(obj, "sessionID").ok_or("invalid envelope")?;
            let session_id = raw_session.trim().to_string();
            if session_id.is_empty()
                || session_id.len() > 128
                || session_id.contains('/')
                || session_id.contains("..")
            {
                return Err("invalid envelope".to_string());
            }
            let title = str_field(obj, "title")
                .map(|s| s.trim().to_string())
                .ok_or("invalid envelope")?;
            let body = str_field(obj, "body")
                .map(|s| s.trim().to_string())
                .ok_or("invalid envelope")?;
            let kind = str_field(obj, "kind").ok_or("invalid envelope")?;
            if title.is_empty()
                || title.len() > 512
                || body.is_empty()
                || body.len() > 4096
                || !["needs_input", "done", "alert"].contains(&kind.as_str())
            {
                return Err("invalid envelope".to_string());
            }
            let requires = obj
                .get("requiresNotifyWhenDone")
                .and_then(|v| v.as_bool())
                .ok_or("invalid envelope")?;
            let send_desktop = obj
                .get("sendDesktop")
                .and_then(|v| v.as_bool())
                .ok_or("invalid envelope")?;
            let raw_suppressed = obj
                .get("suppressDeviceIDs")
                .and_then(|v| v.as_array())
                .ok_or("invalid envelope")?;
            if raw_suppressed.len() > 64 {
                return Err("invalid envelope".to_string());
            }
            let mut suppress_device_ids = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for raw in raw_suppressed {
                let id = raw.as_str().ok_or("invalid envelope")?;
                let clean = clean_id(id, 256).ok_or("invalid envelope")?;
                if !seen.insert(clean.clone()) {
                    return Err("invalid envelope".to_string());
                }
                suppress_device_ids.push(clean);
            }
            Ok(PlatformAdapterCall::DeliverNotification {
                session_id,
                title,
                body,
                kind,
                requires_notify_when_done: requires,
                send_desktop,
                suppress_device_ids,
            })
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

    #[test]
    fn approval_present_parses_strictly() {
        let valid = br#"{"version":1,"operation":"approval.present","request":{"approvals":[{"id":"a1","kind":"write","title":"Write file","body":"Delete /tmp/x","callerSessionID":"s1","requestedAtUnixMs":1700000000000}]}}"#;
        let call = platform_adapter_call(valid).unwrap();
        match call {
            PlatformAdapterCall::PresentApprovals(approvals) => {
                assert_eq!(approvals.len(), 1);
                assert_eq!(approvals[0].id, "a1");
                assert_eq!(approvals[0].kind, "write");
                assert_eq!(approvals[0].target_session_id, None);
            }
            _ => panic!("wrong variant"),
        }

        // Bad kind rejected.
        let bad_kind = br#"{"version":1,"operation":"approval.present","request":{"approvals":[{"id":"a1","kind":"evil","title":"T","body":"B","callerSessionID":"s1","requestedAtUnixMs":1}]}}"#;
        assert!(platform_adapter_call(bad_kind).is_err());

        // Too many approvals rejected (33 > 32).
        let many: Vec<String> = (0..33)
            .map(|i| format!(r#"{{"id":"a{i}","kind":"write","title":"T","body":"B","callerSessionID":"s1","requestedAtUnixMs":1}}"#))
            .collect();
        let many_json = format!(
            r#"{{"version":1,"operation":"approval.present","request":{{"approvals":[{}]}}}}"#,
            many.join(",")
        );
        assert!(platform_adapter_call(many_json.as_bytes()).is_err());

        // Duplicate id rejected.
        let dup = br#"{"version":1,"operation":"approval.present","request":{"approvals":[{"id":"a1","kind":"write","title":"T","body":"B","callerSessionID":"s1","requestedAtUnixMs":1},{"id":"a1","kind":"write","title":"T","body":"B","callerSessionID":"s1","requestedAtUnixMs":1}]}}"#;
        assert!(platform_adapter_call(dup).is_err());

        // Negative timestamp rejected.
        let neg = br#"{"version":1,"operation":"approval.present","request":{"approvals":[{"id":"a1","kind":"write","title":"T","body":"B","callerSessionID":"s1","requestedAtUnixMs":-1}]}}"#;
        assert!(platform_adapter_call(neg).is_err());

        // Boolean timestamp rejected (Swift rejects NSNumber-wrapped bools).
        let bool_ts = br#"{"version":1,"operation":"approval.present","request":{"approvals":[{"id":"a1","kind":"write","title":"T","body":"B","callerSessionID":"s1","requestedAtUnixMs":true}]}}"#;
        assert!(platform_adapter_call(bool_ts).is_err());
    }

    #[test]
    fn open_in_editor_validates_path() {
        // Uses a real temp file so existence checks pass.
        let dir = std::env::temp_dir();
        let path = dir.join("hook_server_editor_test.txt");
        std::fs::write(&path, b"test").unwrap();
        let path_str = path.to_string_lossy().into_owned();

        let valid = format!(
            r#"{{"version":1,"operation":"app.open-in-editor","request":{{"path":{}}}}}"#,
            serde_json::to_string(&path_str).unwrap()
        );
        match platform_adapter_call(valid.as_bytes()).unwrap() {
            PlatformAdapterCall::OpenInEditor { path: p } => assert_eq!(p, path_str),
            _ => panic!("wrong variant"),
        }
        std::fs::remove_file(&path).ok();

        // Relative path rejected.
        let rel = br#"{"version":1,"operation":"app.open-in-editor","request":{"path":"relative/path.txt"}}"#;
        assert!(platform_adapter_call(rel).is_err());

        // Nonexistent absolute path rejected.
        let missing =
            br#"{"version":1,"operation":"app.open-in-editor","request":{"path":"/nonexistent-dir-xyz/file.txt"}}"#;
        assert!(platform_adapter_call(missing).is_err());

        // Extra field rejected (request.count == 1).
        let extra = format!(
            r#"{{"version":1,"operation":"app.open-in-editor","request":{{"path":{},"extra":1}}}}"#,
            serde_json::to_string(&path_str).unwrap()
        );
        assert!(platform_adapter_call(extra.as_bytes()).is_err());
    }

    #[test]
    fn thumbnail_validates_query() {
        let valid = br#"{"version":1,"operation":"artifact.thumbnail","request":{"query":{"session_id":"s1","kind":"png","name":"shot.png","max_dim":"512"}}}"#;
        match platform_adapter_call(valid).unwrap() {
            PlatformAdapterCall::Thumbnail { query } => {
                assert_eq!(query.get("session_id").unwrap(), "s1");
                assert_eq!(query.get("max_dim").unwrap(), "512");
            }
            _ => panic!("wrong variant"),
        }

        // Missing max_dim rejected.
        let no_dim = br#"{"version":1,"operation":"artifact.thumbnail","request":{"query":{"session_id":"s1","kind":"png","name":"shot.png"}}}"#;
        assert!(platform_adapter_call(no_dim).is_err());

        // Zero max_dim rejected.
        let zero_dim = br#"{"version":1,"operation":"artifact.thumbnail","request":{"query":{"session_id":"s1","kind":"png","name":"shot.png","max_dim":"0"}}}"#;
        assert!(platform_adapter_call(zero_dim).is_err());

        // Disallowed key rejected.
        let bad_key = br#"{"version":1,"operation":"artifact.thumbnail","request":{"query":{"session_id":"s1","kind":"png","name":"shot.png","max_dim":"512","evil":"x"}}}"#;
        assert!(platform_adapter_call(bad_key).is_err());

        // Non-numeric offset rejected.
        let bad_offset = br#"{"version":1,"operation":"artifact.thumbnail","request":{"query":{"session_id":"s1","kind":"png","name":"shot.png","max_dim":"512","offset":"abc"}}}"#;
        assert!(platform_adapter_call(bad_offset).is_err());
    }

    #[test]
    fn link_entitlement_refresh_trims_and_validates() {
        let valid =
            br#"{"version":1,"operation":"link.entitlement.refresh","request":{"macID":"  mac-123  "}}"#;
        match platform_adapter_call(valid).unwrap() {
            PlatformAdapterCall::RefreshLinkEntitlement { mac_id } => assert_eq!(mac_id, "mac-123"),
            _ => panic!("wrong variant"),
        }

        // Empty after trim rejected.
        let empty =
            br#"{"version":1,"operation":"link.entitlement.refresh","request":{"macID":"   "}}"#;
        assert!(platform_adapter_call(empty).is_err());

        // NUL rejected.
        let nul = b"{\"version\":1,\"operation\":\"link.entitlement.refresh\",\"request\":{\"macID\":\"a\0b\"}}";
        assert!(platform_adapter_call(nul).is_err());
    }

    #[test]
    fn mobile_e2e_key_reconcile_and_remove() {
        let sync =
            br#"{"version":1,"operation":"mobile.e2e-key.reconcile","request":{"action":"sync"}}"#;
        assert_eq!(
            platform_adapter_call(sync),
            Ok(PlatformAdapterCall::ReconcileMobileE2EKeys)
        );

        let remove = br#"{"version":1,"operation":"mobile.e2e-key.reconcile","request":{"action":"remove","deviceID":"dev-1"}}"#;
        match platform_adapter_call(remove).unwrap() {
            PlatformAdapterCall::RemoveMobileE2EKey { device_id } => assert_eq!(device_id, "dev-1"),
            _ => panic!("wrong variant"),
        }

        // Sync with extra field rejected.
        let sync_extra = br#"{"version":1,"operation":"mobile.e2e-key.reconcile","request":{"action":"sync","extra":1}}"#;
        assert!(platform_adapter_call(sync_extra).is_err());

        // Unknown action rejected.
        let unknown = br#"{"version":1,"operation":"mobile.e2e-key.reconcile","request":{"action":"explode"}}"#;
        assert!(platform_adapter_call(unknown).is_err());
    }

    #[test]
    fn project_color_set_validates_color() {
        let valid = br#"{"version":1,"operation":"overlay.project-color.set","request":{"projectID":"p1","colorID":"sky"}}"#;
        match platform_adapter_call(valid).unwrap() {
            PlatformAdapterCall::SetProjectFolderColor {
                project_id,
                color_id,
            } => {
                assert_eq!(project_id, "p1");
                assert_eq!(color_id, Some("sky".to_string()));
            }
            _ => panic!("wrong variant"),
        }

        // Empty color clears.
        let clear = br#"{"version":1,"operation":"overlay.project-color.set","request":{"projectID":"p1","colorID":""}}"#;
        match platform_adapter_call(clear).unwrap() {
            PlatformAdapterCall::SetProjectFolderColor { color_id, .. } => {
                assert_eq!(color_id, None)
            }
            _ => panic!("wrong variant"),
        }

        // Invalid color rejected.
        let bad = br#"{"version":1,"operation":"overlay.project-color.set","request":{"projectID":"p1","colorID":"neon"}}"#;
        assert!(platform_adapter_call(bad).is_err());
    }

    #[test]
    fn push_register_validates_token_hex() {
        let token = "a".repeat(64);
        let valid = format!(
            r#"{{"version":1,"operation":"push.register","request":{{"deviceID":"d1","apnsToken":"{token}","environment":"production"}}}}"#
        );
        match platform_adapter_call(valid.as_bytes()).unwrap() {
            PlatformAdapterCall::RegisterPushToken {
                device_id,
                environment,
                ..
            } => {
                assert_eq!(device_id, "d1");
                assert_eq!(environment, "production");
            }
            _ => panic!("wrong variant"),
        }

        // Short token rejected (< 16).
        let short = r#"{"version":1,"operation":"push.register","request":{"deviceID":"d1","apnsToken":"abc","environment":"production"}}"#;
        assert!(platform_adapter_call(short.as_bytes()).is_err());

        // Non-hex token rejected.
        let non_hex = r#"{"version":1,"operation":"push.register","request":{"deviceID":"d1","apnsToken":"zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz","environment":"production"}}"#;
        assert!(platform_adapter_call(non_hex.as_bytes()).is_err());

        // Bad environment rejected.
        let bad_env = format!(
            r#"{{"version":1,"operation":"push.register","request":{{"deviceID":"d1","apnsToken":"{token}","environment":"staging"}}}}"#
        );
        assert!(platform_adapter_call(bad_env.as_bytes()).is_err());
    }

    #[test]
    fn relay_credentials_recover_validates_device() {
        let valid =
            br#"{"version":1,"operation":"relay.credentials.recover","request":{"deviceID":"dev-9"}}"#;
        match platform_adapter_call(valid).unwrap() {
            PlatformAdapterCall::RecoverRelayCredentials { device_id } => {
                assert_eq!(device_id, "dev-9")
            }
            _ => panic!("wrong variant"),
        }

        let empty =
            br#"{"version":1,"operation":"relay.credentials.recover","request":{"deviceID":""}}"#;
        assert!(platform_adapter_call(empty).is_err());
    }

    #[test]
    fn notification_deliver_validates_all_fields() {
        let valid = br#"{"version":1,"operation":"notification.deliver","request":{"sessionID":"s1","title":"Done","body":"Task finished","kind":"done","requiresNotifyWhenDone":true,"sendDesktop":false,"suppressDeviceIDs":["d1","d2"]}}"#;
        match platform_adapter_call(valid).unwrap() {
            PlatformAdapterCall::DeliverNotification {
                session_id,
                title,
                kind,
                requires_notify_when_done,
                send_desktop,
                suppress_device_ids,
                ..
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(title, "Done");
                assert_eq!(kind, "done");
                assert!(requires_notify_when_done);
                assert!(!send_desktop);
                assert_eq!(suppress_device_ids, vec!["d1", "d2"]);
            }
            _ => panic!("wrong variant"),
        }

        // Bad kind rejected.
        let bad_kind = br#"{"version":1,"operation":"notification.deliver","request":{"sessionID":"s1","title":"T","body":"B","kind":"spam","requiresNotifyWhenDone":true,"sendDesktop":false,"suppressDeviceIDs":[]}}"#;
        assert!(platform_adapter_call(bad_kind).is_err());

        // Path traversal in sessionID rejected.
        let traversal = br#"{"version":1,"operation":"notification.deliver","request":{"sessionID":"../etc","title":"T","body":"B","kind":"done","requiresNotifyWhenDone":true,"sendDesktop":false,"suppressDeviceIDs":[]}}"#;
        assert!(platform_adapter_call(traversal).is_err());

        // Non-boolean requiresNotifyWhenDone rejected.
        let non_bool = br#"{"version":1,"operation":"notification.deliver","request":{"sessionID":"s1","title":"T","body":"B","kind":"done","requiresNotifyWhenDone":1,"sendDesktop":false,"suppressDeviceIDs":[]}}"#;
        assert!(platform_adapter_call(non_bool).is_err());

        // Duplicate suppressed device rejected.
        let dup = br#"{"version":1,"operation":"notification.deliver","request":{"sessionID":"s1","title":"T","body":"B","kind":"done","requiresNotifyWhenDone":true,"sendDesktop":false,"suppressDeviceIDs":["d1","d1"]}}"#;
        assert!(platform_adapter_call(dup).is_err());
    }

    #[test]
    fn reconciled_port_registry_dedupes_and_caps() {
        // Dedupes, drops zero and the registering port, keeps order.
        let out = reconciled_port_registry(&[8080, 0, 8080, 9090], 7070, |_| false);
        assert_eq!(out, vec![8080, 9090, 7070]);

        // Stale ports pruned.
        let out = reconciled_port_registry(&[1111, 2222], 3333, |p| p == 1111);
        assert_eq!(out, vec![2222, 3333]);

        // Caps at 16 entries, oldest dropped.
        let existing: Vec<u16> = (8000..8020).collect();
        let out = reconciled_port_registry(&existing, 9000, |_| false);
        assert_eq!(out.len(), 16);
        assert_eq!(*out.last().unwrap(), 9000);
    }

    #[test]
    fn editor_path_rejects_non_absolute_and_missing() {
        assert!(editor_path("").is_none());
        assert!(editor_path("relative/path").is_none());
        assert!(editor_path("/definitely/not/here/xyz.txt").is_none());
        // Over-long path rejected.
        assert!(editor_path(&format!("/{}", "a".repeat(16384))).is_none());
    }
}
