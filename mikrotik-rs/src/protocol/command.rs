use crate::protocol::string::{AsciiStringRef, WordContent};
use getrandom;
use std::{marker::PhantomData, mem::size_of};

/// Represents an empty command. Used as a marker in [`CommandBuilder`].
pub struct NoCmd;
/// Represents a command that has at least one operation (e.g., a login or a query).
/// Used as a marker in [`CommandBuilder`].
#[derive(Clone)]
pub struct Cmd;

/// Builds MikroTik router commands using a fluid API.
///
/// Ensures that only commands with at least one operation can be built and sent.
///
/// # Examples
/// ```
/// use mikrotik_rs::protocol::command::CommandBuilder;
/// let cmd = CommandBuilder::new()
///     .command(b"/system/resource/print")
///     .flag_attribute(b"detail")
///     .build();
/// ```
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
    /// * `tag` - A `u16` tag value that identifies the command for RouterOS correlation. **Must be unique**.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mikrotik_rs::protocol::command::CommandBuilder;
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
    /// use mikrotik_rs::protocol::command::CommandBuilder;
    /// let login_cmd = CommandBuilder::login(b"admin", Some(b"password"));
    /// ```
    pub fn login<'u, 'p, U: Into<AsciiStringRef<'u>>, P: Into<AsciiStringRef<'p>>>(
        username: U,
        password: Option<P>,
    ) -> Command {
        Self::new()
            .command(b"/login")
            .attribute(b"name", Some(username))
            .attribute(b"password", password)
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
    /// use mikrotik_rs::protocol::command::CommandBuilder;
    /// let cancel_cmd = CommandBuilder::cancel(1234);
    /// ```
    pub fn cancel(tag: u16) -> Command {
        Self::with_tag(tag)
            .command(b"/cancel")
            .attribute(b"tag", Some(tag.to_string().as_bytes()))
            .build()
    }

    /// Specify the command to be executed.
    ///
    /// # Arguments
    ///
    /// * `command` - The MikroTik command to execute.
    ///
    /// # Returns
    ///
    /// The builder transitioned to the `Cmd` state for attributes configuration.
    pub fn command<W: WordContent + ?Sized>(self, command: &W) -> CommandBuilder<Cmd> {
        let Self { tag, mut cmd, .. } = self;
        // Write the command
        cmd.write_word(command);
        // Tag the command
        cmd.write_word(&[b".tag=", tag.to_string().as_bytes()]);
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
    /// * `value` - The attribute's value, which is optional. If `None`, the attribute is treated as a flag (e.g., `=key=`).
    ///
    /// # Returns
    ///
    /// The builder with the attribute added, allowing for method chaining.
    pub fn attribute<'k, 'v, K: Into<AsciiStringRef<'k>>, V: Into<AsciiStringRef<'v>>>(
        self,
        key: K,
        value: Option<V>,
    ) -> Self {
        let Self { tag, mut cmd, .. } = self;
        match value {
            Some(v) => {
                cmd.write_word(&[b"=", key.into().0, b"=", v.into().0]);
            }
            None => {
                cmd.write_word(&[b"=", key.into().0, b"="]);
            }
        }
        CommandBuilder {
            tag,
            cmd,
            state: PhantomData,
        }
    }

    /// Adds a flag attribute to the command being built.
    ///
    /// # Arguments
    ///
    /// * `key` - The attribute's key.
    ///
    /// # Returns
    ///
    /// The builder with the attribute added, allowing for method chaining.
    pub fn flag_attribute<'k, K: Into<AsciiStringRef<'k>>>(self, key: K) -> Self {
        let Self { tag, mut cmd, .. } = self;
        cmd.write_word(&[b"=", key.into().0, b"="]);
        CommandBuilder {
            tag,
            cmd,
            state: PhantomData,
        }
    }

    /// Adds a query to the command being built.
    /// pushes 'true' if an item has a value of property name, 'false' if it does not.
    ///
    /// #Arguments
    /// * `name`: name of the property to check
    ///
    /// # Returns
    ///
    /// The builder with the attribute added, allowing for method chaining.
    pub fn query_is_present<'s, S: Into<&'s [u8]>>(mut self, name: S) -> Self {
        self.cmd.write_word(&[b"?", name.into()]);
        self
    }

    /// Adds a query to the command being built.
    /// pushes 'true' if an item has a value of property name, 'false' if it does not.
    ///
    /// #Arguments
    /// * `name`: name of the property to check
    ///
    /// # Returns
    ///
    /// The builder with the attribute added, allowing for method chaining.
    pub fn query_not_present<'s, S: Into<&'s [u8]>>(mut self, name: S) -> Self {
        self.cmd.write_word(&[b"?-", name.into()]);
        self
    }
    /// Adds a query to the command being built.
    /// pushes 'true' if the property name has a value equal to x, 'false' otherwise.
    ///
    /// #Arguments
    /// * `name`: name of the property to compare
    /// * `value`: value to be compared with
    ///
    /// # Returns
    ///
    /// The builder with the attribute added, allowing for method chaining.
    pub fn query_equal<'k, 'v, K: Into<&'k [u8]>, V: Into<&'v [u8]>>(
        mut self,
        name: K,
        value: V,
    ) -> Self {
        self.cmd
            .write_word(&[b"?", name.into(), b"=", value.into()]);
        self
    }
    /// Adds a query to the command being built.
    /// pushes 'true' if the property name has a value greater than x, 'false' otherwise.
    ///
    /// #Arguments
    /// * `name`: name of the property to compare
    /// * `value`: value to be compared with
    ///
    /// # Returns
    ///
    /// The builder with the attribute added, allowing for method chaining.
    pub fn query_gt<'k, 'v, K: Into<&'k [u8]>, V: Into<&'v [u8]>>(
        mut self,
        key: K,
        value: V,
    ) -> Self {
        self.cmd
            .write_word(&[b"?>", key.into(), b"=", value.into()]);
        self
    }
    /// Adds a query to the command being built.
    /// pushes 'true' if the property name has a value less than x, 'false' otherwise.
    ///
    /// #Arguments
    /// * `name`: name of the property to compare
    /// * `value`: value to be compared with
    ///
    /// # Returns
    ///
    /// The builder with the attribute added, allowing for method chaining.
    pub fn query_lt<'k, 'v, K: Into<&'k [u8]>, V: Into<&'v [u8]>>(
        mut self,
        key: K,
        value: V,
    ) -> Self {
        self.cmd
            .write_word(&[b"?<", key.into(), b"=", value.into()]);
        self
    }

    /// defines combination of defined operations
    /// https://help.mikrotik.com/docs/spaces/ROS/pages/47579160/API#API-Queries
    /// #Arguments
    /// * `operations`: operation sequence to be applied to the results on the stack
    ///
    /// # Returns
    ///
    /// The builder with the attribute added, allowing for method chaining.
    pub fn query_operations(mut self, operations: impl Iterator<Item = QueryOperator>) -> Self {
        let query: Box<[u8]> = "?#"
            .as_bytes()
            .iter()
            .copied()
            .chain(operations.map(|op| op.code() as u8))
            .collect();
        self.cmd.write_word(query.as_ref());
        self
    }

    /// Finalizes the command construction process, producing a [`Command`].
    ///
    /// # Returns
    ///
    /// A `Command` instance ready for execution.
    pub fn build(self) -> Command {
        let Self { tag, mut cmd, .. } = self;
        // Terminate the command
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
/// use mikrotik_rs::protocol::command::CommandBuilder;
/// let cmd = CommandBuilder::new().command(b"/interface/print").build();
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
    fn write_word<W: WordContent + ?Sized>(&mut self, w: &W) {
        self.write_len(w.byte_count() as u32);
        w.write_to_buffer(&mut self.0);
    }
}

/// Represents a query operator. WIP.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
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
impl QueryOperator {
    #[inline]
    fn code(self) -> char {
        match self {
            QueryOperator::Not => '!',
            QueryOperator::And => '&',
            QueryOperator::Or => '|',
            QueryOperator::Dot => '.',
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
        let builder = CommandBuilder::<NoCmd>::with_tag(1234).command(b"/interface/print");
        println!("{:?}", builder.cmd.0);
        assert_eq!(builder.cmd.0.len(), 27);
        assert_eq!(builder.cmd.0[1..17], b"/interface/print"[..]);
        assert_eq!(builder.cmd.0[18..27], b".tag=1234"[..]);
    }

    #[test]
    fn test_command_builder_attribute() {
        let builder = CommandBuilder::<NoCmd>::with_tag(1234)
            .command(b"/interface/print")
            .attribute(b"name", Some(b"ether1"));

        assert_eq!(builder.cmd.0[28..40], b"=name=ether1"[..]);
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
        let command = CommandBuilder::<NoCmd>::login(b"admin", Some(b"password"));

        assert!(str::from_utf8(&command.data).unwrap().contains("/login"));
        assert!(str::from_utf8(&command.data)
            .unwrap()
            .contains("name=admin"));
        assert!(str::from_utf8(&command.data)
            .unwrap()
            .contains("password=password"));
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

    //#[test]
    //fn test_query_operator_to_string() {
    //    assert_eq!(QueryOperator::Not.to_string(), "!");
    //    assert_eq!(QueryOperator::And.to_string(), "&");
    //    assert_eq!(QueryOperator::Or.to_string(), "|");
    //    assert_eq!(QueryOperator::Dot.to_string(), ".");
    //}
}
