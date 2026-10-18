//! Report whether one named interface is running, from `/interface/ethernet/print`.
//!
//! The other examples print every event they receive. This one is the common shape instead:
//! the router replies once per interface, and the caller cares about exactly one of them, so
//! the loop filters on an attribute rather than acting on everything.
//!
//! Note that attribute values are `Option<String>` — the router can send a key with no value —
//! which is why reading one is a `get` followed by an `as_deref`.
//!
//! Adapted from a contribution by @jessy-lua (#16).

use mikrotik_rs::{CommandBuilder, Event, MikrotikDevice};

const INTERFACE: &str = "ether1";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = MikrotikDevice::connect("192.168.88.1:8728", "admin", Some("admin")).await?;

    let cmd = CommandBuilder::new().command("/interface/ethernet/print").build();
    let mut responses = device.send_command(cmd).await?;

    while let Some(event) = responses.recv().await {
        match event {
            Event::Reply { response, .. } => {
                let attr = |key: &str| response.attributes.get(key).and_then(Option::as_deref);

                if attr("name") == Some(INTERFACE) {
                    // RouterOS reports booleans as the strings "true" and "false".
                    let running = attr("running").unwrap_or("unknown");
                    println!("{INTERFACE} running: {running}");
                }
            }
            Event::Trap { response, .. } => {
                eprintln!("router refused the command: {}", response.message);
            }
            Event::Fatal { reason } => {
                eprintln!("connection lost: {reason}");
                break;
            }
            // The command finished, or had nothing to report.
            Event::Done { .. } | Event::Empty { .. } => {}
        }
    }

    Ok(())
}
