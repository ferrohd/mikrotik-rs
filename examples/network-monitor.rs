use mikrotik_rs::{command::CommandBuilder, device::MikrotikDevice};

// Using the current_thread flavor because multiple threads are not needed for this example
#[tokio::main(flavor = "current_thread")]
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
