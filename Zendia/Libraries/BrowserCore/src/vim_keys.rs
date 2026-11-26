/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The commands behind the vim keyboard scheme that only the key engine runs: the page's modes, what the engine has
//! pending, scrolling the page and editing the focused field. It also says how a typed count reaches a command.
//!
//! The behaviour follows the old Zendia fork: `j` scrolls 60 px times the count, `<C-d>` half the page and `<C-f>` 0.9
//! of it, smoothly, in the element the focus is in or else the page's scroller (the Automation World finds it); a
//! field taking the focus in normal mode switches to insert mode and losing it switches back, unless insert mode was
//! asked for with `I`; `<Esc>` in a field leaves it.

use crate::commands::{ArgumentSchema, ArgumentType, Command, CommandDescriptor, CommandError};
use crate::model::TabId;

/// How many times a count can repeat a command that moves one step at a time.
pub const MAX_REPEAT: u32 = 1000;

/// A scroll of the page that a key asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollMotion {
    Down,
    Up,
    Left,
    Right,
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    Top,
    Bottom,
}

impl ScrollMotion {
    const ALL: [Self; 10] = [
        Self::Down,
        Self::Up,
        Self::Left,
        Self::Right,
        Self::HalfPageDown,
        Self::HalfPageUp,
        Self::PageDown,
        Self::PageUp,
        Self::Top,
        Self::Bottom,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Down => "scroll.down",
            Self::Up => "scroll.up",
            Self::Left => "scroll.left",
            Self::Right => "scroll.right",
            Self::HalfPageDown => "scroll.half_page_down",
            Self::HalfPageUp => "scroll.half_page_up",
            Self::PageDown => "scroll.page_down",
            Self::PageUp => "scroll.page_up",
            Self::Top => "scroll.top",
            Self::Bottom => "scroll.bottom",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Down => "Scroll Down",
            Self::Up => "Scroll Up",
            Self::Left => "Scroll Left",
            Self::Right => "Scroll Right",
            Self::HalfPageDown => "Scroll Half Page Down",
            Self::HalfPageUp => "Scroll Half Page Up",
            Self::PageDown => "Scroll Page Down",
            Self::PageUp => "Scroll Page Up",
            Self::Top => "Go to Top",
            Self::Bottom => "Go to Bottom",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|motion| motion.name() == name)
    }

    /// Whether a count multiplies the motion. Going to the top or the bottom takes none.
    pub fn is_counted(self) -> bool {
        !matches!(self, Self::Top | Self::Bottom)
    }

    /// The Automation World's motion for this one.
    pub fn automation(self) -> zendia_automation::ScrollMotion {
        use zendia_automation::ScrollMotion as Page;
        match self {
            Self::Down => Page::Down,
            Self::Up => Page::Up,
            Self::Left => Page::Left,
            Self::Right => Page::Right,
            Self::HalfPageDown => Page::HalfPageDown,
            Self::HalfPageUp => Page::HalfPageUp,
            Self::PageDown => Page::PageDown,
            Self::PageUp => Page::PageUp,
            Self::Top => Page::Top,
            Self::Bottom => Page::Bottom,
        }
    }
}

/// An edit of the focused field that an insert mode key asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldEdit {
    /// `<C-w>`.
    DeleteWordBackward,
    /// `<C-u>`.
    DeleteToLineStart,
    /// `<C-h>`.
    DeleteCharBackward,
}

impl FieldEdit {
    const ALL: [Self; 3] = [
        Self::DeleteWordBackward,
        Self::DeleteToLineStart,
        Self::DeleteCharBackward,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::DeleteWordBackward => "edit.delete_word_backward",
            Self::DeleteToLineStart => "edit.delete_to_line_start",
            Self::DeleteCharBackward => "edit.delete_char_backward",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::DeleteWordBackward => "Delete Word Before the Cursor",
            Self::DeleteToLineStart => "Delete to the Start of the Line",
            Self::DeleteCharBackward => "Delete the Character Before the Cursor",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|edit| edit.name() == name)
    }
}

/// Commands of the vim key engine.
#[derive(Clone, Debug, PartialEq)]
pub enum VimCommand {
    /// Leaves insert or visual mode for normal mode (`<Esc>` in insert mode).
    ModeNormal,
    /// Insert mode that lasts until `<Esc>`, even with no field focused (`I`).
    ModeInsert,
    /// What the key engine has pending, for the mode indicator and which-key.
    ReportPending {
        keys: Vec<String>,
        count: Option<u32>,
    },
    /// A field of the page took or lost the focus.
    EditableFocusChanged {
        tab_id: Option<TabId>,
        editing: bool,
    },
    /// The active tab changed under the key engine: its mode follows whether a field of the new tab has the focus.
    SyncPageMode,
    /// A key moved to another tab: a field the page kept focused gives up the focus there, so the keys that follow
    /// are normal mode's (`gi` goes back to it).
    LeaveField,
    ScrollPage {
        tab_id: Option<TabId>,
        motion: ScrollMotion,
        count: Option<u32>,
    },
    EditField {
        tab_id: Option<TabId>,
        edit: FieldEdit,
    },
}

impl VimCommand {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ModeNormal => "mode.normal",
            Self::ModeInsert => "mode.insert",
            Self::ReportPending { .. } => "input.report_pending",
            Self::EditableFocusChanged { .. } => "page.editable_focus_changed",
            Self::SyncPageMode => "input.sync_page_mode",
            Self::LeaveField => "input.leave_field",
            Self::ScrollPage { motion, .. } => motion.name(),
            Self::EditField { edit, .. } => edit.name(),
        }
    }
}

fn find_arg<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

fn parse_u64(command: &str, args: &[(String, String)], key: &str) -> Result<Option<u64>, CommandError> {
    find_arg(args, key)
        .map(|raw| {
            raw.parse::<u64>().map_err(|_| CommandError::InvalidArgument {
                command: command.to_string(),
                message: format!("expected integer `{key}`, got `{raw}`"),
            })
        })
        .transpose()
}

fn parse_count(command: &str, args: &[(String, String)]) -> Result<Option<u32>, CommandError> {
    Ok(parse_u64(command, args, "count")?.map(|count| count.min(u64::from(u32::MAX)) as u32))
}

fn parse_tab(command: &str, args: &[(String, String)]) -> Result<Option<TabId>, CommandError> {
    Ok(parse_u64(command, args, "tab_id")?.map(TabId))
}

/// Parses the commands of this module, or returns `None` for a name it does not own.
pub fn parse_command(name: &str, args: &[(String, String)]) -> Result<Option<Command>, CommandError> {
    if let Some(motion) = ScrollMotion::parse(name) {
        return Ok(Some(Command::Vim(VimCommand::ScrollPage {
            tab_id: parse_tab(name, args)?,
            motion,
            count: parse_count(name, args)?,
        })));
    }
    if let Some(edit) = FieldEdit::parse(name) {
        return Ok(Some(Command::Vim(VimCommand::EditField {
            tab_id: parse_tab(name, args)?,
            edit,
        })));
    }
    let command = match name {
        "mode.normal" => VimCommand::ModeNormal,
        "mode.insert" => VimCommand::ModeInsert,
        "input.report_pending" => VimCommand::ReportPending {
            keys: find_arg(args, "keys")
                .map(|keys| keys.split_whitespace().map(str::to_string).collect())
                .unwrap_or_default(),
            count: parse_count(name, args)?,
        },
        "page.editable_focus_changed" => VimCommand::EditableFocusChanged {
            tab_id: parse_tab(name, args)?,
            editing: match find_arg(args, "editing") {
                Some("true") => true,
                Some("false") | None => false,
                Some(raw) => {
                    return Err(CommandError::InvalidArgument {
                        command: name.to_string(),
                        message: format!("expected boolean `editing`, got `{raw}`"),
                    });
                }
            },
        },
        "input.sync_page_mode" => VimCommand::SyncPageMode,
        "input.leave_field" => VimCommand::LeaveField,
        _ => return Ok(None),
    };
    Ok(Some(Command::Vim(command)))
}

/// The descriptors of this module's commands, for the palette, plugins and agents.
pub fn descriptors() -> Vec<CommandDescriptor> {
    let tab_arg = || {
        ArgumentSchema::new(
            "tab_id",
            ArgumentType::TabId,
            false,
            "Tab to act on (defaults to active)",
        )
    };
    let count_arg = || ArgumentSchema::new("count", ArgumentType::Integer, false, "How many times (defaults to 1)");
    let mut descriptors = vec![
        CommandDescriptor {
            name: "mode.normal".to_string(),
            title: "Normal Mode".to_string(),
            description: "Leave insert or visual mode for vim normal mode".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "mode.insert".to_string(),
            title: "Insert Mode".to_string(),
            description: "Pass keys to the page until Escape, even with no field focused".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "input.report_pending".to_string(),
            title: "Report Pending Keys".to_string(),
            description: "Publish the keys and count the vim key engine has pending".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "keys",
                    ArgumentType::String,
                    false,
                    "Pending key tokens, space separated",
                ),
                ArgumentSchema::new("count", ArgumentType::Integer, false, "Pending count"),
            ],
        },
        CommandDescriptor {
            name: "page.editable_focus_changed".to_string(),
            title: "Page Field Focus Changed".to_string(),
            description: "A field of the page took or lost the keyboard focus".to_string(),
            arguments: vec![
                tab_arg(),
                ArgumentSchema::new("editing", ArgumentType::Bool, true, "Whether a field has the focus"),
            ],
        },
        CommandDescriptor {
            name: "input.leave_field".to_string(),
            title: "Leave the Field".to_string(),
            description: "The page's focused field gives up the focus, back to normal mode".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "input.sync_page_mode".to_string(),
            title: "Sync Page Mode".to_string(),
            description: "Match the vim mode to whether a field of the active tab has the focus".to_string(),
            arguments: Vec::new(),
        },
    ];
    for motion in ScrollMotion::ALL {
        let mut arguments = vec![tab_arg()];
        if motion.is_counted() {
            arguments.push(count_arg());
        }
        descriptors.push(CommandDescriptor {
            name: motion.name().to_string(),
            title: motion.title().to_string(),
            description: format!("{} in the page", motion.title()),
            arguments,
        });
    }
    for edit in FieldEdit::ALL {
        descriptors.push(CommandDescriptor {
            name: edit.name().to_string(),
            title: edit.title().to_string(),
            description: format!("{} in the focused field of the page", edit.title()),
            arguments: vec![tab_arg()],
        });
    }
    descriptors
}

/// Commands that take one step per run, so a count runs them that many times (`3J` is three tabs down). Commands that
/// read the count themselves (`scroll.*`) get it as their `count` argument instead.
const REPEATED_BY_COUNT: &[&str] = &[
    "tab.next",
    "tab.previous",
    "tab.move_next",
    "tab.move_prev",
    "navigation.back",
    "navigation.forward",
    "split.grow_height",
    "split.shrink_height",
    "split.grow_width",
    "split.shrink_width",
    "explorer.down",
    "explorer.up",
    "explorer.move_down",
    "explorer.move_up",
    "toolbar.left",
    "toolbar.right",
    "library.down",
    "library.up",
    "library.left",
    "library.right",
    "library.next_section",
    "library.prev_section",
    "library.move_space_left",
    "library.move_space_right",
    "input.next",
    "input.prev",
];

/// The commands a key mapping runs: `command` with the mapping's `args`, and the typed `count`, if any.
pub fn commands_for_key(
    command: &str,
    args: &[(String, String)],
    count: Option<u32>,
) -> Result<Vec<Command>, CommandError> {
    if REPEATED_BY_COUNT.contains(&command) {
        let parsed = Command::from_name_and_args(command, args)?;
        let times = count.unwrap_or(1).clamp(1, MAX_REPEAT) as usize;
        return Ok(vec![parsed; times]);
    }
    let mut args = args.to_vec();
    if let Some(count) = count
        && !args.iter().any(|(name, _)| name == "count")
    {
        args.push(("count".to_string(), count.to_string()));
    }
    Ok(vec![Command::from_name_and_args(command, &args)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_motions_reach_the_page() {
        assert_eq!(
            ScrollMotion::HalfPageDown.automation(),
            zendia_automation::ScrollMotion::HalfPageDown
        );
        assert!(!ScrollMotion::Top.is_counted());
    }

    #[test]
    fn counts_reach_commands() {
        let scroll = commands_for_key("scroll.down", &[], Some(5)).unwrap();
        assert_eq!(
            scroll,
            vec![Command::Vim(VimCommand::ScrollPage {
                tab_id: None,
                motion: ScrollMotion::Down,
                count: Some(5),
            })]
        );
        let tabs = commands_for_key("tab.next", &[], Some(3)).unwrap();
        assert_eq!(tabs, vec![Command::TabNext; 3]);
        let once = commands_for_key("tab.next", &[], None).unwrap();
        assert_eq!(once, vec![Command::TabNext]);
        let huge = commands_for_key("tab.previous", &[], Some(u32::MAX)).unwrap();
        assert_eq!(huge.len(), MAX_REPEAT as usize);
    }

    #[test]
    fn every_vim_profile_command_is_typed() {
        for binding in &crate::input_state::vim_profile().bindings {
            let invocation = &binding.invocation;
            let parsed = Command::from_name_and_args(&invocation.command, &invocation.args_as_pairs())
                .unwrap_or_else(|error| panic!("`{}` does not parse: {error}", invocation.command));
            assert!(
                !matches!(parsed, Command::Custom { .. }),
                "`{}` has no typed command",
                invocation.command
            );
        }
    }

    #[test]
    fn names_round_trip() {
        for descriptor in descriptors() {
            if descriptor.name == "page.editable_focus_changed" {
                continue;
            }
            let parsed = Command::from_name_and_args(&descriptor.name, &[]).unwrap();
            assert_eq!(parsed.name(), descriptor.name);
        }
    }
}
