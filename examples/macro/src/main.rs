use mikrotik_rs::{command, protocol::command::CommandBuilder};

fn main() {
    let macro_command = command!(
        b"/some/random/command",
        attribute1 = b"1",
        attribute2,
        attribute3 = b"2"
    );

    let tag = macro_command.tag;

    let builder_command = CommandBuilder::with_tag(tag)
        .command(b"/some/random/command")
        .attribute(b"attribute1", Some(b"1"))
        .attribute(b"attribute2", Some(b"2"))
        .build();

    assert_eq!(macro_command.data, builder_command.data);
}
