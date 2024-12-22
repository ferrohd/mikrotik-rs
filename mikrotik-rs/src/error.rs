use std::fmt;
use std::io;

use crate::command::response::CommandResponse;
use crate::command::response::TrapResponse;

/// Result type alias for MikroTik device operations
pub type DeviceResult<T> = Result<T, DeviceError>;

/// Custom error type for MikroTik device operations
#[derive(Debug)]
pub enum DeviceError {
    /// Connection related errors (TCP, network issues)
    Connection(io::Error),
    /// Authentication failure
    Authentication { response: TrapResponse },
    /// Received unexpected response [`CommandResponse`]
    Protocol { response: CommandResponse },
    /// Command execution errors
    Command { tag: u16, message: String },
    /// Fatal device errors
    Fatal { reason: String },
    /// Channel communication errors
    Channel { message: String },
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::Connection(err) => write!(f, "Connection error: {}", err),
            DeviceError::Authentication { message } => {
                write!(f, "Authentication error: {}", message)
            }
            DeviceError::Protocol { message } => write!(f, "Protocol error: {}", message),
            DeviceError::Command { tag, message } => {
                write!(f, "Command error (tag {}): {}", tag, message)
            }
            DeviceError::Fatal { message } => write!(f, "Fatal device error: {}", message),
            DeviceError::Channel { message } => write!(f, "Channel error: {}", message),
        }
    }
}

impl std::error::Error for DeviceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DeviceError::Connection(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for DeviceError {
    fn from(error: io::Error) -> Self {
        DeviceError::Connection(error)
    }
}

/// Convert channel send/receive errors to DeviceError
impl<T> From<tokio::sync::mpsc::error::SendError<T>> for DeviceError {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        DeviceError::Channel {
            message: "Failed to send message through channel".to_string(),
        }
    }
}
