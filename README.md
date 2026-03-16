# mikrotik-rs

![docs.rs](https://img.shields.io/docsrs/mikrotik-rs)
![Crates.io](https://img.shields.io/crates/v/mikrotik-rs)
![Crates.io License](https://img.shields.io/crates/l/mikrotik-rs)
![Libraries.io dependency status for latest release](https://img.shields.io/librariesio/release/cargo/mikrotik-rs)
![Crates.io Total Downloads](https://img.shields.io/crates/d/mikrotik-rs)
![GitHub Repo stars](https://img.shields.io/github/stars/ferrohd/mikrotik-rs)

An asynchronous [MikroTik RouterOS API](https://help.mikrotik.com/docs/spaces/ROS/pages/47579160/API) client for Rust, built on [Tokio](https://tokio.rs/).

Send commands, stream responses, and manage multiple concurrent operations against MikroTik routers
with a type-safe, channel-based API.

## Highlights

- **Fully async** -- built on Tokio with non-blocking I/O throughout
- **Concurrent command execution** -- each command gets its own response channel; run as many in parallel as you need
- **Typestate command builder** -- the compiler enforces correct command construction order
- **Compile-time command validation** -- the `command!` macro validates RouterOS command paths at compile time
- **Zero-copy protocol parsing** -- sentences and words are parsed directly from the receive buffer
- **Complete query support** -- equality, comparison, presence, and boolean operators for RouterOS queries
- **Structured error handling** -- a typed error hierarchy with `thiserror`, fully `Clone`-able
- **No `unsafe` code** -- `unsafe_code = "forbid"` is enforced at the workspace level
- **Automatic lifecycle management** -- dropping a response receiver cancels the command on the router; dropping all device handles cancels everything

## Installation

```bash
cargo add mikrotik-rs
```

**Requirements:** Rust 2024 edition, Tokio runtime.

## Quick start

```rust
use mikrotik_rs::{protocol::command::CommandBuilder, MikrotikDevice};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to the router
    let device = MikrotikDevice::connect("192.168.88.1:8728", "admin", Some("password")).await?;

    // Build a command
    let cmd = CommandBuilder::new()
        .command("/system/resource/print")
        .build();

    // Send it -- returns a dedicated channel for this command's responses
    let mut responses = device.send_command(cmd).await?;

    while let Some(result) = responses.recv().await {
        println!("{:?}", result?);
    }

    Ok(())
}
```

`MikrotikDevice` is cheaply `Clone`-able -- share it across tasks, spawn concurrent commands, and let each one
stream responses independently.

## Usage

### Building commands

The `CommandBuilder` uses a typestate pattern: you **must** call `.command()` before `.attribute()` or `.build()`.
The compiler rejects incorrect ordering.

```rust
use mikrotik_rs::protocol::command::CommandBuilder;

let cmd = CommandBuilder::new()
    .command("/interface/ethernet/monitor")
    .attribute("numbers", Some("0,1"))
    .attribute("once", None)
    .build();
```

### The `command!` macro

For static command paths, the `command!` macro validates the path at compile time and provides a
more concise syntax:

```rust
use mikrotik_rs::command;

// Simple command
let cmd = command!("/interface/print");

// Command with attributes
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

Every call to `send_command` returns an `mpsc::Receiver` scoped to that command.
Responses arrive as `CommandResponse` variants:

```rust
use mikrotik_rs::protocol::CommandResponse;

let mut rx = device.send_command(cmd).await?;

while let Some(result) = rx.recv().await {
    match result? {
        CommandResponse::Reply(reply) => {
            // Key-value data from the router
            println!("attributes: {:?}", reply.attributes);
        }
        CommandResponse::Done(_) => {
            println!("command completed");
        }
        CommandResponse::Trap(trap) => {
            // Router-side error (e.g., invalid command, permission denied)
            eprintln!("trap: {} (category: {:?})", trap.message, trap.category);
        }
        CommandResponse::Fatal(reason) => {
            // Fatal protocol error -- connection is dead
            eprintln!("fatal: {reason}");
        }
        CommandResponse::Empty(_) => {
            // RouterOS 7.18+: command had no data to return
        }
    }
}
```

### Streaming responses

Commands that produce continuous output (e.g., traffic monitoring, resource polling) stream
results through the same channel:

```rust
let monitor = CommandBuilder::new()
    .command("/interface/monitor-traffic")
    .attribute("interface", Some("ether1"))
    .build();

let mut rx = device.send_command(monitor).await?;

// Receives updates continuously until the channel is dropped
while let Some(result) = rx.recv().await {
    let reply = result?;
    println!("{:?}", reply);
}
// Dropping `rx` automatically sends /cancel to the router
```

### Queries

Filter results using RouterOS query operations:

```rust
use mikrotik_rs::protocol::command::{CommandBuilder, QueryOperator};

let cmd = CommandBuilder::new()
    .command("/interface/print")
    .query_equal("type", "ether")
    .query_equal("running", "true")
    .query_operations([QueryOperator::And].into_iter())
    .build();
```

Available query methods:

| Method | Wire format | Description |
|---|---|---|
| `query_equal(k, v)` | `?k=v` | Property equals value |
| `query_gt(k, v)` | `?>k=v` | Property greater than value |
| `query_lt(k, v)` | `?<k=v` | Property less than value |
| `query_is_present(k)` | `?k` | Property exists |
| `query_not_present(k)` | `?-k` | Property does not exist |
| `query_operations(ops)` | `?#...` | Boolean stack operations (`And`, `Or`, `Not`, `Dot`) |

### Raw byte attributes

For non-UTF-8 or binary attribute values:

```rust
let cmd = CommandBuilder::new()
    .command("/file/print")
    .attribute_raw("contents", Some(&[0x00, 0xFF, 0xAB]))
    .build();
```

Responses expose both `attributes` (UTF-8 strings) and `attributes_raw` (byte vectors) on `ReplyResponse`.

## Architecture

```
                        ┌──────────────────────────┐
  MikrotikDevice ──────►│   Background Actor Task  │
    (Clone-able)        │                          │
                        │  ┌────────┐  ┌────────┐  │
  send_command() ──────►│  │ Writer │  │ Reader │  │◄──── TCP
                        │  └────────┘  └────────┘  │
                        │        ▲          │       │
                        │        │    route by UUID │
                        │   commands   responses    │
                        └──────────────────────────┘
                                            │
                          ┌─────────────────┼─────────────────┐
                          ▼                 ▼                  ▼
                    Receiver<Cmd1>    Receiver<Cmd2>    Receiver<CmdN>
```

- **Actor pattern** -- a single spawned Tokio task owns the TCP connection and multiplexes
  commands/responses over it.
- **UUID tagging** -- every command is tagged with a UUID v4. Responses from the router include
  this tag, enabling correct routing to per-command channels.
- **Graceful shutdown** -- dropping all `MikrotikDevice` handles cancels every active command
  on the router and tears down the connection cleanly.

## Examples

The repository includes runnable examples in [`examples/`](examples/)

## Minimum supported Rust version

This crate uses the **Rust 2024 edition** and requires a compatible toolchain.

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.

## License

Licensed under either of

- [MIT license](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

## Disclaimer

This project is not affiliated with [MikroTik](https://mikrotik.com). It is an independent,
community-developed library.
