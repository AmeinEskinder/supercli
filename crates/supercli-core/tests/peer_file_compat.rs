//! Peer-file compatibility: the writer side (`remote_server::peer_file_json`,
//! the {url, token, fingerprint} shape written to remote.json / --peer-file
//! files) must be readable by the reader side
//! (`remote_attach::read_peer_file`). Guards the contract both halves rely
//! on, including any future client writer.

#![cfg(unix)]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "supercli-peer-compat-{name}-{}-{nanos}",
        std::process::id()
    ))
}

#[test]
fn writer_shape_round_trips_through_core_reader() {
    let dir = test_dir("roundtrip");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("peer.json");

    // Write with the real writer-side constructor, plus the extra
    // bookkeeping fields write_remote_state adds (the reader ignores them).
    let mut body = supercli_core::remote_server::peer_file_json(
        "https://192.168.1.5:55280",
        "sekrit",
        "aabbcc",
    );
    body["port"] = serde_json::json!(55280);
    body["pid"] = serde_json::json!(1234);
    std::fs::write(&path, format!("{body}\n")).unwrap();

    let (url, token, fingerprint) =
        supercli_core::remote_attach::read_peer_file(path.to_str().unwrap())
            .expect("core must read the writer's peer file");
    assert_eq!(url, "https://192.168.1.5:55280");
    assert_eq!(token, "sekrit");
    assert_eq!(fingerprint.as_deref(), Some("aabbcc"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reader_tolerates_writer_shape_without_fingerprint() {
    let dir = test_dir("nofp");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("peer.json");

    // Loopback peer files may omit the fingerprint (fail-closed gate allows
    // loopback without one).
    std::fs::write(
        &path,
        r#"{"url":"https://127.0.0.1:55280","token":"sekrit"}"#,
    )
    .unwrap();

    let (url, token, fingerprint) =
        supercli_core::remote_attach::read_peer_file(path.to_str().unwrap())
            .expect("core must read a fingerprint-less peer file");
    assert_eq!(url, "https://127.0.0.1:55280");
    assert_eq!(token, "sekrit");
    assert_eq!(fingerprint, None);

    std::fs::remove_dir_all(&dir).ok();
}
