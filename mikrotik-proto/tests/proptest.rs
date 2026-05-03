//! Property-based tests for mikrotik-proto.
//!
//! These tests use proptest to verify invariants with randomized inputs:
//! - Encode/decode roundtrips always produce the original value.
//! - Decoders never panic on arbitrary byte sequences.
//! - The connection state machine never panics on arbitrary input.

use proptest::prelude::*;

use mikrotik_proto::codec::{self, Decode};
use mikrotik_proto::connection::Connection;

// ── Codec roundtrip properties ──

proptest! {
    /// encode_length → decode_length always roundtrips for any u32.
    #[test]
    fn codec_length_roundtrip(len in any::<u32>()) {
        let mut buf = Vec::new();
        codec::encode_length(len, &mut buf);

        match codec::decode_length(&buf).unwrap() {
            Decode::Complete { value: (decoded_len, prefix_bytes), bytes_consumed } => {
                prop_assert_eq!(decoded_len, len);
                prop_assert_eq!(bytes_consumed, buf.len());
                prop_assert_eq!(prefix_bytes, buf.len());
            }
            Decode::Incomplete { .. } => {
                prop_assert!(false, "encoded length should always decode completely");
            }
        }
    }

    /// encode_word → decode_sentence roundtrips: the decoded word matches the input.
    /// Words must be non-empty: a zero-length word is the sentence terminator.
    #[test]
    fn codec_word_roundtrip(word in proptest::collection::vec(any::<u8>(), 1..1024)) {
        let mut buf = Vec::new();
        codec::encode_word(&word, &mut buf);
        codec::encode_terminator(&mut buf);

        match codec::decode_sentence(&buf).unwrap() {
            Decode::Complete { value: raw, .. } => {
                let words: Vec<&[u8]> = raw.words().collect();
                prop_assert_eq!(words.len(), 1);
                prop_assert_eq!(words[0], &word[..]);
            }
            Decode::Incomplete { .. } => {
                prop_assert!(false, "complete word + terminator should decode");
            }
        }
    }

    /// Multiple words roundtrip through encode → decode.
    /// Words must be non-empty: a zero-length word is the sentence terminator.
    #[test]
    fn codec_multi_word_roundtrip(
        words in proptest::collection::vec(
            proptest::collection::vec(any::<u8>(), 1..256),
            1..10
        )
    ) {
        let mut buf = Vec::new();
        for w in &words {
            codec::encode_word(w, &mut buf);
        }
        codec::encode_terminator(&mut buf);

        match codec::decode_sentence(&buf).unwrap() {
            Decode::Complete { value: raw, .. } => {
                let decoded: Vec<&[u8]> = raw.words().collect();
                prop_assert_eq!(decoded.len(), words.len());
                for (decoded_word, original_word) in decoded.iter().zip(words.iter()) {
                    prop_assert_eq!(*decoded_word, &original_word[..]);
                }
            }
            Decode::Incomplete { .. } => {
                prop_assert!(false, "complete sentence should decode");
            }
        }
    }

    /// Encoded length always produces a valid first byte in one of the 5 prefix ranges.
    #[test]
    fn encode_length_valid_prefix(len in any::<u32>()) {
        let mut buf = Vec::new();
        codec::encode_length(len, &mut buf);
        let first = buf[0];

        // Must match one of the 5 valid prefix patterns:
        // 1-byte: 0x00..=0x7F
        // 2-byte: 0x80..=0xBF
        // 3-byte: 0xC0..=0xDF
        // 4-byte: 0xE0..=0xEF
        // 5-byte: 0xF0..=0xF7 (spec says 0xF0..=0xF8 but only 0xF0 is used)
        let valid = first <= 0x7F
            || (0x80..=0xBF).contains(&first)
            || (0xC0..=0xDF).contains(&first)
            || (0xE0..=0xEF).contains(&first)
            || first == 0xF0;
        prop_assert!(valid, "invalid prefix byte: 0x{first:02X} for len 0x{len:X}");
    }
}

// ── No-panic properties (arbitrary input) ──

proptest! {
    /// decode_length never panics on arbitrary bytes.
    /// It may return Ok(Incomplete), Ok(Complete), or Err — but never panics.
    #[test]
    fn decode_length_no_panic(data in proptest::collection::vec(any::<u8>(), 0..8)) {
        let _ = codec::decode_length(&data);
    }

    /// decode_sentence never panics on arbitrary bytes.
    #[test]
    fn decode_sentence_no_panic(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let _ = codec::decode_sentence(&data);
    }

    /// Connection::receive never panics on arbitrary bytes.
    /// It may return Err (protocol/decode error) but must not panic.
    #[test]
    fn connection_receive_no_panic(data in proptest::collection::vec(any::<u8>(), 0..8192)) {
        let mut conn = Connection::new();
        // Ignore errors — we only care that it doesn't panic
        let _ = conn.receive(&data);
        // Drain any events
        while conn.poll_event().is_some() {}
    }

    /// Connection::receive with chunked arbitrary data never panics.
    /// Feed the same data in random-sized chunks.
    #[test]
    fn connection_receive_chunked_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..4096),
        chunk_sizes in proptest::collection::vec(1..64usize, 1..100),
    ) {
        let mut conn = Connection::new();
        let mut offset = 0;
        for chunk_size in chunk_sizes {
            if offset >= data.len() {
                break;
            }
            let end = (offset + chunk_size).min(data.len());
            let _ = conn.receive(&data[offset..end]);
            while conn.poll_event().is_some() {}
            offset = end;
        }
    }
}

// ── Well-formed but semantically random sentences ──

/// Generate a valid wire-format sentence with random attribute-like words.
fn arb_sentence() -> impl Strategy<Value = Vec<u8>> {
    // Category word
    let category = prop_oneof![
        Just(&b"!done"[..]),
        Just(&b"!re"[..]),
        Just(&b"!trap"[..]),
        Just(&b"!fatal"[..]),
        Just(&b"!empty"[..]),
    ];

    // Random attribute-like words
    let attrs = proptest::collection::vec(
        "[a-z]{1,10}=[a-z0-9]{0,20}".prop_map(|s| format!("={s}").into_bytes()),
        0..5,
    );

    (category, attrs).prop_map(|(cat, attrs)| {
        let mut buf = Vec::new();
        codec::encode_word(cat, &mut buf);
        for attr in &attrs {
            codec::encode_word(attr, &mut buf);
        }
        codec::encode_terminator(&mut buf);
        buf
    })
}

proptest! {
    /// Parsing a well-formed sentence never panics.
    /// It may return Err (missing tag, etc.) but must not crash.
    #[test]
    fn parse_well_formed_sentence_no_panic(sentence in arb_sentence()) {
        match codec::decode_sentence(&sentence) {
            Ok(Decode::Complete { value: raw, .. }) => {
                let _ = mikrotik_proto::response::CommandResponse::parse(&raw);
            }
            _ => {} // incomplete or error is fine
        }
    }
}
