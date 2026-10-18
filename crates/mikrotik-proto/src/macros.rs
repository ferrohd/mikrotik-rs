//! Compile-time command path validation and `command!` macro.
//!
//! The `command!` macro provides a convenient syntax for building MikroTik
//! commands with compile-time validation of the command path.

/// A minimal const validator that enforces basic MikroTik command rules:
///
/// 1. Must start with `/`.
/// 2. No empty segments (no `//`).
/// 3. Only allows `[a-zA-Z0-9_-]` plus space or slash as separators.
/// 4. No consecutive spaces or slashes.
/// 5. No trailing delimiter.
///
/// # Panics
///
/// Panics (at compile time when used in a const context) if the command
/// path is empty, doesn't start with `/`, contains invalid characters,
/// has empty segments (`//`), consecutive delimiters, or a trailing delimiter.
#[must_use]
pub const fn check_mikrotik_command(cmd: &str) -> &str {
    let bytes = cmd.as_bytes();
    let len = bytes.len();

    assert!(len != 0, "MikroTik command cannot be empty.");

    assert!(bytes[0] == b'/', "MikroTik command must start with '/'.");

    let mut prev_was_delimiter = true; // start true because first char is '/'
    let mut i = 1;
    while i < len {
        let c = bytes[i] as char;

        if c == '/' || c == ' ' {
            assert!(!prev_was_delimiter, "No empty segments or consecutive delimiters allowed.");
            prev_was_delimiter = true;
        } else {
            let is_valid_char = c.is_ascii_alphanumeric() || c == '-' || c == '_';
            assert!(is_valid_char, "Invalid character in MikroTik command. Must be [a-zA-Z0-9_-]");
            prev_was_delimiter = false;
        }

        i += 1;
    }

    assert!(!prev_was_delimiter, "Command cannot end with a delimiter.");

    cmd
}

/// Builds a [`Command`](crate::command::Command), validating the command path
/// at compile time.
///
/// A key written without a value is a flag — `detail` below reaches the wire
/// as `=detail=`, not as an omitted word.
///
/// The terse form takes a tag from the process-wide counter and needs `std`.
/// Pass one as a leading argument to use the macro anywhere:
///
/// ```rust
/// use mikrotik_proto::command;
/// use mikrotik_proto::tag::{Counter, TagSource};
///
/// let tags = Counter::new();
///
/// let cmd = command!(tags.next_tag(), "/system/resource/print");
/// assert!(!cmd.data().is_empty());
///
/// let cmd = command!(tags.next_tag(), "/interface/print", user = "admin", detail);
/// assert!(!cmd.data().is_empty());
/// ```
///
/// With `std`: `command!("/interface/print", detail)`.
#[macro_export]
macro_rules! command {
    // Listed first; a leading argument that is not a literal falls through
    // to the tagged rule.
    ($cmd:literal $(, $key:ident $(= $value:expr)? )* $(,)?) => {{
        const VALIDATED: &str = $crate::macros::check_mikrotik_command($cmd);

        #[allow(unused_mut)]
        let mut builder = $crate::command::CommandBuilder::new()
            .command(VALIDATED);

        $(
            builder = $crate::command!(@attr builder, stringify!($key) $(, $value)?);
        )*

        builder.build()
    }};

    ($tag:expr, $cmd:literal $(, $key:ident $(= $value:expr)? )* $(,)?) => {{
        const VALIDATED: &str = $crate::macros::check_mikrotik_command($cmd);

        #[allow(unused_mut)]
        let mut builder = $crate::command::CommandBuilder::with_tag($tag)
            .command(VALIDATED);

        $(
            builder = $crate::command!(@attr builder, stringify!($key) $(, $value)?);
        )*

        builder.build()
    }};

    // Pick the method, not the value: a bare key routed through
    // `attribute(key, None)` would omit the word and drop the flag.
    (@attr $builder:expr, $key:expr, $value:expr) => { $builder.attribute($key, Some($value)) };
    (@attr $builder:expr, $key:expr) => { $builder.flag($key) };
}

#[cfg(test)]
mod tests {
    use alloc::string::String;

    /// Helper to parse words from command wire data.
    fn parse_words(data: &[u8]) -> alloc::vec::Vec<String> {
        let mut words = alloc::vec::Vec::new();
        let mut i = 0;
        while i < data.len() {
            if i >= data.len() {
                break;
            }
            let len = data[i] as usize;
            i += 1;
            if len == 0 {
                break;
            }
            assert!(i + len <= data.len(), "Malformed command data");
            let word = &data[i..i + len];
            i += len;
            words.push(String::from_utf8_lossy(word).into_owned());
        }
        words
    }

    /// The terse `command!` form needs `std`.
    fn tag() -> crate::tag::Tag {
        use core::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(1);
        crate::tag::Tag::from_raw(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    #[test]
    fn test_command_no_attributes() {
        let cmd = command!(tag(), "/system/resource/print");
        let words = parse_words(&cmd.data);

        assert_eq!(words[0], "/system/resource/print");
        assert!(words[1].starts_with(".tag="));
        assert_eq!(words.len(), 2);
    }

    #[test]
    fn test_command_with_one_attribute() {
        let cmd = command!(tag(), "/interface/ethernet/print", user = "admin");
        let words = parse_words(&cmd.data);

        assert_eq!(words[0], "/interface/ethernet/print");
        assert!(words[1].starts_with(".tag="));
        assert_eq!(words[2], "=user=admin");
        assert_eq!(words.len(), 3);
    }

    #[test]
    fn test_command_with_multiple_attributes() {
        let cmd = command!(tag(), "/some/random", attribute_no_value, another = "value");
        let words = parse_words(&cmd.data);

        assert_eq!(words[0], "/some/random");
        assert!(words[1].starts_with(".tag="));
        assert_eq!(words[2], "=attribute_no_value=");
        assert_eq!(words[3], "=another=value");
        assert_eq!(words.len(), 4);
    }
}
