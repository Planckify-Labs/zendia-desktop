/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Chrome-mode vim actions: sidebar explorer (`e`), toolbar (`t`), and library (`l`).

use crate::commands::{Command, CommandDescriptor, CommandError};
use crate::library::LibrarySection;

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
        }
    }
}

/// Explorer, toolbar, and library vim actions.
#[derive(Clone, Debug, PartialEq)]
pub enum ChromeAction {
    ExplorerHalfPageDown,
    ExplorerHalfPageUp,
    ExplorerPreview,
    ExplorerFoldToggle,
    ExplorerFoldOpen,
    ExplorerFoldClose,
    ExplorerFind,
    ExplorerYank { markdown: bool },
    ExplorerMoveOutOfFolder,
    ExplorerMoveDown,
    ExplorerMoveUp,
    ExplorerSplitVertical,
    ExplorerSplitHorizontal,
    ExplorerMenu,
    ExplorerCreate,
    ToolbarJump,
    ToolbarMenu,
    LibraryQuit,
    LibraryGoto(LibrarySection),
    Library(LibraryVimAction),
}

impl ChromeAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ExplorerHalfPageDown => "explorer.half_page_down",
            Self::ExplorerHalfPageUp => "explorer.half_page_up",
            Self::ExplorerPreview => "explorer.preview",
            Self::ExplorerFoldToggle => "explorer.fold_toggle",
            Self::ExplorerFoldOpen => "explorer.fold_open",
            Self::ExplorerFoldClose => "explorer.fold_close",
            Self::ExplorerFind => "explorer.find",
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
            Self::LibraryQuit => "library.quit",
            Self::LibraryGoto(section) => match section {
                LibrarySection::History => "library.goto_history",
                LibrarySection::Downloads => "library.goto_downloads",
                LibrarySection::Spaces => "library.goto_spaces",
                LibrarySection::Media => "library.goto_media",
                LibrarySection::SpaceRouting => "library.goto_routing",
            },
            Self::Library(action) => action.name(),
        }
    }
}

/// Parses the commands of this module, or returns `None` for a name it does not own.
pub fn parse_command(name: &str, _args: &[(String, String)]) -> Result<Option<Command>, CommandError> {
    let action = match name {
        "explorer.half_page_down" => ChromeAction::ExplorerHalfPageDown,
        "explorer.half_page_up" => ChromeAction::ExplorerHalfPageUp,
        "explorer.preview" => ChromeAction::ExplorerPreview,
        "explorer.fold_toggle" => ChromeAction::ExplorerFoldToggle,
        "explorer.fold_open" => ChromeAction::ExplorerFoldOpen,
        "explorer.fold_close" => ChromeAction::ExplorerFoldClose,
        "explorer.find" => ChromeAction::ExplorerFind,
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
        "library.quit" => ChromeAction::LibraryQuit,
        "library.goto_history" => ChromeAction::LibraryGoto(LibrarySection::History),
        "library.goto_downloads" => ChromeAction::LibraryGoto(LibrarySection::Downloads),
        "library.goto_spaces" => ChromeAction::LibraryGoto(LibrarySection::Spaces),
        "library.goto_media" => ChromeAction::LibraryGoto(LibrarySection::Media),
        "library.goto_routing" => ChromeAction::LibraryGoto(LibrarySection::SpaceRouting),
        "library.down" => ChromeAction::Library(LibraryVimAction::Down),
        "library.up" => ChromeAction::Library(LibraryVimAction::Up),
        "library.left" => ChromeAction::Library(LibraryVimAction::Left),
        "library.right" => ChromeAction::Library(LibraryVimAction::Right),
        "library.first" => ChromeAction::Library(LibraryVimAction::First),
        "library.last" => ChromeAction::Library(LibraryVimAction::Last),
        "library.half_page_down" => ChromeAction::Library(LibraryVimAction::HalfPageDown),
        "library.half_page_up" => ChromeAction::Library(LibraryVimAction::HalfPageUp),
        "library.menu" => ChromeAction::Library(LibraryVimAction::Menu),
        "library.delete" => ChromeAction::Library(LibraryVimAction::Delete),
        "library.yank" => ChromeAction::Library(LibraryVimAction::Yank),
        "library.jump" => ChromeAction::Library(LibraryVimAction::Jump),
        "library.search" => ChromeAction::Library(LibraryVimAction::Search),
        "library.filter" => ChromeAction::Library(LibraryVimAction::Filter),
        "library.add" => ChromeAction::Library(LibraryVimAction::Add),
        "library.rename_space" => ChromeAction::Library(LibraryVimAction::RenameSpace),
        "library.space_theme" => ChromeAction::Library(LibraryVimAction::SpaceTheme),
        "library.space_icon" => ChromeAction::Library(LibraryVimAction::SpaceIcon),
        "library.move_space_left" => ChromeAction::Library(LibraryVimAction::MoveSpaceLeft),
        "library.move_space_right" => ChromeAction::Library(LibraryVimAction::MoveSpaceRight),
        "library.next_section" => ChromeAction::Library(LibraryVimAction::NextSection),
        "library.prev_section" => ChromeAction::Library(LibraryVimAction::PrevSection),
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
