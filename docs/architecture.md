# mikrotik-rs: Architecture

## Principles

**The protocol core performs no I/O.** `mikrotik-proto` is a state machine: bytes in, bytes out,
events out. It never opens a socket, never spawns a task, and never blocks. Everything that does
is an adapter above it. This is what makes the same protocol implementation usable from a Tokio
server and from a microcontroller, and it is what makes the protocol testable without a router.

**One version for the whole workspace.** The four crates are released together from one release
PR and share `[workspace.package] version`. They are a facade and its adapters; a release that
moved some and not others could only ever produce combinations nobody tested.

## Crates

| Crate | Purpose |
|---|---|
| [`mikrotik-proto`](../crates/mikrotik-proto/) | Sans-IO protocol core (`#![no_std]`) — wire format, commands, responses, connection state machine |
| [`mikrotik-tokio`](../crates/mikrotik-tokio/) | Tokio async adapter — background actor, per-command channels, TLS |
| [`mikrotik-embassy`](../crates/mikrotik-embassy/) | Embassy embedded async adapter — transport-agnostic over `embedded-io-async` |
| [`mikrotik-rs`](../crates/mikrotik-rs/) | Convenience re-exports from all crates |

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

### Why the facade is its own crate

`mikrotik-rs` contains no logic. It exists so that the common case is one dependency and one
feature flag, rather than picking an adapter and matching its version against the core's. Its
features forward: `tokio` pulls in `mikrotik-tokio`, `embassy` pulls in `mikrotik-embassy`,
`tokio-tls` forwards to the adapter's own TLS feature with `?` so it does not silently enable
the adapter.

Dropping the default features leaves the protocol types alone, with no runtime at all.

## mikrotik-proto

- `word` — a single `.tag=`, `=key=value`, `!re`-style word. Parsed lazily from the receive
  buffer with byte-level dispatch, so a sentence costs no allocation until something asks for an
  owned value.
- `codec` — RouterOS's variable-length word-length encoding, and sentence framing.
- `command` — the typestate `CommandBuilder`. `.command()` must precede `.attribute()` and
  `.build()`, and the compiler enforces it rather than a runtime check.
- `macros` — `command!`, which validates a static command path at compile time: no empty
  segments, must start with `/`.
- `tag` — the per-command correlation tag the router echoes back, which is how several commands
  share one connection.
- `response` — `!re` / `!done` / `!trap` / `!fatal` and the 7.18+ empty reply.
- `handshake` — the login exchange, as its own state machine.
- `connection` — the multiplexer: in-flight commands, demultiplexing replies by tag, and
  cancellation.

### The `std` feature

Off by default. It swaps `hashbrown::HashMap` for `std::collections::HashMap`, and it gates
`Tag::new` and `CommandBuilder::new`. The crate otherwise depends only on `alloc`; its whole
dependency list is `hashbrown` and `thiserror`.

`just targets` builds `mikrotik-proto` and `mikrotik-embassy` for `thumbv6m-none-eabi` and
`thumbv7em-none-eabihf` on every CI run. A host build with `--no-default-features` cannot
substitute: the host sysroot still has `std`, so it cannot fail.

## Tags

A tag correlates a command with its responses. The router treats it as an opaque string and
only ever echoes it back, so the single requirement is uniqueness among the commands in flight
on one connection.

It is a `u32`, written as fixed-width zero-padded decimal — `.tag=0000000042`. Fixed width is
not an aesthetic choice: `Tag::to_ascii` returns `[u8; Tag::ASCII_LEN]` in which every byte is
meaningful, so there is no length to carry beside the buffer and no way to transmit padding by
mistake. The width is derived from the representation rather than asserted against it:

```rust
pub const ASCII_LEN: usize = u32::MAX.ilog10() as usize + 1;
```

### Where the counter lives

`Tag::new` and `CommandBuilder::new` take tags from a process-wide `AtomicU32` and need the
`std` feature. Not because the counter needs an allocator, but because a shared counter must be
`Sync`, and `AtomicU32::fetch_add` does not exist on a target without compare-and-swap.
Cortex-M0+ is such a target, and that is the RP2040 — the most common board an Embassy user is
likely to have.

The alternatives were to depend on `critical-section` from a sans-IO crate, which is platform
coupling of exactly the kind this layer exists to avoid, or to drop those targets silently.
Instead, `no_std` callers own a `Counter` and pass tags to `CommandBuilder::with_tag`:

```rust
let tags = Counter::new();
let cmd = command!(tags.next_tag(), "/interface/print", detail);
```

`Counter` holds a `Cell`, not an atomic. Synchronisation belongs to whoever owns the counter,
which is where the concurrency model is actually known — a single Embassy task needs none.
`TagSource` names the contract so the adapters' handles and `Counter` read the same way.

`Tag::LOGIN` is 0 and counters start at 1. No reservation is strictly required, since login is
the only command in flight before authentication and is retired before the `Connection` is
handed over, but it keeps a packet capture unambiguous.

Reusing a tag that is still in flight is rejected with `ConnectionError::DuplicateTag` rather
than silently overwriting the in-flight entry. That is also what bounds the counter's
wraparound after 2^32 commands.

## Attributes and absence

`attribute` takes an `Option`, and the two cases are genuinely different on the wire:

| Call | Wire | Meaning |
|---|---|---|
| `.attribute("comment", None)` | *nothing* | not supplied |
| `.flag("detail")` | `=detail=` | valueless flag |
| `.attribute("comment", Some("hi"))` | `=comment=hi` | value supplied |

`=comment=` instructs RouterOS to set the comment to the empty string, which clears it — so
`None` has to mean "leave this alone" for `.attribute("comment", maybe_comment)` to be safe.

The decoder is deliberately **not** symmetric here: it folds `=key=` back into `None`, so a
flag written with `flag()` reads as `None` rather than `Some("")`. The `(value, value_raw)`
pair still separates an empty value from a non-UTF-8 one. Making the decoder symmetric would
change what every caller reads out of `ReplyResponse` for a distinction nothing needs.

Query words do not share this rule, which is why they have their own `OutWord` variants rather
than a shared `Option`: `?key` is a presence test and `?key=` asks for the empty string, and
both are meaningful.

## mikrotik-tokio

A background actor owns the socket and the `Connection`. `MikrotikDevice` is a cheap `Clone`
handle that talks to it over a channel, so it can be shared across tasks freely.

Each `send_command` returns an `mpsc::Receiver<Event>` scoped to that one command. Dropping the
receiver is what cancels the command on the router — there is no separate cancel call to forget.

TLS is behind `tokio-tls` and brings no crypto provider of its own: `rustls` is pulled in with
`default-features = false`, so the dependent chooses `ring` or `aws-lc-rs`.

## mikrotik-embassy

Unconditionally `#![no_std]`, and transport-agnostic: it is generic over `embedded-io-async`
rather than over a specific network stack, so the same code drives a TCP socket, a UART or a
test pipe. Synchronisation comes from `embassy-sync`.

## Testing strategy

- **Unit and integration tests** per crate, run under `cargo nextest` with a per-test timeout.
  Both adapter suites drive a real loopback socket or an in-memory pipe, which is precisely the
  shape that can block forever; plain `cargo test` has no way to notice.
- **Property tests** (`proptest`) over the codec and the parser, asserting round-trips and the
  absence of panics on arbitrary input. CI runs an extended pass at 8192 cases in its own job.
- **Feature sweeps** with `cargo hack`: every feature alone across the workspace, then the full
  powerset of the facade's five.
- **Cross-target checks** for the platforms built but never run.
- **`cargo-semver-checks`** against the published versions, which matters here because the
  facade re-exports the adapters wholesale — a change to a type in `mikrotik-proto` is a
  `mikrotik-rs` break without `mikrotik-rs` being touched.

Run the lot with `just all`. See [CONTRIBUTING.md](../CONTRIBUTING.md).
