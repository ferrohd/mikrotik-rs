use std::{
    fmt::{self, Debug, Display, Formatter},
    num::ParseIntError,
    str::Utf8Error,
};

/// A parser for parsing bytes into sentences in the Mikrotik API sentence format.
///
/// The Mikrotik API uses a custom protocol to communicate. Each message is a sentence
/// composed of words. This structure represents a sentence and allows iterating over
/// its words.
///
/// Each word in a sentence is encoded with a length prefix, followed by the word's bytes.
/// The length is encoded in a variable number of bytes to save space for short words.
///
/// More details about the protocol can be found in the Mikrotik Wiki:
/// [Mikrotik API Protocol](https://wiki.mikrotik.com/wiki/Manual:API#Protocol)
#[derive(Debug)]
pub struct Sentence<'a> {
    data: &'a [u8],
    position: usize,
    category_found: bool,
}

impl<'a> Sentence<'a> {
    /// Creates a new `Sentence` instance for parsing the given data slice.
    ///
    /// # Arguments
    ///
    /// * `data` - A slice of bytes representing the data of the Mikrotik sentence.
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            position: 0,
            category_found: false,
        }
    }
}

impl<'a> Iterator for Sentence<'a> {
    type Item = Result<Word<'a>, SentenceError>;

    /// Advances the [`Iterator`] and returns the next [`Word`] in the [`Sentence`].
    ///
    /// The word is returned as a slice of the original data. This avoids copying
    /// data but means the lifetime of the returned slice is tied to the lifetime
    /// of the data passed to `Sentence::new`.
    ///
    /// # Errors
    ///
    /// Returns an `Err` if there's an issue decoding the length of the next word
    /// or if the data cannot be interpreted as a valid UTF-8 string slice.
    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.data.len() {
            return None;
        }

        let mut start = self.position;

        match read_length(&self.data[start..]) {
            Ok((lenght, bytes_read)) => {
                // Last word is empty, so we are done.
                if lenght == 0 {
                    return None;
                }
                // Start reading the content skipping the length bytes
                start += bytes_read;

                // Will never run on architectures where usize is < 32 bits so converting to usize is safe.
                let end = start + lenght as usize;

                // Update the position for the next iteration
                self.position = end;

                let word = || -> Result<Word, SentenceError> {
                    // Parse the word
                    let word =
                        Word::try_from(&self.data[start..end]).map_err(SentenceError::WordError)?;

                    // The first word in the sentence must be a category
                    if !self.category_found {
                        if word.category().is_none() {
                            return Err(SentenceError::CategoryError);
                        }
                        self.category_found = true;
                    }

                    Ok(word)
                }();

                Some(word)
            }
            Err(e) => Some(Err(e)),
        }
    }
}

/// Specific errors that can occur while processing a byte sequence into a [`Sentence`].
///
/// Provides information about issues related to converting a sequence of bytes into a [`Sentence`].
#[derive(Debug, PartialEq)]
pub enum SentenceError {
    /// Error indicating that a sequence of bytes could not be converted to a [`Word`].
    ///
    /// This could occur if the byte sequence contains invalid UTF-8 patterns, which is
    /// possible when receiving malformed or unexpected input.
    WordError(WordError),
    /// Error indicating that an issue occurred due to incorrect length or format of the [`Sentence`].
    ///
    /// This could happen if the bytes doe not comply with the expected structure,
    /// making it too short to parse correctly into a [`Sentence`].
    LengthError,
    /// Error indicating that the category of the sentence is invalid.
    /// This could happen if the sentence does not start with a recognized category.
    /// Valid categories are `!done`, `!re`, `!trap`, and `!fatal`.
    CategoryError,
}

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
/// use mikrotik::command::reader::Word;
///
/// let word = Word::try_from(b"=name=ether1");
/// assert_eq!(word.unwrap().attribute(), Some(("name", Some("ether1"))));
/// ```
#[derive(Debug, PartialEq)]
pub enum Word<'a> {
    /// A category word, such as `!done`, `!re`, `!trap`, or `!fatal`.
    Category(ResponseType),
    /// A tag word, such as `.tag=123`.
    Tag(u16),
    /// An attribute word, such as `=name=ether1`.
    Attribute((&'a str, Option<&'a str>)),
    /// An unrecognized word. Usually this is a `!fatal` reason message.
    Generic(&'a str),
}

impl Word<'_> {
    /// Returns the category of the word, if it is a category word.
    pub fn category(&self) -> Option<&ResponseType> {
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
            Word::Attribute((key, value)) => Some((*key, *value)),
            _ => None,
        }
    }

    /// Returns the generic word, if it is a generic word.
    /// This is usually a `!fatal` reason message.
    pub fn generic(&self) -> Option<&str> {
        match self {
            Word::Generic(generic) => Some(generic),
            _ => None,
        }
    }
}

impl Display for Word<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Word::Category(category) => write!(f, "{}", category),
            Word::Tag(tag) => write!(f, ".tag={}", tag),
            Word::Attribute((key, value)) => write!(f, "={}={}", key, value.unwrap_or("")),
            Word::Generic(generic) => write!(f, "{}", generic),
        }
    }
}

impl<'a> TryFrom<&'a [u8]> for Word<'a> {
    type Error = WordError;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        let s = std::str::from_utf8(value)?;

        // Parse tag
        if let Some(stripped) = s.strip_prefix(".tag=") {
            let tag = stripped.parse::<u16>()?;
            return Ok(Word::Tag(tag));
        }

        // Parse attribute pair
        if let Some(stripped) = s.strip_prefix('=') {
            let mut parts = stripped.splitn(2, '=');
            let key = parts.next().ok_or(WordError::Attribute)?;
            let value = parts.next();
            return Ok(Word::Attribute((key, value)));
        }

        // Parse category
        match s {
            "!done" => Ok(Word::Category(ResponseType::Done)),
            "!re" => Ok(Word::Category(ResponseType::Reply)),
            "!trap" => Ok(Word::Category(ResponseType::Trap)),
            "!fatal" => Ok(Word::Category(ResponseType::Fatal)),
            // Unrecognized word, usually a `!fatal` reason message.
            s => Ok(Word::Generic(s)),
        }
    }
}

/// Represents the type of of a response.
/// The type is derived from the first [`Word`] in a [`Sentence`].
/// Valid types are `!done`, `!re`, `!trap`, and `!fatal`.
#[derive(Debug, PartialEq)]
pub enum ResponseType {
    /// Represents a `!done` response.
    Done,
    /// Represents a `!re` response.
    Reply,
    /// Represents a `!trap` response.
    Trap,
    /// Represents a `!fatal` response.
    Fatal,
}

impl Display for ResponseType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ResponseType::Done => write!(f, "!done"),
            ResponseType::Reply => write!(f, "!re"),
            ResponseType::Trap => write!(f, "!trap"),
            ResponseType::Fatal => write!(f, "!fatal"),
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
    /// The word is a attribute word, but the key is missing.
    Attribute,
}

impl From<WordError> for SentenceError {
    fn from(e: WordError) -> Self {
        Self::WordError(e)
    }
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

/// Returns the length and the number of bytes read.
fn read_length(data: &[u8]) -> Result<(u32, usize), SentenceError> {
    let mut c: u32 = data[0] as u32;
    if c & 0x80 == 0x00 {
        Ok((c, 1))
    } else if c & 0xC0 == 0x80 {
        c &= !0xC0;
        c <<= 8;
        c += data[1] as u32;
        return Ok((c, 2));
    } else if c & 0xE0 == 0xC0 {
        c &= !0xE0;
        c <<= 8;
        c += data[1] as u32;
        c <<= 8;
        c += data[2] as u32;
        return Ok((c, 3));
    } else if c & 0xF0 == 0xE0 {
        c &= !0xF0;
        c <<= 8;
        c += data[1] as u32;
        c <<= 8;
        c += data[2] as u32;
        c <<= 8;
        c += data[3] as u32;
        return Ok((c, 4));
    } else if c & 0xF8 == 0xF0 {
        c = data[1] as u32;
        c <<= 8;
        c += data[2] as u32;
        c <<= 8;
        c += data[3] as u32;
        c <<= 8;
        c += data[4] as u32;
        return Ok((c, 5));
    } else {
        Err(SentenceError::LengthError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_parsing() {
        // Test cases for `Word::try_from` function
        assert_eq!(
            Word::try_from(b"!done".as_ref()).unwrap(),
            Word::Category(ResponseType::Done)
        );

        assert_eq!(
            Word::try_from(b".tag=123".as_ref()).unwrap(),
            Word::Tag(123)
        );

        assert_eq!(
            Word::try_from(b"=name=ether1".as_ref()).unwrap(),
            Word::Attribute(("name", Some("ether1")))
        );

        assert_eq!(
            Word::try_from(b"!fatal".as_ref()).unwrap(),
            Word::Category(ResponseType::Fatal)
        );

        assert_eq!(
            Word::try_from(b"unknownword".as_ref()).unwrap(),
            Word::Generic("unknownword")
        );

        // Invalid tag value
        assert!(Word::try_from(b".tag=notanumber".as_ref()).is_err());

        // Invalid UTF-8 sequence
        assert!(Word::try_from(b"\xFF\xFF".as_ref()).is_err());
    }

    #[test]
    fn test_display_for_word() {
        // Test cases for `Display` implementation for `Word`
        let word = Word::Category(ResponseType::Done);
        assert_eq!(format!("{}", word), "!done");

        let word = Word::Tag(123);
        assert_eq!(format!("{}", word), ".tag=123");

        let word = Word::Attribute(("name", Some("ether1")));
        assert_eq!(format!("{}", word), "=name=ether1");

        let word = Word::Generic("unknownword");
        assert_eq!(format!("{}", word), "unknownword");
    }

    #[test]
    fn test_sentence_iterator() {
        let data: &[u8] = &[
            0x05, b'!', b'd', b'o', b'n', b'e', // Word: !done
            0x08, b'.', b't', b'a', b'g', b'=', b'1', b'2', b'3', // Word: .tag=123
            0x0C, b'=', b'n', b'a', b'm', b'e', b'=', b'e', b't', b'h', b'e', b'r',
            b'1', // Word: =name=ether1
            0x00, // End of sentence
        ];

        let mut sentence = Sentence::new(data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(ResponseType::Done)
        );

        assert_eq!(sentence.next().unwrap().unwrap(), Word::Tag(123));

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Attribute(("name", Some("ether1")))
        );

        assert_eq!(sentence.next(), None);
    }

    #[test]
    fn test_sentence_category_error() {
        // Test case where the first word is not a category
        let data: &[u8] = &[
            0x0A, b'.', b't', b'a', b'g', b'=', b'1', b'2', b'3', // Word: .tag=123
            0x0D, b'=', b'n', b'a', b'm', b'e', b'=', b'e', b't', b'h', b'e', b'r',
            b'1', // Word: =name=ether1
        ];

        let mut sentence = Sentence::new(data);

        assert!(sentence.next().unwrap().is_err());
    }

    #[test]
    fn test_sentence_length_error() {
        // Test case where length is invalid
        let data: &[u8] = &[
            0xF8, b'.', b't', b'a', b'g', b'=', b'1', b'2', b'3', // Invalid length prefix
        ];

        let mut sentence = Sentence::new(data);

        assert!(sentence.next().unwrap().is_err());
    }

    #[test]
    fn test_complete_sentence_parsing() {
        let data: &[u8] = &[
            0x05, b'!', b'd', b'o', b'n', b'e', // Word: !done
            0x08, b'.', b't', b'a', b'g', b'=', b'1', b'2', b'3', // Word: .tag=123
            0x0C, b'=', b'n', b'a', b'm', b'e', b'=', b'e', b't', b'h', b'e', b'r',
            b'1', // Word: =name=ether1
            0x00, // End of sentence
        ];

        let mut sentence = Sentence::new(data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(ResponseType::Done)
        );

        assert_eq!(sentence.next().unwrap().unwrap(), Word::Tag(123));

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Attribute(("name", Some("ether1")))
        );

        assert_eq!(sentence.next(), None);
    }

    #[test]
    fn test_sentence_with_invalid_length() {
        let data: &[u8] = &[
            0xF8, b'.', b't', b'a', b'g', b'=', b'1', b'2', b'3', // Invalid length prefix
        ];

        let mut sentence = Sentence::new(data);

        assert!(sentence.next().unwrap().is_err());
    }

    #[test]
    fn test_sentence_without_category() {
        let data: &[u8] = &[
            0x0A, b'.', b't', b'a', b'g', b'=', b'1', b'2', b'3', // Word: .tag=123
            0x0D, b'=', b'n', b'a', b'm', b'e', b'=', b'e', b't', b'h', b'e', b'r',
            b'1', // Word: =name=ether1
        ];

        let mut sentence = Sentence::new(data);

        assert!(sentence.next().unwrap().is_err());
    }

    #[test]
    fn test_mixed_words_sentence() {
        let data: &[u8] = &[
            0x03, b'!', b'r', b'e', // Word: !re
            0x04, b'=', b'a', b'=', b'b', // Word: =a=b
            0x08, b'.', b't', b'a', b'g', b'=', b'4', b'5', b'6', // Word: .tag=456
            0x00, // End of sentence
        ];

        let mut sentence = Sentence::new(data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(ResponseType::Reply)
        );

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Attribute(("a", Some("b")))
        );

        assert_eq!(sentence.next().unwrap().unwrap(), Word::Tag(456));

        assert_eq!(sentence.next(), None);
    }

    #[test]
    fn test_sentence_with_fatal_message() {
        let data: &[u8] = &[
            0x06, b'!', b'f', b'a', b't', b'a', b'l', 0x0B, b's', b'e', b'r', b'v', b'e', b'r',
            b' ', b'd', b'o', b'w', b'n', // Word: !fatal server down
            0x00, // End of sentence
        ];

        let mut sentence = Sentence::new(data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(ResponseType::Fatal)
        );

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Generic("server down")
        );

        assert_eq!(sentence.next(), None);
    }

    #[test]
    fn test_complete_sentence_with_extra_data() {
        let data: &[u8] = &[
            0x05, b'!', b'd', b'o', b'n', b'e', // Word: !done
            0x08, b'.', b't', b'a', b'g', b'=', b'1', b'2', b'3', // Word: .tag=123
            0x0C, b'=', b'n', b'a', b'm', b'e', b'=', b'e', b't', b'h', b'e', b'r',
            b'1', // Word: =name=ether1
            0x00, // End of sentence
            0x07, b'!', b'd', b'o', b'n', b'e', // Extra data: !done
        ];

        let mut sentence = Sentence::new(data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(ResponseType::Done)
        );

        assert_eq!(sentence.next().unwrap().unwrap(), Word::Tag(123));

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Attribute(("name", Some("ether1")))
        );

        assert_eq!(sentence.next(), None);

        // Confirm that extra data is ignored after the end of the sentence
        assert_eq!(sentence.next(), None);
    }
}
