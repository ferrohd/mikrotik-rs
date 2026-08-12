# Changelog

## 0.8.1 / 0.2.1 - 2026-08-12

- Publish the OxiDNS-maintained facade as `oxidns-mikrotik-rs` 0.8.1.
- Publish the patched Tokio adapter as `oxidns-mikrotik-tokio` 0.2.1.
- Replace bounded per-command response channels with unbounded channels so
  burst replies and terminal events are delivered without loss.
- Preserve multiplexing responsiveness and receiver-drop cancellation.
- Document the memory-growth risk for consumers that do not drain long-running
  streaming responses.

### Compatibility

`MikrotikDevice::send_command` now returns
`tokio::sync::mpsc::UnboundedReceiver<Event>` instead of
`tokio::sync::mpsc::Receiver<Event>`.
