//! Show that the `command!` macro and `CommandBuilder` produce byte-identical wire data.
//!
//! The macro is sugar over the builder, including the attribute form with no value
//! (`attribute2` below), which RouterOS reads as "query this field" rather than "set it".
//! Nothing here talks to a router.

use mikrotik_rs::CommandBuilder;
use mikrotik_rs::command;

fn main() {
    let macro_command =
        command!("/some/random/command", attribute1 = "1", attribute2, attribute3 = "2");

    let tag = macro_command.tag;

    let builder_command = CommandBuilder::with_tag(tag)
        .command("/some/random/command")
        .attribute("attribute1", Some("1"))
        .attribute("attribute2", None)
        .attribute("attribute3", Some("2"))
        .build();

    assert_eq!(macro_command.data(), builder_command.data());
    println!("Macro and builder produce identical wire data!");
}
