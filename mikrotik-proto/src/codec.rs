//! Wire-format codec for MikroTik API length-prefixed words and sentences.
//!
//! The MikroTik RouterOS API uses a binary, length-prefixed protocol.
//! Each *word* is encoded as a variable-length prefix (1-5 bytes) followed
//! by the word's content bytes. A *sentence* is a sequence of words terminated
//! by a zero-length word (a single `0x00` byte).
//!
//! # Length encoding scheme
//!
//! | Value range            | Prefix bytes | Encoding                                    |
//! |------------------------|-------------|---------------------------------------------|
//! | `0x00 ..= 0x7F`       | 1           | `[len]`                                     |
//! | `0x80 ..= 0x3FFF`     | 2           | `[0x80 \| (len >> 8), len & 0xFF]`          |
//! | `0x4000 ..= 0x1FFFFF` | 3           | `[0xC0 \| (len >> 16), (len >> 8), len]`    |
//! | `0x200000 ..= 0xFFFFFFF` | 4        | `[0xE0 \| (len >> 24), (len >> 16), ...]`   |
//! | `>= 0x10000000`       | 5           | `[0xF0, (len >> 24), (len >> 16), ...]`     |
//!
//! # Design
//!
//! This codec is **stateless** and performs **no I/O**. Functions operate on
//! byte slices and return either a result with bytes consumed or an
//! `Incomplete` status indicating more data is needed. This follows the
//! `httparse::Status` pattern.

use alloc::vec::Vec;
use core::num::NonZeroUsize;

use crate::error::DecodeError;

/// Result of attempting to decode a frame from a byte buffer.
///
/// This follows the `httparse::Status` pattern — the canonical Rust idiom
/// for incremental, zero-copy parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decode<T> {
    /// A complete value was decoded from the buffer.
    Complete {
        /// The decoded value.
        value: T,
        /// The number of bytes consumed from the input buffer.
        bytes_consumed: usize,
    },
    /// The buffer does not contain enough data to decode a complete value.
    Incomplete {
        /// Minimum number of additional bytes needed, if known.
        needed: Option<NonZeroUsize>,
    },
}

impl<T> Decode<T> {
    /// Returns `true` if the decode was successful.
    pub fn is_complete(&self) -> bool {
        matches!(self, Decode::Complete { .. })
    }

    /// Returns `true` if more data is needed.
    pub fn is_incomplete(&self) -> bool {
        matches!(self, Decode::Incomplete { .. })
    }
}

/// A decoded sentence represented as word spans into the source buffer.
///
/// This is a zero-copy type: all word data is referenced by offset and length
/// within the original `&[u8]` that was passed to [`decode_sentence`].
#[derive(Debug)]
pub struct RawSentence<'a> {
    /// The source buffer this sentence was decoded from.
    data: &'a [u8],
    /// Spans of (offset, length) for each word in the sentence.
    words: Vec<(usize, usize)>,
}

impl<'a> RawSentence<'a> {
    /// Iterate over the raw byte slices of each word in the sentence.
    pub fn words(&self) -> impl Iterator<Item = &'a [u8]> + '_ {
        self.words
            .iter()
            .map(|&(offset, len)| &self.data[offset..offset + len])
    }

    /// Returns the number of words in this sentence.
    pub fn word_count(&self) -> usize {
        self.words.len()
    }

    /// Returns `true` if the sentence contains no words.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}

/// Decode a variable-length integer from the MikroTik wire format.
///
/// Returns the decoded length and the number of prefix bytes consumed,
/// or `Incomplete` if the buffer doesn't contain enough bytes.
///
/// # Errors
///
/// Returns [`DecodeError::InvalidLengthPrefix`] if the first byte has
/// an unrecognized prefix pattern (i.e., bits `0xF8` are all set).
pub fn decode_length(data: &[u8]) -> Result<Decode<(u32, usize)>, DecodeError> {
    if data.is_empty() {
        return Ok(Decode::Incomplete {
            needed: NonZeroUsize::new(1),
        });
    }

    let c = data[0] as u32;
    match c {
        c if c & 0x80 == 0x00 => Ok(Decode::Complete {
            value: (c, 1),
            bytes_consumed: 1,
        }),
        c if c & 0xC0 == 0x80 => {
            if data.len() < 2 {
                return Ok(Decode::Incomplete {
                    needed: NonZeroUsize::new(1),
                });
            }
            let val = ((c & !0xC0) << 8) | data[1] as u32;
            Ok(Decode::Complete {
                value: (val, 2),
                bytes_consumed: 2,
            })
        }
        c if c & 0xE0 == 0xC0 => {
            if data.len() < 3 {
                return Ok(Decode::Incomplete {
                    needed: NonZeroUsize::new(3 - data.len()),
                });
            }
            let val = ((c & !0xE0) << 16) | ((data[1] as u32) << 8) | data[2] as u32;
            Ok(Decode::Complete {
                value: (val, 3),
                bytes_consumed: 3,
            })
        }
        c if c & 0xF0 == 0xE0 => {
            if data.len() < 4 {
                return Ok(Decode::Incomplete {
                    needed: NonZeroUsize::new(4 - data.len()),
                });
            }
            let val = ((c & !0xF0) << 24)
                | ((data[1] as u32) << 16)
                | ((data[2] as u32) << 8)
                | data[3] as u32;
            Ok(Decode::Complete {
                value: (val, 4),
                bytes_consumed: 4,
            })
        }
        c if c & 0xF8 == 0xF0 => {
            let _ = c; // first byte is just the marker
            if data.len() < 5 {
                return Ok(Decode::Incomplete {
                    needed: NonZeroUsize::new(5 - data.len()),
                });
            }
            let val = ((data[1] as u32) << 24)
                | ((data[2] as u32) << 16)
                | ((data[3] as u32) << 8)
                | data[4] as u32;
            Ok(Decode::Complete {
                value: (val, 5),
                bytes_consumed: 5,
            })
        }
        _ => Err(DecodeError::InvalidLengthPrefix(data[0])),
    }
}

/// Attempt to decode one complete sentence from the input buffer.
///
/// A sentence is a sequence of length-prefixed words terminated by a
/// zero-length word (a single `0x00` byte).
///
/// # Returns
///
/// - `Ok(Decode::Complete { value, bytes_consumed })` — a full sentence was decoded.
///   The caller should advance the buffer by `bytes_consumed`.
/// - `Ok(Decode::Incomplete { needed })` — more data is needed to complete the sentence.
/// - `Err(DecodeError)` — the data contains a malformed length prefix.
pub fn decode_sentence(src: &[u8]) -> Result<Decode<RawSentence<'_>>, DecodeError> {
    let mut pos = 0;
    let mut word_spans = Vec::new();

    loop {
        if pos >= src.len() {
            return Ok(Decode::Incomplete {
                needed: NonZeroUsize::new(1),
            });
        }

        match decode_length(&src[pos..])? {
            Decode::Complete {
                value: (length, prefix_len),
                ..
            } => {
                if length == 0 {
                    // Sentence terminator found
                    let consumed = pos + prefix_len;
                    return Ok(Decode::Complete {
                        value: RawSentence {
                            data: src,
                            words: word_spans,
                        },
                        bytes_consumed: consumed,
                    });
                }

                let word_start = pos + prefix_len;
                let word_end = word_start + length as usize;

                if word_end > src.len() {
                    let needed = word_end - src.len();
                    return Ok(Decode::Incomplete {
                        needed: NonZeroUsize::new(needed),
                    });
                }

                word_spans.push((word_start, length as usize));
                pos = word_end;
            }
            Decode::Incomplete { needed } => {
                return Ok(Decode::Incomplete { needed });
            }
        }
    }
}

/// Encode a variable-length prefix into the destination buffer.
///
/// Appends the encoded length prefix (1-5 bytes) to `dst`.
pub fn encode_length(len: u32, dst: &mut Vec<u8>) {
    match len {
        0x00..=0x7F => {
            dst.push(len as u8);
        }
        0x80..=0x3FFF => {
            let l = len | 0x8000;
            dst.push(((l >> 8) & 0xFF) as u8);
            dst.push((l & 0xFF) as u8);
        }
        0x4000..=0x1FFFFF => {
            let l = len | 0xC00000;
            dst.push(((l >> 16) & 0xFF) as u8);
            dst.push(((l >> 8) & 0xFF) as u8);
            dst.push((l & 0xFF) as u8);
        }
        0x200000..=0xFFFFFFF => {
            let l = len | 0xE0000000;
            dst.push(((l >> 24) & 0xFF) as u8);
            dst.push(((l >> 16) & 0xFF) as u8);
            dst.push(((l >> 8) & 0xFF) as u8);
            dst.push((l & 0xFF) as u8);
        }
        _ => {
            dst.push(0xF0);
            dst.push(((len >> 24) & 0xFF) as u8);
            dst.push(((len >> 16) & 0xFF) as u8);
            dst.push(((len >> 8) & 0xFF) as u8);
            dst.push((len & 0xFF) as u8);
        }
    }
}

/// Encode a word (length prefix + content bytes) into the destination buffer.
///
/// Appends the length-prefixed word to `dst`.
pub fn encode_word(word: &[u8], dst: &mut Vec<u8>) {
    encode_length(word.len() as u32, dst);
    dst.extend_from_slice(word);
}

/// Encode a sentence terminator (zero-length word) into the destination buffer.
pub fn encode_terminator(dst: &mut Vec<u8>) {
    dst.push(0x00);
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn test_decode_length_1_byte() {
        let data = [0x7F];
        let result = decode_length(&data).unwrap();
        assert_eq!(
            result,
            Decode::Complete {
                value: (0x7F, 1),
                bytes_consumed: 1,
            }
        );
    }

    #[test]
    fn test_decode_length_2_bytes() {
        let data = [0x80, 0x80];
        let result = decode_length(&data).unwrap();
        assert_eq!(
            result,
            Decode::Complete {
                value: (0x80, 2),
                bytes_consumed: 2,
            }
        );
    }

    #[test]
    fn test_decode_length_3_bytes() {
        let data = [0xC0, 0x40, 0x00];
        let result = decode_length(&data).unwrap();
        assert_eq!(
            result,
            Decode::Complete {
                value: (0x4000, 3),
                bytes_consumed: 3,
            }
        );
    }

    #[test]
    fn test_decode_length_4_bytes() {
        let data = [0xE0, 0x20, 0x00, 0x00];
        let result = decode_length(&data).unwrap();
        assert_eq!(
            result,
            Decode::Complete {
                value: (0x200000, 4),
                bytes_consumed: 4,
            }
        );
    }

    #[test]
    fn test_decode_length_5_bytes() {
        let data = [0xF0, 0x10, 0x00, 0x00, 0x00];
        let result = decode_length(&data).unwrap();
        assert_eq!(
            result,
            Decode::Complete {
                value: (0x10000000, 5),
                bytes_consumed: 5,
            }
        );
    }

    #[test]
    fn test_decode_length_invalid_prefix() {
        let data = [0xF8];
        assert!(decode_length(&data).is_err());
    }

    #[test]
    fn test_decode_length_incomplete_empty() {
        let data: &[u8] = &[];
        let result = decode_length(data).unwrap();
        assert!(result.is_incomplete());
    }

    #[test]
    fn test_decode_length_incomplete_2_byte() {
        let data = [0x80]; // needs 2 bytes, only have 1
        let result = decode_length(&data).unwrap();
        assert!(result.is_incomplete());
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let test_values: &[u32] = &[
            0, 1, 0x7F, 0x80, 0x3FFF, 0x4000, 0x1FFFFF, 0x200000, 0xFFFFFFF, 0x10000000,
        ];
        for &val in test_values {
            let mut buf = Vec::new();
            encode_length(val, &mut buf);
            let result = decode_length(&buf).unwrap();
            match result {
                Decode::Complete {
                    value: (decoded, prefix_len),
                    ..
                } => {
                    assert_eq!(decoded, val, "roundtrip failed for value {val:#X}");
                    assert_eq!(prefix_len, buf.len(), "prefix len mismatch for {val:#X}");
                }
                Decode::Incomplete { .. } => panic!("unexpected Incomplete for {val:#X}"),
            }
        }
    }

    #[test]
    fn test_encode_word() {
        let mut buf = Vec::new();
        encode_word(b"test", &mut buf);
        assert_eq!(buf, vec![0x04, b't', b'e', b's', b't']);
    }

    /// Build wire-format sentence data from a list of word byte slices.
    fn build_sentence(words: &[&[u8]]) -> Vec<u8> {
        let mut data = Vec::new();
        for word in words {
            encode_word(word, &mut data);
        }
        encode_terminator(&mut data);
        data
    }

    #[test]
    fn test_decode_sentence_complete() {
        let data = build_sentence(&[b"!done", b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8"]);
        let result = decode_sentence(&data).unwrap();
        match result {
            Decode::Complete {
                value: raw,
                bytes_consumed,
            } => {
                assert_eq!(bytes_consumed, data.len());
                assert_eq!(raw.word_count(), 2);
                let words: Vec<_> = raw.words().collect();
                assert_eq!(words[0], b"!done");
                assert_eq!(words[1], b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8");
            }
            Decode::Incomplete { .. } => panic!("expected Complete"),
        }
    }

    #[test]
    fn test_decode_sentence_incomplete() {
        let data = build_sentence(&[b"!done", b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8"]);
        // Feed only partial data (cut off the last byte — the terminator)
        let partial = &data[..data.len() - 1];
        let result = decode_sentence(partial).unwrap();
        assert!(result.is_incomplete());
    }

    #[test]
    fn test_decode_sentence_multiple() {
        let s1 = build_sentence(&[b"!done", b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8"]);
        let s2 = build_sentence(&[b"!re", b"=name=ether1"]);
        let mut combined = Vec::new();
        combined.extend_from_slice(&s1);
        combined.extend_from_slice(&s2);

        // First decode should get s1
        let result = decode_sentence(&combined).unwrap();
        match result {
            Decode::Complete {
                value: raw,
                bytes_consumed,
            } => {
                assert_eq!(bytes_consumed, s1.len());
                assert_eq!(raw.word_count(), 2);

                // Second decode from remaining bytes should get s2
                let result2 = decode_sentence(&combined[bytes_consumed..]).unwrap();
                match result2 {
                    Decode::Complete {
                        value: raw2,
                        bytes_consumed: bc2,
                    } => {
                        assert_eq!(bc2, s2.len());
                        assert_eq!(raw2.word_count(), 2);
                    }
                    _ => panic!("expected Complete for second sentence"),
                }
            }
            _ => panic!("expected Complete for first sentence"),
        }
    }

    #[test]
    fn test_decode_sentence_empty_input() {
        let result = decode_sentence(&[]).unwrap();
        assert!(result.is_incomplete());
    }
}
