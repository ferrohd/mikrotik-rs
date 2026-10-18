//! Command tags, which correlate a command with its responses.
//!
//! The router echoes the tag in every response sentence, so it only has to be
//! unique among the commands in flight on one connection. On the wire it is
//! fixed-width zero-padded decimal: `.tag=0000000042`.

use core::fmt;

use thiserror::Error;

/// Identifies a command, so its responses can be told apart from another's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tag(u32);

impl Tag {
    /// The login command's tag. [`Counter`] starts at 1 to stay clear of it.
    pub const LOGIN: Tag = Tag::from_raw(0);

    /// Width of the wire form, derived so it cannot fall behind the type.
    pub const ASCII_LEN: usize = u32::MAX.ilog10() as usize + 1;

    /// Wraps a raw value.
    #[must_use]
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    /// The raw value.
    #[must_use]
    pub const fn to_raw(self) -> u32 {
        self.0
    }

    /// Takes the next tag from a process-wide counter.
    ///
    /// Requires `std`: a shared counter needs an atomic, and a target without
    /// compare-and-swap has none. Elsewhere, use [`Counter`] with
    /// [`CommandBuilder::with_tag`](crate::command::CommandBuilder::with_tag).
    #[cfg(feature = "std")]
    #[must_use]
    pub fn new() -> Self {
        use core::sync::atomic::{AtomicU32, Ordering};

        // Relaxed: uniqueness is the only requirement, never ordering.
        static NEXT: AtomicU32 = AtomicU32::new(1);
        Tag::from_raw(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    /// The wire form: fixed-width zero-padded decimal.
    ///
    /// ```
    /// use mikrotik_proto::tag::Tag;
    ///
    /// assert_eq!(&Tag::from_raw(42).to_ascii(), b"0000000042");
    /// ```
    #[must_use]
    pub const fn to_ascii(self) -> [u8; Self::ASCII_LEN] {
        let mut out = [b'0'; Self::ASCII_LEN];
        let mut value = self.0;
        let mut i = Self::ASCII_LEN;
        while i > 0 {
            i -= 1;
            out[i] = b'0' + (value % 10) as u8;
            value /= 10;
        }
        out
    }
}

/// Why a `.tag=` value could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TagParseError {
    /// No digits.
    #[error("tag is empty")]
    Empty,
    /// Not an ASCII digit.
    #[error("tag contains a non-digit byte")]
    NotADigit,
    /// Larger than [`u32::MAX`].
    #[error("tag exceeds u32::MAX")]
    Overflow,
}

/// Parses the digits of a `.tag=` value. Takes bytes: the wire value is ASCII,
/// so UTF-8 validation would be wasted.
impl TryFrom<&[u8]> for Tag {
    type Error = TagParseError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.is_empty() {
            return Err(TagParseError::Empty);
        }

        let mut value: u32 = 0;
        for &byte in bytes {
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                _ => return Err(TagParseError::NotADigit),
            };
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add(digit))
                .ok_or(TagParseError::Overflow)?;
        }

        Ok(Self(value))
    }
}

#[cfg(feature = "std")]
impl Default for Tag {
    fn default() -> Self {
        Self::new()
    }
}

impl core::str::FromStr for Tag {
    type Err = TagParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.as_bytes())
    }
}

/// Renders the wire form, so a log line matches a packet capture. `Debug`
/// gives the bare number.
impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:0width$}", self.0, width = Self::ASCII_LEN)
    }
}

/// A source of tags unique within one connection.
pub trait TagSource {
    /// Takes the next tag.
    fn next_tag(&self) -> Tag;
}

/// A monotonic tag counter.
///
/// Holds a `Cell`, not an atomic: a target without compare-and-swap cannot
/// have one. Synchronisation is the owner's business — a single task needs
/// none.
///
/// ```
/// use mikrotik_proto::tag::{Counter, TagSource};
///
/// let tags = Counter::new();
/// assert_ne!(tags.next_tag(), tags.next_tag());
/// ```
#[derive(Debug)]
pub struct Counter(core::cell::Cell<u32>);

impl Counter {
    /// A counter starting at 1, clear of [`Tag::LOGIN`].
    #[must_use]
    pub const fn new() -> Self {
        Self(core::cell::Cell::new(1))
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

impl TagSource for Counter {
    fn next_tag(&self) -> Tag {
        // Wrapping is unreachable in practice, and `Connection::send_command`
        // rejects a duplicate rather than misrouting if it ever is reached.
        let value = self.0.get();
        self.0.set(value.wrapping_add(1));
        Tag::from_raw(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn display_matches_the_encoder() {
        for value in [0, 1, 42, 999, u32::MAX] {
            let tag = Tag::from_raw(value);
            assert_eq!(format!("{tag}").as_bytes(), &tag.to_ascii());
        }
    }

    #[test]
    fn ascii_roundtrips() {
        for value in [0, 1, 42, u32::MAX] {
            let tag = Tag::from_raw(value);
            assert_eq!(Tag::try_from(tag.to_ascii().as_slice()), Ok(tag));
        }
    }

    #[test]
    fn rejects_malformed_values() {
        assert_eq!(Tag::try_from(b"".as_slice()), Err(TagParseError::Empty));
        assert_eq!(Tag::try_from(b"4294967296".as_slice()), Err(TagParseError::Overflow));
        for bad in [b"not-a-number".as_slice(), b"12x4", b"-1", b" 1"] {
            assert_eq!(Tag::try_from(bad), Err(TagParseError::NotADigit), "{bad:?}");
        }
    }

    #[test]
    fn counter_is_monotonic_and_skips_the_login_tag() {
        let tags = Counter::new();
        let first = tags.next_tag();
        assert_ne!(first, Tag::LOGIN);
        assert_eq!(first.to_raw() + 1, tags.next_tag().to_raw());
    }
}
