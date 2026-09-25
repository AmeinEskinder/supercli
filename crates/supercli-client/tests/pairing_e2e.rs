//! End-to-end pairing test against a mock Host that speaks the sealed
//! pairing exchange: the client seals with the QR token, the mock opens,
//! seals its response, and the client validates every binding.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;

use supercli_client::pairing::PAIRING_PROTOCOL_VERSION;
use supercli_client::{
    decode_pairing_code, pair, PairedHostRecord, RemoteDeviceIdentity, RemotePairingPayload,
};

const TOKEN: &str = "one-time-token";
const MAC_ID: &str = "host-abc-123";
const DEVICE_ID: &str = "device-1";

/// Read one HTTP request, run the sealed pairing exchange, and answer.
fn mock_pairing_host(listener: TcpListener, e2e_key_b64: &str) -> thread::JoinHandle<()> {
    let endpoint = format!(
        "http://127.0.0.1:{}/mobile",
        listener.local_addr().unwrap().port()
    );
    let e2e_key_b64 = e2e_key_b64.to_string();
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("client connects");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut request_line = String::new();
        reader.read_line(&mut request_line).expect("request line");
        assert!(
            request_line.starts_with("POST /mobile/pair HTTP/1.1"),
            "pair path, got: {request_line}"
        );
        let mut content_length = 0usize;
        let mut chunked = false;
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("header");
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            headers.push(line.to_string());
            let lower = line.to_lowercase();
            if let Some(value) = lower.strip_prefix("content-length:") {
                content_length = value.trim().parse().expect("content length");
            }
            if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
                chunked = true;
            }
        }
        let body = if chunked {
            read_chunked(&mut reader)
        } else {
            assert!(content_length > 0, "sealed body present");
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body).expect("body");
            body
        };

        // Open the client's sealed request with the pairing crypto.
        let envelope: serde_json::Value = serde_json::from_slice(&body).expect("envelope JSON");
        assert_eq!(envelope["v"], 1);
        let salt = base64_decode(envelope["saltB64"].as_str().unwrap());
        let sealed = base64_decode(envelope["sealedB64"].as_str().unwrap());
        let plaintext = aes_gcm_open(&salt, &sealed, TOKEN, MAC_ID, &endpoint);
        let request: serde_json::Value = serde_json::from_slice(&plaintext).expect("request JSON");
        assert_eq!(request["token"], TOKEN);
        assert_eq!(request["device"]["id"], DEVICE_ID);

        // Seal the response the way the Host does.
        let response_body = serde_json::json!({
            "protocolVersion": PAIRING_PROTOCOL_VERSION,
            "macID": MAC_ID,
            "macName": "Test Mac",
            "endpoint": endpoint,
            "deviceID": DEVICE_ID,
            "authToken": "auth-token-1",
            "pairedAtUnixMs": 1700000000000i64,
            "relayCredentials": {
                "relayURL": "wss://relay.example.com",
                "macID": MAC_ID,
                "relayToken": "relay-token-1",
                "e2eKeyB64": e2e_key_b64,
            },
        });
        let response_bytes = serde_json::to_vec(&response_body).unwrap();
        let (resp_salt_b64, resp_sealed_b64) =
            aes_gcm_seal(&response_bytes, TOKEN, MAC_ID, &endpoint);
        let response_envelope = serde_json::json!({
            "v": 1,
            "saltB64": resp_salt_b64,
            "sealedB64": resp_sealed_b64,
        });
        let response_envelope_bytes = serde_json::to_vec(&response_envelope).unwrap();

        let mut stream = reader.into_inner();
        let http = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response_envelope_bytes.len()
        );
        stream.write_all(http.as_bytes()).expect("status");
        stream.write_all(&response_envelope_bytes).expect("body");
        stream.flush().expect("flush");
    })
}

/// Minimal chunked transfer-encoding reader for the mock host.
fn read_chunked(reader: &mut BufReader<std::net::TcpStream>) -> Vec<u8> {
    let mut body = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("chunk size");
        let size = usize::from_str_radix(line.trim().split(';').next().unwrap(), 16)
            .expect("chunk size hex");
        if size == 0 {
            // Consume trailing headers + final CRLF.
            loop {
                let mut trailer = String::new();
                reader.read_line(&mut trailer).expect("trailer");
                if trailer.trim().is_empty() {
                    break;
                }
            }
            break;
        }
        let mut chunk = vec![0u8; size];
        reader.read_exact(&mut chunk).expect("chunk");
        body.extend_from_slice(&chunk);
        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf).expect("chunk crlf");
    }
    assert!(!body.is_empty(), "sealed body present");
    body
}

fn base64_decode(s: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).unwrap()
}

fn base64_encode(b: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(b)
}

/// Minimal reimplementation of the pairing crypto for the mock host side
/// (independent of the client's `pairing` module internals).
fn pairing_key(token: &str, salt: &[u8], direction: &str) -> [u8; 32] {
    use hkdf::Hkdf;
    use sha2::Sha256;
    let hk = Hkdf::<Sha256>::new(Some(salt), token.as_bytes());
    let mut okm = [0u8; 32];
    hk.expand(
        format!("supercli-pairing-v1:{direction}").as_bytes(),
        &mut okm,
    )
    .unwrap();
    okm
}

fn pairing_aad(mac_id: &str, endpoint: &str, direction: &str) -> Vec<u8> {
    format!("supercli-pairing-v1\0{direction}\0{mac_id}\0{endpoint}").into_bytes()
}

fn aes_gcm_open(salt: &[u8], sealed: &[u8], token: &str, mac_id: &str, endpoint: &str) -> Vec<u8> {
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    use aes_gcm::{Aes256Gcm, Key, Nonce};
    let key = pairing_key(token, salt, "phone-to-mac");
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    cipher
        .decrypt(
            Nonce::from_slice(&sealed[..12]),
            Payload {
                msg: &sealed[12..],
                aad: &pairing_aad(mac_id, endpoint, "phone-to-mac"),
            },
        )
        .expect("mock host opens request")
}

fn aes_gcm_seal(plaintext: &[u8], token: &str, mac_id: &str, endpoint: &str) -> (String, String) {
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    use aes_gcm::{Aes256Gcm, Key, Nonce};
    use getrandom::getrandom;
    let mut salt = [0u8; 16];
    getrandom(&mut salt).unwrap();
    let mut nonce = [0u8; 12];
    getrandom(&mut nonce).unwrap();
    let key = pairing_key(token, &salt, "mac-to-phone");
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let mut ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &pairing_aad(mac_id, endpoint, "mac-to-phone"),
            },
        )
        .unwrap();
    let mut sealed = nonce.to_vec();
    sealed.append(&mut ciphertext);
    (base64_encode(&salt), base64_encode(&sealed))
}

#[test]
fn pairing_e2e_sealed_exchange() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let e2e_key_b64 = base64_encode(&[7u8; 32]);
    let server = mock_pairing_host(listener, &e2e_key_b64);

    // The QR code the user scans.
    let code = format!("SUPERCLI:1:127.0.0.1:{port}:HOST-ABC-123:{TOKEN}:1999999999");
    let payload: RemotePairingPayload = decode_pairing_code(&code).expect("decode QR");
    assert_eq!(payload.mac_id, MAC_ID);

    let device = RemoteDeviceIdentity {
        id: DEVICE_ID.to_string(),
        name: "Test Phone".to_string(),
        platform: "ios".to_string(),
        app_version: None,
    };
    // `now` well before expiry.
    let response = pair(&payload, &device, 1_700_000_000_000).expect("pair");
    assert_eq!(response.mac_id, MAC_ID);
    assert_eq!(response.auth_token, "auth-token-1");
    assert_eq!(response.relay_credentials.mac_id, MAC_ID);
    assert_eq!(response.relay_credentials.relay_token, "relay-token-1");
    assert_eq!(
        response.relay_credentials.e2e_key_b64,
        base64_encode(&[7u8; 32])
    );

    // The persisted record the Controller keeps.
    let record = PairedHostRecord::from_pairing_response(&response, None);
    assert_eq!(record.host_id, MAC_ID);
    assert_eq!(record.endpoint, payload.endpoint);
    assert!(record.is_link_enabled());

    server.join().expect("mock host finishes");
}

#[test]
fn pairing_rejects_expired_code() {
    let payload = RemotePairingPayload {
        protocol_version: PAIRING_PROTOCOL_VERSION,
        mac_id: MAC_ID.to_string(),
        mac_name: String::new(),
        endpoint: "http://127.0.0.1:1/mobile".to_string(),
        token: TOKEN.to_string(),
        certificate_fingerprint: None,
        expires_at_unix_ms: 1_000,
    };
    let device = RemoteDeviceIdentity {
        id: DEVICE_ID.to_string(),
        name: "t".to_string(),
        platform: "ios".to_string(),
        app_version: None,
    };
    let err = pair(&payload, &device, 2_000).unwrap_err();
    assert_eq!(err.to_string(), "pairing code expired");
}
