/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Tab, space, URL, find, layout, theme, notification, extension, session, and window actions of the vim profile.

use crate::commands::{Command, CommandDescriptor, CommandError};

/// Tab, space, page, and window-level actions invoked by vim keymaps (and available to the palette/agents).
#[derive(Clone, Debug, PartialEq)]
pub enum TabAction {
    /// Switch to the most recently used other tab in the active space (`<leader>bb`, `<leader>\``).
    TabAlternate,
    /// Close all other unpinned tabs in the active space (`<leader>bo`).
    TabCloseOthers,
    /// Close all unpinned tabs after the active tab in the active space (`<leader>br`).
    TabCloseRight,
    /// Close all unpinned tabs before the active tab in the active space (`<leader>bl`).
    TabCloseLeft,
    /// Close all unpinned normal tabs in the active space (`<leader>bP`).
    TabCloseUnpinned,
    /// Move the active tab one step forward within its section (`]B`).
    TabMoveNext,
    /// Move the active tab one step backward within its section (`[B`).
    TabMovePrev,
    /// Reopen the most recently closed tab (`u`).
    TabReopenClosed,
    /// Open the tab picker (`<leader>bj`).
    TabPick,
    /// Open the active tab's context menu (`<leader>bm`).
    TabMenu,
    /// Switch to the first space (`<leader><Tab>f`).
    SpaceFirst,
    /// Switch to the last space (`<leader><Tab>l`).
    SpaceLast,
    /// Close/delete the active space (`<leader><Tab>d` without `space_id`).
    SpaceCloseActive,
    /// Start renaming the active space (`<leader><Tab>r` without `space_id`).
    SpaceRenameActive,
    /// Open the icon picker for the active space (`<leader><Tab>i` without `space_id`).
    SpaceIconActive,
    /// Open the theme picker for the active space (`<leader><Tab>t` without `space_id`).
    SpaceThemeActive,
    /// Unload tabs in the active space (`<leader><Tab>u` without `space_id`).
    SpaceUnloadActive,
    /// Navigate up one URL path segment (`-`).
    UrlUp,
    /// Open clipboard URL or search in the current tab (`p`) or a new tab (`P`).
    UrlPaste { new_tab: bool },
    /// Open the address bar filtered to open tabs (`<leader>,`, `<leader>fb`).
    FindTabs,
    /// Open the address bar filtered to recent history (`<leader>fr`).
    FindRecent,
    /// Open the address bar in web search mode (`<leader>/`).
    SearchWeb,
    /// Open find in page (`/`).
    PageFind,
    /// The find bar's text changed: the page highlights its matches and goes to the first.
    PageFindQuery { query: String },
    /// The find bar closed: the page's highlights go.
    PageFindClose,
    /// Jump to the next find-in-page match (`n`).
    SearchNext,
    /// Jump to the previous find-in-page match (`N`).
    SearchPrev,
    /// Reset layout preferences to their defaults (`<leader>uR`).
    LayoutReset,
    /// Toggle between dark and light theme (`<leader>ub`).
    ThemeToggleDark,
    /// Dismiss all sidebar notifications and toasts (`<leader>un`).
    NotificationDismissAll,
    /// Open the installed extensions picker (`<leader>x`).
    ExtensionsPick,
    /// Restore closed tabs from the last session (`<leader>qs`).
    SessionRestore,
    /// Minimize the current browser window (`<leader>qm`).
    WindowMinimize,
    /// Toggle maximize/restore on the current browser window (`<leader>qM`).
    WindowToggleMaximize,
    /// Close the current browser window (`<leader>qw`).
    WindowClose,
}

impl TabAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::TabAlternate => "tab.alternate",
            Self::TabCloseOthers => "tab.close_others",
            Self::TabCloseRight => "tab.close_right",
            Self::TabCloseLeft => "tab.close_left",
            Self::TabCloseUnpinned => "tab.close_unpinned",
            Self::TabMoveNext => "tab.move_next",
            Self::TabMovePrev => "tab.move_prev",
            Self::TabReopenClosed => "tab.reopen_closed",
            Self::TabPick => "tab.pick",
            Self::TabMenu => "tab.menu",
            Self::SpaceFirst => "space.first",
            Self::SpaceLast => "space.last",
            Self::SpaceCloseActive => "space.close",
            Self::SpaceRenameActive => "space.rename",
            Self::SpaceIconActive => "space.icon",
            Self::SpaceThemeActive => "space.theme",
            Self::SpaceUnloadActive => "space.unload",
            Self::UrlUp => "url.up",
            Self::UrlPaste { new_tab: false } => "url.paste",
            Self::UrlPaste { new_tab: true } => "url.paste_new_tab",
            Self::FindTabs => "find.tabs",
            Self::FindRecent => "find.recent",
            Self::SearchWeb => "search.web",
            Self::PageFind => "page.find",
            Self::PageFindQuery { .. } => "page.find_query",
            Self::PageFindClose => "page.find_close",
            Self::SearchNext => "search.next",
            Self::SearchPrev => "search.prev",
            Self::LayoutReset => "layout.reset",
            Self::ThemeToggleDark => "theme.toggle_dark",
            Self::NotificationDismissAll => "notification.dismiss_all",
            Self::ExtensionsPick => "extensions.pick",
            Self::SessionRestore => "session.restore",
            Self::WindowMinimize => "window.minimize",
            Self::WindowToggleMaximize => "window.toggle_maximize",
            Self::WindowClose => "window.close",
        }
    }
}

/// Computes the parent URL for `url.up` (`-`), matching the old fork's `ZendiaNavigation.sys.mjs`:
/// only `http://` and `https://` URLs are supported, query and fragment are stripped, trailing slashes are
/// ignored, and the last path segment is removed. Returns `None` if already at the origin root.
pub fn parent_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let scheme_end = if trimmed.starts_with("https://") {
        "https://".len()
    } else if trimmed.starts_with("http://") {
        "http://".len()
    } else {
        return None;
    };
    let rest = &trimmed[scheme_end..];
    // Strip fragment and query.
    let without_frag = rest.split('#').next().unwrap_or(rest);
    let without_query = without_frag.split('?').next().unwrap_or(without_frag);
    let slash_pos = without_query.find('/')?;
    let origin = &trimmed[..scheme_end + slash_pos];
    let path = &without_query[slash_pos..];
    let stripped = path.trim_end_matches('/');
    if stripped.is_empty() {
        return None;
    }
    let last_slash = stripped.rfind('/')?;
    Some(format!("{origin}{}", &stripped[..=last_slash]))
}

/// Parses the commands of this module, or returns `None` for a name it does not own.
pub fn parse_command(name: &str, args: &[(String, String)]) -> Result<Option<Command>, CommandError> {
    let action = match name {
        "tab.alternate" => TabAction::TabAlternate,
        "tab.close_others" => TabAction::TabCloseOthers,
        "tab.close_right" => TabAction::TabCloseRight,
        "tab.close_left" => TabAction::TabCloseLeft,
        "tab.close_unpinned" => TabAction::TabCloseUnpinned,
        "tab.move_next" => TabAction::TabMoveNext,
        "tab.move_prev" => TabAction::TabMovePrev,
        "tab.reopen_closed" => TabAction::TabReopenClosed,
        "tab.pick" => TabAction::TabPick,
        "tab.menu" => TabAction::TabMenu,
        "space.first" => TabAction::SpaceFirst,
        "space.last" => TabAction::SpaceLast,
        "url.up" => TabAction::UrlUp,
        "url.paste" => TabAction::UrlPaste { new_tab: false },
        "url.paste_new_tab" => TabAction::UrlPaste { new_tab: true },
        "find.tabs" => TabAction::FindTabs,
        "find.recent" => TabAction::FindRecent,
        "search.web" => TabAction::SearchWeb,
        "page.find" => TabAction::PageFind,
        "page.find_query" => TabAction::PageFindQuery {
            query: args
                .iter()
                .find(|(key, _)| key == "query")
                .map(|(_, value)| value.clone())
                .unwrap_or_default(),
        },
        "page.find_close" => TabAction::PageFindClose,
        "search.next" => TabAction::SearchNext,
        "search.prev" => TabAction::SearchPrev,
        "layout.reset" => TabAction::LayoutReset,
        "theme.toggle_dark" => TabAction::ThemeToggleDark,
        "notification.dismiss_all" => TabAction::NotificationDismissAll,
        "extensions.pick" => TabAction::ExtensionsPick,
        "session.restore" => TabAction::SessionRestore,
        "window.minimize" => TabAction::WindowMinimize,
        "window.toggle_maximize" => TabAction::WindowToggleMaximize,
        "window.close" => TabAction::WindowClose,
        _ => return Ok(None),
    };
    Ok(Some(Command::Tabs(action)))
}

/// The descriptors of this module's commands.
pub fn descriptors() -> Vec<CommandDescriptor> {
    let items: &[(&str, &str, &str)] = &[
        (
            "tab.alternate",
            "Switch to Other Tab",
            "Switch to the most recently used other tab in the active space",
        ),
        (
            "tab.close_others",
            "Delete Other Tabs",
            "Close all other unpinned tabs in the active space",
        ),
        (
            "tab.close_right",
            "Delete Tabs to the Right",
            "Close unpinned tabs after the active tab",
        ),
        (
            "tab.close_left",
            "Delete Tabs to the Left",
            "Close unpinned tabs before the active tab",
        ),
        (
            "tab.close_unpinned",
            "Delete Unpinned Tabs",
            "Close all unpinned normal tabs in the active space",
        ),
        (
            "tab.move_next",
            "Move Tab Next",
            "Move the active tab one position forward",
        ),
        (
            "tab.move_prev",
            "Move Tab Prev",
            "Move the active tab one position backward",
        ),
        (
            "tab.reopen_closed",
            "Reopen Closed Tab",
            "Reopen the most recently closed tab",
        ),
        ("tab.pick", "Pick Tab", "Open the keyboard tab picker"),
        ("tab.menu", "Tab Menu", "Open the context menu for the active tab"),
        ("space.first", "First Space", "Switch to the first workspace"),
        ("space.last", "Last Space", "Switch to the last workspace"),
        (
            "url.up",
            "Open Parent Path",
            "Navigate up one path segment in the current URL",
        ),
        (
            "url.paste",
            "Open Clipboard URL",
            "Open the clipboard URL or search in the current tab",
        ),
        (
            "url.paste_new_tab",
            "Open Clipboard URL in New Tab",
            "Open the clipboard URL or search in a new tab",
        ),
        ("find.tabs", "Tabs", "Open the address bar filtered to open tabs"),
        (
            "find.recent",
            "Recent",
            "Open the address bar filtered to recent history",
        ),
        (
            "search.web",
            "Search the Web",
            "Open the address bar in web search mode",
        ),
        ("page.find", "Search in Page", "Open the find-in-page bar"),
        (
            "page.find_query",
            "Find Text in Page",
            "Highlight a text's matches in the page",
        ),
        (
            "page.find_close",
            "Close Find in Page",
            "Close the find bar and clear its highlights",
        ),
        (
            "search.next",
            "Next Search Result",
            "Jump to the next find-in-page match",
        ),
        (
            "search.prev",
            "Prev Search Result",
            "Jump to the previous find-in-page match",
        ),
        (
            "layout.reset",
            "Reset Layout to Defaults",
            "Reset sidebar, toolbar, and compact mode settings to defaults",
        ),
        (
            "theme.toggle_dark",
            "Toggle Dark Background",
            "Toggle between dark and light theme",
        ),
        (
            "notification.dismiss_all",
            "Dismiss All Notifications",
            "Dismiss all active notifications and toasts",
        ),
        ("extensions.pick", "Extensions", "Open the installed extensions picker"),
        (
            "session.restore",
            "Restore Last Session",
            "Restore tabs from the last closed session",
        ),
        (
            "window.minimize",
            "Minimize Window",
            "Minimize the current browser window",
        ),
        (
            "window.toggle_maximize",
            "Maximize/Restore Window",
            "Toggle maximize or restore on the current window",
        ),
        ("window.close", "Close Window", "Close the current browser window"),
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
    fn parent_url_strips_segment_query_and_hash() {
        assert_eq!(
            parent_url("https://example.com/a/b/c?x=1#top"),
            Some("https://example.com/a/b/".to_string())
        );
        assert_eq!(
            parent_url("https://example.com/a/b/"),
            Some("https://example.com/a/".to_string())
        );
        assert_eq!(
            parent_url("https://example.com/a"),
            Some("https://example.com/".to_string())
        );
        assert_eq!(parent_url("https://example.com/"), None);
        assert_eq!(parent_url("https://example.com"), None);
        assert_eq!(parent_url("about:blank"), None);
    }

    #[test]
    fn all_tab_action_descriptors_round_trip() {
        for desc in descriptors() {
            let parsed = Command::from_name_and_args(&desc.name, &[]).unwrap();
            assert_eq!(parsed.name(), desc.name);
        }
    }
}
