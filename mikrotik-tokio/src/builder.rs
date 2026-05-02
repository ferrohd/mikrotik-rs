//! Typestate builder for constructing [`MikrotikDevice`] connections.
//!
//! The builder enforces at compile time that TLS configuration is only
//! possible when the `tls` feature is enabled. The transport state type
//! carries its own configuration data — no runtime enums or phantom markers.
//!
//! # Examples
//!
//! **Plaintext TCP:**
//! ```rust,ignore
//! let device = MikrotikDevice::builder("192.168.88.1:8728")
//!     .credentials("admin", Some("password"))
//!     .connect()
//!     .await?;
//! ```
//!
//! **TLS (accept any certificate):**
//! ```rust,ignore
//! let device = MikrotikDevice::builder("192.168.88.1:8729")
//!     .credentials("admin", Some("password"))
//!     .tls_insecure()
//!     .connect()
//!     .await?;
//! ```
//!
//! **TLS (custom `ClientConfig`):**
//! ```rust,ignore
//! let device = MikrotikDevice::builder("192.168.88.1:8729")
//!     .credentials("admin", Some("password"))
//!     .tls_config(my_config, server_name)
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
/// Only available when the `tls` feature is enabled.
#[cfg(feature = "tls")]
pub struct Tls {
    config: alloc::sync::Arc<rustls::ClientConfig>,
    server_name: rustls::pki_types::ServerName<'static>,
}

/// A typestate builder for establishing connections to `MikroTik` devices.
///
/// The `Transport` type parameter carries transport-specific configuration:
/// - [`Plaintext`] — no additional config needed
/// - [`Tls`] — carries the `ClientConfig` and server name (requires `tls` feature)
///
/// Use [`MikrotikDevice::builder()`] to create an instance.
#[must_use]
pub struct DeviceBuilder<A, Transport> {
    addr: A,
    username: String,
    password: Option<String>,
    /// Transport-specific state. [`Plaintext`] carries nothing;
    /// [`Tls`] carries the `ClientConfig` and server name.
    /// Read in the `Tls::connect()` path (behind `#[cfg(feature = "tls")]`).
    #[allow(dead_code)] // consumed by Tls::connect() when tls feature is enabled
    transport: Transport,
}

impl<A: ToSocketAddrs> DeviceBuilder<A, Plaintext> {
    /// Create a new builder targeting the given address.
    ///
    /// The connection starts as plaintext TCP. Use [`.tls_insecure()`](Self::tls_insecure)
    /// or [`.tls_config()`](Self::tls_config) to enable TLS.
    pub(crate) fn new(addr: A) -> Self {
        Self {
            addr,
            username: String::new(),
            password: None,
            transport: Plaintext,
        }
    }

    /// Set the login credentials.
    pub fn credentials(mut self, username: &str, password: Option<&str>) -> Self {
        self.username = String::from(username);
        self.password = password.map(String::from);
        self
    }

    /// Connect over plaintext TCP.
    ///
    /// Establishes a TCP connection, performs the `RouterOS` login handshake,
    /// and spawns a background actor task.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError`](crate::error::DeviceError) if the TCP connection,
    /// login handshake, or actor spawn fails.
    pub async fn connect(self) -> DeviceResult<MikrotikDevice> {
        let stream = tokio::net::TcpStream::connect(self.addr).await?;
        stream.set_nodelay(true)?;
        crate::device::handshake_and_spawn(stream, &self.username, self.password.as_deref()).await
    }

    /// Enable TLS with **no certificate verification**.
    ///
    /// Suitable for `MikroTik` routers which use self-signed certificates.
    /// TLS handshake signatures are still verified to prevent downgrade attacks.
    #[cfg(feature = "tls")]
    pub fn tls_insecure(self) -> DeviceBuilder<A, Tls> {
        let config = crate::tls::insecure_client_config();
        // Use a dummy server name — MikroTik routers don't validate SNI
        let server_name = rustls::pki_types::ServerName::try_from("mikrotik")
            .expect("\"mikrotik\" is a valid DNS name");

        DeviceBuilder {
            addr: self.addr,
            username: self.username,
            password: self.password,
            transport: Tls {
                config,
                server_name,
            },
        }
    }

    /// Enable TLS with a custom [`rustls::ClientConfig`] and server name.
    ///
    /// Use this for custom certificate verification, certificate pinning,
    /// or other advanced TLS configurations.
    #[cfg(feature = "tls")]
    pub fn tls_config(
        self,
        config: alloc::sync::Arc<rustls::ClientConfig>,
        server_name: rustls::pki_types::ServerName<'static>,
    ) -> DeviceBuilder<A, Tls> {
        DeviceBuilder {
            addr: self.addr,
            username: self.username,
            password: self.password,
            transport: Tls {
                config,
                server_name,
            },
        }
    }
}

#[cfg(feature = "tls")]
impl<A: ToSocketAddrs> DeviceBuilder<A, Tls> {
    /// Connect over TLS.
    ///
    /// Establishes a TCP connection, performs the TLS handshake, then the
    /// `RouterOS` login handshake, and spawns a background actor task.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError`](crate::error::DeviceError) if the TCP connection,
    /// TLS handshake, login, or actor spawn fails.
    pub async fn connect(self) -> DeviceResult<MikrotikDevice> {
        let tcp_stream = tokio::net::TcpStream::connect(self.addr).await?;
        tcp_stream.set_nodelay(true)?;

        let connector = tokio_rustls::TlsConnector::from(self.transport.config);
        let tls_stream = connector
            .connect(self.transport.server_name, tcp_stream)
            .await?;

        crate::device::handshake_and_spawn(tls_stream, &self.username, self.password.as_deref())
            .await
    }
}
