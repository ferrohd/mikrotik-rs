# mikrotik-rs: Architecture

## Crates

The protocol core performs no I/O: bytes in, bytes and events out. Everything
that touches a socket is an adapter above it. The four crates share one version
and are released together.

| Crate | Purpose |
|---|---|
| [`mikrotik-proto`](../crates/mikrotik-proto/) | Sans-IO protocol core (`#![no_std]`) — wire format, commands, responses, connection state machine |
| [`mikrotik-tokio`](../crates/mikrotik-tokio/) | Tokio adapter — background actor, per-command channels, TLS |
| [`mikrotik-embassy`](../crates/mikrotik-embassy/) | Embassy adapter — transport-agnostic over `embedded-io-async` |
| [`mikrotik-rs`](../crates/mikrotik-rs/) | Re-exports from all of the above |

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

`mikrotik-proto`'s only dependencies are `hashbrown` and `thiserror`. It is
built for `thumbv6m-none-eabi` and `thumbv7em-none-eabihf` on every CI run; a
host build with `--no-default-features` keeps `std` in the sysroot and so
cannot stand in for that.

## Testing strategy

- **Unit and integration tests** per crate under `cargo nextest`, for its
  per-test timeout: both adapter suites drive a real socket or pipe, which is
  the shape that can hang forever.
- **Property tests** (`proptest`) over the codec and parser — round-trips, and
  no panic on arbitrary input. CI runs an extended pass at 8192 cases.
- **Feature sweeps** with `cargo hack`: each feature alone across the
  workspace, then the facade's powerset.
- **Cross-target builds** for the platforms compiled but never run.
- **`cargo-semver-checks`** against the published versions. The facade
  re-exports the adapters wholesale, so a change to a type in `mikrotik-proto`
  breaks `mikrotik-rs` without `mikrotik-rs` being touched.

Run the lot with `just all`. See [CONTRIBUTING.md](../CONTRIBUTING.md).
