use super::{sentence::SentenceError, word::Word, TrapCategoryError};

/// Possible errors while parsing a [`CommandResponse`] from a [`Sentence`].
///
/// This enum provides more detailed information about issues that can arise while parsing
/// command responses, such as missing tags, missing attributes, or unexpected attributes.
#[derive(Debug)]
pub enum ProtocolError {
    /// Error related to the [`Sentence`].
    ///
    /// This variant encapsulates errors that occur due to issues in parsing a
    /// [`Sentence`] from bytes.
    Sentence(SentenceError),
    /// Error related to the length of a response.
    ///
    /// Indicates that the response is missing some words to be a valid response.
    Incomplete(MissingWord),
    /// The received sequence of words is not a valid response.
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
    TrapCategory(TrapCategoryError),
    // Error involving attributes in a response.
    //
    // Indicates issues related to the unexpected presence of attributes within a response.
    //UnexpectedWord(Word<'a>),
}

impl From<SentenceError> for ProtocolError {
    fn from(e: SentenceError) -> Self {
        ProtocolError::Sentence(e)
    }
}

impl From<MissingWord> for ProtocolError {
    fn from(e: MissingWord) -> Self {
        ProtocolError::Incomplete(e)
    }
}

impl From<TrapCategoryError> for ProtocolError {
    fn from(e: TrapCategoryError) -> Self {
        ProtocolError::TrapCategory(e)
    }
}

/// Types of words that can be missing from a response.
#[derive(Debug)]
pub enum MissingWord {
    /// Missing `.tag` in the response. All responses must have a tag.
    Tag,
    /// Missing category (`!done`, `!repl`, `!trap`, `!fatal`) in the response.
    Category,
    /// Missing message in a [`CommandResponse::FatalResponse`]
    Message,
}

/// Represents the type of a word in a response.
#[derive(Debug)]
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
