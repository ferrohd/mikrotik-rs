# mikrotik-tokio

Tokio-based async client for the [MikroTik RouterOS API](https://help.mikrotik.com/docs/spaces/ROS/pages/47579160/API).

This crate provides a high-level async interface built on top of the sans-IO [`mikrotik-proto`](https://crates.io/crates/mikrotik-proto) crate. It drives the protocol state machine using Tokio's async runtime.

**If you just want to talk to a router**, use [`mikrotik-rs`](https://crates.io/crates/mikrotik-rs) instead.

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
- **`send_command()`** sends a command and returns an `mpsc::Receiver<Event>` scoped to that command.
- **Drop-based cancellation** — dropping a receiver sends `/cancel` to the router. Dropping all `MikrotikDevice` handles shuts down the connection gracefully.

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

Probably never. `mikrotik-rs` exposes features via feature flags and defaults to `tokio` anyway.

## License

Licensed under either of [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE) at your option.
