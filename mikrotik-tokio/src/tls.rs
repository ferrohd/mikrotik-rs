//! TLS support for `MikroTik` device connections.
//!
//! Provides a [`NoVerifier`] that accepts any server certificate — suitable
//! for `MikroTik` routers which use self-signed certificates by default.

use alloc::sync::Arc;
use alloc::vec::Vec;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};

/// A TLS certificate verifier that accepts **any** server certificate.
///
/// This is appropriate for `MikroTik` routers which generate self-signed
/// certificates for their API-SSL service (port 8729).
///
/// **Important:** TLS handshake signatures are still verified to prevent
/// downgrade attacks — only the certificate chain validation is skipped.
#[derive(Debug)]
pub(crate) struct NoVerifier(Arc<CryptoProvider>);

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Build a [`ClientConfig`] that accepts any server certificate.
///
/// Uses the default [`CryptoProvider`] (which must be installed by the user
/// via `ring` or `aws-lc-rs` feature flags).
///
/// # Panics
///
/// Panics if no crypto provider has been installed. Users must enable either
/// the `ring` or `aws-lc-rs` feature flag (which automatically installs
/// the provider).
pub(crate) fn insecure_client_config() -> Arc<ClientConfig> {
    let provider = CryptoProvider::get_default().cloned().expect(
        "a rustls CryptoProvider must be installed — enable the `ring` or `aws-lc-rs` feature",
    );

    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifier(provider)))
        .with_no_client_auth();

    Arc::new(config)
}
