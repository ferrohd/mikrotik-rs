//! Sans-IO connection state machine with multiplexed command/response correlation.
//!
//! The [`Connection`] type manages the entire protocol lifecycle without
//! performing any I/O. It accepts raw bytes from the network, decodes them
//! into responses, and correlates them with in-flight commands via UUID tags.
//!
//! # Usage pattern (mirrors `quinn-proto`)
//!
//! ```rust
//! use mikrotik_proto::connection::{Connection, Event};
//! use mikrotik_proto::command::CommandBuilder;
//!
//! let mut conn = Connection::new();
//!
//! // Send a command
//! let cmd = CommandBuilder::new().command("/system/resource/print").build();
//! let tag = conn.send_command(cmd).unwrap();
//!
//! // In your event loop you would:
//! // 1. Drain outbound data (send to transport)
//! while let Some(transmit) = conn.poll_transmit() {
//!     // transport.write_all(&transmit.data);
//!     assert!(!transmit.data.is_empty());
//! }
//!
//! // 2. Feed incoming bytes from the transport
//! //    conn.receive(&incoming_bytes).unwrap();
//!
//! // 3. Process application events
//! while let Some(event) = conn.poll_event() {
//!     match event {
//!         Event::Reply { tag, response } => { /* handle streaming reply */ }
//!         Event::Done { tag } => { /* command completed */ }
//!         Event::Trap { tag, response } => { /* handle error */ }
//!         Event::Fatal { reason } => { /* connection dead */ }
//!         Event::Empty { tag } => { /* empty response */ }
//!     }
//! }
//! ```

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;

use crate::codec::{self, Decode};
use crate::command::{Command, CommandBuilder};
use crate::error::ConnectionError;
use crate::response::{CommandResponse, ReplyResponse, TrapResponse};
use crate::tag::Tag;
use crate::word::Word;
use hashbrown::HashMap;

/// Application-facing events produced by the connection state machine.
///
/// Events are emitted after calling [`Connection::receive()`] and can be
/// retrieved via [`Connection::poll_event()`].
#[derive(Debug, Clone)]
pub enum Event {
    /// A reply sentence was received for the given command.
    /// The command remains active (more replies may follow until `Done` or `Trap`).
    Reply {
        /// The command tag this reply belongs to.
        tag: Tag,
        /// The parsed reply data.
        response: ReplyResponse,
    },

    /// The command completed successfully. No more responses will arrive for this tag.
    Done {
        /// The command tag that completed.
        tag: Tag,
    },

    /// An empty response was received (`RouterOS` 7.18+).
    /// The command completed with no data.
    Empty {
        /// The command tag that completed.
        tag: Tag,
    },

    /// A trap (error/warning) was received for the given command.
    /// The command is terminated.
    Trap {
        /// The command tag that errored.
        tag: Tag,
        /// The trap details.
        response: TrapResponse,
    },

    /// A fatal error occurred. All in-flight commands are terminated.
    /// The connection should be considered dead after this event.
    Fatal {
        /// The fatal error reason from the router.
        reason: String,
    },
}

/// Outbound data that must be sent to the remote device.
///
/// Returned by [`Connection::poll_transmit()`]. The caller is responsible
/// for writing these bytes to the transport (TCP, TLS, etc.).
#[derive(Debug)]
pub struct Transmit {
    /// The wire-format bytes to send.
    pub data: Vec<u8>,
}

/// Connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The connection is operational and can send/receive commands.
    Active,
    /// A fatal error was received; the connection is dead.
    /// No further operations can be performed.
    Dead,
}

/// Internal tracking state for a single in-flight command.
#[derive(Debug)]
struct CommandState {
    /// Number of `!re` replies received so far (for diagnostics).
    reply_count: usize,
}

/// The sans-IO connection state machine for the `MikroTik` API protocol.
///
/// This type manages:
/// - **Framing:** accumulating incoming bytes into complete sentences.
/// - **Demultiplexing:** routing responses to their originating commands via tags.
/// - **Command lifecycle:** tracking in-flight commands and their completion.
/// - **Outbound buffering:** queuing encoded commands for transmission.
///
/// It does **not** perform any I/O. The caller is responsible for:
/// - Reading bytes from the network and feeding them via [`receive()`](Connection::receive).
/// - Draining outbound bytes via [`poll_transmit()`](Connection::poll_transmit) and sending them.
/// - Polling for application events via [`poll_event()`](Connection::poll_event).
#[derive(Debug)]
pub struct Connection {
    /// Current connection state.
    state: State,
    /// Accumulation buffer for incoming bytes not yet forming a complete sentence.
    recv_buf: Vec<u8>,
    /// Tags of commands currently in-flight, mapped to their tracking state.
    in_flight: HashMap<Tag, CommandState>,
    /// Queue of application-facing events ready to be polled.
    events: VecDeque<Event>,
    /// Queue of outbound wire-format data ready to be sent.
    outbound: VecDeque<Transmit>,
}

impl Connection {
    /// Create a new connection state machine.
    ///
    /// The connection starts in the [`State::Active`] state with no in-flight
    /// commands.
    pub fn new() -> Self {
        Self {
            state: State::Active,
            recv_buf: Vec::new(),
            in_flight: HashMap::new(),
            events: VecDeque::new(),
            outbound: VecDeque::new(),
        }
    }

    // ── Category B: Handle incoming data ──

    /// Feed received bytes into the connection.
    ///
    /// The connection buffers these bytes internally and attempts to decode
    /// complete sentences. Successfully decoded sentences are processed
    /// immediately, producing [`Event`]s retrievable via [`poll_event()`](Self::poll_event).
    ///
    /// # Errors
    ///
    /// Returns [`ConnectionError::Decode`] if the data contains a malformed
    /// length prefix that cannot be recovered from.
    /// Returns [`ConnectionError::Closed`] if the connection has been fatally shut down.
    pub fn receive(&mut self, data: &[u8]) -> Result<(), ConnectionError> {
        if self.state == State::Dead {
            return Err(ConnectionError::Closed);
        }

        self.recv_buf.extend_from_slice(data);

        // Process all complete sentences in the buffer.
        //
        // We parse into owned types within a borrow-limited scope so that
        // the immutable borrow on `self.recv_buf` is released before we
        // need `&mut self` to dispatch events.
        loop {
            // Step 1: Try to decode + parse within a scope that borrows recv_buf
            let outcome = {
                let buf = &self.recv_buf;
                match codec::decode_sentence(buf)? {
                    Decode::Complete {
                        value: raw_sentence,
                        bytes_consumed,
                    } => {
                        let result = match CommandResponse::parse(&raw_sentence) {
                            Ok(response) => Ok(response),
                            Err(e) => {
                                let tag_opt = raw_sentence.words().find_map(|word_bytes| {
                                    Word::try_from(word_bytes).ok().and_then(|w| match w {
                                        Word::Tag(t) => Some(t),
                                        _ => None,
                                    })
                                });
                                Err((e, tag_opt))
                            }
                        };
                        Some((result, bytes_consumed))
                    }
                    Decode::Incomplete { .. } => None,
                }
            };
            // Borrow on self.recv_buf is now released

            // Step 2: Drain and dispatch with full &mut self access
            match outcome {
                Some((parsed, bytes_consumed)) => {
                    self.recv_buf.drain(..bytes_consumed);
                    match parsed {
                        Ok(response) => self.dispatch_response(response),
                        Err((error, tag_opt)) => self.handle_parse_error(&error, tag_opt),
                    }
                }
                None => break,
            }
        }

        Ok(())
    }

    // ── Category C: Application commands ──

    /// Submit a command to be sent to the remote device.
    ///
    /// The command's wire-format bytes are queued internally and will be
    /// available via [`poll_transmit()`](Self::poll_transmit). The command's tag
    /// is registered for response correlation.
    ///
    /// Returns the command's tag for later reference.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectionError::Closed`] if the connection is dead.
    pub fn send_command(&mut self, command: Command) -> Result<Tag, ConnectionError> {
        if self.state == State::Dead {
            return Err(ConnectionError::Closed);
        }

        let tag = command.tag;

        // Queue the wire-format data for transmission
        self.outbound.push_back(Transmit {
            data: command.into_data(),
        });

        // Track as in-flight
        self.in_flight.insert(tag, CommandState { reply_count: 0 });

        Ok(tag)
    }

    /// Cancel an in-flight command.
    ///
    /// Sends a `/cancel` command for the given tag to the router and removes
    /// the command from the in-flight set.
    ///
    /// If the tag is not currently in-flight, this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectionError::Closed`] if the connection is dead.
    pub fn cancel_command(&mut self, tag: Tag) -> Result<(), ConnectionError> {
        if self.state == State::Dead {
            return Err(ConnectionError::Closed);
        }

        if self.in_flight.remove(&tag).is_some() {
            let cancel = CommandBuilder::cancel(tag);
            self.outbound.push_back(Transmit {
                data: cancel.into_data(),
            });
        }

        Ok(())
    }

    /// Cancel all in-flight commands.
    ///
    /// Sends a `/cancel` command for each active tag. Useful during shutdown.
    pub fn cancel_all(&mut self) {
        let tags: Vec<Tag> = self.in_flight.keys().copied().collect();
        for tag in tags {
            let cancel = CommandBuilder::cancel(tag);
            self.outbound.push_back(Transmit {
                data: cancel.into_data(),
            });
        }
        self.in_flight.clear();
    }

    // ── Category D: Poll for outgoing events/data ──

    /// Returns the next chunk of data to transmit to the remote device.
    ///
    /// The caller should send these bytes over the transport and call this
    /// method again until it returns `None`.
    pub fn poll_transmit(&mut self) -> Option<Transmit> {
        self.outbound.pop_front()
    }

    /// Returns the next application-facing event.
    ///
    /// Events are returned in the order they were produced. Call after
    /// [`receive()`](Self::receive) to process new responses.
    pub fn poll_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    // ── Category A: Getters ──

    /// Current connection state.
    pub fn state(&self) -> State {
        self.state
    }

    /// Whether the connection is still active (not fatally closed).
    pub fn is_active(&self) -> bool {
        self.state == State::Active
    }

    /// Number of commands currently in-flight.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    /// Whether there is data pending to be transmitted.
    pub fn has_pending_transmit(&self) -> bool {
        !self.outbound.is_empty()
    }

    /// Number of bytes currently buffered awaiting a complete sentence.
    pub fn recv_buffer_len(&self) -> usize {
        self.recv_buf.len()
    }

    /// Check if a specific tag is currently in-flight.
    pub fn is_in_flight(&self, tag: Tag) -> bool {
        self.in_flight.contains_key(&tag)
    }

    // ── Internal dispatch logic ──

    fn dispatch_response(&mut self, response: CommandResponse) {
        match response {
            CommandResponse::Reply(reply) => {
                let tag = reply.tag;
                if let Some(cmd_state) = self.in_flight.get_mut(&tag) {
                    cmd_state.reply_count += 1;
                    self.events.push_back(Event::Reply {
                        tag,
                        response: reply,
                    });
                }
                // Silently ignore replies for unknown tags (command may have
                // been cancelled and a straggling reply arrived).
            }

            CommandResponse::Done(done) => {
                let tag = done.tag;
                self.in_flight.remove(&tag);
                self.events.push_back(Event::Done { tag });
            }

            CommandResponse::Empty(empty) => {
                let tag = empty.tag;
                self.in_flight.remove(&tag);
                self.events.push_back(Event::Empty { tag });
            }

            CommandResponse::Trap(trap) => {
                let tag = trap.tag;
                self.in_flight.remove(&tag);
                self.events.push_back(Event::Trap {
                    tag,
                    response: trap,
                });
            }

            CommandResponse::Fatal(reason) => {
                // Fatal kills everything
                self.in_flight.clear();
                self.state = State::Dead;
                self.events.push_back(Event::Fatal { reason });
            }
        }
    }

    fn handle_parse_error(&mut self, error: &crate::error::ProtocolError, tag_opt: Option<Tag>) {
        if let Some(tag) = tag_opt {
            self.in_flight.remove(&tag);
            self.events.push_back(Event::Trap {
                tag,
                response: TrapResponse {
                    tag,
                    category: None,
                    message: alloc::format!("Protocol error: {error}"),
                },
            });
        }
        // If no tag found in malformed packet, we can't route the error
        // to a specific command. In the future we could emit a
        // connection-level diagnostic event.
    }
}

impl Default for Connection {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec;
    use alloc::format;
    use alloc::string::String;
    use alloc::vec;

    /// Build wire-format sentence data from word byte slices.
    fn build_sentence(words: &[&[u8]]) -> Vec<u8> {
        let mut data = Vec::new();
        for word in words {
            codec::encode_word(word, &mut data);
        }
        codec::encode_terminator(&mut data);
        data
    }

    fn build_done(tag: Tag) -> Vec<u8> {
        let tag_word = format!(".tag={tag}");
        build_sentence(&[b"!done", tag_word.as_bytes()])
    }

    fn build_empty(tag: Tag) -> Vec<u8> {
        let tag_word = format!(".tag={tag}");
        build_sentence(&[b"!empty", tag_word.as_bytes()])
    }

    fn build_reply(tag: Tag, attrs: &[(&str, &str)]) -> Vec<u8> {
        let tag_word = format!(".tag={tag}");
        let mut words: Vec<Vec<u8>> = vec![b"!re".to_vec(), tag_word.into_bytes()];
        for (k, v) in attrs {
            words.push(format!("={k}={v}").into_bytes());
        }
        let word_refs: Vec<&[u8]> = words.iter().map(|w| w.as_slice()).collect();
        build_sentence(&word_refs)
    }

    fn build_trap(tag: Tag, message: &str) -> Vec<u8> {
        let tag_word = format!(".tag={tag}");
        let msg_word = format!("=message={message}");
        build_sentence(&[b"!trap", tag_word.as_bytes(), msg_word.as_bytes()])
    }

    fn build_fatal(reason: &str) -> Vec<u8> {
        build_sentence(&[b"!fatal", reason.as_bytes()])
    }

    #[test]
    fn test_send_command_queues_transmit() {
        let mut conn = Connection::new();
        let cmd = CommandBuilder::new().command("/test").build();
        let expected_data = cmd.data().to_vec();
        let tag = conn.send_command(cmd).unwrap();

        assert!(conn.is_in_flight(tag));
        assert_eq!(conn.in_flight_count(), 1);
        assert!(conn.has_pending_transmit());

        let transmit = conn.poll_transmit().unwrap();
        assert_eq!(transmit.data, expected_data);
        assert!(!conn.has_pending_transmit());
    }

    #[test]
    fn test_done_response_completes_command() {
        let mut conn = Connection::new();
        let cmd = CommandBuilder::new().command("/test").build();
        let tag = conn.send_command(cmd).unwrap();

        // Drain transmit
        while conn.poll_transmit().is_some() {}

        // Simulate done response
        let wire = build_done(tag);
        conn.receive(&wire).unwrap();

        match conn.poll_event().unwrap() {
            Event::Done { tag: t } => assert_eq!(t, tag),
            other => panic!("expected Done, got {other:?}"),
        }
        assert_eq!(conn.in_flight_count(), 0);
        assert!(conn.poll_event().is_none());
    }

    #[test]
    fn test_empty_response_completes_command() {
        let mut conn = Connection::new();
        let cmd = CommandBuilder::new().command("/test").build();
        let tag = conn.send_command(cmd).unwrap();
        while conn.poll_transmit().is_some() {}

        let wire = build_empty(tag);
        conn.receive(&wire).unwrap();

        match conn.poll_event().unwrap() {
            Event::Empty { tag: t } => assert_eq!(t, tag),
            other => panic!("expected Empty, got {other:?}"),
        }
        assert_eq!(conn.in_flight_count(), 0);
    }

    #[test]
    fn test_streaming_replies_then_done() {
        let mut conn = Connection::new();
        let cmd = CommandBuilder::new().command("/interface/print").build();
        let tag = conn.send_command(cmd).unwrap();
        while conn.poll_transmit().is_some() {}

        // Two replies
        conn.receive(&build_reply(tag, &[("name", "ether1")]))
            .unwrap();
        conn.receive(&build_reply(tag, &[("name", "ether2")]))
            .unwrap();
        // Then done
        conn.receive(&build_done(tag)).unwrap();

        // First reply
        match conn.poll_event().unwrap() {
            Event::Reply { tag: t, response } => {
                assert_eq!(t, tag);
                assert_eq!(
                    response.attributes.get("name"),
                    Some(&Some(String::from("ether1")))
                );
            }
            other => panic!("expected Reply, got {other:?}"),
        }

        // Second reply
        match conn.poll_event().unwrap() {
            Event::Reply { tag: t, response } => {
                assert_eq!(t, tag);
                assert_eq!(
                    response.attributes.get("name"),
                    Some(&Some(String::from("ether2")))
                );
            }
            other => panic!("expected Reply, got {other:?}"),
        }

        // Done
        match conn.poll_event().unwrap() {
            Event::Done { tag: t } => assert_eq!(t, tag),
            other => panic!("expected Done, got {other:?}"),
        }

        assert_eq!(conn.in_flight_count(), 0);
        assert!(conn.poll_event().is_none());
    }

    #[test]
    fn test_trap_response_terminates_command() {
        let mut conn = Connection::new();
        let cmd = CommandBuilder::new().command("/test").build();
        let tag = conn.send_command(cmd).unwrap();
        while conn.poll_transmit().is_some() {}

        conn.receive(&build_trap(tag, "no such command")).unwrap();

        match conn.poll_event().unwrap() {
            Event::Trap { tag: t, response } => {
                assert_eq!(t, tag);
                assert_eq!(response.message, "no such command");
            }
            other => panic!("expected Trap, got {other:?}"),
        }
        assert_eq!(conn.in_flight_count(), 0);
    }

    #[test]
    fn test_fatal_kills_all_commands() {
        let mut conn = Connection::new();
        let cmd1 = CommandBuilder::new().command("/test1").build();
        let cmd2 = CommandBuilder::new().command("/test2").build();
        conn.send_command(cmd1).unwrap();
        conn.send_command(cmd2).unwrap();
        while conn.poll_transmit().is_some() {}

        conn.receive(&build_fatal("out of memory")).unwrap();

        match conn.poll_event().unwrap() {
            Event::Fatal { reason } => assert_eq!(reason, "out of memory"),
            other => panic!("expected Fatal, got {other:?}"),
        }

        assert_eq!(conn.state(), State::Dead);
        assert_eq!(conn.in_flight_count(), 0);

        // Further operations should fail
        let cmd3 = CommandBuilder::new().command("/test3").build();
        assert!(conn.send_command(cmd3).is_err());
        assert!(conn.receive(&[]).is_err());
    }

    #[test]
    fn test_partial_receive() {
        let mut conn = Connection::new();
        let cmd = CommandBuilder::new().command("/test").build();
        let tag = conn.send_command(cmd).unwrap();
        while conn.poll_transmit().is_some() {}

        let wire = build_done(tag);

        // Feed one byte at a time
        for &byte in &wire {
            conn.receive(&[byte]).unwrap();
        }

        match conn.poll_event().unwrap() {
            Event::Done { tag: t } => assert_eq!(t, tag),
            other => panic!("expected Done, got {other:?}"),
        }
    }

    #[test]
    fn test_cancel_command() {
        let mut conn = Connection::new();
        let cmd = CommandBuilder::new().command("/test").build();
        let tag = conn.send_command(cmd).unwrap();
        while conn.poll_transmit().is_some() {}

        conn.cancel_command(tag).unwrap();

        // Should have a cancel transmit queued
        assert!(conn.has_pending_transmit());
        let cancel_transmit = conn.poll_transmit().unwrap();
        assert!(!cancel_transmit.data.is_empty());

        // Command should no longer be in-flight
        assert_eq!(conn.in_flight_count(), 0);
    }

    #[test]
    fn test_cancel_all() {
        let mut conn = Connection::new();
        let cmd1 = CommandBuilder::new().command("/test1").build();
        let cmd2 = CommandBuilder::new().command("/test2").build();
        conn.send_command(cmd1).unwrap();
        conn.send_command(cmd2).unwrap();
        while conn.poll_transmit().is_some() {}

        conn.cancel_all();

        // Should have 2 cancel transmits queued
        let mut cancel_count = 0;
        while conn.poll_transmit().is_some() {
            cancel_count += 1;
        }
        assert_eq!(cancel_count, 2);
        assert_eq!(conn.in_flight_count(), 0);
    }

    #[test]
    fn test_multiple_sentences_in_single_receive() {
        let mut conn = Connection::new();
        let cmd1 = CommandBuilder::new().command("/test1").build();
        let cmd2 = CommandBuilder::new().command("/test2").build();
        let tag1 = conn.send_command(cmd1).unwrap();
        let tag2 = conn.send_command(cmd2).unwrap();
        while conn.poll_transmit().is_some() {}

        // Concatenate both done responses into a single byte buffer
        let mut combined = build_done(tag1);
        combined.extend_from_slice(&build_done(tag2));

        conn.receive(&combined).unwrap();

        // Should get both events
        match conn.poll_event().unwrap() {
            Event::Done { tag } => assert_eq!(tag, tag1),
            other => panic!("expected Done for tag1, got {other:?}"),
        }
        match conn.poll_event().unwrap() {
            Event::Done { tag } => assert_eq!(tag, tag2),
            other => panic!("expected Done for tag2, got {other:?}"),
        }
        assert_eq!(conn.in_flight_count(), 0);
    }

    #[test]
    fn test_reply_for_unknown_tag_is_ignored() {
        let mut conn = Connection::new();
        let unknown_tag = Tag::new();

        // Receive a reply for a tag we never sent
        conn.receive(&build_reply(unknown_tag, &[("name", "test")]))
            .unwrap();

        // No event should be emitted
        assert!(conn.poll_event().is_none());
    }

    #[test]
    fn test_connection_starts_active() {
        let conn = Connection::new();
        assert_eq!(conn.state(), State::Active);
        assert!(conn.is_active());
        assert_eq!(conn.in_flight_count(), 0);
        assert!(!conn.has_pending_transmit());
        assert_eq!(conn.recv_buffer_len(), 0);
    }
}
