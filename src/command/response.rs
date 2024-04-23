use std::{
    collections::HashMap,
    fmt::{Debug, Formatter},
    num::ParseIntError,
};

use super::reader::{Sentence, WordError};

/// Type alias for representing a fatal response string.
pub type FatalResponse = String;
/// Type alias for attribute keys in a [`ReplyResponse`].
pub type AttributeKey = String;
/// Type alias for a map of attributes in a [`ReplyResponse`].
pub type AttributeMap = HashMap<AttributeKey, Attribute>;

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
    type Error = ParsingError;

    fn try_from(mut sentence_iter: Sentence) -> Result<Self, Self::Error> {
        let reply_word = sentence_iter
            .next()
            .ok_or(ParsingError::Sentence(SentenceError::CategoryError(
                SentenceCategoryError::Missing,
            )))??
            .category()
            .ok_or(ParsingError::Sentence(SentenceError::CategoryError(
                SentenceCategoryError::Missing,
            )))?;

        match reply_word {
            "!done" => {
                let tag = sentence_iter.next().and_then(|w| w.ok()?.tag()).ok_or(
                    ParsingError::Sentence(SentenceError::CategoryError(
                        SentenceCategoryError::Missing,
                    )),
                )?;
                let tag = parse_tag(
                    sentence_iter
                        .next()
                        .ok_or(ParsingError::Tag(TagError::Missing))??,
                )?;
                Ok(CommandResponse::Done(DoneResponse { tag }))
            }
            "!re" => {
                // !re is composed of a tag and a list of attributes
                let mut tag = None;
                let mut attributes = HashMap::<AttributeKey, Attribute>::new();

                for word in sentence_iter {
                    let word = word?;
                    match word.chars().next() {
                        Some('.') => {
                            tag = Some(parse_tag(word)?);
                        }
                        Some('=') => {
                            // Attributes are in the format `=key=value`
                            let mut attr = word.splitn(3, '=');
                            let key = attr
                                .nth(1)
                                .ok_or(ParsingError::Attribute("Missing attribute key"))?
                                .to_owned();
                            let value = attr.next().map(String::from).into();
                            attributes.insert(key, value);
                        }
                        _ => {
                            return Err(ParsingError::Attribute(
                                "Unexpected attribute in !re response",
                            ));
                        }
                    }
                }

                Ok(CommandResponse::Reply(ReplyResponse {
                    tag: tag.ok_or(ParsingError::Tag(TagError::Missing))?,
                    attributes,
                }))
            }
            "!trap" => {
                let mut tag = None;
                let mut category = None;
                let mut message = None;

                for word in sentence_iter {
                    let word = word?;
                    match word.chars().next() {
                        Some('.') => {
                            tag = Some(parse_tag(word)?);
                        }
                        Some('=') => {
                            // Attributes are in the format `=key=value`
                            let mut attr = word.splitn(3, '=');
                            let key = attr
                                .nth(1)
                                .ok_or(ParsingError::Attribute("Missing attribute key"))?;
                            match key {
                                "category" => {
                                    category =
                                        attr.next().map(TrapCategory::try_from).transpose()?
                                }
                                "message" => {
                                    message = Some(
                                        attr.next()
                                            .ok_or(ParsingError::Attribute("Missing trap message"))?
                                            .to_owned(),
                                    )
                                }
                                _ => Err(ParsingError::Attribute(
                                    "Unexpected attribute in !trap response",
                                ))?,
                            }
                        }
                        _ => {
                            return Err(ParsingError::Attribute(
                                "Unexpected attribute in !trap response",
                            ));
                        }
                    }
                }

                Ok(CommandResponse::Trap(TrapResponse {
                    tag: tag.ok_or(ParsingError::Tag(TagError::Missing))?,
                    category,
                    message: message
                        .ok_or(ParsingError::Attribute("Missing trap message attribute"))?,
                }))
            }
            "!fatal" => {
                let reason = sentence_iter
                    .next()
                    .ok_or(ParsingError::Attribute("Missing fatal reason"))?;
                Ok(CommandResponse::Fatal(reason?.to_string()))
            }
            s => Err(ParsingError::Sentence(SentenceError::CategoryError(
                SentenceCategoryError::Invalid(s.to_string()),
            ))),
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
    pub attributes: AttributeMap,
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
    type Error = ParsingError;

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
            _ => Err(ParsingError::TrapCategory(TrapCategoryError::OutOfRange)),
        }
    }
}

impl From<&TrapCategory> for u8 {
    fn from(val: &TrapCategory) -> Self {
        match val {
            TrapCategory::MissingItemOrCommand => 0,
            TrapCategory::ArgumentValueFailure => 1,
            TrapCategory::CommandExecutionInterrupted => 2,
            TrapCategory::ScriptingFailure => 3,
            TrapCategory::GeneralFailure => 4,
            TrapCategory::APIFailure => 5,
            TrapCategory::TTYFailure => 6,
            TrapCategory::ReturnValue => 7,
        }
    }
}

impl TryFrom<&str> for TrapCategory {
    type Error = ParsingError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let n = s
            .parse::<u8>()
            .map_err(|e| ParsingError::TrapCategory(TrapCategoryError::Parsing(e)))?;
        TrapCategory::try_from(n)
    }
}

impl TryFrom<Attribute> for TrapCategory {
    type Error = ParsingError;

    fn try_from(attribute: Attribute) -> Result<Self, Self::Error> {
        match attribute {
            Attribute::Value(s) => TrapCategory::try_from(s.as_str()),
            Attribute::Empty => Err(ParsingError::TrapCategory(TrapCategoryError::Missing)),
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

/// Possible errors while parsing network command responses.
///
/// This enum aids in identifying and responding to different parsing issues that can arise,
/// such as invalid input or unexpected response formats.
#[derive(Debug)]
pub enum ParsingError {
    /// Error related to parsing individual `Sentence` elements.
    ///
    /// This variant encapsulates errors that occur due to issues in parsing a single
    /// `Sentence` from a command response.
    Sentence(SentenceError),
    /// Error related to identifying or parsing a `Trap` response category.
    ///
    /// Indicates that an invalid category was encountered during parsing,
    /// which likely points to either a malformed response or a new category that's not
    /// yet supported by the parser.
    TrapCategory(TrapCategoryError),
    /// Error involving attributes in a response.
    ///
    /// Indicates issues related to parsing or expected presence of attributes within a response.
    Attribute(&'static str),
    /// Error related to parsing or missing command tags.
    ///
    /// Command tags are expected in most response types to correlate them with their request.
    /// This error indicates a parsing issue or an outright missing tag.
    Tag(TagError),
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
    /// Error indicating that a trap category could not be parsed as an integer.
    Parsing(ParseIntError),
    /// Error indicating that a trap category is out of range. Valid categories are 0-7.
    OutOfRange,
}

/// Errors that can occur while parsing tags in response sentences.
///
/// This enum provides more detailed information about issues that can arise while parsing tags,
/// such as missing tags or errors while converting tag strings to integers.
#[derive(Debug)]
pub enum TagError {
    /// Error indicating that a tag is missing from the response sentence.
    Missing,
    /// Error indicating that a tag could not be parsed as an integer.
    Invalid(ParseIntError),
}

/// Specific errors that can occur while processing individual sentences
/// in a network command response.
///
/// Designed to provide more granular error information, particularly for issues related to
/// converting a sequence of bytes into a UTF-8 string or the integrity of the sentence data.
#[derive(Debug)]
pub enum SentenceError {
    /// Error indicating that a sequence of bytes could not be converted to a UTF-8 string.
    ///
    /// This could occur if the byte sequence contains invalid UTF-8 patterns, which is
    /// possible when receiving malformed or unexpected input.
    WordError(WordError),
    /// Error indicating that an issue occurred due to incorrect length or format.
    ///
    /// This could happen if the sentence does not comply with the expected structure or
    /// if essential parts of the sentence are missing, making it too short to parse correctly.
    LengthError,
    /// Error indicating that the category of the sentence is invalid.
    /// This could happen if the sentence does not start with a recognized category.
    /// Valid categories are `!done`, `!re`, `!trap`, and `!fatal`.
    CategoryError(SentenceCategoryError),
}

/// Errors that can occur while parsing sentence categories.
#[derive(Debug)]
pub enum SentenceCategoryError {
    /// Error indicating that the category of the sentence is missing.
    Missing,
    /// Error indicating that the category of the sentence is invalid.
    Invalid(String),
}

impl From<SentenceError> for ParsingError {
    fn from(e: SentenceError) -> Self {
        ParsingError::Sentence(e)
    }
}
