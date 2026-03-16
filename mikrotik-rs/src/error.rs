use std::io;

use thiserror::Error;

use crate::protocol::CommandResponse;
use crate::protocol::TrapResponse;
use crate::protocol::error::ProtocolError;
use crate::protocol::word::WordCategory;

/// Result type alias for MikroTik device operations
pub type DeviceResult<T> = Result<T, DeviceError>;

/// Errors related to the device connection actor being unavailable.
///
/// These errors occur when the actor that manages the device connection has shut down
/// or is otherwise unavailable to process requests.
#[derive(Error, Debug, Clone, Copy)]
pub enum ActorError {
    /// Failed to send command because the actor's channel is closed.
    /// This occurs when the actor has shut down (e.g., connection lost,
    /// all device handles dropped, or fatal error occurred).
    #[error("Failed to send command: actor is unavailable (channel closed)")]
    CommandSendFailed,

    /// Login response was not received because the actor shut down during login.
    /// This occurs when the connection is lost or the actor encounters an error
    /// while waiting for the login response from the device.
    #[error("Login response not received: actor shut down during login")]
    LoginResponseLost,
}

/// Custom error type for MikroTik device operations
#[derive(Error, Debug, Clone)]
pub enum DeviceError {
    /// Connection related errors (TCP, network issues)
    #[error("Connection error: {0}")]
    Connection(io::ErrorKind),
    /// Authentication failure
    #[error("Authentication failed: {response}")]
    Authentication {
        /// The response received from the device
        response: TrapResponse,
    },
    /// Actor unavailability errors
    #[error("Actor error: {0}")]
    Actor(#[from] ActorError),
    /// Unexpected sequence of responses received
    #[error("Unexpected response sequence: received {received:?}, expected {expected:?}")]
    ResponseSequence {
        /// The response received from the device
        received: CommandResponse,
        /// The values accepted as valid responses
        expected: Vec<WordCategory>,
    },
    /// Protocol-level parsing errors
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),
}

impl From<io::Error> for DeviceError {
    fn from(error: io::Error) -> Self {
        DeviceError::Connection(error.kind())
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for DeviceError {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        DeviceError::Actor(ActorError::CommandSendFailed)
    }
}
