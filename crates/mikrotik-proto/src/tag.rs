//! Command tag — a unique identifier for correlating commands with responses.
//!
//! Each command sent to a MikroTik router carries a [`Tag`] that the router
//! echoes back in every response sentence (`.tag=<value>`). That is what lets
//! several commands share one connection.
//!
//! A tag is a `u32`, written on the wire as fixed-width zero-padded decimal:
//!
//! ```text
//! .tag=0000000042
//! ```
//!
//! RouterOS treats the value as an opaque string and only ever echoes it, so
//! the only requirement is uniqueness among the commands in flight on one
//! connection. A counter satisfies that without needing entropy, which is what
//! makes this crate buildable for a bare-metal target.

use core::fmt;

use thiserror::Error;

/// A unique tag identifying a command for response correlation.
///
/// Create one with [`Tag::from_raw`], or take them in sequence from a
/// [`Counter`]. [`CommandBuilder`] does the latter for
/// you where a global counter is available.
///
/// [`CommandBuilder`]: crate::command::CommandBuilder
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tag(u32);

impl Tag {
    /// The tag used for the login command.
    ///
    /// Login is the only command that can be in flight before authentication
    /// completes, and it is retired before the [`Connection`] is handed over,
    /// so no reservation is needed to keep this clear of ordinary tags.
    /// Counters start at 1 anyway, which keeps packet captures unambiguous.
    ///
    /// [`Connection`]: crate::connection::Connection
    pub const LOGIN: Tag = Tag::from_raw(0);

    /// Width of the decimal wire form.
    ///
    /// Derived from the representation rather than asserted against it, so
    /// widening `Tag` cannot leave the encoder's buffer behind.
    pub const ASCII_LEN: usize = u32::MAX.ilog10() as usize + 1;

    /// Wrap a raw value.
    #[must_use]
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    /// The underlying value.
    #[must_use]
    pub const fn to_raw(self) -> u32 {
        self.0
    }

    /// Take the next tag from a process-wide counter.
    ///
    /// Convenient, and sufficient for any number of connections in one
    /// process, since a tag only has to be unique among those in flight.
    ///
    /// Requires the `std` feature. A shared counter has to be `Sync`, which
    /// means an atomic, and `AtomicU32::fetch_add` does not exist on a target
    /// without compare-and-swap — Cortex-M0+ among them. Rather than drag a
    /// `critical-section` dependency into a sans-IO crate to paper over that,
    /// `no_std` callers own a [`Counter`] and pass tags to
    /// [`CommandBuilder::with_tag`], which is both portable and explicit about
    /// where the state lives.
    ///
    /// [`CommandBuilder::with_tag`]: crate::command::CommandBuilder::with_tag
    #[cfg(feature = "std")]
    #[must_use]
    pub fn new() -> Self {
        use core::sync::atomic::{AtomicU32, Ordering};

        // Relaxed: uniqueness is the only requirement, never ordering.
        static NEXT: AtomicU32 = AtomicU32::new(1);
        Tag::from_raw(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    /// The tag as fixed-width zero-padded decimal ASCII.
    ///
    /// Fixed width on purpose: every byte of the result is meaningful, so
    /// there is no length to carry alongside it and no way for a caller to
    /// transmit the padding by mistake.
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

/// Reasons a `.tag=` value could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TagParseError {
    /// The value was empty.
    #[error("tag is empty")]
    Empty,
    /// The value contained a byte that is not an ASCII digit.
    #[error("tag contains a non-digit byte")]
    NotADigit,
    /// The value was a number too large for a tag.
    #[error("tag exceeds u32::MAX")]
    Overflow,
}

/// Parse a tag from the ASCII digits of a `.tag=` value.
///
/// Takes bytes rather than `&str` because this is the wire-parsing hot path
/// and the value is ASCII by construction, so UTF-8 validation would be
/// wasted work.
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

/// Renders the wire form, so a log line can be grepped for in a packet
/// capture. `Debug` gives the bare number for when that is what you want.
impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:0width$}", self.0, width = Self::ASCII_LEN)
    }
}

/// A source of command tags unique within one connection.
///
/// Implemented by [`Counter`] and by the device handles in the adapter crates,
/// so the same call reads the same way wherever the tags come from.
pub trait TagSource {
    /// Take the next tag.
    fn next_tag(&self) -> Tag;
}

/// A monotonic tag counter.
///
/// Uses a [`Cell`](core::cell::Cell) rather than an atomic deliberately: a
/// Cortex-M0+ target such as the RP2040 has no compare-and-swap, so
/// `AtomicU32::fetch_add` would not compile there. Synchronisation is left to
/// whoever owns the counter, which is where the concurrency model is actually
/// known — a single Embassy task needs none at all.
///
/// Starts at 1, leaving 0 to [`Tag::LOGIN`] so a packet capture is unambiguous.
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
    /// A counter starting at 1.
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
        // Wrapping is unreachable in practice and harmless if reached: a tag
        // only has to be distinct from those currently in flight, and
        // `Connection::send_command` rejects a duplicate rather than
        // misrouting its responses.
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
        // Two renderings of the same thing: `to_ascii` for the wire, `Display`
        // for everywhere else. They must not drift.
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
