use std::collections::HashMap;
use std::io::Error;

use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::mpsc::{self, Sender};

use crate::command::reader::Sentence;
use crate::command::response::CommandResponse;
use crate::command::CommandBuilder;

pub type ActorResult = io::Result<CommandResponse>;

pub struct ReadActorMessage {
    pub tag: u16,
    pub data: Vec<u8>,
    pub respond_to: mpsc::Sender<ActorResult>,
}

pub struct DeviceConnectionActor;

impl DeviceConnectionActor {
    pub async fn start(
        addr: impl ToSocketAddrs,
        username: &str,
        password: Option<&str>,
    ) -> io::Result<Sender<ReadActorMessage>> {
        let (command_tx_send, mut command_tx_recv) = mpsc::channel::<ReadActorMessage>(16);

        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?;

        let (mut tcp_rx, mut tcp_tx) = stream.into_split();

        let mut shutdown = false;

        tokio::spawn({
            async move {
                let mut running_commands = HashMap::<u16, mpsc::Sender<ActorResult>>::new();
                let mut packet_buf = Vec::<u8>::new();

                while !shutdown {
                    tokio::select! {
                        biased;
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
                                shutdown = true;
                            }
                        },
                        // Read responses from the device
                        bytes_read = tcp_rx.read_buf(&mut packet_buf) => match bytes_read {
                            Ok(0) => {
                                // The device closed the connection, shutdown the actor
                                shutdown = true;
                            },
                            Ok(_) => {
                                // The las byte of the packet is 0, the packet is complete
                                if packet_buf.last() == Some(&0b0) {
                                    let sentence = Sentence::new(&packet_buf);
                                    match CommandResponse::try_from(sentence) {
                                        Ok(response) => match response {
                                            CommandResponse::Done(done) => {
                                                let tag = done.tag;
                                                if let Some(sender) = running_commands.remove(&tag) {
                                                    let _ = sender.send(Ok(CommandResponse::Done(done))).await;
                                                }
                                            }
                                            CommandResponse::Reply(reply) => {
                                                let tag = reply.tag;
                                                if let Some(sender) = running_commands.get(&tag) {
                                                    if sender.send(Ok(CommandResponse::Reply(reply))).await.is_err() {
                                                        // Cancel the command if the channel is closed
                                                        let cancel_cmd = CommandBuilder::cancel(tag);
                                                        if tcp_tx.write_all(cancel_cmd.data.as_ref()).await.is_err() {
                                                            // Error writing the cancel command to the device, shutdown the connection
                                                            shutdown = true;
                                                        }
                                                    }
                                                }
                                            }
                                            CommandResponse::Trap(trap) => {
                                                let tag = trap.tag;
                                                if let Some(sender) = running_commands.remove(&tag) {
                                                    let _ = sender.send(Ok(CommandResponse::Trap(trap))).await;
                                                }
                                            }
                                            CommandResponse::Fatal(reason) => {
                                                // Fatal errors are not associated with a tag and are non-recoverable.
                                                // Shutdown the actor and send the error to all the running commands.
                                                for (_, channel) in running_commands.drain() {
                                                    let _ = channel.send(Ok(
                                                        CommandResponse::Fatal(reason.clone()),
                                                    )).await;
                                                }
                                            },
                                        },
                                        Err(e) => {
                                            println!("Error parsing response: {:?}", e);
                                        }
                                    };
                                    // Reset the packet buffer for the next read
                                    packet_buf.clear();
                                }
                            },
                            Err(e) => {
                                println!("Error reading from device: {:?}", e);
                                notify_error(&mut running_commands, e).await;
                                shutdown = true;
                            }
                        }
                    }
                }
                // Close the TCP connection before shutting down
                let _ = tcp_tx.shutdown().await;
            }
        });

        login(username, password, &command_tx_send).await?;

        Ok(command_tx_send)
    }
}

async fn login(
    username: &str,
    password: Option<&str>,
    command_tx_send: &Sender<ReadActorMessage>,
) -> io::Result<()> {
    let (login_response_tx, mut login_response_rx) = mpsc::channel(1);
    let login = CommandBuilder::login(username, password);
    command_tx_send
        .send(ReadActorMessage {
            tag: login.tag,
            data: login.data,
            respond_to: login_response_tx,
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to send login command"))?;

    login_response_rx.recv().await.ok_or(io::Error::new(
        io::ErrorKind::Other,
        "Failed to receive login response",
    ))??;

    Ok(())
}

// Notify all the running commands about the error
async fn notify_error(
    running_commands: &mut HashMap<u16, mpsc::Sender<ActorResult>>,
    error: io::Error,
) {
    let kind = error.kind();
    for (_, channel) in running_commands.drain() {
        let _ = channel.send(Err(Error::from(kind))).await;
    }
}
