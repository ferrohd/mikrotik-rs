use mikrotik_rs::{CommandBuilder, MikrotikDevice};

// Using the current_thread flavor because multiple threads are not needed for this example
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device =
        MikrotikDevice::connect("192.168.122.144:8728", "admin", Some("admin")).await?;

    let get_system_res = CommandBuilder::new()
        .command("/system/resource/print")
        // Send the update response every 1 second
        .attribute("interval", Some("1"))
        .build();

    let mut res_stream = device.send_command(get_system_res).await?;

    while let Some(event) = res_stream.recv().await {
        println!(">> Get System Res Response {:?}", event);
    }

    Ok(())
}
