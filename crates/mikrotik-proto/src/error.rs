//! Error types for the MikroTik protocol implementation.
//!
//! This module provides a unified error hierarchy covering all levels of
//! protocol processing: wire-format decoding, word parsing, sentence parsing,
//! response parsing, connection state, and login.

use core::{fmt, num::ParseIntError};

use thiserror::Error;

use crate::response::TrapResponse;
use crate::word::Word;

// ── Wire-format codec errors ──

/// Errors from the wire-format codec (length prefix decoding).
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// An invalid length prefix byte was encountered.
    #[error("invalid length prefix byte: 0x{0:02x}")]
    InvalidLengthPrefix(u8),
}

// ── Sentence-level errors ──

/// Errors that can occur while processing a byte sequence into words within a sentence.
#[derive(Error, Debug, PartialEq, Clone)]
pub enum SentenceError {
    /// A sequence of bytes could not be parsed into a valid [`Word`].
    #[error("Word error: {0}")]
    WordError(#[from] crate::word::WordError),
    /// The prefix length of a sentence is incorrect or corrupt.
    #[error("Invalid prefix length")]
    PrefixLength,
}

// ── Protocol-level response parsing errors ──

/// Errors that can occur while parsing a [`CommandResponse`](crate::response::CommandResponse)
/// from a decoded sentence.
#[derive(Error, Debug, Clone)]
pub enum ProtocolError {
    /// Error within the sentence structure (word parsing or length prefix).
    #[error("Sentence error: {0}")]
    Sentence(#[from] SentenceError),
    /// The response is missing required words to be valid.
    #[error("Incomplete response: {0}")]
    Incomplete(#[from] MissingWord),
    /// An unexpected word type was encountered in the response sequence.
    #[error("Unexpected word type: found {word:?}, expected one of {expected:?}")]
    WordSequence {
        /// The unexpected [`WordType`] that was encountered.
        word: WordType,
        /// The expected [`WordType`] variants.
        expected: alloc::vec::Vec<WordType>,
    },
    /// Error parsing or identifying a trap response category.
    #[error("Trap category error: {0}")]
    TrapCategory(#[from] TrapCategoryError),
}

/// Types of words that can be missing from a response.
#[derive(Error, Debug, Clone, Copy)]
pub enum MissingWord {
    /// Missing `.tag` — all tagged responses must have a tag.
    #[error("missing tag")]
    Tag,
    /// Missing category (`!done`, `!re`, `!trap`, `!fatal`, `!empty`).
    #[error("missing category")]
    Category,
    /// Missing message in a fatal response.
    #[error("missing message")]
    Message,
}

/// Discriminant for word types, used in error reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordType {
    /// Tag word (`.tag=...`).
    Tag,
    /// Category word (`!done`, `!re`, etc.).
    Category,
    /// Attribute word (`=key=value`).
    Attribute,
    /// Message word (free-form text).
    Message,
}

impl fmt::Display for WordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WordType::Tag => write!(f, "tag"),
            WordType::Category => write!(f, "category"),
            WordType::Attribute => write!(f, "attribute"),
            WordType::Message => write!(f, "message"),
        }
    }
}

impl From<Word<'_>> for WordType {
    fn from(word: Word) -> Self {
        match word {
            Word::Tag(_) => WordType::Tag,
            Word::Category(_) => WordType::Category,
            Word::Attribute(_) => WordType::Attribute,
            Word::Message(_) => WordType::Message,
        }
    }
}

// ── Trap category errors ──

/// Errors that can occur while parsing trap categories in response sentences.
#[derive(Error, Debug, Clone)]
pub enum TrapCategoryError {
    /// An invalid numeric value was encountered while parsing a trap category.
    #[error("Invalid trap category value: {0}")]
    Invalid(#[source] ParseIntError),
    /// The trap category number is out of the valid range (0-7).
    #[error("Trap category out of range: {0} (valid range: 0-7)")]
    OutOfRange(u8),
    /// An unexpected attribute was found in a trap response.
    #[error("Invalid trap attribute: key={key}, value={value:?}")]
    InvalidAttribute {
        /// The key of the invalid attribute.
        key: alloc::string::String,
        /// The value of the invalid attribute, if present.
        value: Option<alloc::string::String>,
    },
    /// The required `message` attribute is missing from a trap response.
    #[error("Missing message attribute in trap response")]
    MissingMessageAttribute,
}

// ── Connection state machine errors ──

/// Errors from the [`Connection`](crate::connection::Connection) state machine.
#[derive(Error, Debug, Clone)]
pub enum ConnectionError {
    /// A wire-format decoding error occurred.
    #[error("decode error: {0}")]
    Decode(#[from] DecodeError),
    /// A protocol-level parsing error occurred.
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    /// The connection has been fatally shut down and cannot accept new operations.
    #[error("connection is closed")]
    Closed,
}

// ── Login handshake errors ──

/// Errors from the login handshake process.
#[derive(Error, Debug, Clone)]
pub enum LoginError {
    /// The router rejected the login credentials.
    #[error("authentication failed: {0}")]
    Authentication(TrapResponse),
    /// A fatal error occurred during login.
    #[error("fatal error during login: {0}")]
    Fatal(alloc::string::String),
    /// A protocol error occurred during login.
    #[error("protocol error during login: {0}")]
    Protocol(#[from] ProtocolError),
    /// A connection error occurred during login.
    #[error("connection error during login: {0}")]
    Connection(#[from] ConnectionError),
}
