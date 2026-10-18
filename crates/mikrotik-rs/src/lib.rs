//! # mikrotik-rs
//!
//! A client for the MikroTik RouterOS API: send commands, stream responses,
//! run many at once over one connection.
//!
//! This crate is re-exports. The work is split across `mikrotik-proto` (the
//! sans-IO core, always present), `mikrotik-tokio` and `mikrotik-embassy`.
//!
//! ## Feature flags
//!
//! | Feature     | Default | Description |
//! |-------------|---------|-------------|
//! | `std`       | **yes** | `std::collections::HashMap` in response types; disable for `no_std` |
//! | `tokio`     | **yes** | The Tokio adapter and [`MikrotikDevice`] |
//! | `embassy`   | no      | The Embassy adapter |
//! | `tokio-tls` | no      | TLS via `tokio-rustls` (bring your own crypto provider) |
//!
//! TLS needs a crypto provider alongside it:
//!
//! ```toml
//! mikrotik-rs = { version = "0.10", features = ["tokio-tls"] }
//! rustls = { version = "0.23", features = ["ring"] }  # or "aws-lc-rs"
//! ```

// Gated because `MikrotikDevice` is. `[lib] doctest = false` does not save it:
// cargo honours that only until something passes `--doc`, as `test-minimal` does.
#![cfg_attr(
    feature = "tokio",
    doc = r#"
## Examples

```rust,no_run
use mikrotik_rs::{MikrotikDevice, CommandBuilder};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let device = MikrotikDevice::connect("192.168.88.1:8728", "admin", Some("password")).await?;

let cmd = CommandBuilder::new().command("/interface/print").build();
let mut rx = device.send_command(cmd).await?;

while let Some(event) = rx.recv().await {
    println!("{event:?}");
}
# Ok(())
# }
```
"#
)]

#[cfg(target_pointer_width = "16")]
compile_error!("This library supports 32-bit architectures or higher.");

/// The sans-IO protocol crate.
pub use mikrotik_proto as proto;

pub use mikrotik_proto::HashMap;
pub use mikrotik_proto::command::{Command, CommandBuilder, QueryOperator};
pub use mikrotik_proto::connection::{Connection, Event, State, Transmit};
pub use mikrotik_proto::error::{ConnectionError, LoginError, ProtocolError};
pub use mikrotik_proto::handshake::{Authenticated, Handshaking, LoginProgress};
pub use mikrotik_proto::response::{
    CommandResponse, DoneResponse, EmptyResponse, FatalResponse, ReplyResponse, TrapCategory,
    TrapResponse,
};
pub use mikrotik_proto::tag::Tag;

pub use mikrotik_proto::command;

// ── Tokio adapter (behind "tokio" feature) ──

/// Re-export of the Tokio async adapter crate.
///
/// Only available when the `tokio` feature is enabled (default).
#[cfg(feature = "tokio")]
pub use mikrotik_tokio as tokio_client;

#[cfg(feature = "tokio")]
pub use mikrotik_tokio::MikrotikDevice;
#[cfg(feature = "tokio")]
pub use mikrotik_tokio::builder::{DeviceBuilder, Plaintext};
#[cfg(feature = "tokio")]
pub use mikrotik_tokio::error::{ActorError, DeviceError, DeviceResult};

// ── Embassy adapter (behind "embassy" feature) ──

/// The Embassy adapter.
#[cfg(feature = "embassy")]
pub use mikrotik_embassy as embassy_client;
