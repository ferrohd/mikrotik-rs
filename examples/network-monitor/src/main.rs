//! Stream live interface counters with `/interface/monitor-traffic`.
//!
//! The command never completes on its own: `RouterOS` emits a new `!re` sentence per sampling
//! interval until the tag is cancelled, which makes it the simplest demonstration of a
//! subscription-style response stream.

use mikrotik_rs::{CommandBuilder, MikrotikDevice};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = MikrotikDevice::connect("192.168.122.144:8728", "admin", Some("admin")).await?;

    let monitor_cmd = CommandBuilder::new()
        .command("/interface/monitor-traffic")
        .attribute("interface", Some("ether1"))
        .build();

    let mut monitor_responses = device.send_command(monitor_cmd).await?;

    while let Some(event) = monitor_responses.recv().await {
        println!(">> Monitor Traffic Response {event:?}");
    }

    Ok(())
}
