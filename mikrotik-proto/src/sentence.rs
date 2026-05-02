//! Sentence parsing: zero-copy iteration over decoded wire-format sentences.
//!
//! A sentence is a sequence of [`Word`]s decoded from the MikroTik wire format.
//! This module provides the [`Sentence`] iterator that lazily parses words from
//! a [`RawSentence`](crate::codec::RawSentence) produced by the codec layer.

use crate::codec::RawSentence;
use crate::error::SentenceError;
use crate::word::Word;

/// An iterator that lazily parses [`Word`]s from a [`RawSentence`].
///
/// Each call to [`next()`](Iterator::next) takes the next raw word byte slice
/// from the sentence and attempts to parse it into a typed [`Word`].
///
/// This is zero-copy: `Word<'a>` borrows from the original packet buffer.
#[derive(Debug)]
pub struct Sentence<'a> {
    words: alloc::vec::Vec<&'a [u8]>,
    position: usize,
}

impl<'a> Sentence<'a> {
    /// Create a new sentence iterator from a decoded [`RawSentence`].
    pub fn new(raw: &RawSentence<'a>) -> Self {
        Self {
            words: raw.words().collect(),
            position: 0,
        }
    }

    /// Create a sentence iterator directly from raw word byte slices.
    ///
    /// This is useful for testing or when you have pre-split word data.
    pub fn from_word_slices(words: alloc::vec::Vec<&'a [u8]>) -> Self {
        Self { words, position: 0 }
    }
}

impl<'a> Iterator for Sentence<'a> {
    type Item = Result<Word<'a>, SentenceError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.words.len() {
            return None;
        }

        let word_bytes = self.words[self.position];
        self.position += 1;

        Some(Word::try_from(word_bytes).map_err(SentenceError::from))
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::codec;
    use crate::tag::Tag;
    use crate::word::{WordAttribute, WordCategory};

    const TEST_TAG1: Tag = Tag::from_uuid(Uuid::from_bytes([
        0xa1, 0xa2, 0xa3, 0xa4, 0xb1, 0xb2, 0xc1, 0xc2, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7,
        0xd8,
    ]));
    const TEST_TAG2: Tag = Tag::from_uuid(Uuid::from_bytes([
        0xb1, 0xb2, 0xb3, 0xb4, 0xc1, 0xc2, 0xd1, 0xd2, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7,
        0xe8,
    ]));

    /// Build wire-format sentence data from a list of word byte slices.
    fn build_sentence(words: &[&[u8]]) -> alloc::vec::Vec<u8> {
        let mut data = alloc::vec::Vec::new();
        for word in words {
            codec::encode_word(word, &mut data);
        }
        codec::encode_terminator(&mut data);
        data
    }

    fn decode_and_iterate<'a>(data: &'a [u8]) -> Sentence<'a> {
        match codec::decode_sentence(data).unwrap() {
            codec::Decode::Complete { value: raw, .. } => Sentence::new(&raw),
            codec::Decode::Incomplete { .. } => panic!("expected complete sentence"),
        }
    }

    #[test]
    fn test_sentence_iterator() {
        let data = build_sentence(&[
            b"!done",
            b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8",
            b"=name=ether1",
        ]);

        let mut sentence = decode_and_iterate(&data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(WordCategory::Done)
        );
        assert_eq!(sentence.next().unwrap().unwrap(), Word::Tag(TEST_TAG1));
        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Attribute(WordAttribute {
                key: "name",
                value: Some("ether1"),
                value_raw: Some(b"ether1"),
            })
        );
        assert!(sentence.next().is_none());
    }

    #[test]
    fn test_mixed_words_sentence() {
        let data = build_sentence(&[
            b"!re",
            b"=a=b",
            b".tag=b1b2b3b4-c1c2-d1d2-e1e2-e3e4e5e6e7e8",
        ]);

        let mut sentence = decode_and_iterate(&data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(WordCategory::Reply)
        );
        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Attribute(WordAttribute {
                key: "a",
                value: Some("b"),
                value_raw: Some(b"b"),
            })
        );
        assert_eq!(sentence.next().unwrap().unwrap(), Word::Tag(TEST_TAG2));
        assert!(sentence.next().is_none());
    }

    #[test]
    fn test_sentence_with_fatal_message() {
        let data = build_sentence(&[b"!fatal", b"server down"]);

        let mut sentence = decode_and_iterate(&data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(WordCategory::Fatal)
        );
        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Message("server down")
        );
        assert!(sentence.next().is_none());
    }

    #[test]
    fn test_sentence_with_empty_response() {
        let data = build_sentence(&[b"!empty", b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8"]);

        let mut sentence = decode_and_iterate(&data);

        assert_eq!(
            sentence.next().unwrap().unwrap(),
            Word::Category(WordCategory::Empty)
        );
        assert_eq!(sentence.next().unwrap().unwrap(), Word::Tag(TEST_TAG1));
        assert!(sentence.next().is_none());
    }
}
