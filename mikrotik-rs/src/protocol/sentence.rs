use thiserror::Error;

use super::word::{Word, WordError};

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

                let word = || -> Result<Word, SentenceError> {
                    // Parse the word
                    let data = &self
                        .data
                        .get(start..end)
                        .ok_or(SentenceError::PrefixLength)?;
                    let word = Word::try_from(*data).map_err(SentenceError::from)?;

                    Ok(word)
                }();

                // Update the position for the next iteration
                self.position = end;

                Some(word)
            }
            Err(e) => Some(Err(e)),
        }
    }
}

/// Specific errors that can occur while processing a byte sequence into a [`Sentence`].
///
/// Provides information about issues related to converting a sequence of bytes into a [`Sentence`].
#[derive(Error, Debug, PartialEq, Clone)]
pub enum SentenceError {
    /// Error indicating that a sequence of bytes could not be parsed into a [`Word`].
    #[error("Word error: {0}")]
    WordError(#[from] WordError),
    /// Error indicating that the prefix lenght of a [`Sentence`] is incorrect.
    /// This could happen if the length of the word is invalid or the data is corrupted.
    #[error("Invalid prefix length")]
    PrefixLength,
    // Error indicating that the category of the sentence is missing.
    // This could happen if the sentence does not start with a recognized category.
    // Valid categories are `!done`, `!re`, `!trap`, `!fatal`, and `!empty`.
    //Category,
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
        Ok((c, 2))
    } else if c & 0xE0 == 0xC0 {
        c &= !0xE0;
        c <<= 8;
        c += data[1] as u32;
        c <<= 8;
        c += data[2] as u32;
        Ok((c, 3))
    } else if c & 0xF0 == 0xE0 {
        c &= !0xF0;
        c <<= 8;
        c += data[1] as u32;
        c <<= 8;
        c += data[2] as u32;
        c <<= 8;
        c += data[3] as u32;
        Ok((c, 4))
    } else if c & 0xF8 == 0xF0 {
        c = data[1] as u32;
        c <<= 8;
        c += data[2] as u32;
        c <<= 8;
        c += data[3] as u32;
        c <<= 8;
        c += data[4] as u32;
        Ok((c, 5))
    } else {
        Err(SentenceError::PrefixLength)
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::word::{Word, WordCategory};
    use uuid::Uuid;

    use super::*;

    const TEST_UUID1: Uuid = Uuid::from_bytes([
        0xa1, 0xa2, 0xa3, 0xa4, 0xb1, 0xb2, 0xc1, 0xc2, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7,
        0xd8,
    ]);
    const TEST_UUID2: Uuid = Uuid::from_bytes([
        0xb1, 0xb2, 0xb3, 0xb4, 0xc1, 0xc2, 0xd1, 0xd2, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7,
        0xe8,
    ]);

    /// Build wire-format sentence data from a list of word byte slices.
    /// Each word is prefixed with a single-byte length, and a 0x00 terminator is appended.
    fn build_sentence(words: &[&[u8]]) -> Vec<u8> {
        let mut data = Vec::new();
        for word in words {
            let len = word.len();
            assert!(
                len < 0x80,
                "Word too long for single-byte length prefix in test helper"
            );
            data.push(len as u8);
            data.extend_from_slice(word);
        }
        data.push(0); // terminator
        data
    }

    #[test]
    fn test_sentence_iterator() {
        let data = build_sentence(&[
            b"!done",
            b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8",
            b"=name=ether1",
        ]);

        let mut sentence = Sentence::new(&data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(WordCategory::Done)
        );

        assert_eq!(sentence.next().unwrap().unwrap(), Word::Tag(TEST_UUID1));

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Attribute(("name", Some("ether1")).into())
        );

        assert_eq!(sentence.next(), None);
    }

    #[test]
    fn test_sentence_category_error() {
        // Test case where the first word has a wrong length prefix, causing garbled data
        let data: &[u8] = &[
            0x0A, b'.', b't', b'a', b'g', b'=', b'1', b'2',
            b'3', // Malformed: length says 10 but .tag=123 is 8 bytes
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
        let data = build_sentence(&[
            b"!done",
            b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8",
            b"=name=ether1",
        ]);

        let mut sentence = Sentence::new(&data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(WordCategory::Done)
        );

        assert_eq!(sentence.next().unwrap().unwrap(), Word::Tag(TEST_UUID1));

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Attribute(("name", Some("ether1")).into())
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
        // Test case where the first word has a wrong length prefix
        let data: &[u8] = &[
            0x0A, b'.', b't', b'a', b'g', b'=', b'1', b'2',
            b'3', // Malformed: length says 10 but .tag=123 is 8 bytes
            0x0D, b'=', b'n', b'a', b'm', b'e', b'=', b'e', b't', b'h', b'e', b'r',
            b'1', // Word: =name=ether1
        ];

        let mut sentence = Sentence::new(data);

        assert!(sentence.next().unwrap().is_err());
    }

    #[test]
    fn test_mixed_words_sentence() {
        let data = build_sentence(&[
            b"!re",
            b"=a=b",
            b".tag=b1b2b3b4-c1c2-d1d2-e1e2-e3e4e5e6e7e8",
        ]);

        let mut sentence = Sentence::new(&data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(WordCategory::Reply)
        );

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Attribute(("a", Some("b")).into())
        );

        assert_eq!(sentence.next().unwrap().unwrap(), Word::Tag(TEST_UUID2));

        assert_eq!(sentence.next(), None);
    }

    #[test]
    fn test_sentence_with_fatal_message() {
        let data = build_sentence(&[b"!fatal", b"server down"]);

        let mut sentence = Sentence::new(&data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(WordCategory::Fatal)
        );

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Message("server down")
        );

        assert_eq!(sentence.next(), None);
    }

    #[test]
    fn test_complete_sentence_with_extra_data() {
        let mut data = build_sentence(&[
            b"!done",
            b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8",
            b"=name=ether1",
        ]);
        // Append extra data after the sentence terminator
        data.extend_from_slice(&[0x07, b'!', b'd', b'o', b'n', b'e']);

        let mut sentence = Sentence::new(&data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(WordCategory::Done)
        );

        assert_eq!(sentence.next().unwrap().unwrap(), Word::Tag(TEST_UUID1));

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Attribute(("name", Some("ether1")).into())
        );

        assert_eq!(sentence.next(), None);

        // Confirm that extra data is ignored after the end of the sentence
        assert_eq!(sentence.next(), None);
    }

    #[test]
    fn test_sentence_with_empty_response() {
        let data = build_sentence(&[b"!empty", b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8"]);

        let mut sentence = Sentence::new(&data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(WordCategory::Empty)
        );

        assert_eq!(sentence.next().unwrap().unwrap(), Word::Tag(TEST_UUID1));

        assert_eq!(sentence.next(), None);
    }
}
