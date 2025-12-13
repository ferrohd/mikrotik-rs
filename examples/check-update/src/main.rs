use mikrotik_rs::{protocol::command::CommandBuilder, MikrotikDevice};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = MikrotikDevice::connect("192.168.122.144:8728", "admin", Some("admin")).await?;

    let check_updates = CommandBuilder::new()
        .command("/system/package/update/install")
        .build();

    let mut update_responses = device.send_command(check_updates).await?;

    while let Some(res) = update_responses.recv().await {
        println!(">> Get System Res Response {:?}", res);
    }

    Ok(())
}
