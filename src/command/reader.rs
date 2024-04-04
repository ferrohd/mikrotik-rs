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
    type Item = Result<&'a str, SentenceError>;

    /// Advances the iterator and returns the next word in the sentence.
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

                // Convert the bytes to a string
                match std::str::from_utf8(&self.data[start..end]) {
                    Ok(s) => Some(Ok(s)),
                    Err(e) => Some(Err(e.into())),
                }
            }
            Err(e) => Some(Err(e)),
        }
    }
}

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

/// Parses a command tag from a given string slice.
///
/// The tag is expected to be in the form of `.tag=<value>`, where `<value>` is a
/// numeric string representing the tag's value.
///
/// # Arguments
///
/// * `str` - A string slice containing the tag to be parsed.
///
/// # Errors
///
/// Returns a `ParsingError::Tag` error if the tag format is incorrect or if the
/// tag value is not a valid unsigned 16-bit integer.
pub fn parse_tag(str: &str) -> Result<u16, ParsingError> {
    // The tag in in the form of ".tag=1234"
    let tag = str
        .split('=')
        .nth(1)
        .ok_or(ParsingError::Tag(TagError::Missing))?
        .parse()
        .map_err(|e| ParsingError::Tag(TagError::Invalid(e)))?;

    Ok(tag)
}
