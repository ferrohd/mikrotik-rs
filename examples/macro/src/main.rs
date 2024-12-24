use mikrotik_rs::{command, protocol::command::CommandBuilder};

fn main() {
    let macro_command = command!("/some/random/command", attribute1="1", attribute2, attribute3="2");

    let tag = macro_command.tag;

    let builder_command = CommandBuilder::with_tag(tag)
        .command("/some/random/command")
        .attribute("attribute1",Some("1"))
        .attribute("attribute2",Some("2"))
        .build();

    assert_eq!(macro_command.data, builder_command.data);
}
