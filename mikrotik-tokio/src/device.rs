//! Async device client for connecting to `MikroTik` routers.
//!
//! The [`MikrotikDevice`] struct provides an asynchronous interface built on
//! top of the sans-IO [`mikrotik_proto`] crate. It drives the protocol state
//! machine using Tokio's async runtime.

use std::collections::HashMap;

use mikrotik_proto::command::Command;
use mikrotik_proto::connection::{Connection, Event};
use mikrotik_proto::handshake::{Handshaking, LoginProgress};
use mikrotik_proto::tag::Tag;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::mpsc;

use crate::error::{DeviceError, DeviceResult};

/// Internal command sent from the [`MikrotikDevice`] handle to the actor task.
struct DeviceCommand {
    command: Command,
    respond_to: mpsc::Sender<Event>,
}

/// A client for interacting with `MikroTik` devices.
///
/// The `MikrotikDevice` struct provides an asynchronous interface for connecting
/// to a `MikroTik` device and sending commands. It encapsulates the communication
/// through a background actor task that drives the sans-IO [`Connection`] state
/// machine.
///
/// Can be cheaply cloned to share the same connection across multiple tasks.
///
/// # Cancellation
///
/// In-flight commands are automatically cancelled on the router when the
/// response receiver returned by [`send_command`](MikrotikDevice::send_command)
/// is dropped. This follows Rust's RAII pattern — just drop the receiver to
/// stop a long-running command like `/tool/torch` or `/interface/monitor-traffic`.
///
/// # Examples
///
/// ```rust,no_run
/// use mikrotik_tokio::MikrotikDevice;
/// use mikrotik_proto::command::CommandBuilder;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let device = MikrotikDevice::connect("192.168.88.1:8728", "admin", Some("password")).await?;
///
/// let cmd = CommandBuilder::new().command("/interface/print").build();
/// let mut rx = device.send_command(cmd).await?;
///
/// while let Some(event) = rx.recv().await {
///     println!("{event:?}");
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct MikrotikDevice {
    cmd_tx: mpsc::Sender<DeviceCommand>,
}

// Static assertion: MikrotikDevice must be Send + Sync for multi-task use.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    #[allow(dead_code)]
    fn check() {
        assert_send_sync::<MikrotikDevice>();
    }
};

impl MikrotikDevice {
    /// Asynchronously establishes a connection to a `MikroTik` device.
    ///
    /// This connects via plaintext TCP (port 8728), performs the login handshake,
    /// and spawns a background actor task to drive the connection.
    ///
    /// # Parameters
    /// - `addr`: The address of the `MikroTik` device (e.g., `"192.168.88.1:8728"`).
    /// - `username`: The username for authenticating with the device.
    /// - `password`: An optional password for authentication.
    ///
    /// # Returns
    /// - `Ok(Self)`: An instance of [`MikrotikDevice`] on successful connection and login.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError`] if:
    /// - The TCP connection cannot be established
    /// - The login handshake fails (wrong credentials, fatal, or protocol error)
    /// - The remote device closes the connection during login
    pub async fn connect<A: ToSocketAddrs>(
        addr: A,
        username: &str,
        password: Option<&str>,
    ) -> DeviceResult<Self> {
        let mut stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?;

        // Perform the login handshake using the sans-IO handshake state machine
        let mut hs = Handshaking::new(username, password);

        // Flush the login command
        while let Some(transmit) = hs.poll_transmit() {
            stream.write_all(&transmit.data).await?;
        }

        // Read until login completes
        let mut buf = vec![0u8; 4096];
        let conn = loop {
            let n = stream.read(&mut buf).await?;
            if n == 0 {
                return Err(DeviceError::ConnectionClosed);
            }
            hs.receive(&buf[..n])?;

            // Flush any transmits queued by receive (future multi-round-trip support)
            while let Some(transmit) = hs.poll_transmit() {
                stream.write_all(&transmit.data).await?;
            }

            match hs.advance()? {
                LoginProgress::Pending(h) => hs = h,
                LoginProgress::Complete(auth) => break auth.into_connection(),
            }
        };

        // Spawn the actor task
        let (cmd_tx, cmd_rx) = mpsc::channel::<DeviceCommand>(16);
        tokio::spawn(run_actor(stream, conn, cmd_rx));

        Ok(Self { cmd_tx })
    }

    /// Asynchronously sends a command to the connected `MikroTik` device.
    ///
    /// Returns a receiver that will yield [`Event`]s for this command. The
    /// receiver produces:
    /// - [`Event::Reply`] — for each data row (streaming commands may produce many)
    /// - [`Event::Done`] / [`Event::Empty`] — when the command completes
    /// - [`Event::Trap`] — if the command encounters an error
    /// - [`Event::Fatal`] — if a fatal connection error occurs
    ///
    /// # Cancellation
    ///
    /// **Dropping the receiver** automatically sends a `/cancel` to the router
    /// for this command. This is the idiomatic way to stop a long-running
    /// command — just drop the receiver.
    ///
    /// # Errors
    ///
    /// Returns `DeviceError::Actor(ActorError::CommandSendFailed)` if the
    /// connection actor has shut down.
    pub async fn send_command(&self, command: Command) -> DeviceResult<mpsc::Receiver<Event>> {
        let (response_tx, response_rx) = mpsc::channel::<Event>(16);

        self.cmd_tx
            .send(DeviceCommand {
                command,
                respond_to: response_tx,
            })
            .await?;

        Ok(response_rx)
    }
}

impl std::fmt::Debug for DeviceCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceCommand")
            .field("tag", &self.command.tag)
            .finish()
    }
}

/// The background actor task that drives the sans-IO Connection with real I/O.
///
/// This is intentionally a thin glue layer — all complex protocol logic lives
/// in the `mikrotik_proto::Connection` state machine.
async fn run_actor(
    stream: TcpStream,
    mut conn: Connection,
    mut cmd_rx: mpsc::Receiver<DeviceCommand>,
) {
    let (mut rd, mut wr) = stream.into_split();
    let mut buf = vec![0u8; 8192];
    let mut response_map: HashMap<Tag, mpsc::Sender<Event>> = HashMap::new();
    let mut shutdown = false;

    while !shutdown {
        // Flush any pending outbound data before selecting
        while let Some(transmit) = conn.poll_transmit() {
            if wr.write_all(&transmit.data).await.is_err() {
                shutdown = true;
                break;
            }
        }

        if shutdown {
            break;
        }

        tokio::select! {
            biased;

            // Commands first — bounded, fast, prevents starvation under
            // sustained inbound traffic. Network reads are second since
            // they can trigger O(n) event routing.
            msg = cmd_rx.recv() => match msg {
                Some(DeviceCommand { command, respond_to }) => {
                    match conn.send_command(command) {
                        Ok(tag) => {
                            response_map.insert(tag, respond_to);
                        }
                        Err(_) => {
                            shutdown = true;
                        }
                    }
                }
                None => {
                    // All MikrotikDevice handles dropped — graceful shutdown
                    conn.cancel_all();
                    // Flush cancel commands
                    while let Some(transmit) = conn.poll_transmit() {
                        let _ = wr.write_all(&transmit.data).await;
                    }
                    shutdown = true;
                }
            },

            // Read from network → feed to Connection
            result = rd.read(&mut buf) => match result {
                Ok(0) => {
                    // Connection closed by remote
                    shutdown = true;
                }
                Ok(n) => {
                    if conn.receive(&buf[..n]).is_err() {
                        shutdown = true;
                    }

                    // Drain events → route to per-command channels
                    while let Some(event) = conn.poll_event() {
                        route_event(&mut response_map, &mut conn, event);
                    }
                }
                Err(_) => {
                    shutdown = true;
                }
            },
        }
    }

    // Final TCP shutdown
    let _ = wr.shutdown().await;
}

/// Route a protocol event to the appropriate per-command channel.
///
/// Uses `try_send` to avoid blocking the actor on a slow consumer.
/// If a consumer's receiver has been dropped, the command is automatically
/// cancelled on the router — this is the cancel-on-drop mechanism.
fn route_event(
    response_map: &mut HashMap<Tag, mpsc::Sender<Event>>,
    conn: &mut Connection,
    event: Event,
) {
    match &event {
        Event::Reply { tag, .. } => {
            let tag = *tag;
            if let Some(sender) = response_map.get(&tag)
                && sender.try_send(event).is_err()
            {
                // Receiver dropped or full — cancel the command on the router
                response_map.remove(&tag);
                let _ = conn.cancel_command(tag);
            }
        }
        Event::Done { tag } | Event::Empty { tag } | Event::Trap { tag, .. } => {
            let tag = *tag;
            if let Some(sender) = response_map.remove(&tag) {
                // Terminal event — best-effort delivery
                let _ = sender.try_send(event);
            }
        }
        Event::Fatal { .. } => {
            // Fatal affects all commands — best-effort delivery to all
            for (_, sender) in response_map.drain() {
                let _ = sender.try_send(event.clone());
            }
        }
    }
}
