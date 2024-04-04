# mikrotik-rs 📟

![Crates.io](https://img.shields.io/crates/v/mikrotik-rs)
![Crates.io License](https://img.shields.io/crates/l/mikrotik-rs)
![docs.rs](https://img.shields.io/docsrs/mikrotik-rs)
![Libraries.io dependency status for latest release](https://img.shields.io/librariesio/release/cargo/mikrotik-rs)
![Crates.io Total Downloads](https://img.shields.io/crates/d/mikrotik-rs)


This Rust library provides an asynchronous interface to interact with the [Mikrotik API](https://wiki.mikrotik.com/wiki/Manual:API).

## Features 🌟

- **Asynchronous** 🕒: Built on top of the Tokio runtime, this library offers non-blocking I/O operations.
- **Actor Pattern** 🎭: Implements an actor pattern for robust and organized handling of command execution and response retrieval.
- **Concurrent Commands** 🚦: Supports running multiple Mikrotik commands concurrently, with each command and its response efficiently managed via dedicated channels.
- **Error Handling** ⚠️: Designed with error handling in mind, ensuring that network or parsing errors are gracefully handled and reported back to the caller.

## Getting Started 🚀

To use this library in your project, first, add it to your `Cargo.toml`:

```toml
[dependencies]
mikrotik-rust-async = "0.1.0"
tokio = { version = "1", features = ["full"] }
```

Ensure you have Tokio set up in your project as the library relies on the Tokio runtime.

### Basic Usage 📖

```rust
use mikrotik_rs::{command::CommandBuilder, device::MikrotikDevice};
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the MikrotikClient 🤖 with the router's address and access credentials
    let device = MikrotikDevice::connect("192.168.122.144:8728", "admin", Some("admin")).await?;

    // Execute a command 📝
    let get_system_res = CommandBuilder::new()
        .command("/system/resource/print")
        // Send the update response every 1 second
        .attribute("interval", Some("1"))
        .build();
    let response_channel = device.send_command(get_system_res).await;

    // Listen for the command's response 🔊
    while let Some(res) = response_channel.recv().await {
        println!(">> Get System Res Response {:?}", res);
    }

    Ok(())
}
```

### Documentation 📚

For more detailed information on the library's API, please refer to the [documentation](https://docs.rs/mikrotik-rs).

## Contributing 🤝

Contributions are welcome! Whether it's submitting a bug report 🐛, a feature request 💡, or a pull request 🔄, all contributions help improve this library. Before contributing, please read through the [CONTRIBUTING.md](CONTRIBUTING.md) file (if available) for guidelines.

## License 📝

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Disclaimer 🚫

This library is not officially associated with Mikrotik. It is developed as an open-source project to facilitate Rust-based applications interacting with Mikrotik devices.
