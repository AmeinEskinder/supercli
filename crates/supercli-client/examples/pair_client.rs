//! Minimal sealed-pairing client for end-to-end tests.
//!
//! Reads a pairing QR/paste code (`SUPERCLI:1:...`) from argv[1], runs the
//! real sealed `/mobile/pair` exchange via `supercli_client::pair`, and prints
//! the resulting credentials as JSON to stdout:
//!
//! ```json
//! {"device_id": "...", "auth_token": "...", "endpoint": "..."}
//! ```
//!
//! Used by `scripts/e2e-scenario.sh` to exercise the genuine sealed pairing
//! exchange instead of seeding `devices.json`. Not for production use.

use std::time::{SystemTime, UNIX_EPOCH};

use supercli_client::{decode_pairing_code, pair, RemoteDeviceIdentity};

fn main() {
    let code = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: pair_client '<SUPERCLI:1:... code>'");
        std::process::exit(2);
    });
    let payload = decode_pairing_code(&code).unwrap_or_else(|| {
        eprintln!("invalid pairing code");
        std::process::exit(1);
    });
    let device = RemoteDeviceIdentity {
        id: "e2e-controller".to_string(),
        name: "e2e-controller".to_string(),
        platform: "test".to_string(),
        app_version: None,
    };
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    match pair(&payload, &device, now_ms) {
        Ok(response) => {
            println!(
                "{}",
                serde_json::json!({
                    "device_id": response.device_id,
                    "auth_token": response.auth_token,
                    "endpoint": response.endpoint,
                })
            );
        }
        Err(e) => {
            eprintln!("pairing failed: {e}");
            std::process::exit(1);
        }
    }
}
