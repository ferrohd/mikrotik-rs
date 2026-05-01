//! Command builder with typestate pattern and compile-time validation.
//!
//! Commands are built using the [`CommandBuilder`] which uses the typestate
//! pattern to enforce at compile time that a command word is set before
//! attributes can be added.
//!
//! # Examples
//!
//! ```rust,ignore
//! let cmd = CommandBuilder::new()
//!     .command("/system/resource/print")
//!     .attribute("detail", None)
//!     .build();
//! ```

use alloc::vec::Vec;
use core::marker::PhantomData;

use uuid::Uuid;

use crate::codec;

/// Marker type: no command word has been set yet.
pub struct NoCmd;

/// Marker type: a command word has been set, attributes can be added.
#[derive(Clone)]
pub struct Cmd;

/// Builds MikroTik router commands using a fluent typestate API.
///
/// The type parameter ensures that only commands with a command word set
/// can have attributes added or be built.
///
/// # Type states
///
/// - `CommandBuilder<NoCmd>` — initial state, call `.command()` to transition
/// - `CommandBuilder<Cmd>` — command word set, can add attributes and `.build()`
#[derive(Clone)]
pub struct CommandBuilder<State> {
    tag: Uuid,
    buf: Vec<u8>,
    state: PhantomData<State>,
}

impl Default for CommandBuilder<NoCmd> {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandBuilder<NoCmd> {
    /// Begin building a new [`Command`] with a randomly generated tag.
    pub fn new() -> Self {
        Self {
            tag: Uuid::new_v4(),
            buf: Vec::new(),
            state: PhantomData,
        }
    }

    /// Begin building a new [`Command`] with a specified tag.
    ///
    /// # Arguments
    ///
    /// * `tag` — A [`Uuid`] that identifies the command for RouterOS correlation.
    ///   **Must be unique** within a connection.
    pub fn with_tag(tag: Uuid) -> Self {
        Self {
            tag,
            buf: Vec::new(),
            state: PhantomData,
        }
    }

    /// Builds a login command with the provided username and optional password.
    pub fn login(username: &str, password: Option<&str>) -> Command {
        Self::new()
            .command("/login")
            .attribute("name", Some(username))
            .attribute("password", password)
            .build()
    }

    /// Builds a command to cancel a specific running command identified by `tag`.
    pub fn cancel(tag: Uuid) -> Command {
        // Use the same tag so the cancel is correlated
        Self::with_tag(tag)
            .command("/cancel")
            .attribute_tag("tag", tag)
            .build()
    }

    /// Specify the command path to be executed, transitioning to the `Cmd` state.
    ///
    /// # Arguments
    ///
    /// * `command` — The MikroTik command path (e.g., `/system/resource/print`).
    pub fn command(self, command: &str) -> CommandBuilder<Cmd> {
        let Self { tag, mut buf, .. } = self;

        // Write the command word
        codec::encode_word(command.as_bytes(), &mut buf);

        // Write the tag word — avoid format!() allocation by building directly
        // ".tag=" (5 bytes) + UUID hyphenated (36 bytes) = 41 bytes
        let mut tag_buf = [0u8; 41];
        tag_buf[..5].copy_from_slice(b".tag=");
        // Uuid::as_hyphenated() produces a 36-char lowercase hex string
        tag.as_hyphenated().encode_lower(&mut tag_buf[5..]);
        codec::encode_word(&tag_buf, &mut buf);

        CommandBuilder {
            tag,
            buf,
            state: PhantomData,
        }
    }
}

impl CommandBuilder<Cmd> {
    /// Adds an attribute to the command being built.
    ///
    /// # Arguments
    ///
    /// * `key` — The attribute's key.
    /// * `value` — The attribute's value. If `None`, the attribute is treated
    ///   as a flag (e.g., `=key=`).
    pub fn attribute(self, key: &str, value: Option<&str>) -> Self {
        let Self { tag, mut buf, .. } = self;

        // Build the attribute word directly: "=" + key + "=" + value
        // Avoid format!() allocation
        let value_bytes = value.unwrap_or("");
        let word_len = 1 + key.len() + 1 + value_bytes.len();
        codec::encode_length(word_len as u32, &mut buf);
        buf.push(b'=');
        buf.extend_from_slice(key.as_bytes());
        buf.push(b'=');
        buf.extend_from_slice(value_bytes.as_bytes());

        CommandBuilder {
            tag,
            buf,
            state: PhantomData,
        }
    }

    /// Adds an attribute with a raw byte value to the command being built.
    ///
    /// Use this method when your attribute values might contain non-UTF-8 or binary data.
    pub fn attribute_raw(self, key: &str, value: Option<&[u8]>) -> Self {
        let Self { tag, mut buf, .. } = self;

        let value_bytes = value.unwrap_or(&[]);
        let word_len = 1 + key.len() + 1 + value_bytes.len();
        codec::encode_length(word_len as u32, &mut buf);
        buf.push(b'=');
        buf.extend_from_slice(key.as_bytes());
        buf.push(b'=');
        buf.extend_from_slice(value_bytes);

        CommandBuilder {
            tag,
            buf,
            state: PhantomData,
        }
    }

    /// Adds an attribute with a UUID value (used internally for cancel tags).
    fn attribute_tag(self, key: &str, value: Uuid) -> Self {
        let mut tag_str = [0u8; 36];
        value.as_hyphenated().encode_lower(&mut tag_str);
        let tag_str = core::str::from_utf8(&tag_str).expect("UUID is valid UTF-8");
        self.attribute(key, Some(tag_str))
    }

    /// Adds a query to check if a property is present.
    ///
    /// Pushes `true` if an item has a value for the property, `false` if it does not.
    pub fn query_is_present(self, name: &str) -> Self {
        let Self { tag, mut buf, .. } = self;
        let word_len = 1 + name.len(); // "?" + name
        codec::encode_length(word_len as u32, &mut buf);
        buf.push(b'?');
        buf.extend_from_slice(name.as_bytes());
        CommandBuilder {
            tag,
            buf,
            state: PhantomData,
        }
    }

    /// Adds a query to check if a property is absent.
    pub fn query_not_present(self, name: &str) -> Self {
        let Self { tag, mut buf, .. } = self;
        let word_len = 2 + name.len(); // "?-" + name
        codec::encode_length(word_len as u32, &mut buf);
        buf.extend_from_slice(b"?-");
        buf.extend_from_slice(name.as_bytes());
        CommandBuilder {
            tag,
            buf,
            state: PhantomData,
        }
    }

    /// Adds a query to check if a property equals a value.
    pub fn query_equal(self, name: &str, value: &str) -> Self {
        let Self { tag, mut buf, .. } = self;
        let word_len = 1 + name.len() + 1 + value.len(); // "?" + name + "=" + value
        codec::encode_length(word_len as u32, &mut buf);
        buf.push(b'?');
        buf.extend_from_slice(name.as_bytes());
        buf.push(b'=');
        buf.extend_from_slice(value.as_bytes());
        CommandBuilder {
            tag,
            buf,
            state: PhantomData,
        }
    }

    /// Adds a query to check if a property is greater than a value.
    pub fn query_gt(self, key: &str, value: &str) -> Self {
        let Self { tag, mut buf, .. } = self;
        let word_len = 2 + key.len() + 1 + value.len(); // "?>" + key + "=" + value
        codec::encode_length(word_len as u32, &mut buf);
        buf.extend_from_slice(b"?>");
        buf.extend_from_slice(key.as_bytes());
        buf.push(b'=');
        buf.extend_from_slice(value.as_bytes());
        CommandBuilder {
            tag,
            buf,
            state: PhantomData,
        }
    }

    /// Adds a query to check if a property is less than a value.
    pub fn query_lt(self, key: &str, value: &str) -> Self {
        let Self { tag, mut buf, .. } = self;
        let word_len = 2 + key.len() + 1 + value.len(); // "?<" + key + "=" + value
        codec::encode_length(word_len as u32, &mut buf);
        buf.extend_from_slice(b"?<");
        buf.extend_from_slice(key.as_bytes());
        buf.push(b'=');
        buf.extend_from_slice(value.as_bytes());
        CommandBuilder {
            tag,
            buf,
            state: PhantomData,
        }
    }

    /// Adds query operations (combination operators for the query stack).
    ///
    /// See [MikroTik API Queries](https://help.mikrotik.com/docs/spaces/ROS/pages/47579160/API#API-Queries).
    pub fn query_operations(self, operations: impl Iterator<Item = QueryOperator>) -> Self {
        let Self { tag, mut buf, .. } = self;

        // Collect operation chars: "?#" + operator chars
        let ops: Vec<u8> = operations.map(|op| op.code()).collect();
        let word_len = 2 + ops.len(); // "?#" + ops
        codec::encode_length(word_len as u32, &mut buf);
        buf.extend_from_slice(b"?#");
        buf.extend_from_slice(&ops);

        CommandBuilder {
            tag,
            buf,
            state: PhantomData,
        }
    }

    /// Finalizes the command construction, producing a [`Command`].
    pub fn build(self) -> Command {
        let Self { tag, mut buf, .. } = self;
        // Terminate the sentence
        codec::encode_terminator(&mut buf);
        Command { tag, data: buf }
    }
}

/// A finalized command ready to be sent to the router.
///
/// Created via [`CommandBuilder`]. The `data` field contains the complete
/// wire-format bytes (length-prefixed words + null terminator).
#[derive(Debug)]
pub struct Command {
    /// The tag identifying this command for response correlation.
    pub tag: Uuid,
    /// The wire-format encoded command data.
    pub data: Vec<u8>,
}

/// Represents a query operator for MikroTik API query expressions.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum QueryOperator {
    /// Represents the `!` (NOT) operator.
    Not,
    /// Represents the `&` (AND) operator.
    And,
    /// Represents the `|` (OR) operator.
    Or,
    /// Represents the `.` (END) operator.
    Dot,
}

impl QueryOperator {
    #[inline]
    fn code(self) -> u8 {
        match self {
            QueryOperator::Not => b'!',
            QueryOperator::And => b'&',
            QueryOperator::Or => b'|',
            QueryOperator::Dot => b'.',
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    const TEST_UUID: Uuid = Uuid::from_bytes([
        0xa1, 0xa2, 0xa3, 0xa4, 0xb1, 0xb2, 0xc1, 0xc2, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7,
        0xd8,
    ]);
    const TEST_TAG_WORD: &str = ".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8";

    /// Helper to parse the RouterOS length-prefixed "words" out of the command data.
    fn parse_words(data: &[u8]) -> Vec<String> {
        let mut words = Vec::new();
        let mut i = 0;
        while i < data.len() {
            let len = data[i] as usize;
            i += 1;
            if len == 0 {
                break;
            }
            if i + len > data.len() {
                panic!("Malformed command data: length prefix exceeds available data.");
            }
            let word = &data[i..i + len];
            i += len;
            words.push(String::from_utf8_lossy(word).into_owned());
        }
        words
    }

    #[test]
    fn test_command_builder_new() {
        let builder = CommandBuilder::<NoCmd>::new();
        assert_eq!(builder.buf.len(), 0);
        assert!(!builder.tag.is_nil());
    }

    #[test]
    fn test_command_builder_with_tag() {
        let builder = CommandBuilder::<NoCmd>::with_tag(TEST_UUID);
        assert_eq!(builder.tag, TEST_UUID);
    }

    #[test]
    fn test_command_builder_command() {
        let builder = CommandBuilder::<NoCmd>::with_tag(TEST_UUID).command("/interface/print");
        // Word 1: 1-byte len (0x10) + 16 bytes "/interface/print" = 17 bytes
        // Word 2: 1-byte len (0x29) + 41 bytes ".tag=a1a2a3a4-..." = 42 bytes
        // Total: 59 bytes
        assert_eq!(builder.buf.len(), 59);
        assert_eq!(&builder.buf[1..17], b"/interface/print");
        assert_eq!(&builder.buf[18..59], TEST_TAG_WORD.as_bytes());
    }

    #[test]
    fn test_command_builder_attribute() {
        let builder = CommandBuilder::<NoCmd>::with_tag(TEST_UUID)
            .command("/interface/print")
            .attribute("name", Some("ether1"));

        // Attribute starts at offset 59 (after command + tag words) + 1 byte len prefix = 60
        assert_eq!(&builder.buf[60..72], b"=name=ether1");
    }

    #[test]
    fn test_command_builder_login() {
        let command = CommandBuilder::<NoCmd>::login("admin", Some("password"));
        let s = core::str::from_utf8(&command.data).unwrap();
        assert!(s.contains("/login"));
        assert!(s.contains("name=admin"));
        assert!(s.contains("password=password"));
    }

    #[test]
    fn test_command_builder_cancel() {
        let command = CommandBuilder::<NoCmd>::cancel(TEST_UUID);
        let s = core::str::from_utf8(&command.data).unwrap();
        assert!(s.contains("/cancel"));
        assert!(s.contains("tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8"));
    }

    #[test]
    fn test_command_no_attributes() {
        let cmd = CommandBuilder::new()
            .command("/system/resource/print")
            .build();
        let words = parse_words(&cmd.data);

        assert_eq!(words[0], "/system/resource/print");
        assert!(words[1].starts_with(".tag="));
        assert_eq!(words.len(), 2);
    }

    #[test]
    fn test_command_with_one_attribute() {
        let cmd = CommandBuilder::new()
            .command("/interface/ethernet/print")
            .attribute("user", Some("admin"))
            .build();
        let words = parse_words(&cmd.data);

        assert_eq!(words[0], "/interface/ethernet/print");
        assert!(words[1].starts_with(".tag="));
        assert_eq!(words[2], "=user=admin");
        assert_eq!(words.len(), 3);
    }

    #[test]
    fn test_command_with_multiple_attributes() {
        let cmd = CommandBuilder::new()
            .command("/some/random")
            .attribute("attribute_no_value", None)
            .attribute("another", Some("value"))
            .build();
        let words = parse_words(&cmd.data);

        assert_eq!(words[0], "/some/random");
        assert!(words[1].starts_with(".tag="));
        assert_eq!(words[2], "=attribute_no_value=");
        assert_eq!(words[3], "=another=value");
        assert_eq!(words.len(), 4);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        // Build a command, then decode its wire format and verify the words
        let cmd = CommandBuilder::with_tag(TEST_UUID)
            .command("/interface/print")
            .attribute("name", Some("ether1"))
            .attribute("disabled", None)
            .build();

        // Decode the sentence
        match codec::decode_sentence(&cmd.data).unwrap() {
            codec::Decode::Complete { value: raw, .. } => {
                let words: Vec<_> = raw.words().collect();
                assert_eq!(words[0], b"/interface/print");
                assert_eq!(words[1], TEST_TAG_WORD.as_bytes());
                assert_eq!(words[2], b"=name=ether1");
                assert_eq!(words[3], b"=disabled=");
                assert_eq!(words.len(), 4);
            }
            _ => panic!("expected Complete"),
        }
    }
}
