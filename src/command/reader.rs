use std::{num::ParseIntError, str::Utf8Error};

use super::response::{ParsingError, SentenceError, TagError};

/// A parser for parsing sentences in the Mikrotik API sentence format.
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
pub struct Sentence<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Sentence<'a> {
    /// Creates a new `Sentence` instance for parsing the given data slice.
    ///
    /// # Arguments
    ///
    /// * `data` - A slice of bytes representing the data of the Mikrotik sentence.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }
}

impl<'a> Iterator for Sentence<'a> {
    type Item = Result<Word<'a>, SentenceError>;

    /// Advances the [`Iterator`] and returns the next word in the [`Sentence`].
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

                // Return the word
                let word =
                    Word::try_from(&self.data[start..end]).map_err(|e| SentenceError::WordError(e));
                Some(word)
            }
            Err(e) => Some(Err(e)),
        }
    }
}

pub enum Word<'a> {
    Category(&'a str),
    Tag(u16),
    Attribute((&'a str, Option<&'a str>)),
}

impl<'a> Word<'a> {
    pub fn category(&self) -> Option<&str> {
        match self {
            Word::Category(category) => Some(category),
            _ => None,
        }
    }

    pub fn tag(&self) -> Option<u16> {
        match self {
            Word::Tag(tag) => Some(*tag),
            _ => None,
        }
    }

    pub fn attribute(&self) -> Option<(&str, Option<&str>)> {
        match self {
            Word::Attribute((key, value)) => Some((*key, *value)),
            _ => None,
        }
    }
}

impl<'a> TryFrom<&'a [u8]> for Word<'a> {
    type Error = WordError;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        let s = std::str::from_utf8(value)?;

        // Parse tag
        if s.starts_with(".tag=") {
            let tag = s[5..].parse::<u16>()?;
            return Ok(Word::Tag(tag));
        }

        // Parse attribute pair
        if s.starts_with("=") {
            let mut parts = s[1..].splitn(2, '=');
            let key = parts.next().ok_or(WordError::Attribute)?;
            let value = parts.next();
            return Ok(Word::Attribute((key, value)));
        }

        // Parse category
        if s == "!done" || s == "!re" || s == "!trap" || s == "!fatal" {
            return Ok(Word::Category(s));
        }

        Err(WordError::Unrecognized)
    }
}

impl<'a> Word<'a> {}

#[derive(Debug)]
pub enum WordError {
    // The word is not a valid UTF-8 string.
    Utf8(Utf8Error),
    // The word is a tag, but the tag value is invalid.
    Tag(TagError),
    // The word is a attribute word, but the key is missing.
    Attribute,
    // The [`Word`] is not a recognized type.
    Unrecognized,
}

impl From<Utf8Error> for WordError {
    fn from(e: Utf8Error) -> Self {
        Self::Utf8(e)
    }
}

impl From<ParseIntError> for WordError {
    fn from(e: ParseIntError) -> Self {
        Self::Tag(TagError::Invalid(e))
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
