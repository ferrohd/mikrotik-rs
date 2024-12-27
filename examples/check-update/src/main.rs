use mikrotik_rs::{protocol::command::CommandBuilder, MikrotikDevice};

#[tokio::main]
async fn main() {
    let device = MikrotikDevice::connect("192.168.122.144:8728", b"admin", Some(b"admin"))
        .await
        .unwrap();

    let check_updates = CommandBuilder::new()
        .command(b"/system/package/update/install")
        .build();

    let mut update_responses = device.send_command(check_updates).await;

    while let Some(res) = update_responses.recv().await {
        println!(">> Get System Res Response {:?}", res);
    }
}
