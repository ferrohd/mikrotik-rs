//! Embassy-specific error types for `MikroTik` device operations.

use core::fmt;

use embassy_net::tcp::{ConnectError, Error as TcpError};
use mikrotik_proto::error::{ConnectionError, LoginError};

/// Error type for the Embassy `MikroTik` adapter.
#[derive(Debug)]
#[non_exhaustive]
pub enum DeviceError {
    /// TCP connection failed.
    Connect(ConnectError),
    /// TCP read/write error.
    Tcp(TcpError),
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
            Self::Connect(e) => write!(f, "TCP connect error: {e:?}"),
            Self::Tcp(e) => write!(f, "TCP error: {e:?}"),
            Self::Connection(e) => write!(f, "connection state error: {e}"),
            Self::Login(e) => write!(f, "login error: {e}"),
            Self::ConnectionClosed => write!(f, "connection closed by remote device"),
        }
    }
}

impl From<ConnectError> for DeviceError {
    fn from(e: ConnectError) -> Self {
        Self::Connect(e)
    }
}

impl From<TcpError> for DeviceError {
    fn from(e: TcpError) -> Self {
        Self::Tcp(e)
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
