use std::{
    collections::HashMap,
    fmt::{self, Display, Formatter},
    num::ParseIntError,
};

use error::{MissingWord, ProtocolError, WordType};
use sentence::Sentence;
use word::{Word, WordAttribute, WordCategory};

/// Module containing the command parser and response types.
pub mod command;
/// Module containing the error types for the command parser.
pub mod error;
/// Module containing the sentence parser and response types.
pub mod sentence;
/// Module containing the word parser and response types.
pub mod word;

/// Type alias for a fatal response [`String`].
pub type FatalResponse = String;

/// Various types of responses a command can produce.
#[derive(Debug, Clone)]
pub enum CommandResponse {
    /// Represents a successful command completion response.
    Done(DoneResponse),
    /// Represents a reply to a command, including a tag and multiple attributes.
    Reply(ReplyResponse),
    /// Represents an error or warning while executing a command, including a tag and message.
    Trap(TrapResponse),
    /// Represents a fatal error response.
    Fatal(FatalResponse),
}

impl CommandResponse {
    /// Returns the tag associated with the response, if available.
    ///
    /// Returns [`None`] for [`CommandResponse::Fatal`] responses as they do not contain tags.
    pub fn tag(&self) -> Option<u16> {
        match &self {
            Self::Done(d) => Some(d.tag),
            Self::Reply(r) => Some(r.tag),
            Self::Trap(t) => Some(t.tag),
            Self::Fatal(_) => None,
        }
    }
}

impl TryFrom<Sentence<'_>> for CommandResponse {
    type Error = ProtocolError;

    fn try_from(mut sentence_iter: Sentence) -> Result<Self, Self::Error> {
        let word = sentence_iter
            .next()
            .ok_or::<ProtocolError>(MissingWord::Category.into())??;

        let category = word.category().ok_or(ProtocolError::WordSequence {
            word: word.word_type(),
            expected: vec![WordType::Category],
        })?;

        match category {
            WordCategory::Done => {
                let word = sentence_iter
                    .next()
                    .ok_or::<ProtocolError>(MissingWord::Tag.into())??;

                // !done is composed of a single tag
                let tag = word.tag().ok_or(ProtocolError::WordSequence {
                    word: word.into(),
                    expected: vec![WordType::Tag],
                })?;
                Ok(CommandResponse::Done(DoneResponse { tag }))
            }
            WordCategory::Reply => {
                // !re is composed of a tag and a list of attributes
                // The tag is mandatory but its position is not fixed
                let mut tag = None;
                let mut attributes = HashMap::<String, Option<String>>::new();

                for word in sentence_iter {
                    let word = word?;
                    match word {
                        Word::Tag(t) => tag = Some(t),
                        Word::Attribute(WordAttribute { key, value }) => {
                            attributes.insert(key.to_string(), value.map(String::from));
                        }
                        word => {
                            return Err(ProtocolError::WordSequence {
                                word: word.into(),
                                expected: vec![WordType::Tag, WordType::Attribute],
                            });
                        }
                    }
                }

                let tag = tag.ok_or::<ProtocolError>(MissingWord::Category.into())?;

                Ok(CommandResponse::Reply(ReplyResponse { tag, attributes }))
            }
            WordCategory::Trap => {
                // !trap is composed of a tag, and two optional attributes: category and message
                // The tag is mandatory but its position is not fixed
                // The category and message are optional and can appear in any order
                let mut tag = None;
                let mut category = None;
                let mut message = None;

                for word in sentence_iter {
                    let word = word?;
                    match word {
                        Word::Tag(t) => tag = Some(t),
                        Word::Attribute(WordAttribute { key, value }) => match key.as_ref() {
                            "category" => {
                                category =
                                    value.as_deref().map(TrapCategory::try_from).transpose()?;
                            }
                            "message" => {
                                message = value.map(String::from);
                            }
                            key => {
                                return Err(TrapCategoryError::InvalidAttribute {
                                    key: key.into(),
                                    value: value.map(|v| v.into()),
                                }
                                .into());
                            }
                        },
                        word => {
                            return Err(ProtocolError::WordSequence {
                                word: word.into(),
                                expected: vec![WordType::Tag, WordType::Attribute],
                            });
                        }
                    }
                }

                let tag = tag.ok_or::<ProtocolError>(MissingWord::Category.into())?;
                let message = message.ok_or(TrapCategoryError::MissingMessageAttribute)?;

                Ok(CommandResponse::Trap(TrapResponse {
                    tag,
                    category,
                    message,
                }))
            }
            WordCategory::Fatal => {
                // !fatal is composed of a single message
                let word = sentence_iter
                    .next()
                    .ok_or::<ProtocolError>(MissingWord::Message.into())??;

                let reason = word.generic().ok_or(ProtocolError::WordSequence {
                    word: word.word_type(),
                    expected: vec![WordType::Message],
                })?;

                Ok(CommandResponse::Fatal(reason.to_string()))
            }
        }
    }
}

/// Represents a (tagged) successful command completion response.
#[derive(Debug, Clone)]
pub struct DoneResponse {
    /// The tag associated with the command.
    pub tag: u16,
}

impl Display for DoneResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "DoneResponse {{ tag: {} }}", self.tag)
    }
}

/// Represents a reply to a command, including a tag and multiple attributes.
#[derive(Debug, Clone)]
pub struct ReplyResponse {
    /// The tag associated with the command.
    pub tag: u16,
    /// The attributes of the reply.
    pub attributes: HashMap<String, Option<String>>,
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

/// Represents an error or warning while executing a command, including a tag and message.
#[derive(Debug, Clone)]
pub struct TrapResponse {
    /// The tag associated with the command.
    pub tag: u16,
    /// The category of the trap.
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

/// Categories for `TrapResponse`, defining the nature of the trap.
#[derive(Debug, Clone)]
#[repr(u8)]
pub enum TrapCategory {
    /// 0 - missing item or command
    MissingItemOrCommand = 0,
    /// 1 - argument value failure
    ArgumentValueFailure = 1,
    /// 2 - execution of command interrupted
    CommandExecutionInterrupted = 2,
    /// 3 - scripting related failure
    ScriptingFailure = 3,
    /// 4 - general failure
    GeneralFailure = 4,
    /// 5 - API related failure
    APIFailure = 5,
    /// 6 - TTY related failure
    TTYFailure = 6,
    /// 7 - value generated with :return command
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

/// Errors that can occur while parsing trap categories in response sentences.
///
/// This enum provides more detailed information about issues that can arise while parsing trap
/// categories, such as missing categories, errors while converting category strings to integers,
/// or categories that are out of range.
#[derive(Debug)]
pub enum TrapCategoryError {
    /// Invalid value encountered while parsing a trap category.
    Invalid(ParseIntError),
    /// Error indicating that a trap category is out of range. Valid categories are 0-7.
    OutOfRange(u8),
    /// Trap expects a category or message, but got something else.
    InvalidAttribute {
        /// The key of the invalid attribute.
        key: String,
        /// The value of the invalid attribute, if present.
        value: Option<String>,
    },
    /// Missing category attribute in a trap response.
    MissingMessageAttribute,
}
