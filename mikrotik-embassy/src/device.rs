//! Core event loop for driving the `MikroTik` connection over Embassy networking.

use embedded_io_async::Write;

use embassy_futures::select::{select, Either};
use embassy_net::tcp::TcpSocket;
use embassy_net::IpEndpoint;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::{Receiver, Sender};

use mikrotik_proto::command::Command;
use mikrotik_proto::connection::{Connection, Event};
use mikrotik_proto::handshake::{Handshaking, LoginProgress};

use crate::error::DeviceError;

/// Run the `MikroTik` device connection loop.
///
/// This function connects to a `MikroTik` router, performs login authentication,
/// and enters an event loop that processes commands from `cmd_rx` and delivers
/// response events to `evt_tx`.
///
/// # Design
///
/// This is a **free async function** — the caller spawns it as an Embassy task.
/// The function owns the `TcpSocket` for its entire lifetime (Embassy sockets
/// cannot be split into `'static` owned halves like Tokio streams).
///
/// All response events are delivered to a **single output channel**. The consumer
/// matches on [`Event`]'s tag field to correlate responses with commands.
///
/// # Cancellation
///
/// When `cmd_rx`'s senders are all dropped (channel closed), the loop performs
/// a graceful shutdown: cancels all in-flight commands on the router, flushes
/// the cancel transmits, and returns `Ok(())`.
///
/// # Backpressure
///
/// Events are delivered via [`Sender::try_send`]. If the event channel is full,
/// events are dropped to avoid blocking the network loop. Size your event
/// channel capacity accordingly.
///
/// # Errors
///
/// Returns [`DeviceError`] if:
/// - TCP connection fails ([`DeviceError::Connect`])
/// - TCP read/write fails ([`DeviceError::Tcp`])
/// - Login authentication fails ([`DeviceError::Login`])
/// - Protocol error occurs ([`DeviceError::Connection`])
/// - Remote device closes the connection ([`DeviceError::ConnectionClosed`])
///
/// # Example
///
/// ```rust,ignore
/// use embassy_sync::channel::Channel;
/// use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
///
/// static CMD: Channel<CriticalSectionRawMutex, Command, 4> = Channel::new();
/// static EVT: Channel<CriticalSectionRawMutex, Event, 8> = Channel::new();
///
/// #[embassy_executor::task]
/// async fn mikrotik_task(stack: embassy_net::Stack<'static>) {
///     let mut rx_buf = [0; 4096];
///     let mut tx_buf = [0; 4096];
///     let socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
///     let endpoint = IpEndpoint::new(
///         embassy_net::IpAddress::v4(192, 168, 88, 1), 8728,
///     );
///     run(socket, endpoint, "admin", Some("password"),
///         CMD.receiver(), EVT.sender()).await.unwrap();
/// }
/// ```
pub async fn run<'a, M: RawMutex, const CMD_N: usize, const EVT_N: usize>(
    mut socket: TcpSocket<'a>,
    remote: IpEndpoint,
    username: &str,
    password: Option<&str>,
    cmd_rx: Receiver<'a, M, Command, CMD_N>,
    evt_tx: Sender<'a, M, Event, EVT_N>,
) -> Result<(), DeviceError> {
    // ── Phase 1: Connect ──
    socket.connect(remote).await?;

    // ── Phase 2: Login handshake ──
    let mut hs = Handshaking::new(username, password);

    // Flush the login command
    flush_transmits_hs(&mut hs, &mut socket).await?;

    // Read until login completes
    let mut buf = [0u8; 2048];
    let conn = loop {
        let n = socket.read(&mut buf).await?;
        if n == 0 {
            return Err(DeviceError::ConnectionClosed);
        }
        hs.receive(&buf[..n]).map_err(DeviceError::Connection)?;
        flush_transmits_hs(&mut hs, &mut socket).await?;

        match hs.advance().map_err(DeviceError::Login)? {
            LoginProgress::Pending(h) => hs = h,
            LoginProgress::Complete(auth) => break auth.into_connection(),
        }
    };

    // ── Phase 3: Event loop ──
    event_loop(conn, &mut socket, &mut buf, &cmd_rx, &evt_tx).await
}

/// The main event loop: select between incoming commands and network data.
async fn event_loop<'a, M: RawMutex, const CMD_N: usize, const EVT_N: usize>(
    mut conn: Connection,
    socket: &mut TcpSocket<'a>,
    buf: &mut [u8],
    cmd_rx: &Receiver<'a, M, Command, CMD_N>,
    evt_tx: &Sender<'a, M, Event, EVT_N>,
) -> Result<(), DeviceError> {
    loop {
        // Flush pending outbound data before selecting
        flush_transmits_conn(&mut conn, socket).await?;

        // Select: command channel vs network read
        match select(cmd_rx.receive(), socket.read(buf)).await {
            // ── Command received from user ──
            Either::First(command) => {
                conn.send_command(&command)?;
                // Transmits will be flushed at top of next iteration
            }

            // ── Data received from network ──
            Either::Second(result) => {
                let n = result?;
                if n == 0 {
                    return Err(DeviceError::ConnectionClosed);
                }
                conn.receive(&buf[..n])?;

                // Drain all events to the output channel
                while let Some(event) = conn.poll_event() {
                    // Best-effort delivery — drop if channel full
                    let _ = evt_tx.try_send(event);
                }
            }
        }
    }
}

/// Flush all pending transmits from a [`Handshaking`] state to the socket.
async fn flush_transmits_hs(
    hs: &mut Handshaking,
    socket: &mut TcpSocket<'_>,
) -> Result<(), DeviceError> {
    while let Some(transmit) = hs.poll_transmit() {
        socket.write_all(&transmit.data).await?;
    }
    Ok(())
}

/// Flush all pending transmits from a [`Connection`] to the socket.
async fn flush_transmits_conn(
    conn: &mut Connection,
    socket: &mut TcpSocket<'_>,
) -> Result<(), DeviceError> {
    while let Some(transmit) = conn.poll_transmit() {
        socket.write_all(&transmit.data).await?;
    }
    Ok(())
}
