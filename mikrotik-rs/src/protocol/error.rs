use thiserror::Error;

use super::{TrapCategoryError, sentence::SentenceError, word::Word};

/// Possible errors while parsing a [`CommandResponse`] from a [`Sentence`].
///
/// This enum provides more detailed information about issues that can arise while parsing
/// command responses, such as missing tags, missing attributes, or unexpected attributes.
#[derive(Error, Debug, Clone)]
pub enum ProtocolError {
    /// Error related to the [`Sentence`].
    ///
    /// This variant encapsulates errors that occur due to issues in parsing a
    /// [`Sentence`] from bytes.
    #[error("Sentence error: {0}")]
    Sentence(#[from] SentenceError),
    /// Error related to the length of a response.
    ///
    /// Indicates that the response is missing some words to be a valid response.
    #[error("Incomplete response: {0}")]
    Incomplete(#[from] MissingWord),
    /// The received sequence of words is not a valid response.
    #[error("Unexpected word type: found {word:?}, expected one of {expected:?}")]
    WordSequence {
        /// The unexpected [`WordType`] that was encountered.
        word: WordType,
        /// The expected [`WordType`].
        expected: Vec<WordType>,
    },

    /// Error related to identifying or parsing a [`Trap`] response category.
    ///
    /// Indicates that an invalid category was encountered during parsing,
    /// which likely points to either a malformed response.
    #[error("Trap category error: {0}")]
    TrapCategory(#[from] TrapCategoryError),
    // Error involving attributes in a response.
    //
    // Indicates issues related to the unexpected presence of attributes within a response.
    //UnexpectedWord(Word<'a>),
}

impl std::fmt::Display for WordType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WordType::Tag => write!(f, "tag"),
            WordType::Category => write!(f, "category"),
            WordType::Attribute => write!(f, "attribute"),
            WordType::Message => write!(f, "message"),
        }
    }
}

/// Types of words that can be missing from a response.
#[derive(Error, Debug, Clone, Copy)]
pub enum MissingWord {
    /// Missing `.tag` in the response. All responses must have a tag.
    #[error("missing tag")]
    Tag,
    /// Missing category (`!done`, `!re`, `!trap`, `!fatal`, `!empty`) in the response.
    #[error("missing category")]
    Category,
    /// Missing message in a [`CommandResponse::FatalResponse`]
    #[error("missing message")]
    Message,
}

/// Represents the type of a word in a response.
#[derive(Debug, Clone, Copy)]
pub enum WordType {
    /// Tag word.
    Tag,
    /// Category word.
    Category,
    /// Attribute word.
    Attribute,
    /// Message word.
    Message,
}

impl From<Word<'_>> for WordType {
    fn from(word: Word) -> Self {
        match word {
            Word::Tag(_) => WordType::Tag,
            Word::Category(_) => WordType::Category,
            Word::Attribute(_) => WordType::Attribute,
            Word::Message(_) => WordType::Message,
        }
    }
}
