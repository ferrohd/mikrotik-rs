# mikrotik-embassy

Embassy async embedded client for the [MikroTik RouterOS API](https://help.mikrotik.com/docs/spaces/ROS/pages/47579160/API).

This crate provides an embedded-friendly async adapter built on top of the sans-IO [`mikrotik-proto`](https://crates.io/crates/mikrotik-proto) crate. It is **transport-agnostic**: works with any type implementing [`embedded_io_async::Read`](https://docs.rs/embedded-io-async) + [`Write`](https://docs.rs/embedded-io-async): plain TCP, TLS, UART, or anything else.

**If you just want to talk to a router from a standard OS**, use [`mikrotik-rs`](https://crates.io/crates/mikrotik-rs) with the `tokio` feature instead. This crate is for `#![no_std]` embedded targets running Embassy.

## Quick start

```rust,ignore
use embassy_net::tcp::TcpSocket;
use embassy_sync::channel::Channel;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use mikrotik_proto::command::{Command, CommandBuilder};
use mikrotik_proto::connection::Event;

static CMD: Channel<CriticalSectionRawMutex, Command, 4> = Channel::new();
static EVT: Channel<CriticalSectionRawMutex, Event, 8> = Channel::new();

#[embassy_executor::task]
async fn mikrotik_task(stack: embassy_net::Stack<'static>) {
    let mut rx_buf = [0; 4096];
    let mut tx_buf = [0; 4096];
    let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
    socket.connect(endpoint).await.unwrap();

    mikrotik_embassy::run(
        &mut socket, "admin", Some("password"),
        CMD.receiver(), EVT.sender(),
    ).await.unwrap();
}
```

## How it works

`mikrotik_embassy::run()` is a single async function that the caller spawns as an Embassy task. It performs login, then enters an event loop driven by `embassy_futures::select()` between the command channel and transport reads.

- **Transport-agnostic** — generic over `embedded_io_async::Read + Write`. Connect your socket/TLS/UART first, then hand it to `run()`.
- **Single event channel** — all response events go to one `Sender<Event>`. The consumer filters by `Tag` to correlate responses with commands.
- **No heap for the adapter** — the 2048-byte read buffer lives on the stack. (The underlying `mikrotik-proto` crate does use `alloc` for protocol processing.)
- **Backpressure via `try_send`** — if the event channel is full, events are dropped rather than blocking the network loop.

```text
  ┌───────────────────────────────────────────────────────────────┐
  │  User tasks                                                   │
  │                                                               │
  │  Task A ──► CMD_CHANNEL ──┐       ┌──► EVT_CHANNEL ──► Task B │
  │             (Sender)      │       │    (Receiver)             │
  └───────────────────────────┼───────┼───────────────────────────┘
                              │       │
  ┌───────────────────────────┼───────┼─────────────────────┐
  │  run() task               ▼       │                     │
  │                                                         │
  │  ┌───────────────────────────────────────────────────┐  │
  │  │           mikrotik_proto::Connection              │  │
  │  │          (sans-IO state machine)                  │  │
  │  │                                                   │  │
  │  │  send_command() ──▶ poll_transmit() ──▶ write ────┼──┼──▶
  │  │                                                   │  │  T: Read + Write
  │  │  receive() ◀── read ◀─────────────────────────────┼──┼──◀ (TcpSocket,
  │  │     │                                             │  │     TlsConnection,
  │  │     └──▶ poll_event() ──▶ evt_tx.try_send()       │  │     UART, ...)
  │  └───────────────────────────────────────────────────┘  │
  │                                                         │
  └─────────────────────────────────────────────────────────┘
```

## TLS support

Since `run()` is generic over `Read + Write`, TLS works by wrapping the socket before passing it in:

```rust,ignore
use embedded_tls::*;

// Connect TCP first
let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
socket.connect(endpoint).await.unwrap();

// Wrap in TLS
let mut tls = TlsConnection::new(socket, &mut tls_read_buf, &mut tls_write_buf);
tls.open(TlsContext::new(
    &TlsConfig::new(),
    UnsecureProvider::new::<Aes128GcmSha256>(rng),
)).await.unwrap();

// run() doesn't care — it's just Read + Write
mikrotik_embassy::run(
    &mut tls, "admin", Some("password"),
    CMD.receiver(), EVT.sender(),
).await.unwrap();
```

## License

Licensed under either of [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE) at your option.
