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
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::ToSocketAddrs;
use tokio::sync::mpsc;

use crate::builder::{DeviceBuilder, Plaintext};
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
/// # Connection
///
/// Use [`builder()`](Self::builder) for full control over the connection, including
/// optional TLS support (requires the `tokio-tls` feature flag):
///
/// ```rust,ignore
/// // Plaintext TCP
/// let device = MikrotikDevice::builder("192.168.88.1:8728")
///     .credentials("admin", Some("password"))
///     .connect()
///     .await?;
///
/// // TLS (accept self-signed certs — typical for MikroTik)
/// let device = MikrotikDevice::builder("192.168.88.1:8729")
///     .credentials("admin", Some("password"))
///     .tls_insecure()
///     .connect()
///     .await?;
/// ```
///
/// Or use [`connect()`](Self::connect) as a shorthand for plaintext TCP.
///
/// # Cancellation
///
/// In-flight commands are automatically cancelled on the router when the
/// response receiver returned by [`send_command`](Self::send_command)
/// is dropped. This follows Rust's RAII pattern — just drop the receiver to
/// stop a long-running command like `/tool/torch` or `/interface/monitor-traffic`.
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
    /// Create a builder for establishing a connection to a `MikroTik` device.
    ///
    /// The builder supports plaintext TCP and, with the `tokio-tls` feature enabled,
    /// TLS connections (for API-SSL on port 8729).
    ///
    /// See [`DeviceBuilder`] for the full API.
    pub fn builder<A: ToSocketAddrs>(addr: A) -> DeviceBuilder<A, Plaintext> {
        DeviceBuilder::new(addr)
    }

    /// Shorthand for a plaintext TCP connection.
    ///
    /// Equivalent to:
    /// ```rust,ignore
    /// MikrotikDevice::builder(addr)
    ///     .credentials(username, password)
    ///     .connect()
    ///     .await
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError`] if the TCP connection or login handshake fails.
    pub async fn connect<A: ToSocketAddrs>(
        addr: A,
        username: &str,
        password: Option<&str>,
    ) -> DeviceResult<Self> {
        Self::builder(addr)
            .credentials(username, password)
            .connect()
            .await
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

// ── Internal: handshake + spawn (transport-generic) ──

/// Perform the login handshake over any async stream and spawn the actor.
///
/// This is the shared implementation used by both plaintext TCP and TLS
/// connection paths.
pub(crate) async fn handshake_and_spawn<S>(
    mut stream: S,
    username: &str,
    password: Option<&str>,
) -> DeviceResult<MikrotikDevice>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    // Perform the login handshake using the sans-IO handshake state machine
    let mut hs = Handshaking::new(username, password)?;

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

        // Flush any transmits queued by receive
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

    Ok(MikrotikDevice { cmd_tx })
}

// ── Internal: actor event loop (transport-generic) ──

/// The background actor task that drives the sans-IO Connection with real I/O.
///
/// Generic over any `AsyncRead + AsyncWrite` stream — works with both
/// `TcpStream` and `TlsStream<TcpStream>`.
async fn run_actor<S>(stream: S, mut conn: Connection, mut cmd_rx: mpsc::Receiver<DeviceCommand>)
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let (mut rd, mut wr) = tokio::io::split(stream);
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
            // sustained inbound traffic.
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
                    while let Some(transmit) = conn.poll_transmit() {
                        let _ = wr.write_all(&transmit.data).await;
                    }
                    shutdown = true;
                }
            },

            // Read from network → feed to Connection
            result = rd.read(&mut buf) => match result {
                Ok(0) => {
                    shutdown = true;
                }
                Ok(n) => {
                    if conn.receive(&buf[..n]).is_err() {
                        shutdown = true;
                    }

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

    let _ = wr.shutdown().await;
}

/// Route a protocol event to the appropriate per-command channel.
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
                response_map.remove(&tag);
                let _ = conn.cancel_command(tag);
            }
        }
        Event::Done { tag } | Event::Empty { tag } | Event::Trap { tag, .. } => {
            let tag = *tag;
            if let Some(sender) = response_map.remove(&tag) {
                let _ = sender.try_send(event);
            }
        }
        Event::Fatal { .. } => {
            for (_, sender) in response_map.drain() {
                let _ = sender.try_send(event.clone());
            }
        }
        #[allow(unreachable_patterns)]
        _ => {}
    }
}
