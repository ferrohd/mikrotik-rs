//! Command builder with typestate pattern and compile-time validation.
//!
//! Commands are built using the [`CommandBuilder`] which uses the typestate
//! pattern to enforce at compile time that a command word is set before
//! attributes can be added.
//!
//! # Examples
//!
//! ```rust
//! use mikrotik_proto::command::CommandBuilder;
//!
//! let cmd = CommandBuilder::new()
//!     .command("/system/resource/print")
//!     .flag("detail")
//!     .build();
//!
//! // The command has wire-format data ready to send
//! assert!(!cmd.data().is_empty());
//! ```
//!
//! # Attributes and absence
//!
//! [`CommandBuilder::attribute`] takes an [`Option`], and `None` means *this
//! attribute was not supplied* — the word is left out of the sentence entirely:
//!
//! | Call | Wire |
//! |---|---|
//! | `.attribute("comment", None)` | *nothing* |
//! | `.flag("detail")` | `=detail=` |
//! | `.attribute("comment", Some("hi"))` | `=comment=hi` |
//!
//! This matters because `=comment=` tells RouterOS to set the comment to the
//! empty string, which clears it. Threading an `Option` straight from your own
//! data is therefore safe: a `None` leaves the field alone rather than wiping
//! it. Use [`CommandBuilder::flag`] for valueless flags such as `detail` or
//! `once`, which are `=key=` on the wire.

use alloc::vec::Vec;
use core::marker::PhantomData;

use crate::codec;
use crate::tag::Tag;

/// Encode the length prefix for a word that is being assembled in place.
///
/// The builder writes word bodies directly into `buf` to avoid an intermediate
/// allocation, so it computes the length itself instead of going through
/// [`codec::encode_word`]. This helper keeps the `usize` → `u32` narrowing in
/// one place.
///
/// # Panics
///
/// Panics if `word_len` exceeds [`u32::MAX`] (4 GiB), which the wire format
/// cannot represent.
fn encode_word_len(word_len: usize, buf: &mut Vec<u8>) {
    let len = u32::try_from(word_len).expect("word length exceeds u32::MAX");
    codec::encode_length(len, buf);
}

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
#[must_use]
pub struct CommandBuilder<State> {
    tag: Tag,
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
        Self { tag: Tag::new(), buf: Vec::new(), state: PhantomData }
    }

    /// Begin building a new [`Command`] with a specified tag.
    ///
    /// # Arguments
    ///
    /// * `tag` — A [`Tag`] that identifies the command for RouterOS correlation.
    ///   **Must be unique** within a connection.
    pub fn with_tag(tag: Tag) -> Self {
        Self { tag, buf: Vec::new(), state: PhantomData }
    }

    /// Builds a login command with the provided username and optional password.
    #[must_use]
    pub fn login(username: &str, password: Option<&str>) -> Command {
        Self::new()
            .command("/login")
            .attribute("name", Some(username))
            // An absent password is an *empty* password, not an absent attribute:
            // RouterOS requires the `=password=` word, so this must not become
            // `attribute("password", password)` — `None` would omit it entirely.
            .attribute("password", Some(password.unwrap_or("")))
            .build()
    }

    /// Builds a command to cancel a specific running command identified by `tag`.
    #[must_use]
    pub fn cancel(tag: Tag) -> Command {
        // Use the same tag so the cancel is correlated
        Self::with_tag(tag).command("/cancel").attribute_tag("tag", tag).build()
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
        tag.encode_lower(&mut tag_buf[5..]);
        codec::encode_word(&tag_buf, &mut buf);

        CommandBuilder { tag, buf, state: PhantomData }
    }
}

impl CommandBuilder<Cmd> {
    /// Adds an attribute to the command being built.
    ///
    /// # Arguments
    ///
    /// * `key` — The attribute's key.
    /// * `value` — The attribute's value. `None` means the attribute was not
    ///   supplied and **no word is written at all**. For a valueless flag such
    ///   as `detail`, use [`flag`](Self::flag), which writes `=detail=`.
    ///
    /// Passing `Some("")` is equivalent to [`flag`](Self::flag): RouterOS reads
    /// `=key=` as "set this property to the empty string".
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn attribute(self, key: &str, value: Option<&str>) -> Self {
        self.attribute_raw(key, value.map(str::as_bytes))
    }

    /// Adds an attribute with a raw byte value to the command being built.
    ///
    /// Use this method when your attribute values might contain non-UTF-8 or binary data.
    /// `None` omits the word, exactly as in [`attribute`](Self::attribute).
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn attribute_raw(self, key: &str, value: Option<&[u8]>) -> Self {
        let Self { tag, mut buf, .. } = self;

        if let Some(value) = value {
            let word_len = 1 + key.len() + 1 + value.len();
            encode_word_len(word_len, &mut buf);
            buf.push(b'=');
            buf.extend_from_slice(key.as_bytes());
            buf.push(b'=');
            buf.extend_from_slice(value);
        }

        CommandBuilder { tag, buf, state: PhantomData }
    }

    /// Adds a valueless flag, such as `detail` or `once`.
    ///
    /// Writes `=key=` — an attribute with an empty value, which is how RouterOS
    /// spells a flag. This is the counterpart to
    /// [`attribute(key, None)`](Self::attribute), which writes nothing.
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn flag(self, key: &str) -> Self {
        self.attribute_raw(key, Some(&[]))
    }

    /// Adds an attribute with a Tag value (used internally for cancel tags).
    fn attribute_tag(self, key: &str, value: Tag) -> Self {
        let mut tag_str = [0u8; 36];
        let s = value.encode_lower(&mut tag_str);
        self.attribute(key, Some(s))
    }

    /// Adds a query to check if a property is present.
    ///
    /// Pushes `true` if an item has a value for the property, `false` if it does not.
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn query_is_present(self, name: &str) -> Self {
        let Self { tag, mut buf, .. } = self;
        let word_len = 1 + name.len(); // "?" + name
        encode_word_len(word_len, &mut buf);
        buf.push(b'?');
        buf.extend_from_slice(name.as_bytes());
        CommandBuilder { tag, buf, state: PhantomData }
    }

    /// Adds a query to check if a property is absent.
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn query_not_present(self, name: &str) -> Self {
        let Self { tag, mut buf, .. } = self;
        let word_len = 2 + name.len(); // "?-" + name
        encode_word_len(word_len, &mut buf);
        buf.extend_from_slice(b"?-");
        buf.extend_from_slice(name.as_bytes());
        CommandBuilder { tag, buf, state: PhantomData }
    }

    /// Adds a query to check if a property equals a value.
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn query_equal(self, name: &str, value: &str) -> Self {
        let Self { tag, mut buf, .. } = self;
        let word_len = 1 + name.len() + 1 + value.len(); // "?" + name + "=" + value
        encode_word_len(word_len, &mut buf);
        buf.push(b'?');
        buf.extend_from_slice(name.as_bytes());
        buf.push(b'=');
        buf.extend_from_slice(value.as_bytes());
        CommandBuilder { tag, buf, state: PhantomData }
    }

    /// Adds a query to check if a property is greater than a value.
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn query_gt(self, key: &str, value: &str) -> Self {
        let Self { tag, mut buf, .. } = self;
        let word_len = 2 + key.len() + 1 + value.len(); // "?>" + key + "=" + value
        encode_word_len(word_len, &mut buf);
        buf.extend_from_slice(b"?>");
        buf.extend_from_slice(key.as_bytes());
        buf.push(b'=');
        buf.extend_from_slice(value.as_bytes());
        CommandBuilder { tag, buf, state: PhantomData }
    }

    /// Adds a query to check if a property is less than a value.
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn query_lt(self, key: &str, value: &str) -> Self {
        let Self { tag, mut buf, .. } = self;
        let word_len = 2 + key.len() + 1 + value.len(); // "?<" + key + "=" + value
        encode_word_len(word_len, &mut buf);
        buf.extend_from_slice(b"?<");
        buf.extend_from_slice(key.as_bytes());
        buf.push(b'=');
        buf.extend_from_slice(value.as_bytes());
        CommandBuilder { tag, buf, state: PhantomData }
    }

    /// Adds query operations (combination operators for the query stack).
    ///
    /// See [MikroTik API Queries](https://help.mikrotik.com/docs/spaces/ROS/pages/47579160/API#API-Queries).
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn query_operations(self, operations: impl Iterator<Item = QueryOperator>) -> Self {
        let Self { tag, mut buf, .. } = self;

        // Collect operation chars: "?#" + operator chars
        let ops: Vec<u8> = operations.map(QueryOperator::code).collect();
        let word_len = 2 + ops.len(); // "?#" + ops
        encode_word_len(word_len, &mut buf);
        buf.extend_from_slice(b"?#");
        buf.extend_from_slice(&ops);

        CommandBuilder { tag, buf, state: PhantomData }
    }

    /// Finalizes the command construction, producing a [`Command`].
    #[must_use]
    pub fn build(self) -> Command {
        let Self { tag, mut buf, .. } = self;
        // Terminate the sentence
        codec::encode_terminator(&mut buf);
        Command { tag, data: buf }
    }
}

/// A finalized command ready to be sent to the router.
///
/// Created via [`CommandBuilder`]. Contains the complete wire-format bytes
/// (length-prefixed words + null terminator).
///
/// The wire data is immutable after construction to preserve builder invariants.
#[derive(Debug, Clone)]
pub struct Command {
    /// The tag identifying this command for response correlation.
    pub tag: Tag,
    /// The wire-format encoded command data.
    pub(crate) data: Vec<u8>,
}

impl Command {
    /// Returns the wire-format encoded command data.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Consumes the command and returns the wire-format data.
    #[must_use]
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
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
    use uuid::Uuid;

    use super::*;
    use alloc::string::String;

    const TEST_TAG: Tag = Tag::from_uuid(Uuid::from_bytes([
        0xa1, 0xa2, 0xa3, 0xa4, 0xb1, 0xb2, 0xc1, 0xc2, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7,
        0xd8,
    ]));
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
            assert!(
                i + len <= data.len(),
                "Malformed command data: length prefix exceeds available data."
            );
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
        // Tag should be a random non-zero UUID
        let a = builder.tag;
        let b = CommandBuilder::<NoCmd>::new().tag;
        assert_ne!(a, b);
    }

    #[test]
    fn test_command_builder_with_tag() {
        let builder = CommandBuilder::<NoCmd>::with_tag(TEST_TAG);
        assert_eq!(builder.tag, TEST_TAG);
    }

    #[test]
    fn test_command_builder_command() {
        let builder = CommandBuilder::<NoCmd>::with_tag(TEST_TAG).command("/interface/print");
        // Word 1: 1-byte len (0x10) + 16 bytes "/interface/print" = 17 bytes
        // Word 2: 1-byte len (0x29) + 41 bytes ".tag=a1a2a3a4-..." = 42 bytes
        // Total: 59 bytes
        assert_eq!(builder.buf.len(), 59);
        assert_eq!(&builder.buf[1..17], b"/interface/print");
        assert_eq!(&builder.buf[18..59], TEST_TAG_WORD.as_bytes());
    }

    #[test]
    fn test_command_builder_attribute() {
        let builder = CommandBuilder::<NoCmd>::with_tag(TEST_TAG)
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
        let command = CommandBuilder::<NoCmd>::cancel(TEST_TAG);
        let s = core::str::from_utf8(&command.data).unwrap();
        assert!(s.contains("/cancel"));
        assert!(s.contains("tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8"));
    }

    #[test]
    fn test_command_no_attributes() {
        let cmd = CommandBuilder::new().command("/system/resource/print").build();
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
            .flag("attribute_no_value")
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
    fn test_attribute_none_writes_no_word() {
        // `=comment=` tells RouterOS to set the comment to the empty string, which
        // clears it. `None` has to mean "not supplied", or threading an Option
        // straight from caller data silently wipes fields.
        let cmd = CommandBuilder::new()
            .command("/interface/set")
            .attribute("comment", None)
            .attribute("name", Some("ether1"))
            .build();
        let words = parse_words(&cmd.data);

        assert_eq!(words[0], "/interface/set");
        assert!(words[1].starts_with(".tag="));
        assert_eq!(words[2], "=name=ether1");
        assert_eq!(words.len(), 3, "the None attribute must not appear at all");
    }

    #[test]
    fn test_flag_and_empty_value_agree() {
        let flagged = CommandBuilder::with_tag(TEST_TAG).command("/x").flag("detail").build();
        let empty =
            CommandBuilder::with_tag(TEST_TAG).command("/x").attribute("detail", Some("")).build();

        assert_eq!(flagged.data, empty.data);
        assert_eq!(parse_words(&flagged.data)[2], "=detail=");
    }

    #[test]
    fn test_attribute_raw_none_writes_no_word() {
        let cmd =
            CommandBuilder::with_tag(TEST_TAG).command("/x").attribute_raw("blob", None).build();
        assert_eq!(parse_words(&cmd.data).len(), 2);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        // Build a command, then decode its wire format and verify the words
        let cmd = CommandBuilder::with_tag(TEST_TAG)
            .command("/interface/print")
            .attribute("name", Some("ether1"))
            .flag("disabled")
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
            codec::Decode::Incomplete { .. } => panic!("expected Complete"),
        }
    }
}
