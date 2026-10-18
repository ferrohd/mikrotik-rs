//! Trigger a RouterOS package update and stream the installer's progress.
//!
//! `/system/package/update/install` is a long-running command: the router keeps the tag open
//! and reports each stage as a separate `!re` sentence, so the receiver is drained in a loop
//! rather than awaited once.

use mikrotik_rs::{CommandBuilder, MikrotikDevice};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = MikrotikDevice::connect("192.168.122.144:8728", "admin", Some("admin")).await?;

    let check_updates = CommandBuilder::new().command("/system/package/update/install").build();

    let mut update_responses = device.send_command(check_updates).await?;

    while let Some(event) = update_responses.recv().await {
        println!(">> Update Response {event:?}");
    }

    Ok(())
}
