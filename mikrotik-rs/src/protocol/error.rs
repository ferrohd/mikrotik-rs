use super::{TrapCategoryError, sentence::SentenceError, word::Word};

/// Possible errors while parsing a [`CommandResponse`] from a [`Sentence`].
///
/// This enum provides more detailed information about issues that can arise while parsing
/// command responses, such as missing tags, missing attributes, or unexpected attributes.
#[derive(Debug, Clone)]
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

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::Sentence(err) => write!(f, "Sentence error: {}", err),
            ProtocolError::Incomplete(missing) => {
                let msg = match missing {
                    MissingWord::Tag => "missing tag",
                    MissingWord::Category => "missing category",
                    MissingWord::Message => "missing message",
                };
                write!(f, "Incomplete response: {}", msg)
            }
            ProtocolError::WordSequence { word, expected } => {
                write!(
                    f,
                    "Unexpected word type: found {:?}, expected one of {:?}",
                    word, expected
                )
            }
            ProtocolError::TrapCategory(err) => write!(f, "Trap category error: {}", err),
        }
    }
}

impl std::fmt::Display for SentenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SentenceError::WordError(err) => write!(f, "Word error: {}", err),
            SentenceError::PrefixLength => write!(f, "Invalid prefix length"),
        }
    }
}

impl std::fmt::Display for MissingWord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissingWord::Tag => write!(f, "tag"),
            MissingWord::Category => write!(f, "category"),
            MissingWord::Message => write!(f, "message"),
        }
    }
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
#[derive(Debug, Clone)]
pub enum MissingWord {
    /// Missing `.tag` in the response. All responses must have a tag.
    Tag,
    /// Missing category (`!done`, `!re`, `!trap`, `!fatal`, `!empty`) in the response.
    Category,
    /// Missing message in a [`CommandResponse::FatalResponse`]
    Message,
}

/// Represents the type of a word in a response.
#[derive(Debug, Clone)]
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
