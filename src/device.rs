use crate::{
    actor::{ActorResult, DeviceConnectionActor, ReadActorMessage},
    command::{response::CommandResponse, Command},
};
use tokio::{io, net::ToSocketAddrs, sync::mpsc};

/// A client for interacting with MikroTik devices.
///
/// The `MikrotikDevice` struct provides an asynchronous interface for connecting to a MikroTik device
/// and sending commands to it. It encapsulates the communication with the device through a
/// background actor that handles the connection and command execution. Can be cheaply cloned to share
/// the same connection across multiple threads.
#[derive(Clone)]
pub struct MikrotikDevice(mpsc::Sender<ReadActorMessage>);

impl MikrotikDevice {
    /// Asynchronously establishes a connection to a MikroTik device.
    ///
    /// This function initializes the connection to the MikroTik device by starting a `DeviceConnectionActor`
    /// and returns an instance of `MikrotikDevice` that can be used to send commands to the device.
    ///
    /// # Parameters
    /// - `addr`: The address of the MikroTik device. This can be an IP address or a hostname with an optional port number.
    /// - `username`: The username for authenticating with the device.
    /// - `password`: An optional password for authentication. If `None`, no password will be sent.
    ///
    /// # Returns
    /// - `Ok(Self)`: An instance of `MikrotikDevice` on successful connection.
    /// - `Err(io::Error)`: An error if the connection could not be established.
    ///
    /// # Examples
    /// ```no_run
    /// # async fn connect_device() -> io::Result<()> {
    /// let device = MikrotikDevice::connect("192.168.88.1:8728", "admin", Some("password")).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect<A: ToSocketAddrs>(
        addr: A,
        username: &str,
        password: Option<&str>,
    ) -> io::Result<Self> {
        let sender = DeviceConnectionActor::start(addr, username, password).await?;

        Ok(Self(sender))
    }

    /// Asynchronously sends a command to the connected MikroTik device and returns a receiver for the response.
    ///
    /// This method allows sending commands to the MikroTik device and provides an asynchronous channel (receiver)
    /// that will receive the command execution results.
    ///
    /// # Parameters
    /// - `command`: The `Command` to send to the device, consisting of a tag and data associated with the command.
    ///
    /// # Returns
    /// A `mpsc::Receiver<io::Result<CommandResponse>>` that can be awaited to receive the response to the command.
    /// Responses are wrapped in `io::Result` to handle any I/O related errors during command execution or response retrieval.
    ///
    /// # Panics
    /// This method panics if sending the command message to the `DeviceConnectionActor` fails,
    /// which could occur if the actor has been dropped or the channel is disconnected.
    ///
    /// # Examples
    /// ```no_run
    /// # use tokio::sync::mpsc;
    /// # async fn send_command(device: MikrotikDevice) -> io::Result<()> {
    /// let command = CommandBuilder::new().command("/interface/print").build();
    /// let mut response_rx = device.send_command(command).await;
    ///
    /// while let Some(response) = response_rx.recv().await {
    ///     println!("{:?}", response?);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_command(
        &self,
        command: Command,
    ) -> mpsc::Receiver<io::Result<CommandResponse>> {
        let (response_tx, response_rx) = mpsc::channel::<ActorResult>(16);

        let msg = ReadActorMessage {
            tag: command.tag,
            data: command.data,
            respond_to: response_tx,
        };

        self.0.send(msg).await.expect("msg send failed");

        response_rx
    }
}
