use std::{
    collections::HashMap,
    fmt::{Debug, Formatter},
    num::ParseIntError,
};

use super::sentence::{ResponseType, Sentence, SentenceError, Word};

/// Type alias for representing a fatal response string.
pub type FatalResponse = String;

/// Enum representing the various types of responses a command can produce.
#[derive(Debug)]
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
    /// Returns `None` for Fatal responses as they do not contain tags.
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
    type Error = ResponseError;

    fn try_from(mut sentence_iter: Sentence) -> Result<Self, Self::Error> {
        let word = sentence_iter.next().ok_or(ResponseError::Length(
            "Missing category (!done, !repl, !trap, !fatal",
        ))??;
        let category = word
            .category()
            .ok_or(ResponseError::Sentence(SentenceError::CategoryError))?;

        match category {
            ResponseType::Done => {
                let tag = sentence_iter
                    .next()
                    .ok_or(ResponseError::Length("Missing tag in the response"))??
                    .tag()
                    .ok_or(ResponseError::Tag)?;
                Ok(CommandResponse::Done(DoneResponse { tag }))
            }
            ResponseType::Reply => {
                // !re is composed of a tag and a list of attributes
                let mut tag = None;
                let mut attributes = HashMap::<String, Option<String>>::new();

                for word in sentence_iter {
                    let word = word?;
                    match word {
                        Word::Tag(t) => tag = Some(t),
                        Word::Attribute((key, value)) => {
                            attributes.insert(key.to_owned(), value.map(String::from));
                        }
                        word => {
                            return Err(ResponseError::UnexpectedWord(format!(
                                "Unexpected word: {}",
                                word
                            )));
                        }
                    }
                }

                let tag = tag.ok_or(ResponseError::Tag)?;

                Ok(CommandResponse::Reply(ReplyResponse { tag, attributes }))
            }
            ResponseType::Trap => {
                let mut tag = None;
                let mut category = None;
                let mut message = None;

                for word in sentence_iter {
                    let word = word?;
                    match word {
                        Word::Tag(t) => tag = Some(t),
                        Word::Attribute((key, value)) => match key {
                            "category" => {
                                category = value.map(TrapCategory::try_from).transpose()?;
                            }
                            "message" => {
                                message = value.map(String::from);
                            }
                            key => {
                                return Err(ResponseError::UnexpectedWord(format!(
                                    "Expected only =message= and =category= attributes, ={}={} found",
                                    key,
                                    value.unwrap_or("")
                                )));
                            }
                        },
                        word => {
                            return Err(ResponseError::UnexpectedWord(format!(
                                "Unexpected word: {}",
                                word
                            )));
                        }
                    }
                }

                let tag = tag.ok_or(ResponseError::Tag)?;
                let message = message.ok_or(ResponseError::UnexpectedWord(
                    "The trap response is missing the =message= body".to_string(),
                ))?;

                Ok(CommandResponse::Trap(TrapResponse {
                    tag,
                    category,
                    message,
                }))
            }
            ResponseType::Fatal => {
                let word = sentence_iter
                    .next()
                    .ok_or(ResponseError::Length("Missing fatal reason"))??;
                let reason = word.generic().ok_or(ResponseError::UnexpectedWord(format!(
                    "Expected Fatal reason, found: {}",
                    word
                )))?;

                Ok(CommandResponse::Fatal(reason.to_string()))
            }
        }
    }
}

/// Represents a (tagged) successful command completion response.
#[derive(Debug)]
pub struct DoneResponse {
    /// The tag associated with the command.
    pub tag: u16,
}

/// Represents a reply to a command, including a tag and multiple attributes.
#[derive(Debug)]
pub struct ReplyResponse {
    /// The tag associated with the command.
    pub tag: u16,
    /// The attributes of the reply.
    pub attributes: HashMap<String, Option<String>>,
}

/// Represents an error or warning while executing a command, including a tag and message.
#[derive(Debug)]
pub struct TrapResponse {
    /// The tag associated with the command.
    pub tag: u16,
    /// The category of the trap.
    pub category: Option<TrapCategory>,
    /// The message associated with the trap.
    pub message: String,
}

/// Categories for `TrapResponse`, defining the nature of the trap.
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

impl Debug for TrapCategory {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            TrapCategory::MissingItemOrCommand => "Missing item or command",
            TrapCategory::ArgumentValueFailure => "Argument value failure",
            TrapCategory::CommandExecutionInterrupted => "Command execution interrupted",
            TrapCategory::ScriptingFailure => "Scripting failure",
            TrapCategory::GeneralFailure => "General failure",
            TrapCategory::APIFailure => "API failure",
            TrapCategory::TTYFailure => "TTY failure",
            TrapCategory::ReturnValue => "Return value",
        };
        write!(f, "{}", s)
    }
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
    type Error = ResponseError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let n = s
            .parse::<u8>()
            .map_err(|e| ResponseError::TrapCategory(TrapCategoryError::Invalid(e)))?;
        TrapCategory::try_from(n).map_err(ResponseError::TrapCategory)
    }
}

impl TryFrom<Attribute> for TrapCategory {
    type Error = ResponseError;

    fn try_from(attribute: Attribute) -> Result<Self, Self::Error> {
        match attribute {
            Attribute::Value(s) => TrapCategory::try_from(s.as_str()),
            Attribute::Empty => Err(ResponseError::TrapCategory(TrapCategoryError::Missing)),
        }
    }
}

/// An enum representing an optional attribute in a network command response.
///
/// Attributes may either have a value (`Value`) or represent the absence of a value (`Empty`).
#[derive(Debug)]
pub enum Attribute {
    /// Represents an attribute that contains a value.
    Value(String),
    /// Represents an attribute without a value, indicating the attribute's absence.
    Empty,
}

impl Attribute {
    /// Returns the attribute as an `Option<&String>`.
    ///
    /// Useful for potentially missing values.
    pub fn as_option(&self) -> Option<&String> {
        match self {
            Attribute::Value(s) => Some(s),
            Attribute::Empty => None,
        }
    }
}

impl From<String> for Attribute {
    fn from(s: String) -> Self {
        Attribute::Value(s)
    }
}

impl From<Option<String>> for Attribute {
    fn from(s: Option<String>) -> Self {
        match s {
            Some(s) => Attribute::Value(s),
            None => Attribute::Empty,
        }
    }
}

impl From<Attribute> for Option<String> {
    fn from(attribute: Attribute) -> Self {
        match attribute {
            Attribute::Value(s) => Some(s),
            Attribute::Empty => None,
        }
    }
}

/// Possible errors while construting a [`CommandResponse`] from a [`Sentence`].
///
/// This enum provides more detailed information about issues that can arise while parsing
/// command responses, such as missing tags, missing attributes, or unexpected attributes.
#[derive(Debug)]
pub enum ResponseError {
    /// Error related to the [`Sentence`].
    ///
    /// This variant encapsulates errors that occur due to issues in parsing a single
    /// [`Sentence`] from the bytes.
    Sentence(SentenceError),
    /// Error related to the length of a response.
    ///
    /// Indicates that the response is missing some words to be a valid response.
    Length(&'static str),
    /// The response is missing a tag.
    Tag,
    /// Error related to identifying or parsing a `Trap` response category.
    ///
    /// Indicates that an invalid category was encountered during parsing,
    /// which likely points to either a malformed response or a new category that's not
    /// yet supported by the parser.
    TrapCategory(TrapCategoryError),
    /// Error involving attributes in a response.
    ///
    /// Indicates issues related to the unexpected presence of attributes within a response.
    UnexpectedWord(String),
}

/// Errors that can occur while parsing trap categories in response sentences.
///
/// This enum provides more detailed information about issues that can arise while parsing trap
/// categories, such as missing categories, errors while converting category strings to integers,
/// or categories that are out of range.
#[derive(Debug)]
pub enum TrapCategoryError {
    /// Error indicating that a trap category is missing from the response sentence.
    Missing,
    /// Inv
    Invalid(ParseIntError),
    /// Error indicating that a trap category is out of range. Valid categories are 0-7.
    OutOfRange(u8),
}

impl From<SentenceError> for ResponseError {
    fn from(e: SentenceError) -> Self {
        ResponseError::Sentence(e)
    }
}
