//! Certificate-pinned TLS for the Direct `/mobile` endpoint.
//!
//! Swift parity with `RemoteCertificatePinningDelegate`: the Direct HTTPS
//! connection is pinned to the lowercase hex SHA-256 of the leaf
//! certificate's DER. Chain validation is deliberately skipped — Hosts
//! serve self-signed certificates, and the fingerprint delivered inside
//! the sealed pairing response is the trust root. The TLS handshake
//! signatures are still verified against the pinned leaf, so the peer
//! must hold the certificate's private key.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};
use sha2::{Digest, Sha256};

/// Normalize a fingerprint the way Swift's
/// `RemoteDirectTransportPolicy.normalizedFingerprint` does: trim and
/// lowercase. Returns `None` for empty input.
pub fn normalize_fingerprint(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_lowercase();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Lowercase hex SHA-256 of a certificate's DER encoding — the fingerprint
/// format both sides compare.
pub fn leaf_fingerprint_hex(cert_der: &[u8]) -> String {
    let digest = Sha256::digest(cert_der);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        write!(hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    hex
}

/// rustls verifier that pins the leaf certificate's SHA-256 fingerprint.
/// Anything else — wrong cert, CA-signed cert for the same name — fails
/// the handshake.
#[derive(Debug)]
pub struct PinningVerifier {
    pinned: String,
    provider: Arc<CryptoProvider>,
}

impl PinningVerifier {
    /// `None` when the fingerprint is not a plausible 64-char hex SHA-256.
    pub fn new(pinned_fingerprint: &str) -> Option<Self> {
        let pinned = normalize_fingerprint(pinned_fingerprint)?;
        if pinned.len() != 64 || !pinned.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(Self {
            pinned,
            provider: Arc::new(rustls::crypto::ring::default_provider()),
        })
    }

    pub fn pinned_fingerprint(&self) -> &str {
        &self.pinned
    }
}

impl ServerCertVerifier for PinningVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if leaf_fingerprint_hex(end_entity.as_ref()) == self.pinned {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(Error::InvalidCertificate(
                rustls::CertificateError::UnknownIssuer,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
            .to_vec()
    }
}

/// A rustls client config that trusts exactly one certificate: the pinned
/// leaf. `None` when the fingerprint is malformed.
pub fn pinned_client_config(pinned_fingerprint: &str) -> Option<Arc<ClientConfig>> {
    let verifier = Arc::new(PinningVerifier::new(pinned_fingerprint)?);
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(rustls::ALL_VERSIONS)
        .expect("ring provides TLS 1.2/1.3")
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    Some(Arc::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_helpers() {
        assert_eq!(
            normalize_fingerprint("  AB:cd\n"),
            Some("ab:cd".to_string())
        );
        assert_eq!(normalize_fingerprint("   "), None);
        assert!(PinningVerifier::new("not-hex").is_none());
        assert!(PinningVerifier::new(&"ab".repeat(32)).is_some());
        // Uppercase fingerprints are accepted and normalized.
        let upper = "AB".repeat(32);
        let verifier = PinningVerifier::new(&upper).unwrap();
        assert_eq!(verifier.pinned_fingerprint(), &"ab".repeat(32));
    }

    #[test]
    fn leaf_fingerprint_is_sha256_hex() {
        let fp = leaf_fingerprint_hex(b"fake-der");
        assert_eq!(fp.len(), 64);
        assert!(fp.bytes().all(|b| b.is_ascii_hexdigit()));
        // Matches sha256sum of the same input.
        let expected = {
            let d = Sha256::digest(b"fake-der");
            let mut s = String::new();
            for byte in d {
                use std::fmt::Write;
                write!(s, "{byte:02x}").unwrap();
            }
            s
        };
        assert_eq!(fp, expected);
    }
}
