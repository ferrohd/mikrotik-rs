//! Word types: the fundamental unit of the MikroTik API protocol.
//!
//! Every MikroTik API sentence is composed of *words*. Each word is a
//! length-prefixed byte sequence that falls into one of four categories:
//!
//! - **Category** — response type identifier (`!done`, `!re`, `!trap`, `!fatal`, `!empty`)
//! - **Tag** — command correlation tag (`.tag=<uuid>`)
//! - **Attribute** — key-value pair (`=key=value`)
//! - **Message** — free-form text (used for `!fatal` reasons)

use core::{
    fmt::{self, Display, Formatter},
    str::Utf8Error,
};

use thiserror::Error;

use crate::error::WordType;
use crate::tag::Tag;

/// Represents a word in a `MikroTik` API sentence.
///
/// Words are the fundamental unit of the `MikroTik` wire protocol.
/// This type borrows from the underlying byte buffer for zero-copy parsing.
///
/// # Variants
///
/// - `Category` — response type (`!done`, `!re`, `!trap`, `!fatal`, `!empty`)
/// - `Tag` — command correlation UUID (`.tag=<uuid>`)
/// - `Attribute` — key-value pair (`=key=value`)
/// - `Message` — free-form text (typically a `!fatal` reason)
#[derive(Debug, PartialEq)]
pub enum Word<'a> {
    /// A category word, such as `!done`, `!re`, `!trap`, `!fatal`, or `!empty`.
    Category(WordCategory),
    /// A tag word, such as `.tag=550e8400-e29b-41d4-a716-446655440000`.
    Tag(Tag),
    /// An attribute word, such as `=name=ether1`.
    Attribute(WordAttribute<'a>),
    /// An unrecognized word. Usually this is a `!fatal` reason message.
    Message(&'a str),
}

impl Word<'_> {
    /// Returns the category of the word, if it is a category word.
    pub fn category(&self) -> Option<&WordCategory> {
        match self {
            Word::Category(category) => Some(category),
            _ => None,
        }
    }

    /// Returns the tag of the word, if it is a tag word.
    pub fn tag(&self) -> Option<Tag> {
        match self {
            Word::Tag(tag) => Some(*tag),
            _ => None,
        }
    }

    /// Returns the generic message, if it is a message word.
    /// This is usually a `!fatal` reason message.
    pub fn generic(&self) -> Option<&str> {
        match self {
            Word::Message(generic) => Some(generic),
            _ => None,
        }
    }

    /// Returns the type discriminant of this word.
    pub fn word_type(&self) -> WordType {
        match self {
            Word::Category(_) => WordType::Category,
            Word::Tag(_) => WordType::Tag,
            Word::Attribute(_) => WordType::Attribute,
            Word::Message(_) => WordType::Message,
        }
    }
}

impl Display for Word<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Word::Category(category) => write!(f, "{category}"),
            Word::Tag(tag) => write!(f, ".tag={tag}"),
            Word::Attribute(WordAttribute {
                key,
                value,
                value_raw: _,
            }) => {
                write!(f, "={}={}", key, value.unwrap_or(""))
            }
            Word::Message(generic) => write!(f, "{generic}"),
        }
    }
}

impl<'a> TryFrom<&'a [u8]> for Word<'a> {
    type Error = WordError;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        // Dispatch on the first byte to avoid redundant UTF-8 validation.
        // Categories are matched as raw byte slices (ASCII-only, no UTF-8 needed).
        // Tags parse the UUID directly from ASCII bytes.
        // Only messages and unknown words pay for UTF-8 validation.
        match value.first() {
            // Category words: "!done", "!re", "!trap", "!fatal", "!empty"
            Some(b'!') => match value {
                b"!done" => Ok(Word::Category(WordCategory::Done)),
                b"!re" => Ok(Word::Category(WordCategory::Reply)),
                b"!trap" => Ok(Word::Category(WordCategory::Trap)),
                b"!fatal" => Ok(Word::Category(WordCategory::Fatal)),
                b"!empty" => Ok(Word::Category(WordCategory::Empty)),
                _ => Ok(Word::Message(core::str::from_utf8(value)?)),
            },
            // Tag words: ".tag=<uuid>" — parse UUID directly from ASCII bytes
            Some(b'.') => {
                if value.starts_with(b".tag=") {
                    let tag = Tag::try_from_ascii_bytes(&value[5..])?;
                    Ok(Word::Tag(tag))
                } else {
                    Ok(Word::Message(core::str::from_utf8(value)?))
                }
            }
            // Attribute words: "=key=value"
            Some(b'=') => Ok(Word::Attribute(WordAttribute::try_from(value)?)),
            // Everything else is a message (must be valid UTF-8)
            _ => Ok(Word::Message(core::str::from_utf8(value)?)),
        }
    }
}

/// Represents the type of a response sentence.
///
/// Derived from the first [`Word`] in a sentence.
/// Valid types are `!done`, `!re`, `!trap`, `!fatal`, and `!empty`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WordCategory {
    /// Represents a `!done` response — command completed successfully.
    Done,
    /// Represents a `!re` response — a reply with attribute data.
    Reply,
    /// Represents a `!trap` response — an error or warning.
    Trap,
    /// Represents a `!fatal` response — a fatal connection error.
    Fatal,
    /// Represents a `!empty` response (`RouterOS` 7.18+) — no data to reply with.
    Empty,
}

impl TryFrom<&str> for WordCategory {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "!done" => Ok(Self::Done),
            "!re" => Ok(Self::Reply),
            "!trap" => Ok(Self::Trap),
            "!fatal" => Ok(Self::Fatal),
            "!empty" => Ok(Self::Empty),
            _ => Err(()),
        }
    }
}

impl Display for WordCategory {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            WordCategory::Done => write!(f, "!done"),
            WordCategory::Reply => write!(f, "!re"),
            WordCategory::Trap => write!(f, "!trap"),
            WordCategory::Fatal => write!(f, "!fatal"),
            WordCategory::Empty => write!(f, "!empty"),
        }
    }
}

/// Represents a key-value attribute pair in a `MikroTik` API sentence.
///
/// Attributes are encoded as `=key=value` on the wire. The key is always
/// valid UTF-8, but the value may contain arbitrary binary data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WordAttribute<'a> {
    /// The key of the attribute (always valid UTF-8).
    pub key: &'a str,
    /// The value as a UTF-8 string, if it is valid UTF-8. `None` if empty or non-UTF-8.
    pub value: Option<&'a str>,
    /// The raw byte value. `None` if the value is empty.
    pub value_raw: Option<&'a [u8]>,
}

impl<'a> TryFrom<&'a [u8]> for WordAttribute<'a> {
    type Error = WordError;

    fn try_from(word: &'a [u8]) -> Result<Self, Self::Error> {
        // First byte must be '=' for attributes
        if word.is_empty() || word[0] != b'=' {
            return Err(WordError::Attribute);
        }

        // Find the second '=' that separates key from value
        let mut parts = word[1..].splitn(2, |&b| b == b'=');

        // Key part must exist and be valid UTF-8
        let key_bytes = parts.next().ok_or(WordError::Attribute)?;
        let key = core::str::from_utf8(key_bytes).map_err(|_| WordError::AttributeKeyNotUtf8)?;

        // Value part is optional; treat an empty value as None.
        let value_raw = parts.next().filter(|value| !value.is_empty());

        // If we have a non-empty value, try to decode as UTF-8
        let value = value_raw.and_then(|v| core::str::from_utf8(v).ok());

        Ok(Self {
            key,
            value,
            value_raw,
        })
    }
}

/// Errors that can occur while parsing a [`Word`].
#[derive(Error, Debug, PartialEq, Clone)]
pub enum WordError {
    /// The word is not a valid UTF-8 string.
    #[error("UTF-8 decoding error: {0}")]
    Utf8(#[from] Utf8Error),
    /// The word is a tag, but the tag value is invalid.
    #[error("Tag parsing error: {0}")]
    Tag(#[from] uuid::Error),
    /// The word is an attribute pair, but the format is invalid.
    #[error("Invalid attribute format")]
    Attribute,
    /// The key part of the attribute pair is not valid UTF-8.
    #[error("Attribute key is not valid UTF-8")]
    AttributeKeyNotUtf8,
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::format;

    use uuid::Uuid;

    use super::*;

    impl<'a> From<(&'a str, Option<&'a str>)> for WordAttribute<'a> {
        fn from(value: (&'a str, Option<&'a str>)) -> Self {
            Self {
                key: value.0,
                value: value.1,
                value_raw: value.1.map(|v| v.as_bytes()),
            }
        }
    }

    #[test]
    fn test_word_parsing() {
        assert_eq!(
            Word::try_from(b"!done".as_ref()).unwrap(),
            Word::Category(WordCategory::Done)
        );

        assert_eq!(
            Word::try_from(b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8".as_ref()).unwrap(),
            Word::Tag(Tag::from(Uuid::from_bytes([
                0xa1, 0xa2, 0xa3, 0xa4, 0xb1, 0xb2, 0xc1, 0xc2, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6,
                0xd7, 0xd8
            ])))
        );

        assert_eq!(
            Word::try_from(b"=name=ether1".as_ref()).unwrap(),
            Word::Attribute(("name", Some("ether1")).into())
        );

        assert_eq!(
            Word::try_from(b"=tag=".as_ref()).unwrap(),
            Word::Attribute(("tag", None).into())
        );

        assert_eq!(
            Word::try_from(b"!fatal".as_ref()).unwrap(),
            Word::Category(WordCategory::Fatal)
        );

        assert_eq!(
            Word::try_from(b"!empty".as_ref()).unwrap(),
            Word::Category(WordCategory::Empty)
        );

        assert_eq!(
            Word::try_from(b"unknownword".as_ref()).unwrap(),
            Word::Message("unknownword")
        );

        // Invalid tag value
        assert!(Word::try_from(b".tag=not-a-valid-uuid".as_ref()).is_err());

        // Invalid UTF-8 sequence
        assert!(Word::try_from(b"\xFF\xFF".as_ref()).is_err());
    }

    #[test]
    fn test_display_for_word() {
        let word = Word::Category(WordCategory::Done);
        assert_eq!(format!("{}", word), "!done");

        let word = Word::Tag(Tag::from(Uuid::from_bytes([
            0xa1, 0xa2, 0xa3, 0xa4, 0xb1, 0xb2, 0xc1, 0xc2, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6,
            0xd7, 0xd8,
        ])));
        assert_eq!(
            format!("{}", word),
            ".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8"
        );

        let word = Word::Attribute(("name", Some("ether1")).into());
        assert_eq!(format!("{}", word), "=name=ether1");

        let word = Word::Attribute(("disabled", None).into());
        assert_eq!(format!("{}", word), "=disabled=");

        let word = Word::Message("unknownword");
        assert_eq!(format!("{}", word), "unknownword");

        let word = Word::Category(WordCategory::Empty);
        assert_eq!(format!("{}", word), "!empty");
    }
}
