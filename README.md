# mikrotik-rs 📟

![docs.rs](https://img.shields.io/docsrs/mikrotik-rs)
![Crates.io](https://img.shields.io/crates/v/mikrotik-rs)
![Crates.io License](https://img.shields.io/crates/l/mikrotik-rs)
![Libraries.io dependency status for latest release](https://img.shields.io/librariesio/release/cargo/mikrotik-rs)
![Crates.io Total Downloads](https://img.shields.io/crates/d/mikrotik-rs)
![GitHub Repo stars](https://img.shields.io/github/stars/ferrohd/mikrotik-rs)

This Rust library provides an asynchronous interface to interact with the [Mikrotik API](https://wiki.mikrotik.com/wiki/Manual:API).

## Features 🌟

- **No Unsafe Code** 💥: Built entirely in safe Rust 🦀
- **Zero-copy Parsing**: Avoid unnecessary memory allocations by parsing the API responses in-place.
- **Concurrent Commands** 🚦: Supports running multiple Mikrotik commands concurrently, with each command and its response managed via dedicated channels.
- **Query Support** 🔎: Full RouterOS query operations
- **Error Handling** ⚠️: Designed with error handling in mind, ensuring that network or parsing errors are gracefully handled and reported back to the caller.

## Getting Started 🚀

To use this library in your project, run the following command in your project's directory:

```bash
cargo add mikrotik-rs
```

Ensure you have Tokio set up in your project as the library relies on the Tokio runtime.

### Basic Usage 📖

```rust
use mikrotik_rs::{protocol::command::CommandBuilder, MikrotikDevice};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the MikrotikClient 🤖 with the router's address and access credentials
    let device = MikrotikDevice::connect("192.168.122.144:8728", "admin", Some("admin")).await?;

    // Buuild a command 📝
    let system_resource_cmd = CommandBuilder::new()
        .command("/system/resource/print")
        // Send the update response every 1 second
        .attribute("interval", Some("1"))
        .build();

    // Send the command to the device 📡
    // Returns a channel to listen for the command's response(s)
    let response_channel = device.send_command(system_resource_cmd).await;

    // Listen for the command's response 🔊
    while let Some(res) = response_channel.recv().await {
        println!(">> Get System Res Response {:?}", res);
    }

    Ok(())
}
```

Feeling lazy? This library provides a convenient `command!` macro for creating commands with compile-time validation:

```rust
// Simple command using macro
let cmd = command!("/interface/print");

// Command with attributes
let cmd = command!(
    "/interface/ethernet/monitor",
    numbers="0,1",
    once
);

// The macro validates commands at compile time:
let cmd = command!("invalid//command");  // Error: no empty segments allowed
let cmd = command!("no-leading-slash");  // Error: must start with '/'
```

#### Handling Responses

Responses are handled through dedicated channels:

```rust
let response_rx = device.send_command(command).await;

while let Some(response) = response_rx.recv().await {
    match response? {
        CommandResponse::Done(done) => {
            println!("Command completed: {:?}", done);
        }
        CommandResponse::Reply(reply) => {
            println!("Got data: {:?}", reply.attributes);
        }
        CommandResponse::Trap(trap) => {
            println!("Error occurred: {:?}", trap.message);
        }
        CommandResponse::Fatal(reason) => {
            println!("Fatal error: {}", reason);
        }
    }
}
```

### Documentation 📚

For more detailed information on the library's API, please refer to the [documentation](https://docs.rs/mikrotik-rs).

## Contributing 🤝

Contributions are welcome! Whether it's submitting a bug report 🐛, a feature request 💡, or a pull request 🔄, all contributions help improve this library. Before contributing, please read through the [CONTRIBUTING.md](CONTRIBUTING.md) file for guidelines.

## License 📝

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Disclaimer 🚫

This library is not officially associated with Mikrotik. It is developed as an open-source project to facilitate Rust-based applications interacting with Mikrotik devices.
