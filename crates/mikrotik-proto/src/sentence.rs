//! Outbound sentence assembly.
//!
//! A sentence is a sequence of length-prefixed words followed by a zero-length
//! terminator. The length prefix has to agree with the bytes that follow it, and
//! the only way to guarantee that is to never state the length at all:
//! [`Sentence::parts`] sums it from the content it is about to write.
//!
//! [`OutWord`] is the other half. Every word this crate can emit is a variant,
//! and [`Sentence::word`] is the single place any of their byte layouts is
//! written. Adding a word kind makes the compiler point at the one match that
//! has to handle it, and no builder method ever touches a raw byte.

use alloc::vec::Vec;

use crate::codec;
use crate::command::QueryOperator;
use crate::tag::Tag;

/// A word of an outbound sentence, described rather than encoded.
///
/// Note that attributes and queries are deliberately *not* unified behind a
/// shared `Option<&str>` value. For an attribute, "no value" means the word is
/// omitted entirely; for a query, `?key` is a presence test and `?key=` asks
/// for the empty string. Giving them separate variants makes those two
/// readings structurally distinct rather than a convention to remember.
#[derive(Debug, Clone, Copy)]
pub(crate) enum OutWord<'a> {
    /// The command path, written bare: `/interface/print`.
    Command(&'a str),
    /// The correlation tag: `.tag=0000000042`.
    Tag(Tag),
    /// An attribute: `=key=value`. An empty value is a flag, `=key=`.
    Attribute { key: &'a str, value: &'a [u8] },
    /// Query: the property has a value, `?key`.
    Present(&'a str),
    /// Query: the property has no value, `?-key`.
    Absent(&'a str),
    /// Query: the property equals a value, `?key=value`.
    Equal { key: &'a str, value: &'a str },
    /// Query: the property is greater than a value, `?>key=value`.
    Greater { key: &'a str, value: &'a str },
    /// Query: the property is less than a value, `?<key=value`.
    Less { key: &'a str, value: &'a str },
    /// Query: stack operations, `?#` followed by operator codes.
    Operations(&'a [QueryOperator]),
}

/// An outbound sentence under construction.
///
/// Wraps the buffer so that nothing outside this module can push a raw byte
/// into it, and so the terminator is written exactly once, by [`finish`].
///
/// [`finish`]: Sentence::finish
#[derive(Debug, Clone, Default)]
pub(crate) struct Sentence(Vec<u8>);

impl Sentence {
    /// Append one word.
    pub(crate) fn word(&mut self, word: OutWord<'_>) {
        match word {
            OutWord::Command(path) => self.parts(&[path.as_bytes()]),
            OutWord::Tag(tag) => self.parts(&[b".tag=", &tag.to_ascii()]),
            OutWord::Attribute { key, value } => {
                self.parts(&[b"=", key.as_bytes(), b"=", value]);
            }
            OutWord::Present(name) => self.parts(&[b"?", name.as_bytes()]),
            OutWord::Absent(name) => self.parts(&[b"?-", name.as_bytes()]),
            OutWord::Equal { key, value } => {
                self.parts(&[b"?", key.as_bytes(), b"=", value.as_bytes()]);
            }
            OutWord::Greater { key, value } => {
                self.parts(&[b"?>", key.as_bytes(), b"=", value.as_bytes()]);
            }
            OutWord::Less { key, value } => {
                self.parts(&[b"?<", key.as_bytes(), b"=", value.as_bytes()]);
            }
            // The only arm needing a scratch buffer: an operator's wire byte
            // cannot be projected out of `&[QueryOperator]` without one, and
            // `unsafe_code = "forbid"` rules out reinterpreting the slice.
            OutWord::Operations(ops) => {
                let codes: Vec<u8> = ops.iter().copied().map(QueryOperator::code).collect();
                self.parts(&[b"?#", &codes]);
            }
        }
    }

    /// Write one word, taking its length from the content rather than a caller.
    ///
    /// # Panics
    ///
    /// Panics if the parts total more than [`u32::MAX`] bytes, which the wire
    /// format cannot represent.
    fn parts(&mut self, parts: &[&[u8]]) {
        let len: usize = parts.iter().map(|part| part.len()).sum();
        let len = u32::try_from(len).expect("word length exceeds u32::MAX");
        codec::encode_length(len, &mut self.0);
        for part in parts {
            self.0.extend_from_slice(part);
        }
    }

    /// Terminate the sentence and return the wire bytes.
    pub(crate) fn finish(mut self) -> Vec<u8> {
        codec::encode_terminator(&mut self.0);
        self.0
    }
}
