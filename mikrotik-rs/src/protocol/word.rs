use std::{
    borrow::Cow,
    fmt::{self, Display, Formatter},
    num::ParseIntError,
    str::Utf8Error,
};

use super::error::WordType;

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
/// let word = Word::try_from(b"=name=ether1" as &[u8]);
/// assert_eq!(word.unwrap().attribute(), Some(("name", Some("ether1"))));
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
    Message(Cow<'a, str>),
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
    pub fn attribute(&self) -> Option<(&str, Option<&str>)> {
        match self {
            Word::Attribute(WordAttribute { key, value }) => Some((key.as_ref(), value.as_deref())),
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
            Word::Attribute(WordAttribute { key, value }) => {
                write!(f, "={}={}", key, value.as_deref().unwrap_or(""))
            }
            Word::Message(generic) => write!(f, "{}", generic),
        }
    }
}

impl<'a> TryFrom<&'a [u8]> for Word<'a> {
    type Error = WordError;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        let s = encoding_rs::mem::decode_latin1(value);

        // Parse tag
        if let Some(stripped) = s.strip_prefix(".tag=") {
            let tag = stripped.parse::<u16>()?;
            return Ok(Word::Tag(tag));
        }

        // Parse attribute pair
        if s.starts_with('=') {
            let attribute = WordAttribute::try_from(s)?;
            return Ok(Word::Attribute(attribute));
        }

        // Parse category
        match WordCategory::try_from(s.as_ref()) {
            Ok(category) => Ok(Word::Category(category)),
            // If the word is not a category, tag, or attribute, it's likely a generic word
            Err(_) => Ok(Word::Message(s)),
        }
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

impl TryFrom<&str> for WordCategory {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "!done" => Ok(Self::Done),
            "!re" => Ok(Self::Reply),
            "!trap" => Ok(Self::Trap),
            "!fatal" => Ok(Self::Fatal),
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
    pub key: Cow<'a, str>,
    /// The value of the attribute, if present.
    pub value: Option<Cow<'a, str>>,
}

impl<'a> TryFrom<&'a str> for WordAttribute<'a> {
    type Error = WordError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let mut parts = value
            .strip_prefix('=')
            .ok_or(WordError::Attribute)?
            .splitn(2, '=');
        let key = Cow::Borrowed(parts.next().ok_or(WordError::Attribute)?);
        let value = parts.next().map(Cow::Borrowed);
        Ok(Self { key, value })
    }
}

impl<'a> TryFrom<Cow<'a, str>> for WordAttribute<'a> {
    type Error = WordError;

    fn try_from(value: Cow<'a, str>) -> Result<Self, Self::Error> {
        Ok(match value {
            Cow::Borrowed(value) => {
                let mut parts = value
                    .strip_prefix('=')
                    .ok_or(WordError::Attribute)?
                    .splitn(2, '=');
                let key = Cow::Borrowed(parts.next().ok_or(WordError::Attribute)?);
                let value = parts.next().map(Cow::Borrowed);
                Self { key, value }
            }
            Cow::Owned(value) => {
                let mut parts = value
                    .strip_prefix('=')
                    .ok_or(WordError::Attribute)?
                    .splitn(2, '=');
                let key = parts.next().ok_or(WordError::Attribute)?.to_string().into();
                let value = parts.next().map(|s| s.to_string().into());
                Self { key, value }
            }
        })
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
    use encoding_rs::mem::encode_latin1_lossy;

    impl<'a> From<(&'a str, Option<&'a str>)> for WordAttribute<'a> {
        fn from(value: (&'a str, Option<&'a str>)) -> Self {
            Self {
                key: Cow::Borrowed(value.0),
                value: value.1.map(Cow::Borrowed),
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
            Word::try_from(b"unknownword".as_ref()).unwrap(),
            Word::Message("unknownword".into())
        );

        // Invalid tag value
        assert!(Word::try_from(b".tag=notanumber".as_ref()).is_err());

        // extended characters
        assert_eq!(
            Word::try_from(b"\xFF\xFF".as_ref()).unwrap(),
            Word::Message("ÿÿ".into())
        );
        assert_eq!(
            Word::try_from(encode_latin1_lossy("äöüàéèÄÖÜÀÉÈ").as_ref()).unwrap(),
            Word::Message("äöüàéèÄÖÜÀÉÈ".into())
        );
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

        let word = Word::Message("unknownword".into());
        assert_eq!(format!("{}", word), "unknownword");
    }
}
