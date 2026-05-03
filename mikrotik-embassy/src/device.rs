//! Core event loop for driving the `MikroTik` connection over any
//! [`embedded_io_async`] transport.

use embedded_io::Error;
use embedded_io_async::{Read, Write};

use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::{Receiver, Sender};

use mikrotik_proto::command::Command;
use mikrotik_proto::connection::{Connection, Event};
use mikrotik_proto::handshake::{Handshaking, LoginProgress};

use crate::error::DeviceError;

/// Run the `MikroTik` device connection loop over an already-connected transport.
///
/// This function performs login authentication and enters an event loop that
/// processes commands from `cmd_rx` and delivers response events to `evt_tx`.
///
/// # Transport
///
/// The `transport` parameter is generic over [`embedded_io_async::Read`] +
/// [`embedded_io_async::Write`]. It must be **already connected** before
/// calling this function. This works with any transport:
///
/// - **Plain TCP**: `embassy_net::tcp::TcpSocket` (after `socket.connect(endpoint).await?`)
/// - **TLS**: `embedded_tls::TlsConnection` (after `tls.open(context).await?`)
/// - **Any other**: UART, pipes, mocks — anything implementing `Read + Write`
///
/// # Design
///
/// This is a **free async function** — the caller spawns it as an Embassy task.
/// All response events are delivered to a **single output channel**. The consumer
/// matches on [`Event`]'s tag field to correlate responses with commands.
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
/// - Transport I/O fails ([`DeviceError::Io`])
/// - Login authentication fails ([`DeviceError::Login`])
/// - Protocol error occurs ([`DeviceError::Connection`])
/// - Remote device closes the connection ([`DeviceError::ConnectionClosed`])
///
/// # Example — Plain TCP
///
/// ```rust,ignore
/// use embassy_net::tcp::TcpSocket;
/// use embassy_sync::channel::Channel;
/// use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
/// use mikrotik_proto::command::Command;
/// use mikrotik_proto::connection::Event;
///
/// static CMD: Channel<CriticalSectionRawMutex, Command, 4> = Channel::new();
/// static EVT: Channel<CriticalSectionRawMutex, Event, 8> = Channel::new();
///
/// #[embassy_executor::task]
/// async fn mikrotik_task(stack: embassy_net::Stack<'static>) {
///     let mut rx_buf = [0; 4096];
///     let mut tx_buf = [0; 4096];
///     let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
///     socket.connect(endpoint).await.unwrap();
///
///     mikrotik_embassy::run(
///         &mut socket, "admin", Some("password"),
///         CMD.receiver(), EVT.sender(),
///     ).await.unwrap();
/// }
/// ```
///
/// # Example — TLS (with `embedded-tls`)
///
/// ```rust,ignore
/// // Connect TCP first
/// let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
/// socket.connect(endpoint).await.unwrap();
///
/// // Wrap in TLS — no certificate verification (MikroTik self-signed certs)
/// let mut tls = TlsConnection::new(socket, &mut tls_read_buf, &mut tls_write_buf);
/// tls.open(TlsContext::new(
///     &TlsConfig::new(),
///     UnsecureProvider::new::<Aes128GcmSha256>(rng),
/// )).await.unwrap();
///
/// // run() doesn't care — it's just Read + Write
/// mikrotik_embassy::run(
///     &mut tls, "admin", Some("password"),
///     CMD.receiver(), EVT.sender(),
/// ).await.unwrap();
/// ```
pub async fn run<T, M, const CMD_N: usize, const EVT_N: usize>(
    transport: &mut T,
    username: &str,
    password: Option<&str>,
    cmd_rx: Receiver<'_, M, Command, CMD_N>,
    evt_tx: Sender<'_, M, Event, EVT_N>,
) -> Result<(), DeviceError>
where
    T: Read + Write,
    M: RawMutex,
{
    // ── Phase 1: Login handshake ──
    let mut hs = Handshaking::new(username, password)?;
    flush_transmits_hs(&mut hs, transport).await?;

    let mut buf = [0u8; 2048];
    let conn = loop {
        let n = read_some(transport, &mut buf).await?;
        hs.receive(&buf[..n]).map_err(DeviceError::Connection)?;
        flush_transmits_hs(&mut hs, transport).await?;

        match hs.advance().map_err(DeviceError::Login)? {
            LoginProgress::Pending(h) => hs = h,
            LoginProgress::Complete(auth) => break auth.into_connection(),
        }
    };

    // ── Phase 2: Event loop ──
    event_loop(conn, transport, &mut buf, &cmd_rx, &evt_tx).await
}

/// Read at least one byte from the transport, returning the number of bytes read.
///
/// # Errors
///
/// Returns [`DeviceError::ConnectionClosed`] if the transport returns 0 bytes (EOF).
/// Returns [`DeviceError::Io`] on transport errors.
async fn read_some<T: Read>(transport: &mut T, buf: &mut [u8]) -> Result<usize, DeviceError> {
    let n = transport.read(buf).await.map_err(map_io)?;
    if n == 0 {
        return Err(DeviceError::ConnectionClosed);
    }
    Ok(n)
}

/// The main event loop: select between incoming commands and network data.
async fn event_loop<T, M, const CMD_N: usize, const EVT_N: usize>(
    mut conn: Connection,
    transport: &mut T,
    buf: &mut [u8],
    cmd_rx: &Receiver<'_, M, Command, CMD_N>,
    evt_tx: &Sender<'_, M, Event, EVT_N>,
) -> Result<(), DeviceError>
where
    T: Read + Write,
    M: RawMutex,
{
    loop {
        // Flush pending outbound data before selecting
        flush_transmits_conn(&mut conn, transport).await?;

        // Select: command channel vs transport read
        match select(cmd_rx.receive(), transport.read(buf)).await {
            // ── Command received from user ──
            Either::First(command) => {
                conn.send_command(command)?;
                // Transmits will be flushed at top of next iteration
            }

            // ── Data received from transport ──
            Either::Second(result) => {
                let n = result.map_err(map_io)?;
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

/// Flush all pending transmits from a [`Handshaking`] state to the transport.
async fn flush_transmits_hs<T: Write>(
    hs: &mut Handshaking,
    transport: &mut T,
) -> Result<(), DeviceError> {
    while let Some(transmit) = hs.poll_transmit() {
        transport.write_all(&transmit.data).await.map_err(map_io)?;
    }
    Ok(())
}

/// Flush all pending transmits from a [`Connection`] to the transport.
async fn flush_transmits_conn<T: Write>(
    conn: &mut Connection,
    transport: &mut T,
) -> Result<(), DeviceError> {
    while let Some(transmit) = conn.poll_transmit() {
        transport.write_all(&transmit.data).await.map_err(map_io)?;
    }
    Ok(())
}

/// Map any [`embedded_io::Error`] to a [`DeviceError::Io`] by extracting the kind.
fn map_io<E: Error>(e: E) -> DeviceError {
    DeviceError::Io(e.kind())
}
