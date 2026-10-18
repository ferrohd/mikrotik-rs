//! Typestate builder for [`MikrotikDevice`] connections. The transport state
//! carries its own configuration, so TLS settings only exist when the
//! `tokio-tls` feature does.
//!
//! ```rust,ignore
//! let device = MikrotikDevice::builder("192.168.88.1:8729")
//!     .credentials("admin", Some("password"))
//!     .tls_insecure()           // or .tls_config(config, server_name)
//!     .connect()
//!     .await?;
//! ```

use alloc::string::String;

use tokio::net::ToSocketAddrs;

use crate::device::MikrotikDevice;
use crate::error::DeviceResult;

/// Plaintext TCP transport — carries no additional configuration.
pub struct Plaintext;

/// TLS transport — carries the [`rustls::ClientConfig`] and server name for SNI.
///
/// Only available when the `tokio-tls` feature is enabled.
#[cfg(feature = "tokio-tls")]
pub struct Tls {
    config: alloc::sync::Arc<rustls::ClientConfig>,
    server_name: rustls::pki_types::ServerName<'static>,
}

/// A typestate builder, where `Transport` carries that transport's own
/// configuration. Start from [`MikrotikDevice::builder()`].
#[must_use]
pub struct DeviceBuilder<A, Transport> {
    addr: A,
    username: String,
    password: Option<String>,
    /// Transport-specific state. [`Plaintext`] carries nothing;
    /// [`Tls`] carries the `ClientConfig` and server name.
    /// Read in the `Tls::connect()` path (behind `#[cfg(feature = "tokio-tls")]`).
    #[allow(dead_code)] // consumed by Tls::connect() when tls feature is enabled
    transport: Transport,
}

impl<A: ToSocketAddrs> DeviceBuilder<A, Plaintext> {
    /// Create a new builder targeting the given address.
    ///
    /// The connection starts as plaintext TCP. Use [`.tls_insecure()`](Self::tls_insecure)
    /// or [`.tls_config()`](Self::tls_config) to enable TLS.
    pub(crate) fn new(addr: A) -> Self {
        Self { addr, username: String::new(), password: None, transport: Plaintext }
    }

    /// Set the login credentials.
    pub fn credentials(mut self, username: &str, password: Option<&str>) -> Self {
        self.username = String::from(username);
        self.password = password.map(String::from);
        self
    }

    /// Connects over plaintext TCP, logs in and spawns the actor.
    ///
    /// # Errors
    ///
    /// [`DeviceError`](crate::error::DeviceError) if any of those fail.
    pub async fn connect(self) -> DeviceResult<MikrotikDevice> {
        let stream = tokio::net::TcpStream::connect(self.addr).await?;
        stream.set_nodelay(true)?;
        crate::device::handshake_and_spawn(stream, &self.username, self.password.as_deref()).await
    }

    /// Enables TLS with **no certificate verification**, for the self-signed
    /// certificates MikroTik ships. Handshake signatures are still verified.
    ///
    /// # Panics
    ///
    /// Panics if the built-in placeholder server name fails to parse, which
    /// cannot happen for the fixed `"mikrotik"` literal.
    #[cfg(feature = "tokio-tls")]
    pub fn tls_insecure(self) -> DeviceBuilder<A, Tls> {
        let config = crate::tls::insecure_client_config();
        // Use a dummy server name — MikroTik routers don't validate SNI
        let server_name = rustls::pki_types::ServerName::try_from("mikrotik")
            .expect("\"mikrotik\" is a valid DNS name");

        DeviceBuilder {
            addr: self.addr,
            username: self.username,
            password: self.password,
            transport: Tls { config, server_name },
        }
    }

    /// Enable TLS with a custom [`rustls::ClientConfig`] and server name.
    ///
    /// Use this for custom certificate verification, certificate pinning,
    /// or other advanced TLS configurations.
    #[cfg(feature = "tokio-tls")]
    pub fn tls_config(
        self,
        config: alloc::sync::Arc<rustls::ClientConfig>,
        server_name: rustls::pki_types::ServerName<'static>,
    ) -> DeviceBuilder<A, Tls> {
        DeviceBuilder {
            addr: self.addr,
            username: self.username,
            password: self.password,
            transport: Tls { config, server_name },
        }
    }
}

#[cfg(feature = "tokio-tls")]
impl<A: ToSocketAddrs> DeviceBuilder<A, Tls> {
    /// Connects over TLS, logs in and spawns the actor.
    ///
    /// # Errors
    ///
    /// [`DeviceError`](crate::error::DeviceError) if any of those fail.
    pub async fn connect(self) -> DeviceResult<MikrotikDevice> {
        let tcp_stream = tokio::net::TcpStream::connect(self.addr).await?;
        tcp_stream.set_nodelay(true)?;

        let connector = tokio_rustls::TlsConnector::from(self.transport.config);
        let tls_stream = connector.connect(self.transport.server_name, tcp_stream).await?;

        crate::device::handshake_and_spawn(tls_stream, &self.username, self.password.as_deref())
            .await
    }
}
