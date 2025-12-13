use std::{
    fmt::{self, Display, Formatter},
    num::ParseIntError,
    str::Utf8Error,
};

use super::error::WordType;

/// Represents a word in a Mikrotik [`Sentence`].
///
/// Words can be of three types:
/// - A category word, which represents the type of sentence, such as `!done`, `!re`, `!trap`, `!fatal`, or `!empty`.
/// - A tag word, which represents a tag value like `.tag=123`.
/// - An attribute word, which represents a key-value pair like `=name=ether1`.
///
/// The word can be converted into one of these types using the [`TryFrom`] trait.
///
/// # Examples
///
/// ```
/// use mikrotik::command::reader::Word;
///
/// let word = Word::try_from(b"=name=ether1");
/// assert_eq!(word.unwrap().attribute(), Some(("name", Some("ether1"))));
/// ```
#[derive(Debug, PartialEq)]
pub enum Word<'a> {
    /// A category word, such as `!done`, `!re`, `!trap`, `!fatal`, or `!empty`.
    Category(WordCategory),
    /// A tag word, such as `.tag=123`.
    Tag(u16),
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
    pub fn tag(&self) -> Option<u16> {
        match self {
            Word::Tag(tag) => Some(*tag),
            _ => None,
        }
    }

    /// Returns the generic word, if it is a generic word.
    /// This is usually a `!fatal` reason message.
    pub fn generic(&self) -> Option<&str> {
        match self {
            Word::Message(generic) => Some(generic),
            _ => None,
        }
    }

    /// Returns the type of the Word.
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
            Word::Category(category) => write!(f, "{}", category),
            Word::Tag(tag) => write!(f, ".tag={}", tag),
            Word::Attribute(WordAttribute {
                key,
                value,
                value_raw: _,
            }) => {
                write!(f, "={}={}", key, value.unwrap_or(""))
            }
            Word::Message(generic) => write!(f, "{}", generic),
        }
    }
}

impl<'a> TryFrom<&'a [u8]> for Word<'a> {
    type Error = WordError;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        // First, check if it's a category or tag word by attempting UTF-8 conversion
        // Categories and tags must be valid UTF-8 as they are fixed API words
        if let Ok(s) = std::str::from_utf8(value) {
            // Try to parse as category first
            if let Ok(category) = WordCategory::try_from(s) {
                return Ok(Word::Category(category));
            }

            // Try to parse as tag if it starts with ".tag="
            if let Some(stripped) = s.strip_prefix(".tag=") {
                let tag = stripped.parse::<u16>()?;
                return Ok(Word::Tag(tag));
            }
        }

        // Handle attributes - we know they start with = regardless of UTF-8 validity
        if !value.is_empty() && value[0] == b'=' {
            // Pass the raw bytes to WordAttribute which now handles UTF-8 validation internally
            return Ok(Word::Attribute(WordAttribute::try_from(value)?));
        }

        // If all else fails, return as a message (must be valid UTF-8!)
        Ok(Word::Message(std::str::from_utf8(value)?))
    }
}

/// Represents the type of of a response.
/// The type is derived from the first [`Word`] in a [`Sentence`].
/// Valid types are `!done`, `!re`, `!trap`, `!fatal`, and `!empty`.
#[derive(Debug, Clone, PartialEq)]
pub enum WordCategory {
    /// Represents a `!done` response.
    Done,
    /// Represents a `!re` response.
    Reply,
    /// Represents a `!trap` response.
    Trap,
    /// Represents a `!fatal` response.
    Fatal,
    /// Represents a `!empty` response.
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

/// Represents a key-value pair in a Mikrotik [`Sentence`].
#[derive(Debug, PartialEq)]
pub struct WordAttribute<'a> {
    /// The key of the attribute.
    pub key: &'a str,
    /// The value of the attribute, if present and in valid UTF-8.
    pub value: Option<&'a str>,
    /// The value of the attribute, if present, in bytes.
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
        let key = std::str::from_utf8(key_bytes).map_err(|_| WordError::AttributeKeyNotUtf8)?;

        // Value part is optional
        let value_raw = parts.next();

        // If we have a value, try to decode as UTF-8 but keep raw bytes regardless
        let value = value_raw.and_then(|v| std::str::from_utf8(v).ok());

        Ok(Self {
            key,
            value_raw,
            value,
        })
    }
}

/// Represents an error that occurred while parsing a [`Word`].
#[derive(Debug, PartialEq, Clone)]
pub enum WordError {
    /// The word is not a valid UTF-8 string.
    Utf8(Utf8Error),
    /// The word is a tag, but the tag value is invalid.
    Tag(ParseIntError),
    /// The word is an attribute pair, but the format is invalid.
    Attribute,
    /// The key part of the attribute pair is not valid UTF-8.
    AttributeKeyNotUtf8,
}

impl From<Utf8Error> for WordError {
    fn from(e: Utf8Error) -> Self {
        Self::Utf8(e)
    }
}

impl From<ParseIntError> for WordError {
    fn from(e: ParseIntError) -> Self {
        Self::Tag(e)
    }
}

impl std::fmt::Display for WordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WordError::Utf8(err) => write!(f, "UTF-8 decoding error: {}", err),
            WordError::Tag(err) => write!(f, "Tag parsing error: {}", err),
            WordError::Attribute => write!(f, "Invalid attribute format"),
            WordError::AttributeKeyNotUtf8 => write!(f, "Attribute key is not valid UTF-8"),
        }
    }
}

#[cfg(test)]
mod tests {
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
        // Test cases for `Word::try_from` function
        assert_eq!(
            Word::try_from(b"!done".as_ref()).unwrap(),
            Word::Category(WordCategory::Done)
        );

        assert_eq!(
            Word::try_from(b".tag=123".as_ref()).unwrap(),
            Word::Tag(123)
        );

        assert_eq!(
            Word::try_from(b"=name=ether1".as_ref()).unwrap(),
            Word::Attribute(("name", Some("ether1")).into())
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
        assert!(Word::try_from(b".tag=notanumber".as_ref()).is_err());

        // Invalid UTF-8 sequence
        assert!(Word::try_from(b"\xFF\xFF".as_ref()).is_err());
    }

    #[test]
    fn test_display_for_word() {
        // Test cases for `Display` implementation for `Word`
        let word = Word::Category(WordCategory::Done);
        assert_eq!(format!("{}", word), "!done");

        let word = Word::Tag(123);
        assert_eq!(format!("{}", word), ".tag=123");

        let word = Word::Attribute(("name", Some("ether1")).into());
        assert_eq!(format!("{}", word), "=name=ether1");

        let word = Word::Message("unknownword");
        assert_eq!(format!("{}", word), "unknownword");

        let word = Word::Category(WordCategory::Empty);
        assert_eq!(format!("{}", word), "!empty");
    }
}
