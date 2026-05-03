# mikrotik-proto

Sans-IO protocol implementation for the [MikroTik RouterOS API](https://help.mikrotik.com/docs/spaces/ROS/pages/47579160/API).

This crate provides a runtime-agnostic, `#![no_std]`-compatible implementation of the MikroTik wire protocol. It handles encoding, decoding, command building, response parsing, and connection state management.

**If you just want to talk to a router**, use [`mikrotik-rs`](https://crates.io/crates/mikrotik-rs) instead. This crate is for building your own runtime adapter or embedding the protocol in constrained environments.

## Highlights

- **`#![no_std]`** — only depends on `alloc`. No `std`, no OS, no runtime.
- **Zero-copy parsing** — words are parsed lazily from the receive buffer with byte-level dispatch (no redundant UTF-8 validation).
- **Typestate command builder** — the compiler enforces correct command construction order.
- **Compile-time validation** — the `command!` macro validates RouterOS command paths at compile time.
- **Connection state machine** — multiplexes concurrent commands over a single connection via tags.
- **Typestate login handshake** — the type system enforces that authentication completes before commands can be sent.
- **No `unsafe` code** — `unsafe_code = "forbid"` is enforced at the workspace level.

## Usage pattern

The `Connection` type mirrors the design of [`quinn-proto`](https://docs.rs/quinn-proto): you feed it bytes, poll for outbound data, and poll for application events.

```rust
use mikrotik_proto::connection::{Connection, Event};
use mikrotik_proto::command::CommandBuilder;

let mut conn = Connection::new();

// Build and send a command
let cmd = CommandBuilder::new()
    .command("/system/resource/print")
    .build();
let tag = conn.send_command(cmd).unwrap();

// In your event loop:
// 1. Drain outbound data and write it to your transport
while let Some(transmit) = conn.poll_transmit() {
    // transport.write_all(&transmit.data);
}

// 2. Feed incoming bytes from the transport
// conn.receive(&incoming_bytes).unwrap();

// 3. Process application events
while let Some(event) = conn.poll_event() {
    match event {
        Event::Reply { tag, response } => { /* streaming data row */ }
        Event::Done { tag } => { /* command completed */ }
        Event::Trap { tag, response } => { /* router-side error */ }
        Event::Fatal { reason } => { /* connection dead */ }
        Event::Empty { tag } => { /* no data (RouterOS 7.18+) */ }
    }
}
```

## Login handshake

The `Handshaking` / `Authenticated` typestate enforces that you cannot send commands before logging in:

```rust
use mikrotik_proto::handshake::{Handshaking, LoginProgress};

let mut hs = Handshaking::new("admin", Some("password")).unwrap();

// 1. Send login bytes
while let Some(transmit) = hs.poll_transmit() {
    // transport.write_all(&transmit.data);
}

// 2. Feed response bytes and advance
// hs.receive(&response_bytes).unwrap();
// match hs.advance().unwrap() {
//     LoginProgress::Pending(h) => hs = h,
//     LoginProgress::Complete(auth) => {
//         let conn = auth.into_connection();
//         // now you can send_command()
//     }
// }
```

## Architecture

```text
                        ┌─────────────────────────────────────────┐
                        │            mikrotik-proto                │
                        │                                         │
  &[u8] from ───────────┤  ┌───────────────────┐         ┌──────┐│
  transport             │  │ codec             │────────▶│ word ││
                        │  │ (RawSentence +    │         └──────┘│
                        │  │  typed_words())   │            │     │
                        │  └───────────────────┘            │     │
                        │      │                            │     │
                        │      │  decode    ┌──────────┐    │parse│
                        │      ├───────────▶│ response │◀───┘     │
                        │      │            └──────────┘          │
                        │      │                  │               │
                        │      │ encode     ┌─────▼──────┐        │
                        │      ◀────────────┤ connection │        │
                        │  ┌───────┐        │            │        │
                        │  │command│───────▶│ state mgmt │        │
                        │  └───────┘        │ mux/demux  │        │
                        │                   └─────┬──────┘        │
                        │                         │               │
                        │  ┌─────────┐    wraps   │               │
                        │  │handshake│◀───────────┘               │
                        │  └─────────┘                            │
                        │      │                                  │
                        ├──────┼──────────────────────────────────┤
                        │      ▼                                  │
                        │  poll_transmit() ──▶ Vec<u8> to transport
                        │  poll_event()   ──▶ Event to application
                        └─────────────────────────────────────────┘
```

## License

Licensed under either of [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE) at your option.
