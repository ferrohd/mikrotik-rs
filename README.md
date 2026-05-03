# mikrotik-rs

![docs.rs](https://img.shields.io/docsrs/mikrotik-rs)
![Crates.io](https://img.shields.io/crates/v/mikrotik-rs)
![Crates.io License](https://img.shields.io/crates/l/mikrotik-rs)
![Crates.io Total Downloads](https://img.shields.io/crates/d/mikrotik-rs)
![GitHub Repo stars](https://img.shields.io/github/stars/ferrohd/mikrotik-rs)

A Rust client for the [MikroTik RouterOS API](https://help.mikrotik.com/docs/spaces/ROS/pages/47579160/API).

Send commands, stream responses, and manage multiple concurrent operations against MikroTik routers
with a type-safe, channel-based API.

## Highlights

- **Sans-IO protocol core** — `#![no_std]`-compatible, runtime-agnostic protocol implementation
- **Tokio adapter** — high-level async client with background actor, per-command channels, and TLS support
- **Embassy adapter** — embedded-friendly async client, transport-agnostic over `embedded-io-async`
- **Typestate command builder** — the compiler enforces correct command construction order
- **Compile-time command validation** — the `command!` macro validates RouterOS command paths at compile time
- **Zero-copy protocol parsing** — words are parsed lazily from the receive buffer with byte-level dispatch
- **Concurrent command execution** — each command is tagged with a UUID v4 for response demultiplexing
- **Automatic lifecycle management** — dropping a response receiver cancels the command on the router
- **No `unsafe` code** — `unsafe_code = "forbid"` is enforced at the workspace level

## Installation

```bash
cargo add mikrotik-rs
```

## Quick start

```rust
use mikrotik_rs::{MikrotikDevice, CommandBuilder, Event};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = MikrotikDevice::connect("192.168.88.1:8728", "admin", Some("password")).await?;

    let cmd = CommandBuilder::new()
        .command("/system/resource/print")
        .build();

    let mut rx = device.send_command(cmd).await?;

    while let Some(event) = rx.recv().await {
        match event {
            Event::Reply { response, .. } => println!("{:?}", response.attributes),
            Event::Done { .. } => break,
            Event::Trap { response, .. } => eprintln!("error: {}", response.message),
            other => println!("{other:?}"),
        }
    }

    Ok(())
}
```

`MikrotikDevice` is cheaply `Clone`-able — share it across tasks, spawn concurrent commands, and let
each one stream responses independently.

## Feature flags

| Feature     | Default | Description |
|-------------|---------|-------------|
| `tokio`     | **yes** | Enables the Tokio async adapter and `MikrotikDevice` client |
| `embassy`   | no      | Enables the Embassy embedded async adapter |
| `tokio-tls` | no      | Enables TLS support via `tokio-rustls` |

```toml
# Plaintext (default)
mikrotik-rs = "0.7"

# TLS for API-SSL (port 8729) — bring your own crypto provider
mikrotik-rs = { version = "0.7", features = ["tokio-tls"] }
rustls = { version = "0.23", features = ["ring"] }  # or "aws-lc-rs"

# Embassy embedded adapter (no_std)
mikrotik-rs = { version = "0.7", default-features = false, features = ["embassy"] }

# Protocol types only (no runtime)
mikrotik-rs = { version = "0.7", default-features = false }
```

## Usage

### Building commands

The `CommandBuilder` uses a typestate pattern: you **must** call `.command()` before `.attribute()` or `.build()`.
The compiler rejects incorrect ordering.

```rust
use mikrotik_rs::CommandBuilder;

let cmd = CommandBuilder::new()
    .command("/interface/ethernet/monitor")
    .attribute("numbers", Some("0,1"))
    .attribute("once", None)
    .build();
```

### The `command!` macro

For static command paths, the `command!` macro validates the path at compile time:

```rust
use mikrotik_rs::command;

let cmd = command!("/interface/print");

let cmd = command!(
    "/interface/ethernet/monitor",
    numbers = "0,1",
    once
);

// These fail at compile time:
// command!("invalid//command");   // no empty segments
// command!("no-leading-slash");   // must start with '/'
```

### Handling responses

Every call to `send_command` returns an `mpsc::Receiver<Event>` scoped to that command:

```rust
use mikrotik_rs::Event;

let mut rx = device.send_command(cmd).await?;

while let Some(event) = rx.recv().await {
    match event {
        Event::Reply { response, .. } => {
            println!("attributes: {:?}", response.attributes);
        }
        Event::Done { .. } => {
            println!("command completed");
        }
        Event::Trap { response, .. } => {
            eprintln!("trap: {} (category: {:?})", response.message, response.category);
        }
        Event::Fatal { reason } => {
            eprintln!("fatal: {reason}");
        }
        Event::Empty { .. } => {
            // RouterOS 7.18+: command had no data to return
        }
    }
}
```

### TLS connections

With the `tokio-tls` feature enabled, use the builder for TLS:

```rust
// Accept self-signed certs (typical for MikroTik routers)
let device = MikrotikDevice::builder("192.168.88.1:8729")
    .credentials("admin", Some("password"))
    .tls_insecure()
    .connect()
    .await?;

// Or with a custom rustls ClientConfig
let device = MikrotikDevice::builder("192.168.88.1:8729")
    .credentials("admin", Some("password"))
    .tls_config(my_config, server_name)
    .connect()
    .await?;
```

### Streaming responses

Commands that produce continuous output stream results through the same channel:

```rust
let monitor = CommandBuilder::new()
    .command("/interface/monitor-traffic")
    .attribute("interface", Some("ether1"))
    .build();

let mut rx = device.send_command(monitor).await?;

while let Some(event) = rx.recv().await {
    println!("{event:?}");
}
// Dropping `rx` automatically sends /cancel to the router
```

### Queries

Filter results using RouterOS query operations:

```rust
use mikrotik_rs::{CommandBuilder, QueryOperator};

let cmd = CommandBuilder::new()
    .command("/interface/print")
    .query_equal("type", "ether")
    .query_equal("running", "true")
    .query_operations([QueryOperator::And].into_iter())
    .build();
```

| Method | Wire format | Description |
|---|---|---|
| `query_equal(k, v)` | `?k=v` | Property equals value |
| `query_gt(k, v)` | `?>k=v` | Property greater than value |
| `query_lt(k, v)` | `?<k=v` | Property less than value |
| `query_is_present(k)` | `?k` | Property exists |
| `query_not_present(k)` | `?-k` | Property does not exist |
| `query_operations(ops)` | `?#...` | Boolean stack operations (`And`, `Or`, `Not`, `Dot`) |

## Workspace

The library is split into focused crates:

| Crate | Purpose |
|---|---|
| [`mikrotik-proto`](mikrotik-proto/) | Sans-IO protocol core (`#![no_std]`) — wire format, commands, responses, connection state machine |
| [`mikrotik-tokio`](mikrotik-tokio/) | Tokio async adapter — background actor, per-command channels, TLS |
| [`mikrotik-embassy`](mikrotik-embassy/) | Embassy embedded async adapter — transport-agnostic over `embedded-io-async` |
| [`mikrotik-rs`](mikrotik-rs/) | Convenience re-exports from all crates |

```text
┌─────────────────────────────────────────────────────────────────┐
│                        mikrotik-rs                              │
│                    (re-exports from all)                        │
├──────────────────────────┬──────────────────────────────────────┤
│      mikrotik-tokio      │         mikrotik-embassy             │
│   (Tokio async adapter)  │   (Embassy embedded adapter)         │
├──────────────────────────┴──────────────────────────────────────┤
│                       mikrotik-proto                            │
│           Sans-IO protocol core (#![no_std])                    │
└─────────────────────────────────────────────────────────────────┘
```

## Examples

The repository includes runnable examples in [`examples/`](examples/).

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.

## Disclaimer

This project is not affiliated with [MikroTik](https://mikrotik.com). It is an independent,
community-developed library.
