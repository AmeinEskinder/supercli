//! Relay end-to-end crypto, byte-compatible with the Swift (`CryptoKit`)
//! and JS (`WebCrypto`) implementations.
//!
//! Design (from `RelayProtocol.swift`):
//! - Per-device static 32-byte `e2e_key`, exchanged at pairing over the LAN.
//! - Per-connection handshake: both sides contribute a fresh 16-byte salt;
//!   session keys are `HKDF-SHA256(e2e_key ‖ shared_secret,
//!   client_salt ‖ host_salt, "unpeel-relay-v1:{c2h,h2c}")` — one
//!   AES-256-GCM key per direction.
//! - Nonces are 12 bytes: a 4-byte direction tag (`c2h!` / `h2c!`) followed
//!   by an 8-byte strictly increasing counter. Receivers enforce
//!   monotonicity, so replayed or reordered ciphertexts are rejected even
//!   though they would decrypt.
//! - Any AEAD failure is terminal for the session: callers must drop the
//!   connection, never skip a frame.
//!
//! Byte compatibility is pinned by `protocol/relay-kat-vectors-v1.json`
//! (see `tests/kat.rs`).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use thiserror::Error;

const INFO_PREFIX: &str = "unpeel-relay-v1:";
const CLIENT_TAG: &[u8; 4] = b"c2h!";
const HOST_TAG: &[u8; 4] = b"h2c!";
/// `[counter u64 BE]` plus the 16-byte AES-GCM authentication tag.
pub const AEAD_OVERHEAD_BYTES: usize = 8 + 16;
/// Cap matching the relay Worker's enforcement (complete sealed payload).
pub const MAX_FRAME_BYTES: usize = 512 * 1024;
/// Largest plaintext that fits in one accepted relay frame.
pub const MAX_PLAINTEXT_BYTES: usize = MAX_FRAME_BYTES - AEAD_OVERHEAD_BYTES;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("invalid key/salt length")]
    BadLength,
    #[error("plaintext exceeds maximum frame size")]
    FrameTooLarge,
    #[error("send counter exhausted")]
    CounterExhausted,
    #[error("frame failed authentication")]
    OpenFailed,
    #[error("replayed or reordered frame")]
    ReplayDetected,
    #[error("AEAD failure: {0}")]
    Aead(String),
}

fn derive_key(ikm: &[u8], salt: &[u8], info: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut okm = [0u8; 32];
    hk.expand(format!("{INFO_PREFIX}{info}").as_bytes(), &mut okm)
        .expect("HKDF expand with fixed length cannot fail");
    okm
}

fn nonce_for(tag: &[u8; 4], counter: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[..4].copy_from_slice(tag);
    n[4..].copy_from_slice(&counter.to_be_bytes());
    n
}

/// One direction-pair of a relay E2E session.
pub struct RelayCryptoSession {
    send_key: [u8; 32],
    receive_key: [u8; 32],
    send_tag: [u8; 4],
    receive_tag: [u8; 4],
    send_counter: u64,
    last_received_counter: u64,
}

impl RelayCryptoSession {
    /// Derive the per-connection session. `is_host` selects which derived
    /// key encrypts which direction — both sides call this with identical
    /// inputs.
    ///
    /// The input keying material is the **static per-device key
    /// concatenated with a fresh ephemeral X25519 shared secret**, so each
    /// connection's keys depend on ephemeral material neither the relay
    /// nor a later static-key theft can reconstruct (forward secrecy).
    pub fn new(
        e2e_key: &[u8; 32],
        shared_secret: &[u8; 32],
        client_salt: &[u8; 16],
        host_salt: &[u8; 16],
        is_host: bool,
    ) -> Result<Self, CryptoError> {
        let mut ikm = [0u8; 64];
        ikm[..32].copy_from_slice(e2e_key);
        ikm[32..].copy_from_slice(shared_secret);
        let mut salt = [0u8; 32];
        salt[..16].copy_from_slice(client_salt);
        salt[16..].copy_from_slice(host_salt);

        let client_to_host = derive_key(&ikm, &salt, "c2h");
        let host_to_client = derive_key(&ikm, &salt, "h2c");

        Ok(Self {
            send_key: if is_host {
                host_to_client
            } else {
                client_to_host
            },
            receive_key: if is_host {
                client_to_host
            } else {
                host_to_client
            },
            send_tag: if is_host { *HOST_TAG } else { *CLIENT_TAG },
            receive_tag: if is_host { *CLIENT_TAG } else { *HOST_TAG },
            send_counter: 0,
            last_received_counter: 0,
        })
    }

    /// Encrypt one frame. Output layout: `[counter u64 BE][GCM
    /// ciphertext+tag]`.
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if plaintext.len() > MAX_PLAINTEXT_BYTES {
            return Err(CryptoError::FrameTooLarge);
        }
        if self.send_counter == u64::MAX {
            return Err(CryptoError::CounterExhausted);
        }
        self.send_counter += 1;
        let counter = self.send_counter;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.send_key));
        let ct = cipher
            .encrypt(
                Nonce::from_slice(&nonce_for(&self.send_tag, counter)),
                plaintext,
            )
            .map_err(|e| CryptoError::Aead(e.to_string()))?;
        let mut out = Vec::with_capacity(8 + ct.len());
        out.extend_from_slice(&counter.to_be_bytes());
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// Decrypt one frame, enforcing the strictly-increasing counter. The
    /// counter is authenticated by construction: it *is* the nonce, so a
    /// forged counter makes the AEAD open fail.
    pub fn open(&mut self, frame: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if frame.len() < 8 + 16 {
            return Err(CryptoError::OpenFailed);
        }
        let mut counter_bytes = [0u8; 8];
        counter_bytes.copy_from_slice(&frame[..8]);
        let counter = u64::from_be_bytes(counter_bytes);
        if counter <= self.last_received_counter {
            return Err(CryptoError::ReplayDetected);
        }
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.receive_key));
        let pt = cipher
            .decrypt(
                Nonce::from_slice(&nonce_for(&self.receive_tag, counter)),
                &frame[8..],
            )
            .map_err(|_| CryptoError::OpenFailed)?;
        self.last_received_counter = counter;
        Ok(pt)
    }
}

/// Handshake helpers: ephemeral-DH + static-key authentication.
pub mod handshake {
    use super::*;
    use hmac::{Hmac, Mac};

    /// HMAC-SHA256 over the length-prefixed handshake transcript, keyed by
    /// a dedicated key derived from the static device key. Both sides
    /// compute it over identical inputs and compare constant-time, so a
    /// relay that swaps the (unauthenticated) plaintext ephemeral keys or
    /// downgrades the version is detected before any sealed frame is
    /// accepted.
    pub fn transcript_mac(
        e2e_key: &[u8; 32],
        device_id: &str,
        client_salt: &[u8; 16],
        host_salt: &[u8; 16],
        client_ephemeral_public_key: &[u8],
        host_ephemeral_public_key: &[u8],
    ) -> [u8; 32] {
        // CryptoKit's HKDF with no salt argument uses an empty salt.
        let hk = Hkdf::<Sha256>::new(None, e2e_key);
        let mut mac_key = [0u8; 32];
        hk.expand(
            format!("{INFO_PREFIX}handshake-mac").as_bytes(),
            &mut mac_key,
        )
        .expect("HKDF expand with fixed length cannot fail");

        let mut transcript = Vec::new();
        transcript.extend_from_slice(&1u32.to_be_bytes()); // protocol version
        for field in [
            device_id.as_bytes(),
            client_salt.as_slice(),
            host_salt.as_slice(),
            client_ephemeral_public_key,
            host_ephemeral_public_key,
        ] {
            // Length-prefix every field so a variable-length device_id can't
            // shift bytes into an adjacent field.
            transcript.extend_from_slice(&(field.len() as u32).to_be_bytes());
            transcript.extend_from_slice(field);
        }

        let mut mac =
            <Hmac<Sha256> as Mac>::new_from_slice(&mac_key).expect("HMAC accepts any key length");
        mac.update(&transcript);
        mac.finalize().into_bytes().into()
    }

    /// Constant-time equality for the MAC comparison.
    pub fn constant_time_equal(a: &[u8], b: &[u8]) -> bool {
        use subtle::ConstantTimeEq;
        a.ct_eq(b).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(a: u8) -> [u8; 32] {
        let mut x = [0u8; 32];
        for (i, b) in x.iter_mut().enumerate() {
            *b = a.wrapping_add(i as u8);
        }
        x
    }

    #[test]
    fn roundtrip_and_replay_rejected() {
        let e2e = k(0);
        let ss = k(0x40);
        let cs: [u8; 16] = k(0x10)[..16].try_into().unwrap();
        let hs: [u8; 16] = k(0xA0)[..16].try_into().unwrap();
        let mut client = RelayCryptoSession::new(&e2e, &ss, &cs, &hs, false).unwrap();
        let mut host = RelayCryptoSession::new(&e2e, &ss, &cs, &hs, true).unwrap();
        let f1 = client.seal(b"hello").unwrap();
        let f2 = client.seal(b"world").unwrap();
        assert_eq!(host.open(&f1).unwrap(), b"hello");
        // Replay of f1 after f2 must be rejected even though it decrypts.
        assert!(matches!(host.open(&f1), Err(CryptoError::ReplayDetected)));
        assert_eq!(host.open(&f2).unwrap(), b"world");
        // Counter prefix is big-endian and strictly increasing.
        assert_eq!(&f1[..8], &1u64.to_be_bytes());
        assert_eq!(&f2[..8], &2u64.to_be_bytes());
    }

    #[test]
    fn tampered_frame_fails() {
        let e2e = k(0);
        let ss = k(0x40);
        let cs: [u8; 16] = k(0x10)[..16].try_into().unwrap();
        let hs: [u8; 16] = k(0xA0)[..16].try_into().unwrap();
        let mut client = RelayCryptoSession::new(&e2e, &ss, &cs, &hs, false).unwrap();
        let mut host = RelayCryptoSession::new(&e2e, &ss, &cs, &hs, true).unwrap();
        let mut f = client.seal(b"secret").unwrap();
        f[10] ^= 0x01;
        assert!(matches!(host.open(&f), Err(CryptoError::OpenFailed)));
    }
}
