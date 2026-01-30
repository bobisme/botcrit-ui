//! Command definitions for the command palette.

use crate::message::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandId {
    Quit,
    CycleTheme,
    ToggleDiffView,
    ToggleDiffWrap,
    ToggleSidebar,
    OpenFileInEditor,
}

#[derive(Clone)]
pub struct CommandSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub id: CommandId,
}

pub fn get_commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            name: "quit",
            description: "Quit the application",
            id: CommandId::Quit,
        },
        CommandSpec {
            name: "theme: cycle",
            description: "Cycle to the next theme",
            id: CommandId::CycleTheme,
        },
        CommandSpec {
            name: "diff: toggle view",
            description: "Toggle between unified and side-by-side diff",
            id: CommandId::ToggleDiffView,
        },
        CommandSpec {
            name: "diff: toggle wrap",
            description: "Toggle line wrapping in diffs",
            id: CommandId::ToggleDiffWrap,
        },
        CommandSpec {
            name: "sidebar: toggle",
            description: "Show or hide the file sidebar",
            id: CommandId::ToggleSidebar,
        },
        CommandSpec {
            name: "editor: open file",
            description: "Open the current file in an external editor",
            id: CommandId::OpenFileInEditor,
        },
    ]
}

pub fn command_id_to_message(id: CommandId) -> Message {
    match id {
        CommandId::Quit => Message::Quit,
        CommandId::CycleTheme => Message::CycleTheme,
        CommandId::ToggleDiffView => Message::ToggleDiffView,
        CommandId::ToggleDiffWrap => Message::ToggleDiffWrap,
        CommandId::ToggleSidebar => Message::ToggleSidebar,
        CommandId::OpenFileInEditor => Message::OpenFileInEditor,
    }
}
