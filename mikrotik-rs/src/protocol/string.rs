use std::fmt::{Debug, Display, Formatter, Write};
use std::ops::Deref;
use std::str::FromStr;

/// A reference to a byte array, treated as ascii string
#[derive(PartialEq, Eq, Clone, Copy, Default, Hash, Ord, PartialOrd)]
pub struct AsciiStringRef<'a>(pub &'a [u8]);
/// A byte array used as a string with only ascii characters
#[derive(PartialEq, Eq, Clone, Default, Hash, Ord, PartialOrd)]
pub struct AsciiString(pub Box<[u8]>);

impl<'a> From<&'a [u8]> for AsciiStringRef<'a> {
    fn from(value: &'a [u8]) -> Self {
        AsciiStringRef(value)
    }
}
impl<'a, const N: usize> From<&'a [u8; N]> for AsciiStringRef<'a> {
    fn from(value: &'a [u8; N]) -> Self {
        AsciiStringRef(value)
    }
}

/// An error on creating a ascii string
#[derive(Debug)]
pub enum EncodingError {
    /// Min 1 character is not an ascii character
    NonAsciiCharacterFound {
        ///  position of the first problem
        pos: usize,
        ///  first non ascii character
        character: char,
    },
}
impl<'a> TryFrom<&'a str> for AsciiStringRef<'a> {
    type Error = EncodingError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        if value.is_ascii() {
            Ok(AsciiStringRef(value.as_bytes()))
        } else {
            let (pos, character) = value
                .chars()
                .enumerate()
                .find(|(_, c)| !c.is_ascii())
                .expect("There must be a non ascii character wenn value.is_ascii() returned false");
            Err(EncodingError::NonAsciiCharacterFound { pos, character })
        }
    }
}
impl AsciiString {
    /// derive a AsciiStringRef out of this AsciiString
    pub fn as_deref(&self) -> AsciiStringRef<'_> {
        AsciiStringRef(self.0.as_ref())
    }
}
impl AsciiStringRef<'_> {
    /// parse anything out of this AsciiStringRef
    pub fn parse<R: FromStr>(&self) -> Result<R, R::Err> {
        String::from_utf8_lossy(self.0).parse::<R>()
    }
    /// copy content into a AsciiString
    pub fn to_ascii_string(&self) -> AsciiString {
        AsciiString(Box::from(self.0))
    }
}

impl TryFrom<AsciiString> for String {
    type Error = EncodingError;

    fn try_from(value: AsciiString) -> Result<Self, Self::Error> {
        if value.0.is_ascii() {
            Ok(String::from_utf8_lossy(value.0.as_ref()).into_owned())
        } else {
            let (pos, character) = value
                .0
                .iter()
                .enumerate()
                .find(|(_, c)| !c.is_ascii())
                .map(|(i, c)| (i, *c as char))
                .expect("There must be a non ascii character wenn value.is_ascii() returned false");
            Err(EncodingError::NonAsciiCharacterFound { pos, character })
        }
    }
}
impl TryFrom<AsciiString> for Box<str> {
    type Error = EncodingError;

    fn try_from(value: AsciiString) -> Result<Self, Self::Error> {
        if value.0.is_ascii() {
            Ok(String::from_utf8_lossy(value.0.as_ref()).into())
        } else {
            let (pos, character) = value
                .0
                .iter()
                .enumerate()
                .find(|(_, c)| !c.is_ascii())
                .map(|(i, c)| (i, *c as char))
                .expect("There must be a non ascii character wenn value.is_ascii() returned false");
            Err(EncodingError::NonAsciiCharacterFound { pos, character })
        }
    }
}

impl Deref for AsciiString {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Deref for AsciiStringRef<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

fn format_mikrotik_bytes(f: &mut Formatter, bytes: &[u8]) -> std::fmt::Result {
    for byte in bytes {
        if byte.is_ascii() {
            f.write_char(*byte as char)?
        } else {
            write!(f, "\\x{byte:02x}")?
        }
    }
    Ok(())
}

impl Debug for AsciiString {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        format_mikrotik_bytes(f, &self.0)
    }
}

impl Debug for AsciiStringRef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        format_mikrotik_bytes(f, self.0)
    }
}
impl Display for AsciiString {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        format_mikrotik_bytes(f, &self.0)
    }
}

impl Display for AsciiStringRef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        format_mikrotik_bytes(f, self.0)
    }
}
const EMPTY_STR: AsciiStringRef<'static> = AsciiStringRef(&[]);
impl Default for &AsciiStringRef<'_> {
    fn default() -> Self {
        &EMPTY_STR
    }
}
/// a data type can be written as a word into miktrotik API
pub trait WordContent {
    /// count of bytes to be written
    fn byte_count(&self) -> usize;
    /// write the bytes
    fn write_to_buffer(&self, buffer: &mut Vec<u8>);
}
impl WordContent for [u8] {
    fn byte_count(&self) -> usize {
        self.len()
    }
    fn write_to_buffer(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(self);
    }
}
impl WordContent for &[u8] {
    fn byte_count(&self) -> usize {
        self.len()
    }
    fn write_to_buffer(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(self);
    }
}
impl<const N: usize> WordContent for [u8; N] {
    fn byte_count(&self) -> usize {
        N
    }
    fn write_to_buffer(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(self);
    }
}
impl WordContent for [&[u8]] {
    fn byte_count(&self) -> usize {
        self.iter().map(|x| x.byte_count()).sum()
    }
    fn write_to_buffer(&self, buffer: &mut Vec<u8>) {
        for segment in self.iter() {
            buffer.extend_from_slice(segment);
        }
    }
}
impl<const N: usize> WordContent for [&[u8]; N] {
    fn byte_count(&self) -> usize {
        self.iter().map(|x| x.byte_count()).sum()
    }
    fn write_to_buffer(&self, buffer: &mut Vec<u8>) {
        for segment in self.iter() {
            buffer.extend_from_slice(segment);
        }
    }
}
impl WordContent for &str {
    fn byte_count(&self) -> usize {
        self.bytes().len()
    }
    fn write_to_buffer(&self, buffer: &mut Vec<u8>) {
        assert!(
            self.is_ascii(),
            "There is a non ascii character in the string"
        );
        buffer.extend_from_slice(self.as_bytes());
    }
}
