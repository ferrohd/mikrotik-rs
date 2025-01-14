use mikrotik_rs::{
    command::CommandBuilder, 
    command::response::CommandResponse,
    MikrotikDevice
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Simplified connection with destructuring
    let device = MikrotikDevice::connect("192.168.88.1:8728", "admin", Some("admin")).await?;

    // More concise command builder with clear intent
    let ethernet_cmd = CommandBuilder::new()
        .command("/interface/ethernet/print")
        .attribute("interval", Some("10"))
        .build();

    // Use pattern matching and early return for error handling
    let mut response_channel = device.send_command(ethernet_cmd).await;

    while let Some(result) = response_channel.recv().await {
        match result {
            Ok(CommandResponse::Reply(reply)) 
                if reply.attributes.get("name").map_or(false, |name| name.as_deref() == Some("ether1")) => {
                    let running = reply.attributes.get("running")
                        .and_then(|v| v.as_ref())
                        .map_or("Unknown", |r| r);
                    
                    println!("Interface 'ether1' is running: {}", running);
            },
            Err(e) => eprintln!("Error: {:?}", e),
            _ => {}
        }
    }

    Ok(())
}
