use mikrotik_rs::{protocol::command::CommandBuilder, MikrotikDevice};

// Using the current_thread flavor because multiple threads are not needed for this example
#[tokio::main]
async fn main() {
    let device = MikrotikDevice::connect("192.168.122.144:8728", b"admin", Some(b"admin"))
        .await
        .unwrap();

    let get_system_res = CommandBuilder::new()
        .command(b"/system/resource/print")
        // Send the update response every 1 second
        .attribute(b"interval", Some(b"1"))
        .build();

    let mut users_stream = device.send_command(get_system_res).await;

    while let Some(res) = users_stream.recv().await {
        println!(">> Get System Res Response {:?}", res);
    }
}
