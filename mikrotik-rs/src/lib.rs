#![warn(missing_docs)]
//! # MikroTik-rs
//!
//! `mikrotik-rs` is an asynchronous Rust library for interfacing with MikroTik routers.
//! It allows for sending commands and receiving responses in parallel through channels.
//!
//! ## Features
//! - Asynchronous command execution
//! - Parallel command handling with responses through channels
//! - Non-blocking communication with the router
//!
//! ## Examples
//!
//! Basic usage:
//!
//! ```rust,no_run
//! use mikrotik_rs::{protocol::command::CommandBuilder, MikrotikDevice};
//! use tokio;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Router's address with port
//!     let addr = "192.168.88.1:8728";
//!
//!     // Router's username and password
//!     let username = "admin";
//!     let password = "password";
//!
//!     let mut client = MikrotikDevice::connect(addr, username, Some(password)).await?;
//!
//!     let command = CommandBuilder::new().command("/interface/print")?.build(); // Example command
//!     let mut response_channel = client.send_command(command).await;
//!     while let Some(response) = response_channel.recv().await {
//!        println!("{:?}", response);
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ## Usage
//!
//! Add this to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! mikrotik-rs = "0.1"
//! tokio = { version = "1", features = ["full"] }
//! ```
//!
//! ## Note
//!
//! This library requires the `tokio` runtime.

#[cfg(target_pointer_width = "16")]
compiler_error!("This library supports 32-bit architectures or higher.");

mod actor;
/// Device module for connecting to MikroTik routers and sending commands.
mod device;
/// Error module for handling errors during device operations.
pub mod error;
/// Macros module to make your life easier.
pub mod macros;
/// Protocol module for handling MikroTik API communication.
pub mod protocol;

pub use device::MikrotikDevice;
