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
//! ```no_run
//! use mikrotik_rs::device::MikrotikDevice;
//! use tokio;
//!
//! #[tokio::main]
//! async fn main() -> io::Result<()> {
//!     // Router's address with port
//!     let addr = "192.168.88.1:8728";
//!
//!     // Router's username and password
//!     let username = "admin";
//!     let password = "password";
//!
//!     let mut client = MikrotikDevice::connect(addr, username, Some(password)).await?;
//!
//!     let command = CommandBuilder::new().command("/interface/print").build(); // Example command
//!     let response_channel = client.send_command(command).await?;
//!     while let Some(response) = response_channel.recv().await {
//!        println!("{:?}", response);
//!     }
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
mod actor;
/// Command module for building and sending commands to MikroTik routers.
pub mod command;
/// Device module for connecting to MikroTik routers and sending commands.
pub mod device;
