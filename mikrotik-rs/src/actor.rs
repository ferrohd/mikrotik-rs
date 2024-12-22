use std::collections::HashMap;
use std::io::Error;

use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::mpsc::{self, Sender};

use crate::command::response::CommandResponse;
use crate::command::sentence::Sentence;
use crate::command::CommandBuilder;
use crate::error::{DeviceError, DeviceResult};

/// A command result sent back to the client.
///
/// The result of a command execution, which can be either a successful response or an error.
/// The [`CommandResult`] is a type alias for [`io::Result<CommandResponse>`]
pub type CommandResult = io::Result<CommandResponse>;

/// Command message with data to write to the device
pub struct ReadActorMessage {
    pub tag: u16,
    pub data: Vec<u8>,
    pub respond_to: Sender<DeviceResult<CommandResult>>,
}

pub struct DeviceConnectionActor;

impl DeviceConnectionActor {
    /// Connect to the device, spawn the read/write loop, and log in.
    pub async fn start(
        addr: impl ToSocketAddrs,
        username: &str,
        password: Option<&str>,
    ) -> DeviceResult<Sender<ReadActorMessage>> {
        let (command_tx_send, mut command_tx_recv) = mpsc::channel::<ReadActorMessage>(16);

        // Connect to the device
        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?;

        // Split for independent read/write
        let (mut tcp_rx, mut tcp_tx) = stream.into_split();

        // These flags manage the main loop’s state
        let mut command_channel_open = true;
        let mut shutdown = false;

        // Spawn the main loop
        tokio::spawn(async move {
            let mut running_commands = HashMap::<u16, Sender<DeviceResult<CommandResponse>>>::new();
            let mut packet_buf = Vec::new();

            // Loop until forced to shutdown or no active commands left
            while !shutdown && (command_channel_open || !running_commands.is_empty()) {
                tokio::select! {
                    // Prefer reading from the device
                    biased;

                    // Handle device responses
                    bytes_read = tcp_rx.read_buf(&mut packet_buf) => match bytes_read {
                        Ok(0) => {
                            // Device closed connection
                            notify_error(&mut running_commands, DeviceError::Connection(
                                io::Error::new(io::ErrorKind::UnexpectedEof, "Device closed connection")
                            )).await;
                            shutdown = true;
                        }
                        Ok(_) => {
                            // Process all null-terminated packets in buffer
                            while let Some(null_idx) = packet_buf.iter().position(|&b| b == 0) {
                                let packet: Vec<_> = packet_buf.drain(..=null_idx).collect();
                                process_packet(&packet, &mut running_commands, &mut tcp_tx, &mut shutdown).await;
                            }
                        }
                        Err(e) => {
                            eprintln!("Error reading from device: {:?}", e);
                            notify_error(&mut running_commands, e).await;
                            shutdown = true;
                        }
                    },

                    // Send commands to the device
                    maybe_actor_message = command_tx_recv.recv() => match maybe_actor_message {
                        Some(ReadActorMessage { tag, data, respond_to }) => {
                            // Error writing the command to the device, shutdown the connection
                            match tcp_tx.write_all(&data).await {
                                Ok(_) => {
                                    // The command is sent, store the channel to send the responses back
                                    running_commands.insert(tag, respond_to);
                                }
                                Err(e) => {
                                    // Error writing the command to the device, notify every running command and shutdown the connection
                                    notify_error(&mut running_commands, e).await;
                                    shutdown = true;
                                }
                            }
                        }
                        None => {
                            // The command channel is closed, we won't receive more commands
                            command_channel_open = false;
                        }
                    }
                }
            }

            // Final attempt to gracefully close TCP
            let _ = tcp_tx.shutdown().await;
        });

        // Attempt login
        login(username, password, &command_tx_send).await?;
        Ok(command_tx_send)
    }
}

/// Process a complete packet from the device
async fn process_packet(
    packet: &[u8],
    running_commands: &mut HashMap<u16, Sender<DeviceResult<CommandResponse>>>,
    tcp_tx: &mut (impl AsyncWriteExt + Unpin),
    shutdown: &mut bool,
) {
    let sentence = Sentence::new(packet);
    match CommandResponse::try_from(sentence) {
        Ok(response) => match response {
            CommandResponse::Done(done) => {
                if let Some(sender) = running_commands.remove(&done.tag) {
                    let _ = sender.send(Ok(CommandResponse::Done(done))).await;
                }
            }
            CommandResponse::Reply(reply) => {
                let tag = reply.tag;
                if let Some(sender) = running_commands.get(&tag) {
                    // If the receiver is gone, cancel the command
                    if sender
                        .send(Ok(CommandResponse::Reply(reply)))
                        .await
                        .is_err()
                    {
                        if let Err(e) = tcp_tx
                            .write_all(CommandBuilder::cancel(tag).data.as_ref())
                            .await
                        {
                            eprintln!("Error sending cancel command: {:?}", e);
                            *shutdown = true;
                        }
                    }
                }
            }
            CommandResponse::Trap(trap) => {
                if let Some(sender) = running_commands.remove(&trap.tag) {
                    let _ = sender.send(Ok(CommandResponse::Trap(trap))).await;
                }
            }
            CommandResponse::Fatal(reason) => {
                // A fatal error is not tag-bound => clean up everything
                for (_, sender) in running_commands.drain() {
                    let _ = sender
                        .send(Ok(CommandResponse::Fatal(reason.clone())))
                        .await;
                }
                *shutdown = true;
            }
        },
        Err(e) => eprintln!("Error parsing response: {:?}", e),
    }
}

/// Log in by sending the login command. Returns an error if login fails.
async fn login(
    username: &str,
    password: Option<&str>,
    command_tx_send: &Sender<ReadActorMessage>,
) -> DeviceResult<()> {
    let (login_response_tx, mut login_response_rx) = mpsc::channel(1);
    let login_cmd = CommandBuilder::login(username, password);

    command_tx_send
        .send(ReadActorMessage {
            tag: login_cmd.tag,
            data: login_cmd.data,
            respond_to: login_response_tx,
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to send login command"))?;

    match login_response_rx
        .recv()
        .await
        .ok_or_else(|| DeviceError::Channel {
            message: "No login response received".to_string(),
        })???
    {
        CommandResponse::Done(_) => Ok(()),
        CommandResponse::Trap(trap) => Err(DeviceError::Authentication {
            response: trap,
        }),
        other => Err(DeviceError::Protocol {
            response: other,
        }),
    }
}

/// Notify all running commands of an I/O error (e.g. disconnect).
async fn notify_error(
    running_commands: &mut HashMap<u16, Sender<DeviceResult<CommandResponse>>>,
    error: DeviceError,
) {
    for (_, channel) in running_commands.drain() {
        let _ = channel.send(Err(error.clone())).await;
    }
}
