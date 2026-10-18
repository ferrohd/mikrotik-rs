//! Outbound sentence assembly.
//!
//! A word's length prefix must agree with the bytes after it, so the length is
//! never written by a caller: [`Sentence::parts`] sums it from the content.

use alloc::vec::Vec;

use crate::codec;
use crate::command::QueryOperator;
use crate::tag::Tag;

/// A word of an outbound sentence, described rather than encoded.
///
/// Attributes and queries stay separate because they read `None` differently:
/// an attribute without a value is omitted, while `?key` is a presence test
/// and `?key=` asks for the empty string.
#[derive(Debug, Clone, Copy)]
pub(crate) enum OutWord<'a> {
    /// `/interface/print`
    Command(&'a str),
    /// `.tag=0000000042`
    Tag(Tag),
    /// `=key=value`; an empty value is a flag
    Attribute { key: &'a str, value: &'a [u8] },
    /// `?key`
    Present(&'a str),
    /// `?-key`
    Absent(&'a str),
    /// `?key=value`
    Equal { key: &'a str, value: &'a str },
    /// `?>key=value`
    Greater { key: &'a str, value: &'a str },
    /// `?<key=value`
    Less { key: &'a str, value: &'a str },
    /// `?#` followed by operator codes
    Operations(&'a [QueryOperator]),
}

/// An outbound sentence under construction. Wraps the buffer so no caller can
/// push a raw byte or forget the terminator.
#[derive(Debug, Clone, Default)]
pub(crate) struct Sentence(Vec<u8>);

impl Sentence {
    pub(crate) fn word(&mut self, word: OutWord<'_>) {
        match word {
            OutWord::Command(p) => self.parts(&[p.as_bytes()]),
            OutWord::Tag(tag) => self.parts(&[b".tag=", &tag.to_ascii()]),
            OutWord::Attribute { key, value } => self.parts(&[b"=", key.as_bytes(), b"=", value]),
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
            // Needs a scratch buffer: an operator's byte cannot be projected
            // out of the slice without one, and `unsafe_code` is forbidden.
            OutWord::Operations(ops) => {
                let codes: Vec<u8> = ops.iter().copied().map(QueryOperator::code).collect();
                self.parts(&[b"?#", &codes]);
            }
        }
    }

    /// # Panics
    ///
    /// If the parts total more than [`u32::MAX`] bytes.
    fn parts(&mut self, parts: &[&[u8]]) {
        let len: usize = parts.iter().map(|part| part.len()).sum();
        let len = u32::try_from(len).expect("word length exceeds u32::MAX");
        codec::encode_length(len, &mut self.0);
        for part in parts {
            self.0.extend_from_slice(part);
        }
    }

    pub(crate) fn finish(mut self) -> Vec<u8> {
        codec::encode_terminator(&mut self.0);
        self.0
    }
}
