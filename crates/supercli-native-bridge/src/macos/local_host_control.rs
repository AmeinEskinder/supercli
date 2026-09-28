//! Port of `LocalHostControl.swift` — same-user admin client.
//!
//! Verbs for the canonical workspace worker. Intentionally local-only:
//! they travel over the mode-0600 `host.sock`; the native app never edits
//! Host authorization files itself in client-only mode.
//!
//! Request/response types and JSON shapes are cross-platform and tested.
//! The actual bridge call (`supercli_native_bridge_local_host_control`)
//! is `#[cfg(target_os = "macos")]`.

/// Pairing status from the Host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PairingStatus {
    Active,
    Completed,
    Closed,
}

/// A same-user admin request to the workspace worker.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Request {
    pub action: String,
    #[serde(rename = "advertisedHost", skip_serializing_if = "Option::is_none")]
    pub advertised_host: Option<String>,
    #[serde(rename = "advertisedPort", skip_serializing_if = "Option::is_none")]
    pub advertised_port: Option<u16>,
    #[serde(rename = "deviceID", skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed: Option<bool>,
}

impl Request {
    pub fn begin() -> Self {
        Self {
            action: "begin".to_string(),
            advertised_host: None,
            advertised_port: None,
            device_id: None,
            allowed: None,
        }
    }

    pub fn status() -> Self {
        Self {
            action: "status".to_string(),
            advertised_host: None,
            advertised_port: None,
            device_id: None,
            allowed: None,
        }
    }

    pub fn cancel() -> Self {
        Self {
            action: "cancel".to_string(),
            advertised_host: None,
            advertised_port: None,
            device_id: None,
            allowed: None,
        }
    }

    pub fn devices() -> Self {
        Self {
            action: "devices".to_string(),
            advertised_host: None,
            advertised_port: None,
            device_id: None,
            allowed: None,
        }
    }

    pub fn revoke_device(id: &str) -> Self {
        Self {
            action: "revoke-device".to_string(),
            advertised_host: None,
            advertised_port: None,
            device_id: Some(id.to_string()),
            allowed: None,
        }
    }

    pub fn set_relay_allowed(id: &str, allowed: bool) -> Self {
        Self {
            action: "set-relay-allowed".to_string(),
            advertised_host: None,
            advertised_port: None,
            device_id: Some(id.to_string()),
            allowed: Some(allowed),
        }
    }
}

/// The bridge call envelope: home + request.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CallConfig {
    #[serde(rename = "supercliHome")]
    pub supercli_home: String,
    pub request: Request,
}

/// Pairing response: the one-time code.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct PairingResponse {
    pub code: String,
}

/// Status response.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct StatusResponse {
    pub status: PairingStatus,
}

/// Devices snapshot response.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DevicesResponse {
    pub devices: Vec<serde_json::Value>,
    #[serde(rename = "directEndpoint")]
    pub direct_endpoint: Option<String>,
}

/// Bridge error envelope.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct BridgeError {
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_actions_match_swift() {
        assert_eq!(Request::begin().action, "begin");
        assert_eq!(Request::status().action, "status");
        assert_eq!(Request::cancel().action, "cancel");
        assert_eq!(Request::devices().action, "devices");
        assert_eq!(Request::revoke_device("d1").action, "revoke-device");
        assert_eq!(
            Request::set_relay_allowed("d1", true).action,
            "set-relay-allowed"
        );
    }

    #[test]
    fn revoke_device_carries_device_id() {
        let req = Request::revoke_device("device-123");
        assert_eq!(req.device_id, Some("device-123".to_string()));
        assert_eq!(req.allowed, None);
        // Serializes with the Swift field names.
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["action"], "revoke-device");
        assert_eq!(json["deviceID"], "device-123");
    }

    #[test]
    fn set_relay_allowed_carries_flag() {
        let req = Request::set_relay_allowed("device-123", false);
        assert_eq!(req.allowed, Some(false));
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["allowed"], false);
    }

    #[test]
    fn call_config_uses_swift_field_names() {
        let config = CallConfig {
            supercli_home: "/home/user/.supercli".to_string(),
            request: Request::devices(),
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["supercliHome"], "/home/user/.supercli");
        assert_eq!(json["request"]["action"], "devices");
    }

    #[test]
    fn pairing_status_deserializes() {
        let resp: StatusResponse = serde_json::from_str(r#"{"status":"active"}"#).unwrap();
        assert_eq!(resp.status, PairingStatus::Active);
        let resp: StatusResponse = serde_json::from_str(r#"{"status":"completed"}"#).unwrap();
        assert_eq!(resp.status, PairingStatus::Completed);
    }

    #[test]
    fn bridge_error_carries_message() {
        let err: BridgeError = serde_json::from_str(r#"{"message":"rejected"}"#).unwrap();
        assert_eq!(err.message, Some("rejected".to_string()));
    }
}
