//! Response types for parsed command responses.
//!
//! The MikroTik RouterOS API produces several types of responses to commands:
//!
//! - **Done** — command completed successfully
//! - **Reply** — a data row (for commands that return tabular data)
//! - **Trap** — an error or warning
//! - **Fatal** — a fatal error that kills the connection
//! - **Empty** — no data to reply with (RouterOS 7.18+)

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};

use hashbrown::HashMap;

use crate::codec::RawSentence;
use crate::error::{MissingWord, ProtocolError, TrapCategoryError, WordType};
use crate::tag::Tag;
use crate::word::{Word, WordAttribute, WordCategory};

/// Type alias for a fatal response message.
pub type FatalResponse = String;

/// Various types of responses a command can produce.
#[derive(Debug, Clone)]
pub enum CommandResponse {
    /// Represents a successful command completion response.
    Done(DoneResponse),
    /// Represents a reply to a command, including a tag and multiple attributes.
    Reply(ReplyResponse),
    /// Represents an error or warning while executing a command.
    Trap(TrapResponse),
    /// Represents a fatal error response (affects all in-flight commands).
    Fatal(FatalResponse),
    /// Represents an empty response (introduced in `RouterOS` 7.18).
    /// Commands which do not have any data to reply with return this response.
    Empty(EmptyResponse),
}

impl CommandResponse {
    /// Returns the tag associated with the response, if available.
    ///
    /// Returns [`None`] for [`CommandResponse::Fatal`] responses as they do not contain tags.
    pub fn tag(&self) -> Option<Tag> {
        match self {
            Self::Done(d) => Some(d.tag),
            Self::Reply(r) => Some(r.tag),
            Self::Trap(t) => Some(t.tag),
            Self::Fatal(_) => None,
            Self::Empty(e) => Some(e.tag),
        }
    }

    /// Parse a [`CommandResponse`] from a [`RawSentence`].
    ///
    /// This is the primary entry point for converting decoded wire-format
    /// sentences into typed response values.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError`] if the sentence structure is invalid,
    /// contains unexpected word types, or is missing required fields.
    pub fn parse(raw: &RawSentence<'_>) -> Result<Self, ProtocolError> {
        let mut words = raw.typed_words();

        let word = words
            .next()
            .ok_or::<ProtocolError>(MissingWord::Category.into())??;

        let category = word.category().ok_or(ProtocolError::WordSequence {
            word: word.word_type(),
            expected: alloc::vec![WordType::Category],
        })?;

        match category {
            WordCategory::Done => {
                let word = words
                    .next()
                    .ok_or::<ProtocolError>(MissingWord::Tag.into())??;

                let tag = word.tag().ok_or(ProtocolError::WordSequence {
                    word: word.into(),
                    expected: alloc::vec![WordType::Tag],
                })?;
                Ok(CommandResponse::Done(DoneResponse { tag }))
            }
            WordCategory::Reply => {
                let mut tag = None;
                let mut attributes = HashMap::<String, Option<String>>::new();
                let mut attributes_raw = HashMap::<String, Option<Vec<u8>>>::new();

                for word in words {
                    let word = word?;
                    match word {
                        Word::Tag(t) => tag = Some(t),
                        Word::Attribute(WordAttribute {
                            key,
                            value,
                            value_raw,
                        }) => {
                            attributes.insert(String::from(key), value.map(String::from));
                            attributes_raw.insert(String::from(key), value_raw.map(Vec::from));
                        }
                        word => {
                            return Err(ProtocolError::WordSequence {
                                word: word.into(),
                                expected: alloc::vec![WordType::Tag, WordType::Attribute],
                            });
                        }
                    }
                }

                let tag = tag.ok_or::<ProtocolError>(MissingWord::Tag.into())?;

                Ok(CommandResponse::Reply(ReplyResponse {
                    tag,
                    attributes,
                    attributes_raw,
                }))
            }
            WordCategory::Trap => {
                let mut tag = None;
                let mut category = None;
                let mut message = None;

                for word in words {
                    let word = word?;
                    match word {
                        Word::Tag(t) => tag = Some(t),
                        Word::Attribute(WordAttribute {
                            key,
                            value,
                            value_raw: _,
                        }) => match key {
                            "category" => {
                                category = value.map(TrapCategory::try_from).transpose()?;
                            }
                            "message" => {
                                message = value.map(String::from);
                            }
                            key => {
                                return Err(TrapCategoryError::InvalidAttribute {
                                    key: String::from(key),
                                    value: value.map(String::from),
                                }
                                .into());
                            }
                        },
                        word => {
                            return Err(ProtocolError::WordSequence {
                                word: word.into(),
                                expected: alloc::vec![WordType::Tag, WordType::Attribute],
                            });
                        }
                    }
                }

                let tag = tag.ok_or::<ProtocolError>(MissingWord::Tag.into())?;
                let message = message.ok_or(TrapCategoryError::MissingMessageAttribute)?;

                Ok(CommandResponse::Trap(TrapResponse {
                    tag,
                    category,
                    message,
                }))
            }
            WordCategory::Fatal => {
                let word = words
                    .next()
                    .ok_or::<ProtocolError>(MissingWord::Message.into())??;

                let reason = word.generic().ok_or(ProtocolError::WordSequence {
                    word: word.word_type(),
                    expected: alloc::vec![WordType::Message],
                })?;

                Ok(CommandResponse::Fatal(String::from(reason)))
            }
            WordCategory::Empty => {
                let word = words
                    .next()
                    .ok_or::<ProtocolError>(MissingWord::Tag.into())??;

                let tag = word.tag().ok_or(ProtocolError::WordSequence {
                    word: word.into(),
                    expected: alloc::vec![WordType::Tag],
                })?;
                Ok(CommandResponse::Empty(EmptyResponse { tag }))
            }
        }
    }
}

/// Represents a successful command completion response.
#[derive(Debug, Clone)]
pub struct DoneResponse {
    /// The tag associated with the command.
    pub tag: Tag,
}

impl Display for DoneResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "DoneResponse {{ tag: {} }}", self.tag)
    }
}

/// Represents an empty response (`RouterOS` 7.18+).
///
/// Commands which do not have any data to reply with return this response.
#[derive(Debug, Clone)]
pub struct EmptyResponse {
    /// The tag associated with the command.
    pub tag: Tag,
}

impl Display for EmptyResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "EmptyResponse {{ tag: {} }}", self.tag)
    }
}

/// Represents a reply to a command, including a tag and multiple attributes.
#[derive(Debug, Clone)]
pub struct ReplyResponse {
    /// The tag associated with the command.
    pub tag: Tag,
    /// The attributes of the reply (UTF-8 string values).
    pub attributes: HashMap<String, Option<String>>,
    /// The raw byte attributes of the reply (for non-UTF-8 values).
    pub attributes_raw: HashMap<String, Option<Vec<u8>>>,
}

impl Display for ReplyResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ReplyResponse {{ tag: {}, attributes: {:?} }}",
            self.tag, self.attributes
        )
    }
}

/// Represents an error or warning while executing a command.
#[derive(Debug, Clone)]
pub struct TrapResponse {
    /// The tag associated with the command.
    pub tag: Tag,
    /// The category of the trap (if provided).
    pub category: Option<TrapCategory>,
    /// The message associated with the trap.
    pub message: String,
}

impl Display for TrapResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TrapResponse {{ tag: {}, category: {:?}, message: \"{}\" }}",
            self.tag, self.category, self.message
        )
    }
}

/// Categories for `TrapResponse`, defining the nature of the error.
///
/// See [MikroTik API documentation](https://help.mikrotik.com/docs/spaces/ROS/pages/47579160/API)
/// for details on each category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrapCategory {
    /// 0 — missing item or command
    MissingItemOrCommand = 0,
    /// 1 — argument value failure
    ArgumentValueFailure = 1,
    /// 2 — execution of command interrupted
    CommandExecutionInterrupted = 2,
    /// 3 — scripting related failure
    ScriptingFailure = 3,
    /// 4 — general failure
    GeneralFailure = 4,
    /// 5 — API related failure
    APIFailure = 5,
    /// 6 — TTY related failure
    TTYFailure = 6,
    /// 7 — value generated with :return command
    ReturnValue = 7,
}

impl TryFrom<u8> for TrapCategory {
    type Error = TrapCategoryError;

    fn try_from(n: u8) -> Result<Self, Self::Error> {
        match n {
            0 => Ok(TrapCategory::MissingItemOrCommand),
            1 => Ok(TrapCategory::ArgumentValueFailure),
            2 => Ok(TrapCategory::CommandExecutionInterrupted),
            3 => Ok(TrapCategory::ScriptingFailure),
            4 => Ok(TrapCategory::GeneralFailure),
            5 => Ok(TrapCategory::APIFailure),
            6 => Ok(TrapCategory::TTYFailure),
            7 => Ok(TrapCategory::ReturnValue),
            n => Err(TrapCategoryError::OutOfRange(n)),
        }
    }
}

impl TryFrom<&str> for TrapCategory {
    type Error = ProtocolError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let n = s
            .parse::<u8>()
            .map_err(|e| ProtocolError::TrapCategory(TrapCategoryError::Invalid(e)))?;
        TrapCategory::try_from(n).map_err(ProtocolError::from)
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    use uuid::Uuid;

    use super::*;
    use crate::codec;
    use crate::tag::Tag;

    /// Build wire-format sentence data from a list of word byte slices.
    fn build_sentence(words: &[&[u8]]) -> Vec<u8> {
        let mut data = Vec::new();
        for word in words {
            codec::encode_word(word, &mut data);
        }
        codec::encode_terminator(&mut data);
        data
    }

    fn parse_response(data: &[u8]) -> Result<CommandResponse, ProtocolError> {
        match codec::decode_sentence(data).unwrap() {
            codec::Decode::Complete { value: raw, .. } => CommandResponse::parse(&raw),
            codec::Decode::Incomplete { .. } => panic!("expected complete sentence"),
        }
    }

    const TEST_TAG: Tag = Tag::from_uuid(Uuid::from_bytes([
        0xa1, 0xa2, 0xa3, 0xa4, 0xb1, 0xb2, 0xc1, 0xc2, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7,
        0xd8,
    ]));

    #[test]
    fn test_parse_done_response() {
        let data = build_sentence(&[b"!done", b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8"]);
        let response = parse_response(&data).unwrap();
        match response {
            CommandResponse::Done(done) => assert_eq!(done.tag, TEST_TAG),
            other => panic!("expected Done, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_reply_response() {
        let data = build_sentence(&[
            b"!re",
            b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8",
            b"=name=ether1",
            b"=type=ether",
        ]);
        let response = parse_response(&data).unwrap();
        match response {
            CommandResponse::Reply(reply) => {
                assert_eq!(reply.tag, TEST_TAG);
                assert_eq!(
                    reply.attributes.get("name"),
                    Some(&Some(String::from("ether1")))
                );
                assert_eq!(
                    reply.attributes.get("type"),
                    Some(&Some(String::from("ether")))
                );
            }
            other => panic!("expected Reply, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_reply_with_tag_after_attributes() {
        let data = build_sentence(&[
            b"!re",
            b"=name=ether1",
            b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8",
        ]);
        let response = parse_response(&data).unwrap();
        match response {
            CommandResponse::Reply(reply) => {
                assert_eq!(reply.tag, TEST_TAG);
                assert_eq!(
                    reply.attributes.get("name"),
                    Some(&Some(String::from("ether1")))
                );
            }
            other => panic!("expected Reply, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_trap_response() {
        let data = build_sentence(&[
            b"!trap",
            b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8",
            b"=category=0",
            b"=message=no such command",
        ]);
        let response = parse_response(&data).unwrap();
        match response {
            CommandResponse::Trap(trap) => {
                assert_eq!(trap.tag, TEST_TAG);
                assert_eq!(trap.category, Some(TrapCategory::MissingItemOrCommand));
                assert_eq!(trap.message, "no such command");
            }
            other => panic!("expected Trap, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_fatal_response() {
        let data = build_sentence(&[b"!fatal", b"out of memory"]);
        let response = parse_response(&data).unwrap();
        match response {
            CommandResponse::Fatal(reason) => assert_eq!(reason, "out of memory"),
            other => panic!("expected Fatal, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_empty_response() {
        let data = build_sentence(&[b"!empty", b".tag=a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8"]);
        let response = parse_response(&data).unwrap();
        match response {
            CommandResponse::Empty(empty) => assert_eq!(empty.tag, TEST_TAG),
            other => panic!("expected Empty, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_missing_category_is_error() {
        // Empty sentence (just terminator)
        let data = vec![0x00];
        let result = parse_response(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_reply_missing_tag_is_error() {
        let data = build_sentence(&[b"!re", b"=name=ether1"]);
        let result = parse_response(&data);
        assert!(result.is_err());
    }
}
