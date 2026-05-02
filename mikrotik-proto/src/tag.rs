//! Command tag — a unique identifier for correlating commands with responses.
//!
//! Each command sent to a `MikroTik` router is assigned a [`Tag`] that the
//! router echoes back in all response sentences (`.tag=<value>`). This allows
//! multiplexing multiple in-flight commands over a single connection.
//!
//! `Tag` is a newtype wrapper around [`Uuid`] that provides type safety —
//! you cannot accidentally pass a random `Uuid` where a command tag is expected.

use core::fmt;

use uuid::Uuid;

/// A unique tag identifying a command for response correlation.
///
/// Tags are generated automatically by [`CommandBuilder`](crate::command::CommandBuilder)
/// using UUID v4, or can be created explicitly via [`Tag::new`] or [`Tag::from_uuid`].
///
/// The router echoes the tag in every response sentence belonging to the command,
/// allowing the [`Connection`](crate::connection::Connection) to demultiplex responses.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tag(Uuid);

impl Tag {
    /// Generate a new random tag (UUID v4).
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a tag from an existing [`Uuid`] in a const context.
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Encode the tag as a hyphenated lowercase string into the provided buffer.
    ///
    /// The buffer must be at least 36 bytes. Returns the written slice.
    pub fn encode_lower<'a>(&self, buf: &'a mut [u8]) -> &'a str {
        self.0.as_hyphenated().encode_lower(buf)
    }
}

impl Default for Tag {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tag({})", self.0.as_hyphenated())
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.as_hyphenated())
    }
}

impl From<Uuid> for Tag {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<Tag> for Uuid {
    fn from(tag: Tag) -> Self {
        tag.0
    }
}

/// Parse a tag from a hyphenated UUID string (e.g., from `.tag=...` wire data).
impl core::str::FromStr for Tag {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<Uuid>().map(Self)
    }
}
