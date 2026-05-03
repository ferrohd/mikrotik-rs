//! Typestate-enforced login handshake.
//!
//! The MikroTik RouterOS API requires a `/login` command to be sent and
//! acknowledged before any other commands can be issued. This module
//! enforces this requirement at compile time using the typestate pattern:
//!
//! ```text
//! Handshaking ──[login success]──▶ Authenticated
//!     │                                │
//!     │ .receive(&bytes)               │ .connection() -> &mut Connection
//!     │ .poll_transmit()               │ .send_command(...)
//!     │ .advance() -> LoginProgress    │ .receive(&bytes)
//!     │                                │ ...
//! ```
//!
//! You cannot call `send_command` on a `Handshaking` — the method doesn't
//! exist. The only way to get an `Authenticated` connection is by successfully
//! completing the login handshake.

use crate::command::CommandBuilder;
use crate::connection::{Connection, Event, State, Transmit};
use crate::error::{ConnectionError, LoginError};
use crate::tag::Tag;

/// A connection that has not yet authenticated.
///
/// The login command is queued during construction. The caller must:
/// 1. Call [`poll_transmit()`](Handshaking::poll_transmit) and send the bytes.
/// 2. Feed response bytes via [`receive()`](Handshaking::receive).
/// 3. Call [`advance()`](Handshaking::advance) to check for completion.
pub struct Handshaking {
    inner: Connection,
    login_tag: Tag,
}

/// A fully authenticated connection, ready for commands.
///
/// This type guarantees that the login handshake has completed successfully.
/// Use [`connection()`](Authenticated::connection) to access the underlying
/// [`Connection`] for sending commands and processing events.
pub struct Authenticated {
    inner: Connection,
}

/// Result of checking login progress.
pub enum LoginProgress {
    /// Still waiting for the login response. Feed more data and try again.
    Pending(Handshaking),
    /// Login completed successfully. The connection is ready for commands.
    Complete(Authenticated),
}

impl core::fmt::Debug for LoginProgress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LoginProgress::Pending(_) => f.debug_tuple("Pending").field(&"...").finish(),
            LoginProgress::Complete(_) => f.debug_tuple("Complete").field(&"...").finish(),
        }
    }
}

impl Handshaking {
    /// Create a new connection and initiate the login sequence.
    ///
    /// The login command is immediately queued for transmission.
    /// Call [`poll_transmit()`](Self::poll_transmit) to get the bytes to send.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectionError`] if the internal connection could not
    /// accept the login command.
    pub fn new(username: &str, password: Option<&str>) -> Result<Self, ConnectionError> {
        let mut conn = Connection::new();
        let login_cmd = CommandBuilder::login(username, password);
        let tag = conn.send_command(login_cmd)?;

        Ok(Handshaking {
            inner: conn,
            login_tag: tag,
        })
    }

    /// Feed received bytes from the transport.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectionError`] if the data is malformed or the connection is dead.
    pub fn receive(&mut self, data: &[u8]) -> Result<(), ConnectionError> {
        self.inner.receive(data)
    }

    /// Get the next chunk of data to transmit.
    pub fn poll_transmit(&mut self) -> Option<Transmit> {
        self.inner.poll_transmit()
    }

    /// Check if authentication has completed.
    ///
    /// **Consumes `self`** — returns either `Pending(self)` to continue
    /// waiting or `Complete(Authenticated)` on success.
    ///
    /// # Errors
    ///
    /// Returns [`LoginError`] if:
    /// - The router rejected the credentials (`LoginError::Authentication`)
    /// - A fatal error occurred (`LoginError::Fatal`)
    pub fn advance(mut self) -> Result<LoginProgress, LoginError> {
        while let Some(event) = self.inner.poll_event() {
            match event {
                Event::Done { tag } if tag == self.login_tag => {
                    return Ok(LoginProgress::Complete(Authenticated { inner: self.inner }));
                }
                Event::Trap { tag, response } if tag == self.login_tag => {
                    return Err(LoginError::Authentication(response));
                }
                Event::Fatal { reason } => {
                    return Err(LoginError::Fatal(reason));
                }
                _ => {
                    // Unexpected event during login — ignore
                }
            }
        }

        Ok(LoginProgress::Pending(self))
    }

    /// Get a reference to the underlying connection state (read-only introspection).
    pub fn state(&self) -> State {
        self.inner.state()
    }

    /// The login command's tag.
    pub fn login_tag(&self) -> Tag {
        self.login_tag
    }
}

impl Authenticated {
    /// Get a mutable reference to the underlying connection.
    ///
    /// Use this to send commands, receive data, and poll for events.
    pub fn connection(&mut self) -> &mut Connection {
        &mut self.inner
    }

    /// Consume this handle and return the inner connection.
    pub fn into_connection(self) -> Connection {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec;
    use alloc::format;
    use alloc::vec::Vec;

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

    fn build_trap(tag: Tag, message: &str) -> Vec<u8> {
        let tag_word = format!(".tag={tag}");
        let msg_word = format!("=message={message}");
        build_sentence(&[b"!trap", tag_word.as_bytes(), msg_word.as_bytes()])
    }

    #[test]
    fn test_successful_login() {
        let mut hs = Handshaking::new("admin", Some("password")).unwrap();

        // Must have pending transmit (the login command)
        let transmit = hs.poll_transmit().unwrap();
        assert!(!transmit.data.is_empty());
        assert!(hs.poll_transmit().is_none()); // only one command

        // Simulate login success
        let done_wire = build_done(hs.login_tag());
        hs.receive(&done_wire).unwrap();

        match hs.advance().unwrap() {
            LoginProgress::Complete(auth) => {
                let conn = auth.into_connection();
                assert!(conn.is_active());
                assert_eq!(conn.in_flight_count(), 0);
            }
            LoginProgress::Pending(_) => panic!("expected login to complete"),
        }
    }

    #[test]
    fn test_failed_login() {
        let mut hs = Handshaking::new("admin", Some("wrong")).unwrap();
        while hs.poll_transmit().is_some() {}

        let trap_wire = build_trap(hs.login_tag(), "invalid user name or password");
        hs.receive(&trap_wire).unwrap();

        match hs.advance() {
            Err(LoginError::Authentication(trap)) => {
                assert_eq!(trap.message, "invalid user name or password");
            }
            other => panic!("expected Authentication error, got {other:?}"),
        }
    }

    #[test]
    fn test_fatal_during_login() {
        let mut hs = Handshaking::new("admin", Some("pass")).unwrap();
        while hs.poll_transmit().is_some() {}

        let fatal_wire = build_sentence(&[b"!fatal", b"connection limit reached"]);
        hs.receive(&fatal_wire).unwrap();

        match hs.advance() {
            Err(LoginError::Fatal(reason)) => {
                assert_eq!(reason, "connection limit reached");
            }
            other => panic!("expected Fatal error, got {other:?}"),
        }
    }

    #[test]
    fn test_partial_login_response() {
        let mut hs = Handshaking::new("admin", Some("pass")).unwrap();
        while hs.poll_transmit().is_some() {}

        let done_wire = build_done(hs.login_tag());

        // Feed only the first half
        let mid = done_wire.len() / 2;
        hs.receive(&done_wire[..mid]).unwrap();

        // Should still be pending
        let hs = match hs.advance().unwrap() {
            LoginProgress::Pending(hs) => hs,
            LoginProgress::Complete(_) => panic!("should still be pending"),
        };

        // Feed the rest — but we consumed self, need to re-bind
        let mut hs = hs;
        hs.receive(&done_wire[mid..]).unwrap();

        match hs.advance().unwrap() {
            LoginProgress::Complete(_) => {} // success
            LoginProgress::Pending(_) => panic!("should have completed"),
        }
    }
}
