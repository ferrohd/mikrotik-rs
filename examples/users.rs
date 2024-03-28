use mikrotik_rs::{command::CommandBuilder, device::MikrotikDevice};

// Using the current_thread flavor because multiple threads are not needed for this example
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let device = MikrotikDevice::connect("192.168.100.149:8728", "admin", Some("admin"))
        .await
        .unwrap();

    let get_users_cmd = CommandBuilder::new().command("/user/active/print").build();

    let mut users_stream = device.send_command(get_users_cmd).await;

    while let Some(interface) = users_stream.recv().await {
        println!(">> Get Users Response {:?}", interface);
    }
}
