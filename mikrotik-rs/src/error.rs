use std::fmt;
use std::fmt::Formatter;
use std::io;

use crate::protocol::word::WordCategory;
use crate::protocol::CommandResponse;
use crate::protocol::TrapResponse;

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

impl std::error::Error for DeviceError {}

/// Result type alias command builder operations
pub type CommandResult<T> = Result<T, CommandError>;

/// Error building a command with given parameters
#[derive(Debug, Clone)]
pub enum CommandError {
    /// There is an invalid character in the input data
    HasInvalidCharacter(char),
}
impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::HasInvalidCharacter(ch) => {
                let codepoint = u32::from(*ch);
                write!(
                    f,
                    "The input contains an invalid character: 0x{codepoint:x} \"{ch}\""
                )
            }
        }
    }
}
impl std::error::Error for CommandError {}
