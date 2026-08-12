# oxidns-mikrotik-tokio

OxiDNS-maintained fork of the Tokio-based async client for the [MikroTik RouterOS API](https://help.mikrotik.com/docs/spaces/ROS/pages/47579160/API).

This fork replaces bounded per-command response channels with unbounded channels,
preventing reply and terminal events from being silently lost during burst traffic.

This crate provides a high-level async interface built on top of the sans-IO [`mikrotik-proto`](https://crates.io/crates/mikrotik-proto) crate. It drives the protocol state machine using Tokio's async runtime.

**If you just want to talk to a router**, use [`oxidns-mikrotik-rs`](https://crates.io/crates/oxidns-mikrotik-rs) instead.

```toml
mikrotik-tokio = { package = "oxidns-mikrotik-tokio", version = "0.2.1" }
```

## Quick start

```rust,no_run
use mikrotik_tokio::MikrotikDevice;
use mikrotik_tokio::proto::command::CommandBuilder;
use mikrotik_tokio::proto::connection::Event;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = MikrotikDevice::connect(
        "192.168.88.1:8728",
        "admin",
        Some("password"),
    ).await?;

    let cmd = CommandBuilder::new()
        .command("/system/resource/print")
        .build();

    let mut rx = device.send_command(cmd).await?;

    while let Some(event) = rx.recv().await {
        match event {
            Event::Reply { response, .. } => {
                println!("{:?}", response.attributes);
            }
            Event::Done { .. } => break,
            other => println!("{other:?}"),
        }
    }

    Ok(())
}
```

## How it works

`MikrotikDevice` is a thin async wrapper around `mikrotik_proto::Connection`:

- **`connect()`** opens a TCP connection, performs the login handshake, and spawns a background actor task.
- **`send_command()`** sends a command and returns an `mpsc::UnboundedReceiver<Event>` scoped to that command.
- **Drop-based cancellation** — dropping a receiver sends `/cancel` to the router. Dropping all `MikrotikDevice` handles shuts down the connection gracefully.

Response channels are unbounded so queue capacity never drops protocol events
or blocks unrelated multiplexed commands behind a slow consumer. Long-running
streaming commands must continuously drain or drop their receiver to avoid
unbounded memory growth.

```text
  ┌──────────────────┐   ┌──────────────────┐
  │ MikrotikDevice   │   │ MikrotikDevice   │  (Clone-able handles)
  │    (clone 1)     │   │    (clone 2)     │
  └────────┬─────────┘   └────────┬─────────┘
           │  send_command()      │
           └──────────┬───────────┘
                      │ mpsc::Sender<DeviceCommand>
                      ▼
  ┌────────────────────────────────────────────────────────┐
  │                Background Actor Task                   │
  │                                                        │
  │  ┌──────────────────────────────────────────────────┐  │
  │  │           mikrotik_proto::Connection             │  │
  │  │          (sans-IO state machine)                 │  │
  │  │                                                  │  │
  │  │  send_command() ──▶ poll_transmit() ──▶ Writer ──┼──┼──▶ TCP
  │  │                                                  │  │
  │  │  receive() ◀── Reader ◀──────────────────────────┼──┼─── TCP
  │  │     │                                            │  │
  │  │     └──▶ poll_event()                            │  │
  │  └─────────────┼────────────────────────────────────┘  │
  │                │                                       │
  │          route by tag                                  │
  │       ┌────────┼────────┐                              │
  │       ▼        ▼        ▼                              │
  │   Sender₁  Sender₂  Sender₃   response_map             │
  └───────┼────────┼────────┼──────────────────────────────┘
          ▼        ▼        ▼
    Receiver₁ Receiver₂ Receiver₃  (one per command)
```

The actor owns the TCP connection and all protocol logic. Each command gets its own response channel, enabling safe concurrent use from multiple tasks.

## When to use this crate directly

Probably never. `oxidns-mikrotik-rs` exposes features via feature flags and defaults to `tokio` anyway.

## License

Licensed under the GNU Affero General Public License v3.0 (`AGPL-3.0-only`).
