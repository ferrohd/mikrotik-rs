use super::error::WordType;
use crate::protocol::string::AsciiStringRef;
use std::{
    fmt::{self, Display, Formatter},
    num::ParseIntError,
    str::Utf8Error,
};

/// Represents a word in a Mikrotik [`Sentence`].
///
/// Words can be of three types:
/// - A category word, which represents the type of sentence, such as `!done`, `!re`, `!trap`, or `!fatal`.
/// - A tag word, which represents a tag value like `.tag=123`.
/// - An attribute word, which represents a key-value pair like `=name=ether1`.
///
/// The word can be converted into one of these types using the [`TryFrom`] trait.
///
/// # Examples
///
/// ```
/// use mikrotik_rs::protocol::word::Word;
///
/// let word = Word::try_from(b"=name=ether1");
/// assert_eq!(word.unwrap().attribute(), Some((b"name" as &[u8], Some(b"ether1" as &[u8]))));
/// ```
#[derive(Debug, PartialEq)]
pub enum Word<'a> {
    /// A category word, such as `!done`, `!re`, `!trap`, or `!fatal`.
    Category(WordCategory),
    /// A tag word, such as `.tag=123`.
    Tag(u16),
    /// An attribute word, such as `=name=ether1`.
    Attribute(WordAttribute<'a>),
    /// An unrecognized word. Usually this is a `!fatal` reason message.
    Message(AsciiStringRef<'a>),
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

    /// Returns the attribute of the word, if it is an attribute word.
    pub fn attribute(&self) -> Option<(&[u8], Option<&[u8]>)> {
        match self {
            Word::Attribute(WordAttribute { key, value }) => Some((key.as_ref(), value.as_deref())),
            _ => None,
        }
    }

    /// Returns the generic word, if it is a generic word.
    /// This is usually a `!fatal` reason message.
    pub fn generic(&self) -> Option<AsciiStringRef> {
        match self {
            Word::Message(generic) => Some(*generic),
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
            Word::Attribute(WordAttribute { key, value }) => {
                write!(f, "={}={}", key, value.as_ref().unwrap_or_default())
            }
            Word::Message(generic) => write!(f, "{}", generic),
        }
    }
}

impl<'a> TryFrom<AsciiStringRef<'a>> for Word<'a> {
    type Error = WordError;
    fn try_from(value: AsciiStringRef<'a>) -> Result<Self, Self::Error> {
        let s = value;
        // Parse tag
        if let Some(stripped) = s.strip_prefix(b".tag=") {
            let tag = AsciiStringRef(stripped).parse::<u16>()?;
            return Ok(Word::Tag(tag));
        }

        // Parse attribute pair
        if s.starts_with(b"=") {
            let attribute = WordAttribute::try_from(s)?;
            return Ok(Word::Attribute(attribute));
        }

        // Parse category
        match WordCategory::try_from(s.0) {
            Ok(category) => Ok(Word::Category(category)),
            // If the word is not a category, tag, or attribute, it's likely a generic word
            Err(_) => Ok(Word::Message(s)),
        }
    }
}
impl<'a, const N: usize> TryFrom<&'a [u8; N]> for Word<'a> {
    type Error = WordError;

    fn try_from(value: &'a [u8; N]) -> Result<Self, Self::Error> {
        Word::try_from(AsciiStringRef(value))
    }
}

/// Represents the type of of a response.
/// The type is derived from the first [`Word`] in a [`Sentence`].
/// Valid types are `!done`, `!re`, `!trap`, and `!fatal`.
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
}

impl TryFrom<&[u8]> for WordCategory {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match value {
            b"!done" => Ok(Self::Done),
            b"!re" => Ok(Self::Reply),
            b"!trap" => Ok(Self::Trap),
            b"!fatal" => Ok(Self::Fatal),
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
        }
    }
}

/// Represents a key-value pair in a Mikrotik [`Sentence`].
#[derive(Debug, PartialEq)]
pub struct WordAttribute<'a> {
    /// The key of the attribute.
    pub key: AsciiStringRef<'a>,
    /// The value of the attribute, if present.
    pub value: Option<AsciiStringRef<'a>>,
}

impl<'a> TryFrom<AsciiStringRef<'a>> for WordAttribute<'a> {
    type Error = WordError;

    fn try_from(value: AsciiStringRef<'a>) -> Result<Self, Self::Error> {
        let mut parts = value
            .0
            .strip_prefix(b"=")
            .ok_or(WordError::Attribute)?
            .splitn(2, |b| *b == b'=');
        let key = AsciiStringRef(
            parts
                .next()
                .expect("there should be always at least one part"),
        );
        let value = parts.next().map(AsciiStringRef);
        Ok(Self { key, value })
    }
}
impl<'a> From<(&'a [u8], Option<&'a [u8]>)> for WordAttribute<'a> {
    fn from((key, value): (&'a [u8], Option<&'a [u8]>)) -> Self {
        Self {
            key: key.into(),
            value: value.map(AsciiStringRef),
        }
    }
}
impl<'a, const KL: usize, const VL: usize> From<(&'a [u8; KL], Option<&'a [u8; VL]>)>
    for WordAttribute<'a>
{
    fn from((key, value): (&'a [u8; KL], Option<&'a [u8; VL]>)) -> Self {
        Self {
            key: key.into(),
            value: value.map(|s| AsciiStringRef(s)),
        }
    }
}

/// Represents an error that occurred while parsing a [`Word`].
#[derive(Debug, PartialEq)]
pub enum WordError {
    /// The word is not a valid UTF-8 string.
    Utf8(Utf8Error),
    /// The word is a tag, but the tag value is invalid.
    Tag(ParseIntError),
    /// The word is an attribute pair, but the format is invalid.
    Attribute,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_parsing() {
        // Test cases for `Word::try_from` function
        assert_eq!(
            Word::try_from(b"!done").unwrap(),
            Word::Category(WordCategory::Done)
        );

        assert_eq!(Word::try_from(b".tag=123").unwrap(), Word::Tag(123));

        assert_eq!(
            Word::try_from(b"=name=ether1").unwrap(),
            Word::Attribute((b"name", Some(b"ether1")).into())
        );

        assert_eq!(
            Word::try_from(b"!fatal").unwrap(),
            Word::Category(WordCategory::Fatal)
        );

        assert_eq!(
            Word::try_from(b"unknownword").unwrap(),
            Word::Message(b"unknownword".into())
        );

        // Invalid tag value
        assert!(Word::try_from(b".tag=notanumber").is_err());

        // extended characters
        assert_eq!(
            Word::try_from(b"\xFF\xFF").unwrap(),
            Word::Message(b"\xff\xff".into())
        );
    }

    #[test]
    fn test_display_for_word() {
        // Test cases for `Display` implementation for `Word`
        let word = Word::Category(WordCategory::Done);
        assert_eq!(format!("{}", word), "!done");

        let word = Word::Tag(123);
        assert_eq!(format!("{}", word), ".tag=123");

        let word = Word::Attribute((b"name", Some(b"ether1")).into());
        assert_eq!(format!("{}", word), "=name=ether1");

        let word = Word::Message(b"unknownword".into());
        assert_eq!(format!("{}", word), "unknownword");
    }
}
