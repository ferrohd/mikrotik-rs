//! Error types for the Embassy `MikroTik` adapter.

use core::fmt;

use mikrotik_proto::error::{ConnectionError, LoginError};

/// Error type for the Embassy `MikroTik` adapter.
///
/// This error is transport-agnostic: I/O errors from any transport
/// (`TcpSocket`, `TlsConnection`, UART, etc.) are mapped to
/// [`embedded_io::ErrorKind`].
#[derive(Debug)]
pub enum DeviceError {
    /// Transport I/O error (read/write failure).
    ///
    /// The original transport error is mapped to an [`embedded_io::ErrorKind`]
    /// to keep this type transport-agnostic.
    Io(embedded_io::ErrorKind),
    /// Protocol-level connection state machine error.
    Connection(ConnectionError),
    /// Login authentication or protocol error.
    Login(LoginError),
    /// The remote device closed the connection (read returned 0 bytes).
    ConnectionClosed,
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(kind) => write!(f, "transport I/O error: {kind:?}"),
            Self::Connection(e) => write!(f, "connection state error: {e}"),
            Self::Login(e) => write!(f, "login error: {e}"),
            Self::ConnectionClosed => write!(f, "connection closed by remote device"),
        }
    }
}

impl From<ConnectionError> for DeviceError {
    fn from(e: ConnectionError) -> Self {
        Self::Connection(e)
    }
}

impl From<LoginError> for DeviceError {
    fn from(e: LoginError) -> Self {
        Self::Login(e)
    }
}
