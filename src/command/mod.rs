use getrandom;
use std::{marker::PhantomData, mem::size_of};

/// Internal module for handling command responses.
pub mod sentence;
/// Module with structures for command responses.
pub mod response;

/// Represents an empty command. Used as a marker in [`CommandBuilder`].
pub struct NoCmd;

/// Represents a command that has at least one operation (e.g., a login or a query).
/// Used as a marker in [`CommandBuilder`].
#[derive(Clone)]
pub struct Cmd;

/// [`CommandBuilder`] is used to construct commands to be sent to MikroTik routers.
/// It transitions from [`NoCmd`] state to [`Cmd`] state as parts of the command
/// are being specified. This enforces at compile time that only complete commands can be built.
#[derive(Clone)]
pub struct CommandBuilder<Cmd> {
    tag: u16,
    cmd: CommandBuffer,
    state: PhantomData<Cmd>,
}

impl Default for CommandBuilder<NoCmd> {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandBuilder<NoCmd> {
    /// Begin building a new [`Command`] with a randomly generated tag.
    pub fn new() -> Self {
        let mut dest = [0_u8; size_of::<u16>()];
        getrandom::getrandom(&mut dest).expect("Failed to generate random tag");
        Self {
            tag: u16::from_be_bytes(dest),
            cmd: CommandBuffer::default(),
            state: PhantomData,
        }
    }
    /// Begin building a new [`Command`] with a specified tag.
    ///
    /// # Arguments
    ///
    /// * `tag` - A `u16` tag value that uniquely identifies the command. **Must be unique**.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let builder = CommandBuilder::with_tag(1234);
    /// ```
    pub fn with_tag(tag: u16) -> Self {
        Self {
            tag,
            cmd: CommandBuffer::default(),
            state: PhantomData,
        }
    }

    /// Builds a login command with the provided username and optional password.
    ///
    /// # Arguments
    ///
    /// * `username` - The username for the login command.
    /// * `password` - An optional password for the login command.
    ///
    /// # Returns
    ///
    /// A `Command` which represents the login operation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let login_cmd = CommandBuilder::login("admin", Some("password"));
    /// ```
    pub fn login(username: &str, password: Option<&str>) -> Command {
        Self::new()
            .command("/login")
            .attribute("name", Some(username))
            .attribute("password", password)
            .build()
    }

    /// Builds a command to cancel a specific running command identified by `tag`.
    ///
    /// # Arguments
    ///
    /// * `tag` - The tag of the command to be canceled.
    ///
    /// # Returns
    ///
    /// A `Command` which represents the cancel operation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let cancel_cmd = CommandBuilder::cancel(1234);
    /// ```
    pub fn cancel(tag: u16) -> Command {
        Self::with_tag(tag)
            .command("/cancel")
            .attribute("tag", Some(tag.to_string().as_str()))
            .build()
    }
}

impl CommandBuilder<NoCmd> {
    /// Transitions the builder to the `Cmd` state by specifying the command to be executed.
    ///
    /// # Arguments
    ///
    /// * `command` - The MikroTik command to execute.
    ///
    /// # Returns
    ///
    /// The builder transitioned to the `Cmd` state for further configuration.
    pub fn command(self, command: &str) -> CommandBuilder<Cmd> {
        let Self { tag, mut cmd, .. } = self;
        // FIX: This allocation should be avoided
        // Write the command
        cmd.write_word(command.as_bytes());
        // FIX: This allocation should be avoided
        // Tag the command
        cmd.write_word(format!(".tag={}", tag).as_bytes());
        CommandBuilder {
            tag,
            cmd,
            state: PhantomData,
        }
    }
}

impl CommandBuilder<Cmd> {
    /// Adds an attribute to the command being built.
    ///
    /// # Arguments
    ///
    /// * `key` - The attribute's key.
    /// * `value` - The attribute's value, which is optional. If `None`, the attribute is treated as a flag.
    ///
    /// # Returns
    ///
    /// The builder with the attribute added, allowing for method chaining.
    pub fn attribute(self, key: &str, value: Option<&str>) -> CommandBuilder<Cmd> {
        let Self { tag, mut cmd, .. } = self;
        match value {
            Some(v) => {
                // FIX: This allocation should be avoided
                cmd.write_word(format!("={key}={v}").as_bytes());
            }
            None => {
                // FIX: This allocation should be avoided
                cmd.write_word(format!("={key}=").as_bytes());
            }
        };
        CommandBuilder {
            tag,
            cmd,
            state: PhantomData,
        }
    }

    /// Finalizes the command construction process, producing a [`Command`].
    ///
    /// # Returns
    ///
    /// A `Command` instance ready for execution.
    pub fn build(self) -> Command {
        let Self { tag, mut cmd, .. } = self;
        // Terminating the command
        cmd.write_len(0);
        Command { tag, data: cmd.0 }
    }
}

/// Represents a final command, complete with a tag and data, ready to be sent to the router.
/// To create a [`Command`], use a [`CommandBuilder`].
///
/// - `tag` is used to identify the command and correlate with its [`response::CommandResponse`]s when it is received.
/// - `data` contains the command itself, which is a sequence of bytes, null-terminated.
///
/// # Examples
///
/// ```rust
/// let cmd = CommandBuilder::new().command("/interface/print").build();
/// ```
#[derive(Debug)]
pub struct Command {
    /// The tag of the command.
    pub tag: u16,
    /// The data of the command.
    pub data: Vec<u8>,
}

#[derive(Default, Clone)]
struct CommandBuffer(Vec<u8>);

impl CommandBuffer {
    fn write_str(&mut self, str_buff: &[u8]) {
        self.0.extend_from_slice(str_buff);
    }
    fn write_len(&mut self, len: u32) {
        match len {
            0x00..=0x7F => self.write_str(&[len as u8]),
            0x80..=0x3FFF => {
                let l = len | 0x8000;
                self.write_str(&[((l >> 8) & 0xFF) as u8]);
                self.write_str(&[(l & 0xFF) as u8]);
            }
            0x4000..=0x1FFFFF => {
                let l = len | 0xC00000;
                self.write_str(&[((l >> 16) & 0xFF) as u8]);
                self.write_str(&[((l >> 8) & 0xFF) as u8]);
                self.write_str(&[(l & 0xFF) as u8]);
            }
            0x200000..=0xFFFFFFF => {
                let l = len | 0xE0000000;
                self.write_str(&[((l >> 24) & 0xFF) as u8]);
                self.write_str(&[((l >> 16) & 0xFF) as u8]);
                self.write_str(&[((l >> 8) & 0xFF) as u8]);
                self.write_str(&[(l & 0xFF) as u8]);
            }
            _ => {
                self.write_str(&[0xF0_u8]);
                self.write_str(&[((len >> 24) & 0xFF) as u8]);
                self.write_str(&[((len >> 16) & 0xFF) as u8]);
                self.write_str(&[((len >> 8) & 0xFF) as u8]);
                self.write_str(&[(len & 0xFF) as u8]);
            }
        }
    }
    fn write_word(&mut self, w: &[u8]) {
        self.write_len(w.len() as u32);
        self.write_str(w);
    }
}

//   pub fn query_is_present(&mut self, key: &str) {
//        let query = format!("?{key}");
//        self.write_word(query.as_str());
//    }
//    pub fn query_not_present(&mut self, key: &str) {
//        let query = format!("?-{key}");
//        self.write_word(query.as_str());
//    }
//    pub fn query_equal(&mut self, key: &str, value: &str) {
//        let query = format!("?{key}={value}");
//        self.write_word(query.as_str());
//    }
//    pub fn query_gt(&mut self, key: &str, value: &str) {
//        let query = format!("?>{key}={value}");
//        self.write_word(query.as_str());
//    }
//    pub fn query_lt(&mut self, key: &str, value: &str) {
//        let query = format!("?<{key}={value}");
//        self.write_word(query.as_str());
//    }
//    pub fn query_operator(&mut self, operator: QueryOperator) {
//        let query = format!("?#{operator}");
//        self.write_word(query.as_str());
//    }

/// Represents a query operator. WIP.
pub enum QueryOperator {
    /// Represents the `!` operator.
    Not,
    /// Represents the `&` operator.
    And,
    /// Represents the `|` operator.
    Or,
    /// Represents the `.` operator.
    Dot,
}

impl std::fmt::Display for QueryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryOperator::Not => write!(f, "!"),
            QueryOperator::And => write!(f, "&"),
            QueryOperator::Or => write!(f, "|"),
            QueryOperator::Dot => write!(f, "."),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::str;

    #[test]
    fn test_command_builder_new() {
        let builder = CommandBuilder::<NoCmd>::new();
        assert_eq!(builder.cmd.0.len(), 0);
        assert!(builder.tag != 0); // Ensure that random tag is generated
    }

    #[test]
    fn test_command_builder_with_tag() {
        let tag = 1234;
        let builder = CommandBuilder::<NoCmd>::with_tag(tag);
        assert_eq!(builder.tag, tag);
    }

    #[test]
    fn test_command_builder_command() {
        let builder = CommandBuilder::<NoCmd>::with_tag(1234).command("/interface/print");
        println!("{:?}", builder.cmd.0);
        assert_eq!(builder.cmd.0.len(), 27);
        assert_eq!(builder.cmd.0[1..17], b"/interface/print"[..]);
        assert_eq!(builder.cmd.0[18..27], b".tag=1234"[..]);
    }

    #[test]
    fn test_command_builder_attribute() {
        let builder = CommandBuilder::<NoCmd>::with_tag(1234)
            .command("/interface/print")
            .attribute("name", Some("ether1"));

        assert_eq!(
            builder.cmd.0[28..40],
            b"=name=ether1"[..]
        );
    }

    //#[test]
    //fn test_command_builder_build() {
    //    let command = CommandBuilder::<NoCmd>::with_tag(1234)
    //        .command("/interface/print")
    //        .attribute("name", Some("ether1"))
    //        .attribute("disabled", None)
    //        .build();
//
    //    let expected_data: &[u8] = [
    //        b"\x10/interface/print",
    //        b"\x09.tag=1234",
    //        b"\x0C=name=ether1",
    //        b"\x0A=disabled=",
    //        b"\x00",
    //    ].concat();
//
    //    assert_eq!(command.data, expected_data);
    //}

    #[test]
    fn test_command_builder_login() {
        let command = CommandBuilder::<NoCmd>::login("admin", Some("password"));

        assert!(str::from_utf8(&command.data).unwrap().contains("/login"));
        assert!(str::from_utf8(&command.data).unwrap().contains("name=admin"));
        assert!(str::from_utf8(&command.data).unwrap().contains("password=password"));
    }

    #[test]
    fn test_command_builder_cancel() {
        let command = CommandBuilder::<NoCmd>::cancel(1234);

        assert!(str::from_utf8(&command.data).unwrap().contains("/cancel"));
        assert!(str::from_utf8(&command.data).unwrap().contains("tag=1234"));
    }

    #[test]
    fn test_command_buffer_write_len() {
        let mut buffer = CommandBuffer::default();

        buffer.write_len(0x7F);
        assert_eq!(buffer.0, vec![0x7F]);

        buffer.0.clear();
        buffer.write_len(0x80);
        assert_eq!(buffer.0, vec![0x80, 0x80]);

        buffer.0.clear();
        buffer.write_len(0x4000);
        assert_eq!(buffer.0, vec![0xC0, 0x40, 0x00]);

        buffer.0.clear();
        buffer.write_len(0x200000);
        assert_eq!(buffer.0, vec![0xE0, 0x20, 0x00, 0x00]);

        buffer.0.clear();
        buffer.write_len(0x10000000);
        assert_eq!(buffer.0, vec![0xF0, 0x10, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_command_buffer_write_word() {
        let mut buffer = CommandBuffer::default();
        buffer.write_word(b"test");
        assert_eq!(buffer.0, vec![0x04, b't', b'e', b's', b't']);
    }

    #[test]
    fn test_query_operator_to_string() {
        assert_eq!(QueryOperator::Not.to_string(), "!");
        assert_eq!(QueryOperator::And.to_string(), "&");
        assert_eq!(QueryOperator::Or.to_string(), "|");
        assert_eq!(QueryOperator::Dot.to_string(), ".");
    }
}
