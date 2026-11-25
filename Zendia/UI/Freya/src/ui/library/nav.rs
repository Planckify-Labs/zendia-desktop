/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The library's keyboard layer seam (Zendia's `ZendiaLibraryView`).
//!
//! A section calls [`use_library_nav`] at the top of its `render` with the rows a cursor can walk, in the order they
//! are drawn, and wraps each row in [`LibraryCursorRing`]. The keyboard layer moves the cursor over the registered
//! rows and, on Enter, invokes the row's command. Row keys are strings made from core ids (`history:42`,
//! `download:7`, `space:3`), never Freya ids, and they only need to be unique within one section.

use crate::ui::core_view::CoreView;
use crate::ui::theme::{ResolvedScheme, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::library::LibrarySection;
use zendia_browser_core::model::HistoryEntryId;

/// One row or tile the cursor can stop on.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryNavRow {
    pub key: String,
    /// What Enter does on this row, if anything.
    pub activate: Option<Command>,
}

impl LibraryNavRow {
    pub fn new(key: impl Into<String>, activate: Option<Command>) -> Self {
        Self {
            key: key.into(),
            activate,
        }
    }
}

/// How the registered rows are laid out, which decides what the arrow keys and h/j/k/l do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryNavLayout {
    List,
    /// Tiles in rows of `columns`, left to right.
    Grid {
        columns: usize,
    },
    /// Side-by-side columns (the Spaces section's cards), each its own list.
    Columns,
}

/// The rows the active section registered.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryNavRows {
    pub section: LibrarySection,
    pub layout: LibraryNavLayout,
    pub rows: Vec<LibraryNavRow>,
}

/// Directional or jump cursor motion inside the active library section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavMove {
    Down,
    Up,
    Left,
    Right,
    First,
    Last,
    HalfPageDown,
    HalfPageUp,
}

/// The keyboard layer's view state, part of [`super::LibraryViewState`].
#[derive(Clone, Copy, PartialEq)]
pub struct LibraryNavState {
    pub rows: State<Option<LibraryNavRows>>,
    /// Key of the row under the cursor.
    pub cursor: State<Option<String>>,
    /// Accessibility id of the active section's search input, if any.
    pub search_a11y_id: State<Option<AccessibilityId>>,
    /// Accessibility id of the library's list focus sink so Escape can pull focus out of the search box.
    pub list_focus_a11y_id: AccessibilityId,
    /// True while a section's filter panel is open (Escape closes it before closing the library).
    pub filter_panel_open: State<Option<State<bool>>>,
    /// Pending `g` prefix in LIBRARY mode (`gg`, `gh`, `gd`, `gs`, `gm`, `gr`).
    pub pending_g: State<bool>,
    /// True while the library holds keyboard focus (false after clicking on the page card).
    pub keyboard_focused: State<bool>,
}

impl LibraryNavState {
    /// Moves the cursor according to `motion` over the registered rows.
    pub fn move_cursor(&self, motion: NavMove) {
        let Some(registered) = self.rows.peek().clone() else {
            return;
        };
        let current = self.cursor.peek().clone();
        let next = step_cursor(&registered.rows, registered.layout, current.as_deref(), motion);
        let mut cursor = self.cursor;
        cursor.set_if_modified(next);
    }

    /// Returns the `Command` registered for the row currently under the cursor.
    pub fn active_row_command(&self) -> Option<Command> {
        let registered = self.rows.peek().clone()?;
        let cursor = self.cursor.peek().clone()?;
        registered
            .rows
            .into_iter()
            .find(|row| row.key == cursor)
            .and_then(|row| row.activate)
    }

    /// Invokes the `Command` of the row under the cursor, if any.
    pub fn activate_cursor(&self, core: &CoreView) -> bool {
        if let Some(cmd) = self.active_row_command() {
            core.invoke_or_log(cmd);
            return true;
        }
        false
    }

    /// True when the active section's search box currently owns Freya keyboard focus.
    pub fn is_search_focused(&self) -> bool {
        self.search_a11y_id.peek().is_some_and(|id| id.is_focused())
    }

    /// Moves keyboard focus to the active section's search input, closing any open filter panel first.
    pub fn focus_search(&self) -> bool {
        if let Some(mut open) = *self.filter_panel_open.peek() {
            open.set_if_modified(false);
        }
        if let Some(id) = *self.search_a11y_id.peek() {
            id.request_focus();
            return true;
        }
        false
    }

    /// Moves keyboard focus out of the search box onto the library list sink.
    pub fn focus_list(&self) {
        self.list_focus_a11y_id.request_focus();
    }

    /// Closes the active section's filter panel if it is open. Returns `true` when it consumed the Escape.
    pub fn close_filter_panel_if_open(&self) -> bool {
        if let Some(mut open) = *self.filter_panel_open.peek()
            && *open.peek()
        {
            open.set(false);
            return true;
        }
        false
    }

    /// Opens the active section's filter panel if the section exposes one.
    pub fn open_filter_panel(&self) -> bool {
        if let Some(mut open) = *self.filter_panel_open.peek() {
            open.set_if_modified(true);
            return true;
        }
        false
    }

    /// Clears transient prefix state (such as a pending `g`).
    pub fn reset_pending(&self) {
        let mut pending = self.pending_g;
        pending.set_if_modified(false);
    }
}

pub(super) fn use_library_nav_state() -> LibraryNavState {
    LibraryNavState {
        rows: use_state(|| None),
        cursor: use_state(|| None),
        search_a11y_id: use_state(|| None),
        list_focus_a11y_id: use_hook(AccessibilityId::new_unique),
        filter_panel_open: use_state(|| None),
        pending_g: use_state(|| false),
        keyboard_focused: use_state(|| false),
    }
}

/// Registers the active section's walkable rows. Call it once, unconditionally, at the top of the section's
/// `render`. Returns the key under the cursor.
pub fn use_library_nav(section: LibrarySection, layout: LibraryNavLayout, rows: Vec<LibraryNavRow>) -> Option<String> {
    let state = super::try_use_library_state();
    let next = LibraryNavRows { section, layout, rows };
    if let Some(state) = state {
        let prev_section = state.nav.rows.peek().as_ref().map(|r| r.section);
        let section_changed = prev_section.is_some_and(|prev| prev != section);
        let mut registered = state.nav.rows;
        if registered.peek().as_ref() != Some(&next) {
            registered.set(Some(next.clone()));
        }

        // Keep the cursor on a valid row of the active section; a new section or newly populated list starts at the top.
        let mut cursor = state.nav.cursor;
        let current = cursor.peek().clone();
        let still_valid = !section_changed
            && current
                .as_deref()
                .is_some_and(|key| next.rows.iter().any(|row| row.key == key));
        if !still_valid {
            let fallback = next.rows.first().map(|row| row.key.clone());
            cursor.set_if_modified(fallback);
        }

        if section_changed {
            let mut search_id = state.nav.search_a11y_id;
            search_id.set_if_modified(None);
            let mut filter_panel = state.nav.filter_panel_open;
            filter_panel.set_if_modified(None);
            state.nav.reset_pending();
        }

        return cursor.read().clone();
    }
    None
}

/// Pure cursor movement over a flat row list according to `layout`.
pub fn step_cursor(
    rows: &[LibraryNavRow],
    layout: LibraryNavLayout,
    current: Option<&str>,
    motion: NavMove,
) -> Option<String> {
    if rows.is_empty() {
        return None;
    }

    let current_idx = current.and_then(|key| rows.iter().position(|row| row.key == key));

    let next_idx = match layout {
        LibraryNavLayout::List => step_list(rows.len(), current_idx, motion),
        LibraryNavLayout::Grid { columns } => step_grid(rows.len(), columns.max(1), current_idx, motion),
        LibraryNavLayout::Columns => step_columns(rows, current_idx, motion),
    };

    rows.get(next_idx).map(|row| row.key.clone())
}

fn step_list(len: usize, current: Option<usize>, motion: NavMove) -> usize {
    let last = len - 1;
    let idx = match current {
        Some(i) => i,
        None => {
            return match motion {
                NavMove::Up | NavMove::Last => last,
                _ => 0,
            };
        }
    };
    match motion {
        NavMove::Down => (idx + 1).min(last),
        NavMove::Up => idx.saturating_sub(1),
        NavMove::Left | NavMove::Right => idx,
        NavMove::First => 0,
        NavMove::Last => last,
        NavMove::HalfPageDown => (idx + 6).min(last),
        NavMove::HalfPageUp => idx.saturating_sub(6),
    }
}

fn step_grid(len: usize, cols: usize, current: Option<usize>, motion: NavMove) -> usize {
    let last = len - 1;
    let idx = match current {
        Some(i) => i,
        None => {
            return match motion {
                NavMove::Up | NavMove::Last => last,
                _ => 0,
            };
        }
    };
    match motion {
        NavMove::Down => {
            if idx + cols < len {
                idx + cols
            } else {
                idx
            }
        }
        NavMove::Up => {
            if idx >= cols {
                idx - cols
            } else {
                idx
            }
        }
        NavMove::Right => {
            if (idx + 1) % cols != 0 && idx + 1 < len {
                idx + 1
            } else {
                idx
            }
        }
        NavMove::Left => {
            if idx % cols != 0 {
                idx - 1
            } else {
                idx
            }
        }
        NavMove::First => 0,
        NavMove::Last => last,
        NavMove::HalfPageDown => (idx + cols * 3).min(last),
        NavMove::HalfPageUp => idx.saturating_sub(cols * 3),
    }
}

/// Partitions flat rows into columns where each column begins with a `space:`, `column:`, or `add` key.
fn partition_columns(rows: &[LibraryNavRow]) -> Vec<Vec<usize>> {
    let is_column_start = |key: &str| key.starts_with("space:") || key.starts_with("column:") || key.starts_with("add");
    let mut columns: Vec<Vec<usize>> = Vec::new();
    for (idx, row) in rows.iter().enumerate() {
        if columns.is_empty() || is_column_start(&row.key) {
            columns.push(vec![idx]);
        } else if let Some(last) = columns.last_mut() {
            last.push(idx);
        }
    }
    columns
}

fn step_columns(rows: &[LibraryNavRow], current: Option<usize>, motion: NavMove) -> usize {
    let columns = partition_columns(rows);
    if columns.is_empty() {
        return 0;
    }

    let (col, row) = current
        .and_then(|target| {
            columns
                .iter()
                .enumerate()
                .find_map(|(c, col_rows)| col_rows.iter().position(|&idx| idx == target).map(|r| (c, r)))
        })
        .unwrap_or((0, 0));

    let (next_col, next_row) = match motion {
        NavMove::Down => (col, (row + 1).min(columns[col].len() - 1)),
        NavMove::Up => (col, row.saturating_sub(1)),
        NavMove::Right => {
            let nc = (col + 1).min(columns.len() - 1);
            (nc, row.min(columns[nc].len() - 1))
        }
        NavMove::Left => {
            let nc = col.saturating_sub(1);
            (nc, row.min(columns[nc].len() - 1))
        }
        NavMove::First => (col, 0),
        NavMove::Last => (col, columns[col].len() - 1),
        NavMove::HalfPageDown => (col, (row + 6).min(columns[col].len() - 1)),
        NavMove::HalfPageUp => (col, row.saturating_sub(6)),
    };

    columns[next_col][next_row]
}

/// Handles global keyboard events while the library is open (and its search input is not eating the key).
///
/// Returns `true` when the key was handled and should not fall through to the standard keymap or page.
pub fn handle_library_key_down(e: &KeyboardEventData, core: &CoreView, library: &super::LibraryViewState) -> bool {
    if !*library.open.peek() {
        return false;
    }

    // If the user clicked outside onto the page, let the page/keymap have keys unless Escape closes or they return.
    if !*library.nav.keyboard_focused.peek() {
        return false;
    }

    // While the search input has focus, its own `on_pre_key_down` handles C-j/C-k/arrows/Enter/Tab/Escape
    // and lets plain characters type into the box.
    if library.nav.is_search_focused() {
        // Still allow Shift+Alt+L or Ctrl+Shift+Y etc. to reach the standard keymap.
        if e.modifiers.contains(Modifiers::ALT) || e.modifiers.contains(Modifiers::META) {
            return false;
        }
        if e.modifiers.contains(Modifiers::CONTROL) && e.modifiers.contains(Modifiers::SHIFT) {
            return false;
        }
        return false;
    }

    let vim_enabled = core.model().peek().vim_mode_enabled;
    let ctrl = e.modifiers.contains(Modifiers::CONTROL);
    let alt_or_meta = e.modifiers.contains(Modifiers::ALT) || e.modifiers.contains(Modifiers::META);
    let shift = e.modifiers.contains(Modifiers::SHIFT);

    // Let Alt / Super / Ctrl+Shift shortcuts (like Shift+Alt+L, Ctrl+Shift+Y) pass through to the keymap.
    if alt_or_meta || (ctrl && shift) {
        library.nav.reset_pending();
        return false;
    }

    // Ctrl shortcuts inside the library (both vim mode on and off).
    if ctrl {
        library.nav.reset_pending();
        let handled = match &e.key {
            Key::Character(ch) => match ch.to_ascii_lowercase().as_str() {
                "j" | "n" => {
                    library.nav.move_cursor(NavMove::Down);
                    true
                }
                "k" | "p" => {
                    library.nav.move_cursor(NavMove::Up);
                    true
                }
                "d" if vim_enabled => {
                    library.nav.move_cursor(NavMove::HalfPageDown);
                    true
                }
                "u" if vim_enabled => {
                    library.nav.move_cursor(NavMove::HalfPageUp);
                    true
                }
                _ => false,
            },
            _ => false,
        };
        return handled;
    }

    // Pending `g` prefix in LIBRARY mode (`gg`, `gh`, `gd`, `gs`, `gm`, `gr`).
    if vim_enabled && *library.nav.pending_g.peek() {
        library.nav.reset_pending();
        if let Key::Character(ch) = &e.key {
            match ch.as_str() {
                "g" => {
                    library.nav.move_cursor(NavMove::First);
                    return true;
                }
                "h" => {
                    core.invoke_or_log(Command::LibraryOpen {
                        section: Some(LibrarySection::History),
                    });
                    return true;
                }
                "d" => {
                    core.invoke_or_log(Command::LibraryOpen {
                        section: Some(LibrarySection::Downloads),
                    });
                    return true;
                }
                "s" => {
                    core.invoke_or_log(Command::LibraryOpen {
                        section: Some(LibrarySection::Spaces),
                    });
                    return true;
                }
                "m" => {
                    core.invoke_or_log(Command::LibraryOpen {
                        section: Some(LibrarySection::Media),
                    });
                    return true;
                }
                "r" => {
                    core.invoke_or_log(Command::LibraryOpen {
                        section: Some(LibrarySection::SpaceRouting),
                    });
                    return true;
                }
                _ => return true,
            }
        }
        return true;
    }

    // Standard keys that work whether vim mode is enabled or not.
    match &e.key {
        Key::Named(NamedKey::Escape) => {
            if !library.nav.close_filter_panel_if_open() {
                core.invoke_or_log(Command::LibraryClose);
            }
            return true;
        }
        Key::Named(NamedKey::ArrowDown) => {
            library.nav.move_cursor(NavMove::Down);
            return true;
        }
        Key::Named(NamedKey::ArrowUp) => {
            library.nav.move_cursor(NavMove::Up);
            return true;
        }
        Key::Named(NamedKey::ArrowLeft) => {
            library.nav.move_cursor(NavMove::Left);
            return true;
        }
        Key::Named(NamedKey::ArrowRight) => {
            library.nav.move_cursor(NavMove::Right);
            return true;
        }
        Key::Named(NamedKey::Home) => {
            library.nav.move_cursor(NavMove::First);
            return true;
        }
        Key::Named(NamedKey::End) => {
            library.nav.move_cursor(NavMove::Last);
            return true;
        }
        Key::Named(NamedKey::Enter) => {
            library.nav.activate_cursor(core);
            return true;
        }
        Key::Named(NamedKey::Tab) => {
            let delta = if shift { -1 } else { 1 };
            if let Some(next) = library.adjacent_visible_section(delta) {
                core.invoke_or_log(Command::LibraryOpen { section: Some(next) });
            }
            return true;
        }
        _ => {}
    }

    if !vim_enabled {
        return false;
    }

    // Vim LIBRARY mode keys (ZendiaLibraryView.sys.mjs + vim.toml).
    if let Key::Character(ch) = &e.key {
        match ch.as_str() {
            "j" => {
                library.nav.move_cursor(NavMove::Down);
                return true;
            }
            "k" => {
                library.nav.move_cursor(NavMove::Up);
                return true;
            }
            "h" => {
                library.nav.move_cursor(NavMove::Left);
                return true;
            }
            "l" => {
                library.nav.move_cursor(NavMove::Right);
                return true;
            }
            "g" if !shift => {
                let mut pending = library.nav.pending_g;
                pending.set(true);
                return true;
            }
            "G" => {
                library.nav.move_cursor(NavMove::Last);
                return true;
            }
            "o" => {
                library.nav.activate_cursor(core);
                return true;
            }
            "/" | "i" => {
                library.nav.focus_search();
                return true;
            }
            "f" => {
                library.nav.open_filter_panel();
                return true;
            }
            "q" => {
                if !library.nav.close_filter_panel_if_open() {
                    core.invoke_or_log(Command::LibraryClose);
                }
                return true;
            }
            "d" => {
                if let Some(id) = cursor_history_entry_id(library) {
                    core.invoke_or_log(Command::HistoryDelete { entry_id: id });
                }
                return true;
            }
            "y" => {
                if let Some(id) = cursor_history_entry_id(library) {
                    core.invoke_or_log(Command::HistoryCopyUrl { entry_id: id });
                }
                return true;
            }
            _ => {}
        }
    }

    false
}

fn cursor_history_entry_id(library: &super::LibraryViewState) -> Option<HistoryEntryId> {
    let key = library.nav.cursor.peek().clone()?;
    let id = key.strip_prefix("history:")?.parse::<u64>().ok()?;
    Some(HistoryEntryId(id))
}

/// Draws the keyboard cursor's teal ring around one row or tile when `focused`.
#[derive(Clone, PartialEq)]
pub struct LibraryCursorRing {
    pub focused: bool,
    pub corner_radius: f32,
    pub child: Element,
}

impl LibraryCursorRing {
    pub fn new(focused: bool, child: impl IntoElement) -> Self {
        Self {
            focused,
            corner_radius: 8.0,
            child: child.into_element(),
        }
    }

    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = radius;
        self
    }
}

impl Component for LibraryCursorRing {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let teal = theme.rgba(ThemeRole::ModeLibrary);
        let (ring, fill) = if self.focused {
            let fill_alpha = match theme.scheme {
                ResolvedScheme::Dark => 0.14,
                ResolvedScheme::Light => 0.14,
            };
            (teal.to_color(), teal.with_alpha(fill_alpha).to_color())
        } else {
            (Color::TRANSPARENT, Color::TRANSPARENT)
        };
        rect()
            .width(Size::fill())
            .corner_radius(CornerRadius::new_all(self.corner_radius))
            .background(fill)
            .border(Border::new().width(1.5).alignment(BorderAlignment::Inner).fill(ring))
            .child(self.child.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: &str) -> LibraryNavRow {
        LibraryNavRow::new(key, None)
    }

    #[test]
    fn list_movement_clamps_and_jumps() {
        let rows = vec![row("history:1"), row("history:2"), row("history:3")];
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::List, None, NavMove::Down),
            Some("history:1".to_string())
        );
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::List, Some("history:1"), NavMove::Down),
            Some("history:2".to_string())
        );
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::List, Some("history:3"), NavMove::Down),
            Some("history:3".to_string())
        );
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::List, Some("history:2"), NavMove::Up),
            Some("history:1".to_string())
        );
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::List, Some("history:1"), NavMove::Last),
            Some("history:3".to_string())
        );
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::List, Some("history:3"), NavMove::First),
            Some("history:1".to_string())
        );
    }

    #[test]
    fn grid_movement_respects_columns_and_row_edges() {
        let rows: Vec<_> = (0..5).map(|i| row(&format!("media:{i}"))).collect();
        let layout = LibraryNavLayout::Grid { columns: 2 };
        assert_eq!(
            step_cursor(&rows, layout, Some("media:0"), NavMove::Right),
            Some("media:1".to_string())
        );
        // Does not wrap across right edge.
        assert_eq!(
            step_cursor(&rows, layout, Some("media:1"), NavMove::Right),
            Some("media:1".to_string())
        );
        assert_eq!(
            step_cursor(&rows, layout, Some("media:0"), NavMove::Down),
            Some("media:2".to_string())
        );
        assert_eq!(
            step_cursor(&rows, layout, Some("media:2"), NavMove::Down),
            Some("media:4".to_string())
        );
        // Bottom-right hole does not move past len.
        assert_eq!(
            step_cursor(&rows, layout, Some("media:3"), NavMove::Down),
            Some("media:3".to_string())
        );
        assert_eq!(
            step_cursor(&rows, layout, Some("media:4"), NavMove::Up),
            Some("media:2".to_string())
        );
    }

    #[test]
    fn columns_movement_steps_between_space_cards_and_clamps_row() {
        let rows = vec![
            row("space:1"),
            row("tab:10"),
            row("tab:11"),
            row("space:2"),
            row("tab:20"),
            row("add-space"),
        ];
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::Columns, Some("space:1"), NavMove::Down),
            Some("tab:10".to_string())
        );
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::Columns, Some("tab:11"), NavMove::Right),
            Some("tab:20".to_string())
        );
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::Columns, Some("tab:20"), NavMove::Right),
            Some("add-space".to_string())
        );
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::Columns, Some("add-space"), NavMove::Left),
            Some("space:2".to_string())
        );
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::Columns, Some("tab:11"), NavMove::First),
            Some("space:1".to_string())
        );
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::Columns, Some("space:1"), NavMove::Last),
            Some("tab:11".to_string())
        );
    }
}
