/// A minimal const validator that enforces some basic MikroTik command rules:
/// 1. Must start with `/`.
/// 2. No empty segments (no `//`).
/// 3. Only allows [a-zA-Z0-9_-] plus space or slash as separators.
/// 4. No consecutive spaces or slashes.
///
/// Panics **at compile time** if invalid.
pub const fn check_mikrotik_command(cmd: &str) -> &str {
    let bytes = cmd.as_bytes();
    let len = bytes.len();

    // Reject empty string
    if len == 0 {
        panic!("MikroTik command cannot be empty.");
    }

    // Must start with slash
    if bytes[0] != b'/' {
        panic!("MikroTik command must start with '/'.");
    }

    // Track if the previous character was a space or slash to detect duplicates
    let mut prev_was_delimiter = true; // start true because first char is '/'

    // Validate each character
    let mut i = 1;
    while i < len {
        let c = bytes[i] as char;

        // Check allowed delimiters vs allowed segment chars
        if c == '/' || c == ' ' {
            if prev_was_delimiter {
                // Found "//" or double-space
                panic!("No empty segments or consecutive delimiters allowed.");
            }
            prev_was_delimiter = true;
        } else {
            // Must be [a-zA-Z0-9_-]
            let is_valid_char = c.is_ascii_alphanumeric() || c == '-' || c == '_';
            if !is_valid_char {
                panic!("Invalid character in MikroTik command. Must be [a-zA-Z0-9_-]");
            }
            prev_was_delimiter = false;
        }

        i += 1;
    }

    // If the command ends on a delimiter, we have a trailing slash or space
    if prev_was_delimiter {
        panic!("Command cannot end with a delimiter.");
    }

    // If we got here, it's valid
    cmd
}

/// Macro that enforces Mikrotik command syntax **at compile time**.
///
/// Usage Examples:
/// ```rust
///fn main() {
///    // OK
///    let _ok = command!("/random command print");
///
///    let _with_attrs = command!("/random command", attr1="value1", attr2);
///}
/// ```
#[macro_export]
macro_rules! command {
    // Case: command literal plus one or more attributes (with or without `= value`)
    ($cmd:literal $(, $key:ident $(= $value:expr)? )* $(,)?) => {{
        const VALIDATED: &str = $crate::macros::check_mikrotik_command($cmd);

        #[allow(unused_mut)]
        let mut builder = $crate::protocol::command::CommandBuilder::new()
            .command(VALIDATED);

        $(
            builder = builder.attribute(
                stringify!($key),
                command!(@opt $($value)?)
            );
        )*

        builder.build()
    }};

    // Internal rule that expands to `Some($value)` if given, otherwise `None`
    (@opt $value:expr) => { Some($value) };
    (@opt) => { None };
}

#[cfg(test)]
mod test {
    /// Helper to parse the RouterOS length-prefixed “words” out of the command data.
    ///
    /// The builder writes each word as:
    ///   [1-byte length][word bytes] ...
    /// with a final 0-length to signal the end.
    fn parse_words(data: &[u8]) -> Vec<String> {
        let mut words = Vec::new();
        let mut i = 0;
        while i < data.len() {
            // read a single-byte length
            if i >= data.len() {
                break;
            }
            let len = data[i] as usize;
            i += 1;
            if len == 0 {
                // length==0 signals end
                break;
            }
            if i + len > data.len() {
                panic!("Malformed command data: length prefix exceeds available data.");
            }
            let word = &data[i..i + len];
            i += len;
            // Convert to String for easier assertions
            words.push(String::from_utf8_lossy(word).to_string());
        }
        words
    }

    #[test]
    fn test_command_no_attributes() {
        let cmd = command!("/system/resource/print");
        let words = parse_words(&cmd.data);

        // Word[0] => actual command
        assert_eq!(words[0], "/system/resource/print");

        // Word[1] => .tag=xxxx
        // We can’t check the exact tag value because it's random, but we can ensure it starts with ".tag="
        assert!(
            words[1].starts_with(".tag="),
            "Tag word should start with .tag="
        );

        // Should only have these two words (plus the 0-length terminator, which we skip).
        assert_eq!(words.len(), 2, "Expected two words (command + .tag=).");
    }

    #[test]
    fn test_command_with_one_attribute() {
        let cmd = command!("/interface/ethernet/print", user = "admin");
        let words = parse_words(&cmd.data);

        assert_eq!(words[0], "/interface/ethernet/print");
        assert!(
            words[1].starts_with(".tag="),
            "Expected .tag= as second word"
        );
        // Word[2] => "=user=admin"
        assert_eq!(words[2], "=user=admin");
        // So total 3 words plus 0-terminator
        assert_eq!(words.len(), 3);
    }

    #[test]
    fn test_command_with_multiple_attributes() {
        let cmd = command!("/some/random", attribute_no_value, another = "value");
        let words = parse_words(&cmd.data);

        // Word[0] => "/some/random"
        assert_eq!(words[0], "/some/random");
        // Word[1] => ".tag=xxxx"
        assert!(words[1].starts_with(".tag="));
        // Word[2] => "=attribute_no_value="
        assert_eq!(words[2], "=attribute_no_value=");
        // Word[3] => "=another=value"
        assert_eq!(words[3], "=another=value");
        // Total 4 words plus terminator
        assert_eq!(words.len(), 4);
    }
}
