use std::fmt;
use std::io;

use crate::protocol::CommandResponse;
use crate::protocol::TrapResponse;
use crate::protocol::error::ProtocolError;
use crate::protocol::word::WordCategory;

/// Result type alias for MikroTik device operations
pub type DeviceResult<T> = Result<T, DeviceError>;

/// Custom error type for MikroTik device operations
#[derive(Debug, Clone)]
pub enum DeviceError {
    /// Connection related errors (TCP, network issues)
    Connection(io::ErrorKind),
    /// Authentication failure
    Authentication {
        /// The response received from the device
        response: TrapResponse,
    },
    /// Channel errors
    Channel {
        /// Error message
        message: String,
    },
    /// Unexpected sequence of responses received
    ResponseSequence {
        /// The response received from the device
        received: CommandResponse,
        /// The values accepted as valid responses
        expected: Vec<WordCategory>,
    },
    /// Protocol-level parsing errors
    Protocol(ProtocolError),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::Connection(err) => write!(f, "Connection error: {}", err),
            DeviceError::Authentication { response } => {
                write!(f, "Authentication failed: {}", response)
            }
            DeviceError::Channel { message } => write!(f, "Channel error: {}", message),
            DeviceError::ResponseSequence { received, expected } => write!(
                f,
                "Unexpected response sequence: received {:?}, expected {:?}",
                received, expected
            ),
            DeviceError::Protocol(err) => write!(f, "Protocol error: {}", err),
        }
    }
}

impl From<io::Error> for DeviceError {
    fn from(error: io::Error) -> Self {
        DeviceError::Connection(error.kind())
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for DeviceError {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        DeviceError::Channel {
            message: "Failed to send message through channel".to_string(),
        }
    }
}

impl From<ProtocolError> for DeviceError {
    fn from(error: ProtocolError) -> Self {
        DeviceError::Protocol(error)
    }
}

impl std::error::Error for DeviceError {}
