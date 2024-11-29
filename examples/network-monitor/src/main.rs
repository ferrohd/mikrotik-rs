use mikrotik_rs::{command::CommandBuilder, MikrotikDevice};

#[tokio::main]
async fn main() {
    let device = MikrotikDevice::connect("192.168.122.144:8728", "admin", Some("admin"))
        .await
        .unwrap();

    let monitor_cmd = CommandBuilder::new()
        .command("/interface/monitor-traffic")
        .attribute("interface", Some("ether1"))
        .build();

    let mut monitor_responses = device.send_command(monitor_cmd).await;

    while let Some(res) = monitor_responses.recv().await {
        println!(">> Get System Res Response {:?}", res);
    }
}
