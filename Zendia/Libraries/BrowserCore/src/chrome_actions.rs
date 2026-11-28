/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Chrome-mode vim actions: sidebar explorer (`e`), toolbar (`t`), and library (`l`).

use crate::commands::{Command, CommandDescriptor, CommandError};
use crate::library::LibrarySection;
use crate::model::{FolderId, TabId};

/// Half-page jump distance in explorer and library lists.
pub const HALF_PAGE_STEP: isize = 8;

/// Library-mode actions forwarded to the active window's library view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryVimAction {
    Down,
    Up,
    Left,
    Right,
    First,
    Last,
    HalfPageDown,
    HalfPageUp,
    Activate,
    Quit,
    Menu,
    Delete,
    Yank,
    Jump,
    Search,
    Filter,
    Add,
    RenameSpace,
    SpaceTheme,
    SpaceIcon,
    MoveSpaceLeft,
    MoveSpaceRight,
    NextSection,
    PrevSection,
    GotoSection(LibrarySection),
}

impl LibraryVimAction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Down => "library.down",
            Self::Up => "library.up",
            Self::Left => "library.left",
            Self::Right => "library.right",
            Self::First => "library.first",
            Self::Last => "library.last",
            Self::HalfPageDown => "library.half_page_down",
            Self::HalfPageUp => "library.half_page_up",
            Self::Activate => "library.open",
            Self::Quit => "library.quit",
            Self::Menu => "library.menu",
            Self::Delete => "library.delete",
            Self::Yank => "library.yank",
            Self::Jump => "library.jump",
            Self::Search => "library.search",
            Self::Filter => "library.filter",
            Self::Add => "library.add",
            Self::RenameSpace => "library.rename_space",
            Self::SpaceTheme => "library.space_theme",
            Self::SpaceIcon => "library.space_icon",
            Self::MoveSpaceLeft => "library.move_space_left",
            Self::MoveSpaceRight => "library.move_space_right",
            Self::NextSection => "library.next_section",
            Self::PrevSection => "library.prev_section",
            Self::GotoSection(LibrarySection::History) => "library.goto_history",
            Self::GotoSection(LibrarySection::Downloads) => "library.goto_downloads",
            Self::GotoSection(LibrarySection::Spaces) => "library.goto_spaces",
            Self::GotoSection(LibrarySection::Media) => "library.goto_media",
            Self::GotoSection(LibrarySection::SpaceRouting) => "library.goto_routing",
        }
    }
}

/// Explorer, toolbar, and library vim actions.
#[derive(Clone, Debug, PartialEq)]
pub enum ChromeAction {
    ExplorerHalfPageDown {
        count: Option<u32>,
    },
    ExplorerHalfPageUp {
        count: Option<u32>,
    },
    ExplorerPreview,
    ExplorerFoldToggle,
    ExplorerFoldOpen,
    ExplorerFoldClose,
    ExplorerFind,
    ExplorerEscape,
    ExplorerAdd,
    ExplorerAddFolder,
    ExplorerDelete,
    ExplorerRename,
    ExplorerDuplicate,
    ExplorerTogglePin,
    ExplorerToggleEssential,
    ExplorerReload,
    ExplorerUnload,
    ExplorerToggleMute,
    ExplorerMoveToFolderPick,
    ExplorerMoveToFolderApply {
        folder_id: Option<FolderId>,
        create_new: bool,
        tab_ids: Vec<TabId>,
        rename_new: bool,
    },
    ExplorerYank {
        markdown: bool,
    },
    ExplorerMoveOutOfFolder,
    ExplorerMoveDown,
    ExplorerMoveUp,
    ExplorerSplitVertical,
    ExplorerSplitHorizontal,
    ExplorerMenu,
    ExplorerCreate,
    ToolbarJump,
    ToolbarMenu,
    ToolbarUrlbar,
    LibraryQuit,
    LibraryGoto(LibrarySection),
    Library {
        action: LibraryVimAction,
        count: Option<u32>,
    },
}

impl ChromeAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ExplorerHalfPageDown { .. } => "explorer.half_page_down",
            Self::ExplorerHalfPageUp { .. } => "explorer.half_page_up",
            Self::ExplorerPreview => "explorer.preview",
            Self::ExplorerFoldToggle => "explorer.fold_toggle",
            Self::ExplorerFoldOpen => "explorer.fold_open",
            Self::ExplorerFoldClose => "explorer.fold_close",
            Self::ExplorerFind => "explorer.find",
            Self::ExplorerEscape => "explorer.escape",
            Self::ExplorerAdd => "explorer.add",
            Self::ExplorerAddFolder => "explorer.add_folder",
            Self::ExplorerDelete => "explorer.delete",
            Self::ExplorerRename => "explorer.rename",
            Self::ExplorerDuplicate => "explorer.duplicate",
            Self::ExplorerTogglePin => "explorer.toggle_pin",
            Self::ExplorerToggleEssential => "explorer.toggle_essential",
            Self::ExplorerReload => "explorer.reload",
            Self::ExplorerUnload => "explorer.unload",
            Self::ExplorerToggleMute => "explorer.toggle_mute",
            Self::ExplorerMoveToFolderPick => "explorer.move_to_folder",
            Self::ExplorerMoveToFolderApply { .. } => "explorer.move_to_folder_apply",
            Self::ExplorerYank { markdown: false } => "explorer.yank",
            Self::ExplorerYank { markdown: true } => "explorer.yank_markdown",
            Self::ExplorerMoveOutOfFolder => "explorer.move_out_of_folder",
            Self::ExplorerMoveDown => "explorer.move_down",
            Self::ExplorerMoveUp => "explorer.move_up",
            Self::ExplorerSplitVertical => "explorer.split_vertical",
            Self::ExplorerSplitHorizontal => "explorer.split_horizontal",
            Self::ExplorerMenu => "explorer.menu",
            Self::ExplorerCreate => "explorer.create",
            Self::ToolbarJump => "toolbar.jump",
            Self::ToolbarMenu => "toolbar.menu",
            Self::ToolbarUrlbar => "toolbar.urlbar",
            Self::LibraryQuit => "library.quit",
            Self::LibraryGoto(section) => match section {
                LibrarySection::History => "library.goto_history",
                LibrarySection::Downloads => "library.goto_downloads",
                LibrarySection::Spaces => "library.goto_spaces",
                LibrarySection::Media => "library.goto_media",
                LibrarySection::SpaceRouting => "library.goto_routing",
            },
            Self::Library { action, .. } => action.name(),
        }
    }
}

fn find_arg<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn parse_count_arg(command: &str, args: &[(String, String)]) -> Result<Option<u32>, CommandError> {
    match find_arg(args, "count") {
        None => Ok(None),
        Some(raw) => raw
            .parse::<u64>()
            .map(|v| Some(v.min(u64::from(u32::MAX)) as u32))
            .map_err(|_| CommandError::InvalidArgument {
                command: command.to_string(),
                message: format!("expected integer `count`, got `{raw}`"),
            }),
    }
}

/// Parses the commands of this module, or returns `None` for a name it does not own.
pub fn parse_command(name: &str, args: &[(String, String)]) -> Result<Option<Command>, CommandError> {
    let action = match name {
        "explorer.half_page_down" => ChromeAction::ExplorerHalfPageDown {
            count: parse_count_arg(name, args)?,
        },
        "explorer.half_page_up" => ChromeAction::ExplorerHalfPageUp {
            count: parse_count_arg(name, args)?,
        },
        "explorer.preview" => ChromeAction::ExplorerPreview,
        "explorer.fold_toggle" => ChromeAction::ExplorerFoldToggle,
        "explorer.fold_open" => ChromeAction::ExplorerFoldOpen,
        "explorer.fold_close" => ChromeAction::ExplorerFoldClose,
        "explorer.find" => ChromeAction::ExplorerFind,
        "explorer.escape" => ChromeAction::ExplorerEscape,
        "explorer.add" => ChromeAction::ExplorerAdd,
        "explorer.add_folder" => ChromeAction::ExplorerAddFolder,
        "explorer.delete" => ChromeAction::ExplorerDelete,
        "explorer.rename" => ChromeAction::ExplorerRename,
        "explorer.duplicate" => ChromeAction::ExplorerDuplicate,
        "explorer.toggle_pin" => ChromeAction::ExplorerTogglePin,
        "explorer.toggle_essential" => ChromeAction::ExplorerToggleEssential,
        "explorer.reload" => ChromeAction::ExplorerReload,
        "explorer.unload" => ChromeAction::ExplorerUnload,
        "explorer.toggle_mute" => ChromeAction::ExplorerToggleMute,
        "explorer.move_to_folder" => ChromeAction::ExplorerMoveToFolderPick,
        "explorer.move_to_folder_apply" => {
            let folder_id = match find_arg(args, "folder_id") {
                None | Some("") => None,
                Some(raw) => Some(FolderId(raw.parse::<u64>().map_err(|_| {
                    CommandError::InvalidArgument {
                        command: name.to_string(),
                        message: format!("invalid `folder_id`: `{raw}`"),
                    }
                })?)),
            };
            let create_new = find_arg(args, "create_new") == Some("true");
            let rename_new = find_arg(args, "rename_new") == Some("true");
            let tab_ids = match find_arg(args, "tab_ids") {
                None | Some("") => Vec::new(),
                Some(raw) => raw
                    .split(',')
                    .filter(|s| !s.trim().is_empty())
                    .map(|part| {
                        part.trim()
                            .parse::<u64>()
                            .map(TabId)
                            .map_err(|_| CommandError::InvalidArgument {
                                command: name.to_string(),
                                message: format!("invalid tab id `{part}`"),
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            };
            ChromeAction::ExplorerMoveToFolderApply {
                folder_id,
                create_new,
                tab_ids,
                rename_new,
            }
        }
        "explorer.yank" => ChromeAction::ExplorerYank { markdown: false },
        "explorer.yank_markdown" => ChromeAction::ExplorerYank { markdown: true },
        "explorer.move_out_of_folder" => ChromeAction::ExplorerMoveOutOfFolder,
        "explorer.move_down" => ChromeAction::ExplorerMoveDown,
        "explorer.move_up" => ChromeAction::ExplorerMoveUp,
        "explorer.split_vertical" => ChromeAction::ExplorerSplitVertical,
        "explorer.split_horizontal" => ChromeAction::ExplorerSplitHorizontal,
        "explorer.menu" => ChromeAction::ExplorerMenu,
        "explorer.create" => ChromeAction::ExplorerCreate,
        "toolbar.jump" => ChromeAction::ToolbarJump,
        "toolbar.menu" => ChromeAction::ToolbarMenu,
        "toolbar.urlbar" => ChromeAction::ToolbarUrlbar,
        "library.quit" => ChromeAction::LibraryQuit,
        "library.goto_history" => ChromeAction::LibraryGoto(LibrarySection::History),
        "library.goto_downloads" => ChromeAction::LibraryGoto(LibrarySection::Downloads),
        "library.goto_spaces" => ChromeAction::LibraryGoto(LibrarySection::Spaces),
        "library.goto_media" => ChromeAction::LibraryGoto(LibrarySection::Media),
        "library.goto_routing" => ChromeAction::LibraryGoto(LibrarySection::SpaceRouting),
        "library.down" => ChromeAction::Library {
            action: LibraryVimAction::Down,
            count: parse_count_arg(name, args)?,
        },
        "library.up" => ChromeAction::Library {
            action: LibraryVimAction::Up,
            count: parse_count_arg(name, args)?,
        },
        "library.left" => ChromeAction::Library {
            action: LibraryVimAction::Left,
            count: parse_count_arg(name, args)?,
        },
        "library.right" => ChromeAction::Library {
            action: LibraryVimAction::Right,
            count: parse_count_arg(name, args)?,
        },
        "library.first" => ChromeAction::Library {
            action: LibraryVimAction::First,
            count: parse_count_arg(name, args)?,
        },
        "library.last" => ChromeAction::Library {
            action: LibraryVimAction::Last,
            count: parse_count_arg(name, args)?,
        },
        "library.half_page_down" => ChromeAction::Library {
            action: LibraryVimAction::HalfPageDown,
            count: parse_count_arg(name, args)?,
        },
        "library.half_page_up" => ChromeAction::Library {
            action: LibraryVimAction::HalfPageUp,
            count: parse_count_arg(name, args)?,
        },
        "library.menu" => ChromeAction::Library {
            action: LibraryVimAction::Menu,
            count: None,
        },
        "library.delete" => ChromeAction::Library {
            action: LibraryVimAction::Delete,
            count: None,
        },
        "library.yank" => ChromeAction::Library {
            action: LibraryVimAction::Yank,
            count: None,
        },
        "library.jump" => ChromeAction::Library {
            action: LibraryVimAction::Jump,
            count: None,
        },
        "library.search" => ChromeAction::Library {
            action: LibraryVimAction::Search,
            count: None,
        },
        "library.filter" => ChromeAction::Library {
            action: LibraryVimAction::Filter,
            count: None,
        },
        "library.add" => ChromeAction::Library {
            action: LibraryVimAction::Add,
            count: None,
        },
        "library.rename_space" => ChromeAction::Library {
            action: LibraryVimAction::RenameSpace,
            count: None,
        },
        "library.space_theme" => ChromeAction::Library {
            action: LibraryVimAction::SpaceTheme,
            count: None,
        },
        "library.space_icon" => ChromeAction::Library {
            action: LibraryVimAction::SpaceIcon,
            count: None,
        },
        "library.move_space_left" => ChromeAction::Library {
            action: LibraryVimAction::MoveSpaceLeft,
            count: parse_count_arg(name, args)?,
        },
        "library.move_space_right" => ChromeAction::Library {
            action: LibraryVimAction::MoveSpaceRight,
            count: parse_count_arg(name, args)?,
        },
        "library.next_section" => ChromeAction::Library {
            action: LibraryVimAction::NextSection,
            count: parse_count_arg(name, args)?,
        },
        "library.prev_section" => ChromeAction::Library {
            action: LibraryVimAction::PrevSection,
            count: parse_count_arg(name, args)?,
        },
        _ => return Ok(None),
    };
    Ok(Some(Command::Chrome(action)))
}

/// The descriptors of this module's commands.
pub fn descriptors() -> Vec<CommandDescriptor> {
    let items: &[(&str, &str, &str)] = &[
        (
            "explorer.half_page_down",
            "Explorer: Half Page Down",
            "Move explorer cursor half a page down",
        ),
        (
            "explorer.half_page_up",
            "Explorer: Half Page Up",
            "Move explorer cursor half a page up",
        ),
        (
            "explorer.preview",
            "Explorer: Preview",
            "Activate the tab under the explorer cursor without closing explorer",
        ),
        (
            "explorer.fold_toggle",
            "Explorer: Toggle Folder",
            "Toggle folding on the folder under the explorer cursor",
        ),
        (
            "explorer.fold_open",
            "Explorer: Open Folder",
            "Expand the folder under the explorer cursor",
        ),
        (
            "explorer.fold_close",
            "Explorer: Close Folder",
            "Collapse the folder under the explorer cursor",
        ),
        (
            "explorer.find",
            "Explorer: Filter Rows",
            "Jump to a sidebar row by filtering",
        ),
        (
            "explorer.escape",
            "Explorer: Clear Selection or Close",
            "Clear explorer selection if active, otherwise leave explorer",
        ),
        (
            "explorer.add",
            "Explorer: New Tab",
            "Close explorer and open a new tab via the URL bar",
        ),
        (
            "explorer.add_folder",
            "Explorer: New Folder",
            "Create a new folder in the active space and start renaming it",
        ),
        (
            "explorer.delete",
            "Explorer: Close or Delete",
            "Close selected tabs or delete the folder under the explorer cursor",
        ),
        (
            "explorer.rename",
            "Explorer: Rename",
            "Rename the tab or folder under the explorer cursor",
        ),
        (
            "explorer.duplicate",
            "Explorer: Duplicate",
            "Duplicate the selected or focused explorer tabs",
        ),
        (
            "explorer.toggle_pin",
            "Explorer: Toggle Pin",
            "Pin or unpin the selected or focused explorer tabs",
        ),
        (
            "explorer.toggle_essential",
            "Explorer: Toggle Essential",
            "Add or remove the selected or focused explorer tabs from Essentials",
        ),
        (
            "explorer.reload",
            "Explorer: Reload Tab",
            "Reload the selected or focused explorer tabs",
        ),
        (
            "explorer.unload",
            "Explorer: Unload Tab",
            "Unload the selected or focused explorer tabs",
        ),
        (
            "explorer.toggle_mute",
            "Explorer: Toggle Mute",
            "Mute or unmute the selected or focused explorer tabs",
        ),
        (
            "explorer.move_to_folder",
            "Explorer: Move to Folder",
            "Pick a folder for the selected or focused explorer tabs",
        ),
        (
            "toolbar.urlbar",
            "Toolbar: Focus Address Bar",
            "Leave toolbar focus and focus the address bar",
        ),
        (
            "explorer.yank",
            "Explorer: Yank URL",
            "Copy the URL of the selected or focused explorer tab",
        ),
        (
            "explorer.yank_markdown",
            "Explorer: Yank Markdown Link",
            "Copy a Markdown link for the selected or focused explorer tab",
        ),
        (
            "explorer.move_out_of_folder",
            "Explorer: Move Out of Folder",
            "Remove the focused explorer tab from its folder",
        ),
        (
            "explorer.move_down",
            "Explorer: Move Down",
            "Move the focused explorer tab down one position",
        ),
        (
            "explorer.move_up",
            "Explorer: Move Up",
            "Move the focused explorer tab up one position",
        ),
        (
            "explorer.split_vertical",
            "Explorer: Split Right",
            "Split the active tab side by side with the focused explorer tab",
        ),
        (
            "explorer.split_horizontal",
            "Explorer: Split Below",
            "Split the active tab stacked with the focused explorer tab",
        ),
        (
            "explorer.menu",
            "Explorer: Context Menu",
            "Open the context menu for the focused explorer row",
        ),
        (
            "explorer.create",
            "Explorer: New",
            "Create a new tab or folder from explorer mode",
        ),
        (
            "toolbar.jump",
            "Toolbar: Jump Hints",
            "Label toolbar buttons for direct keyboard activation",
        ),
        (
            "toolbar.menu",
            "Toolbar: Button Menu",
            "Open the menu for the focused toolbar button",
        ),
        ("library.quit", "Library: Close", "Close the library panel"),
        (
            "library.goto_history",
            "Library: History",
            "Switch the library to History",
        ),
        (
            "library.goto_downloads",
            "Library: Downloads",
            "Switch the library to Downloads",
        ),
        ("library.goto_spaces", "Library: Spaces", "Switch the library to Spaces"),
        ("library.goto_media", "Library: Media", "Switch the library to Media"),
        (
            "library.goto_routing",
            "Library: Space Routing",
            "Switch the library to Space Routing",
        ),
        ("library.down", "Library: Down", "Move library cursor down"),
        ("library.up", "Library: Up", "Move library cursor up"),
        ("library.left", "Library: Left", "Move library cursor left"),
        ("library.right", "Library: Right", "Move library cursor right"),
        (
            "library.first",
            "Library: First Item",
            "Jump to the first item in the library",
        ),
        (
            "library.last",
            "Library: Last Item",
            "Jump to the last item in the library",
        ),
        (
            "library.half_page_down",
            "Library: Half Page Down",
            "Move library cursor half a page down",
        ),
        (
            "library.half_page_up",
            "Library: Half Page Up",
            "Move library cursor half a page up",
        ),
        (
            "library.menu",
            "Library: Menu",
            "Open context menu for the focused library item",
        ),
        ("library.delete", "Library: Delete", "Delete the focused library item"),
        (
            "library.yank",
            "Library: Yank",
            "Copy the URL or path of the focused library item",
        ),
        ("library.jump", "Library: Jump", "Show jump hints in the library"),
        ("library.search", "Library: Search", "Focus the library search input"),
        ("library.filter", "Library: Filter", "Open library filters"),
        (
            "library.add",
            "Library: Add Space or Route",
            "Create a new space or space route in the library",
        ),
        (
            "library.rename_space",
            "Library: Rename Space",
            "Rename the focused space in the library",
        ),
        (
            "library.space_theme",
            "Library: Space Theme",
            "Edit the theme of the focused space",
        ),
        (
            "library.space_icon",
            "Library: Space Icon",
            "Edit the icon of the focused space",
        ),
        (
            "library.move_space_left",
            "Library: Move Space Left",
            "Move the focused space left",
        ),
        (
            "library.move_space_right",
            "Library: Move Space Right",
            "Move the focused space right",
        ),
        (
            "library.next_section",
            "Library: Next Section",
            "Switch to the next library section",
        ),
        (
            "library.prev_section",
            "Library: Previous Section",
            "Switch to the previous library section",
        ),
    ];
    items
        .iter()
        .map(|&(name, title, description)| CommandDescriptor {
            name: name.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            arguments: Vec::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_chrome_action_descriptors_round_trip() {
        for desc in descriptors() {
            let parsed = Command::from_name_and_args(&desc.name, &[]).unwrap();
            assert_eq!(parsed.name(), desc.name);
        }
    }
}
