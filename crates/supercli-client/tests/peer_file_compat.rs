//! Peer-file compatibility: files written by the client's
//! `remote_supercli_peer::save_peer` (remote-peer.json, 0600, atomic) must be
//! readable by core's `remote_attach::read_peer_file` — the reader behind
//! `--peer-file` and this machine's remote.json. Pins the writer↔reader
//! contract both crates rely on.

#![cfg(all(unix, not(target_arch = "wasm32")))]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use supercli_client::remote_supercli_peer::{peer_file_path, save_peer, RemoteSupercliPeer};

fn test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "supercli-client-peer-{name}-{}-{nanos}",
        std::process::id()
    ))
}

fn peer(url: &str, fingerprint: Option<&str>) -> RemoteSupercliPeer {
    RemoteSupercliPeer {
        url: url.to_string(),
        token: "sekrit-token".to_string(),
        fingerprint: fingerprint.map(str::to_string),
        name: Some("Test Mac".to_string()),
    }
}

#[test]
fn client_peer_file_is_readable_by_core() {
    let dir = test_dir("roundtrip");
    let expected = peer("https://192.168.1.5:55280", Some("aabbccddeeff"));
    save_peer(&dir, &expected).expect("client must write the peer file");
    let path = peer_file_path(&dir);

    let (url, token, fingerprint) =
        supercli_core::remote_attach::read_peer_file(path.to_str().unwrap())
            .expect("core must read the client's peer file");
    assert_eq!(url, expected.url);
    assert_eq!(token, expected.token);
    assert_eq!(fingerprint.as_deref(), Some("aabbccddeeff"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn client_peer_file_without_fingerprint_is_readable_by_core() {
    let dir = test_dir("nofp");
    let expected = peer("https://127.0.0.1:55280", None);
    save_peer(&dir, &expected).expect("client must write the peer file");
    let path = peer_file_path(&dir);

    let (url, token, fingerprint) =
        supercli_core::remote_attach::read_peer_file(path.to_str().unwrap())
            .expect("core must read a fingerprint-less client peer file");
    assert_eq!(url, expected.url);
    assert_eq!(token, expected.token);
    assert_eq!(fingerprint, None);

    std::fs::remove_dir_all(&dir).ok();
}
