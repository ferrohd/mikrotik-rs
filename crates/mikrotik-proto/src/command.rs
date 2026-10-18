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

use crate::sentence::{OutWord, Sentence};
use crate::tag::Tag;

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
    sentence: Sentence,
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
        Self::with_tag(Tag::new())
    }

    /// Begin building a new [`Command`] with a specified tag.
    ///
    /// # Arguments
    ///
    /// * `tag` — A [`Tag`] that identifies the command for RouterOS correlation.
    ///   **Must be unique** within a connection.
    pub fn with_tag(tag: Tag) -> Self {
        Self { tag, sentence: Sentence::default(), state: PhantomData }
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
        let Self { tag, mut sentence, .. } = self;
        sentence.word(OutWord::Command(command));
        sentence.word(OutWord::Tag(tag));
        CommandBuilder { tag, sentence, state: PhantomData }
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
    pub fn attribute_raw(mut self, key: &str, value: Option<&[u8]>) -> Self {
        if let Some(value) = value {
            self.sentence.word(OutWord::Attribute { key, value });
        }
        self
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
        let mut buf = [0u8; 36];
        let ascii = value.encode_lower(&mut buf);
        self.attribute_raw(key, Some(ascii.as_bytes()))
    }

    /// Adds a query to check if a property is present.
    ///
    /// Pushes `true` if an item has a value for the property, `false` if it does not.
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn query_is_present(mut self, name: &str) -> Self {
        self.sentence.word(OutWord::Present(name));
        self
    }

    /// Adds a query to check if a property is absent.
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn query_not_present(mut self, name: &str) -> Self {
        self.sentence.word(OutWord::Absent(name));
        self
    }

    /// Adds a query to check if a property equals a value.
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn query_equal(mut self, name: &str, value: &str) -> Self {
        self.sentence.word(OutWord::Equal { key: name, value });
        self
    }

    /// Adds a query to check if a property is greater than a value.
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn query_gt(mut self, key: &str, value: &str) -> Self {
        self.sentence.word(OutWord::Greater { key, value });
        self
    }

    /// Adds a query to check if a property is less than a value.
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn query_lt(mut self, key: &str, value: &str) -> Self {
        self.sentence.word(OutWord::Less { key, value });
        self
    }

    /// Adds query operations (combination operators for the query stack).
    ///
    /// See [MikroTik API Queries](https://help.mikrotik.com/docs/spaces/ROS/pages/47579160/API#API-Queries).
    ///
    /// # Panics
    ///
    /// Panics if the encoded word exceeds [`u32::MAX`] bytes.
    pub fn query_operations(mut self, operations: &[QueryOperator]) -> Self {
        self.sentence.word(OutWord::Operations(operations));
        self
    }

    /// Finalizes the command construction, producing a [`Command`].
    #[must_use]
    pub fn build(self) -> Command {
        let Self { tag, sentence, .. } = self;
        Command { tag, data: sentence.finish() }
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
    /// The operator's byte as it appears after `?#` on the wire.
    #[inline]
    pub(crate) fn code(self) -> u8 {
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
    use crate::codec;
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
    fn test_command_builder_new_tags_are_distinct() {
        let a = CommandBuilder::<NoCmd>::new().tag;
        let b = CommandBuilder::<NoCmd>::new().tag;
        assert_ne!(a, b, "each builder must take a fresh tag");
    }

    #[test]
    fn test_command_builder_with_tag() {
        let builder = CommandBuilder::<NoCmd>::with_tag(TEST_TAG);
        assert_eq!(builder.tag, TEST_TAG);
    }

    #[test]
    fn test_command_builder_command() {
        let cmd = CommandBuilder::<NoCmd>::with_tag(TEST_TAG).command("/interface/print").build();
        let words = parse_words(&cmd.data);

        assert_eq!(words[0], "/interface/print");
        assert_eq!(words[1], TEST_TAG_WORD);
        assert_eq!(words.len(), 2);
    }

    #[test]
    fn test_command_builder_attribute() {
        let cmd = CommandBuilder::<NoCmd>::with_tag(TEST_TAG)
            .command("/interface/print")
            .attribute("name", Some("ether1"))
            .build();
        let words = parse_words(&cmd.data);

        assert_eq!(words[2], "=name=ether1");
        assert_eq!(words.len(), 3);
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
