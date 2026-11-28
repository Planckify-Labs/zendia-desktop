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
    /// `edit.delete_to_line_end` (`edit.deleteToLineEnd`).
    DeleteToLineEnd,
    /// `<C-h>`.
    DeleteCharBackward,
    /// `edit.delete_char_forward` (`edit.deleteCharForward`).
    DeleteCharForward,
}

impl FieldEdit {
    const ALL: [Self; 5] = [
        Self::DeleteWordBackward,
        Self::DeleteToLineStart,
        Self::DeleteToLineEnd,
        Self::DeleteCharBackward,
        Self::DeleteCharForward,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::DeleteWordBackward => "edit.delete_word_backward",
            Self::DeleteToLineStart => "edit.delete_to_line_start",
            Self::DeleteToLineEnd => "edit.delete_to_line_end",
            Self::DeleteCharBackward => "edit.delete_char_backward",
            Self::DeleteCharForward => "edit.delete_char_forward",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::DeleteWordBackward => "Delete Word Before the Cursor",
            Self::DeleteToLineStart => "Delete to the Start of the Line",
            Self::DeleteToLineEnd => "Delete to the End of the Line",
            Self::DeleteCharBackward => "Delete the Character Before the Cursor",
            Self::DeleteCharForward => "Delete the Character After the Cursor",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        match name {
            "edit.deleteWordBackward" => Some(Self::DeleteWordBackward),
            "edit.deleteToLineStart" => Some(Self::DeleteToLineStart),
            "edit.deleteToLineEnd" => Some(Self::DeleteToLineEnd),
            "edit.deleteCharBackward" => Some(Self::DeleteCharBackward),
            "edit.deleteCharForward" => Some(Self::DeleteCharForward),
            _ => Self::ALL.into_iter().find(|edit| edit.name() == name),
        }
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
    /// An auxiliary page surface (such as an extension popup or web chat panel) took or gave up keyboard focus.
    KeyedPageChanged {
        tab_id: Option<TabId>,
    },
    /// Registers or unregisters an auxiliary Hub view so page commands and vim modes can target it.
    RegisterAuxiliaryPage {
        tab_id: TabId,
        registered: bool,
    },
    /// Reports the visible row capacity and `[first_visible, end_visible)` slice of the sidebar explorer (`EXP-06`, `EXP-15`).
    ReportExplorerViewport {
        visible_rows: usize,
        first_visible: usize,
        end_visible: usize,
    },
    /// Reports which side panel currently holds keyboard focus (`None` when the page does), keeping the panel row in sync (`WIN-06`).
    PanelFocusChanged {
        panel_id: Option<crate::contributions::SidePanelId>,
    },
    /// Reports that the floating address bar was dismissed without submitting, cleaning up any empty URL split pane (`WIN-20`).
    AddressBarCancelled,
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
            Self::KeyedPageChanged { .. } => "page.keyed_changed",
            Self::RegisterAuxiliaryPage { .. } => "page.register_auxiliary",
            Self::ReportExplorerViewport { .. } => "explorer.report_viewport",
            Self::PanelFocusChanged { .. } => "panel.focus_changed",
            Self::AddressBarCancelled => "address_bar.cancelled",
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
        "page.keyed_changed" => VimCommand::KeyedPageChanged {
            tab_id: parse_tab(name, args)?,
        },
        "page.register_auxiliary" => VimCommand::RegisterAuxiliaryPage {
            tab_id: parse_tab(name, args)?.ok_or_else(|| CommandError::InvalidArgument {
                command: name.to_string(),
                message: "missing required `tab_id`".to_string(),
            })?,
            registered: match find_arg(args, "registered") {
                Some("true") | None => true,
                Some("false") => false,
                Some(raw) => {
                    return Err(CommandError::InvalidArgument {
                        command: name.to_string(),
                        message: format!("expected boolean `registered`, got `{raw}`"),
                    });
                }
            },
        },
        "input.sync_page_mode" => VimCommand::SyncPageMode,
        "input.leave_field" => VimCommand::LeaveField,
        "explorer.report_viewport" => VimCommand::ReportExplorerViewport {
            visible_rows: parse_u64(name, args, "visible_rows")?.unwrap_or(16) as usize,
            first_visible: parse_u64(name, args, "first_visible")?.unwrap_or(0) as usize,
            end_visible: parse_u64(name, args, "end_visible")?.unwrap_or(0) as usize,
        },
        "panel.focus_changed" => VimCommand::PanelFocusChanged {
            panel_id: find_arg(args, "panel_id")
                .filter(|raw| !raw.trim().is_empty())
                .map(crate::contributions::SidePanelId::new),
        },
        "address_bar.cancelled" => VimCommand::AddressBarCancelled,
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
            name: "page.keyed_changed".to_string(),
            title: "Keyed Page Changed".to_string(),
            description: "An auxiliary page view took or released keyboard focus".to_string(),
            arguments: vec![tab_arg()],
        },
        CommandDescriptor {
            name: "page.register_auxiliary".to_string(),
            title: "Register Auxiliary Page".to_string(),
            description: "Register or unregister an auxiliary Hub page view".to_string(),
            arguments: vec![
                ArgumentSchema::new("tab_id", ArgumentType::TabId, true, "Auxiliary view ID"),
                ArgumentSchema::new("registered", ArgumentType::Bool, false, "Whether the view is open"),
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
        CommandDescriptor {
            name: "explorer.report_viewport".to_string(),
            title: "Report Explorer Viewport".to_string(),
            description: "Report visible sidebar explorer rows for half-page jumps and row hints".to_string(),
            arguments: vec![
                ArgumentSchema::new("visible_rows", ArgumentType::Integer, false, "Visible row capacity"),
                ArgumentSchema::new("first_visible", ArgumentType::Integer, false, "First visible row index"),
                ArgumentSchema::new(
                    "end_visible",
                    ArgumentType::Integer,
                    false,
                    "One-past-last visible row index",
                ),
            ],
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
    "explorer.move_down",
    "explorer.move_up",
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

        // Standard profile (`standard.toml`) must also resolve solely to typed commands (`RT-01`).
        for binding in &zendia_keymap::Profile::standard().bindings {
            let invocation = &binding.invocation;
            let parsed = Command::from_name_and_args(&invocation.command, &invocation.args_as_pairs())
                .unwrap_or_else(|error| panic!("standard `{}` does not parse: {error}", invocation.command));
            assert!(
                !matches!(parsed, Command::Custom { .. }),
                "standard `{}` has no typed command",
                invocation.command
            );
        }

        // Built-in pickers (`tabs`, `agents`) must only name typed commands (`VimParity.md:1266`).
        let model = crate::stub::sample_model(true);
        for picker_name in ["tabs", "agents"] {
            let picker = crate::picker::build_builtin_picker(picker_name, &model).expect("builtin picker");
            for item in &picker.items {
                let parsed = Command::from_name_and_args(&item.command, &item.command_args).unwrap_or_else(|error| {
                    panic!("picker `{}` command `{}` failed: {error}", picker.id.0, item.command)
                });
                assert!(
                    !matches!(parsed, Command::Custom { .. }),
                    "picker `{}` item `{}` uses untyped command `{}`",
                    picker.id.0,
                    item.id,
                    item.command
                );
            }
        }

        // Every unmapped Appendix A action (`VimParity.md:1689-1704`) must translate to a typed command (`CFG-04`).
        for fork_action in [
            "input.first",
            "jump.land",
            "edit.deleteCharForward",
            "edit.deleteToLineEnd",
            "find.bookmarks",
            "splits.grid",
            "ui.appMenu",
            "captcha.fill",
            "yank.put",
            "yank.clear",
            "yank.urlMarkdown",
            "webchat.newChat",
            "webchat.reload",
            "webchat.accounts",
            "webchat.sync",
            "ublock.popup",
            "ublock.zapper",
            "ublock.picker",
            "ublock.logger",
            "ublock.dashboard",
            "ublock.relax",
            "ublock.toggleCosmetic",
            "ublock.toggleJavascript",
            "agent.observe",
            "agent.act",
            "agent.extract",
            "agent.fetch",
            "agent.version",
        ] {
            let inv = zendia_keymap::translate_fork_action(fork_action);
            let parsed = Command::from_name_and_args(&inv.command, &inv.args_as_pairs())
                .unwrap_or_else(|error| panic!("fork action `{fork_action}` -> `{}` failed: {error}", inv.command));
            assert!(
                !matches!(parsed, Command::Custom { .. }),
                "fork action `{fork_action}` -> `{}` resolved to Custom",
                inv.command
            );
        }
    }

    #[test]
    fn which_key_groups_and_wk01_descriptions_match_fork() {
        let profile = crate::input_state::vim_profile();
        let group_names: std::collections::BTreeSet<&str> = profile.groups.iter().map(|g| g.name.as_str()).collect();
        for expected_group in [
            "spaces",
            "ai",
            "buffer",
            "captcha",
            "file/find",
            "library",
            "quit/session",
            "search",
            "ui",
            "windows",
            "prev",
            "next",
            "goto",
            "fold",
            "around",
            "inside",
            "ask web chat",
        ] {
            assert!(
                group_names.contains(expected_group),
                "missing which-key group `{expected_group}`"
            );
        }

        let check_desc = |mode: &str, keys: &str, expected: &str| {
            let seq = zendia_keymap::KeySequence::parse(keys)
                .unwrap()
                .expand_leader_strokes(profile.leader_sequence.strokes());
            let binding = profile
                .bindings
                .iter()
                .rfind(|b| b.mode.as_deref() == Some(mode) && b.keys == seq)
                .unwrap_or_else(|| panic!("missing `{keys}` in mode `{mode}`"));
            assert_eq!(
                binding.description.as_deref(),
                Some(expected),
                "wrong description for `{mode}` `{keys}`"
            );
        };

        check_desc("i", "<A-p>", "Put Previous Yank");
        check_desc("i", "<A-n>", "Put Next Yank");
        check_desc("n", "<leader>bP", "Delete Non-Pinned Tabs");
        check_desc("n", "<leader>fn", "New Tab");
        check_desc("n", "<leader>ff", "Open URL Bar");
        check_desc("n", "yy", "Copy URL");
        check_desc("n", "<leader><Tab>i", "Change Space Icon");
        check_desc("n", "<leader>ww", "Go to Next Window");
        check_desc("n", "<leader>wp", "Go to Last Used Window");
        check_desc("n", "<leader>wm", "Fill View with Window");
        check_desc("n", "<leader>w=", "Make Windows Equal");
        check_desc("n", "<leader>wR", "Rotate Windows Back");
        check_desc("n", "<leader>wH", "Move Window to Far Left");
        check_desc("n", "<leader>wJ", "Move Window to Bottom");
        check_desc("n", "<leader>wK", "Move Window to Top");
        check_desc("n", "<leader>wL", "Move Window to Far Right");
        check_desc("n", "<leader>wg", "Move Window…");
        check_desc("n", "<leader>wa", "Add Tab to Split…");
        check_desc("v", "<leader>ae", "Explain the Selection");
        check_desc("v", "<leader>as", "Summarize the Selection");
        check_desc("v", "<leader>at", "Translate the Selection");
        check_desc("v", "<leader>ad", "Define the Selection");
        check_desc("v", "<leader>af", "Fact-Check the Selection");
        check_desc("v", "<leader>aa", "Ask About the Selection…");
        check_desc("e", "j", "Down");
        check_desc("e", "k", "Up");
        check_desc("e", "l", "Open, or Right on the Foot");
        check_desc("e", "o", "Open, Keep Explorer");
        check_desc("e", "h", "Close Folder");
        check_desc("e", "zR", "Open All Folders");
        check_desc("e", "zM", "Close All Folders");
        check_desc("e", "s", "Jump");
        check_desc("e", "/", "Find Tab");
        check_desc("e", "d", "Delete");
        check_desc("e", "<lt>", "Move out of Folder");
        check_desc("e", "<C-v>", "Open in Vertical Split");
        check_desc("e", "<C-s>", "Open in Horizontal Split");
        check_desc("e", "R", "Reload");
        check_desc("e", "U", "Unload");
        check_desc("e", "m", "Menu");
        check_desc("e", "+", "Create Menu");
        check_desc("e", "<Tab>", "Select");
        check_desc("e", "q", "Close Explorer");
        check_desc("e", "<Esc>", "Clear Selection or Close");
        check_desc("t", "h", "Left");
        check_desc("t", "l", "Right");
        check_desc("t", "0", "First Item");
        check_desc("t", "$", "Last Item");
        check_desc("t", "<CR>", "Press");
        check_desc("t", "m", "Menu");
        check_desc("t", "s", "Jump");
        check_desc("t", "i", "Edit Address");
        check_desc("t", "q", "Close Toolbar");
    }

    #[test]
    fn names_round_trip() {
        for descriptor in descriptors() {
            if matches!(
                descriptor.name.as_str(),
                "page.editable_focus_changed" | "page.register_auxiliary"
            ) {
                continue;
            }
            let parsed = Command::from_name_and_args(&descriptor.name, &[]).unwrap();
            assert_eq!(parsed.name(), descriptor.name);
        }
    }
}
