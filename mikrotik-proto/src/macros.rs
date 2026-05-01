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
/// **Panics at compile time** if the command path is invalid.
pub const fn check_mikrotik_command(cmd: &str) -> &str {
    let bytes = cmd.as_bytes();
    let len = bytes.len();

    if len == 0 {
        panic!("MikroTik command cannot be empty.");
    }

    if bytes[0] != b'/' {
        panic!("MikroTik command must start with '/'.");
    }

    let mut prev_was_delimiter = true; // start true because first char is '/'
    let mut i = 1;
    while i < len {
        let c = bytes[i] as char;

        if c == '/' || c == ' ' {
            if prev_was_delimiter {
                panic!("No empty segments or consecutive delimiters allowed.");
            }
            prev_was_delimiter = true;
        } else {
            let is_valid_char = c.is_ascii_alphanumeric() || c == '-' || c == '_';
            if !is_valid_char {
                panic!("Invalid character in MikroTik command. Must be [a-zA-Z0-9_-]");
            }
            prev_was_delimiter = false;
        }

        i += 1;
    }

    if prev_was_delimiter {
        panic!("Command cannot end with a delimiter.");
    }

    cmd
}

/// Macro that enforces MikroTik command syntax **at compile time**.
///
/// # Examples
///
/// ```rust,ignore
/// // Simple command
/// let cmd = command!("/system/resource/print");
///
/// // Command with attributes
/// let cmd = command!("/interface/print", user="admin", detail);
/// ```
#[macro_export]
macro_rules! command {
    // Case: command literal plus optional attributes (with or without `= value`)
    ($cmd:literal $(, $key:ident $(= $value:expr)? )* $(,)?) => {{
        const VALIDATED: &str = $crate::macros::check_mikrotik_command($cmd);

        #[allow(unused_mut)]
        let mut builder = $crate::command::CommandBuilder::new()
            .command(VALIDATED);

        $(
            builder = builder.attribute(
                stringify!($key),
                command!(@opt $($value)?)
            );
        )*

        builder.build()
    }};

    // Internal rule: expands to `Some($value)` if given, otherwise `None`
    (@opt $value:expr) => { Some($value) };
    (@opt) => { None };
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
            if i + len > data.len() {
                panic!("Malformed command data");
            }
            let word = &data[i..i + len];
            i += len;
            words.push(String::from_utf8_lossy(word).into_owned());
        }
        words
    }

    #[test]
    fn test_command_no_attributes() {
        let cmd = command!("/system/resource/print");
        let words = parse_words(&cmd.data);

        assert_eq!(words[0], "/system/resource/print");
        assert!(words[1].starts_with(".tag="));
        assert_eq!(words.len(), 2);
    }

    #[test]
    fn test_command_with_one_attribute() {
        let cmd = command!("/interface/ethernet/print", user = "admin");
        let words = parse_words(&cmd.data);

        assert_eq!(words[0], "/interface/ethernet/print");
        assert!(words[1].starts_with(".tag="));
        assert_eq!(words[2], "=user=admin");
        assert_eq!(words.len(), 3);
    }

    #[test]
    fn test_command_with_multiple_attributes() {
        let cmd = command!("/some/random", attribute_no_value, another = "value");
        let words = parse_words(&cmd.data);

        assert_eq!(words[0], "/some/random");
        assert!(words[1].starts_with(".tag="));
        assert_eq!(words[2], "=attribute_no_value=");
        assert_eq!(words[3], "=another=value");
        assert_eq!(words.len(), 4);
    }
}
