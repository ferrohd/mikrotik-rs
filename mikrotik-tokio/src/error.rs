//! Tokio-specific error types for MikroTik device operations.

use std::io;

use mikrotik_proto::error::{ConnectionError, LoginError, ProtocolError};
use mikrotik_proto::response::TrapResponse;
use thiserror::Error;

/// Result type alias for `MikroTik` device operations.
pub type DeviceResult<T> = Result<T, DeviceError>;

/// Errors related to the device connection actor being unavailable.
#[derive(Error, Debug, Clone, Copy)]
pub enum ActorError {
    /// Failed to send command because the actor's channel is closed.
    #[error("Failed to send command: actor is unavailable (channel closed)")]
    CommandSendFailed,

    /// Login response was not received because the actor shut down during login.
    #[error("Login response not received: actor shut down during login")]
    LoginResponseLost,
}

/// Custom error type for `MikroTik` device operations.
#[derive(Error, Debug)]
pub enum DeviceError {
    /// Connection related errors (TCP, network issues).
    #[error("Connection error: {0}")]
    Connection(#[from] io::Error),

    /// Authentication failure.
    #[error("Authentication failed: {response}")]
    Authentication {
        /// The trap response received from the device.
        response: TrapResponse,
    },

    /// Actor unavailability errors.
    #[error("Actor error: {0}")]
    Actor(#[from] ActorError),

    /// Protocol-level errors from the sans-IO layer.
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    /// Connection state machine errors.
    #[error("Connection state error: {0}")]
    ConnectionState(#[from] ConnectionError),

    /// Login errors.
    #[error("Login error: {0}")]
    Login(#[from] LoginError),

    /// The connection was closed by the remote device.
    #[error("Connection closed by remote device")]
    ConnectionClosed,
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for DeviceError {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        DeviceError::Actor(ActorError::CommandSendFailed)
    }
}
