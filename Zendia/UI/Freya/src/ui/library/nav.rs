/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The library's keyboard layer seam (Zendia's `ZendiaLibraryView`).
//!
//! A section calls [`use_library_nav`] at the top of its `render` with the rows a cursor can walk, in the order they
//! are drawn, and wraps each row in [`LibraryCursorRing`]. When vim mode is enabled, keys in the list go through
//! [`zendia_keymap::KeyEngine`] in mode `"l"` (`LIB-02`, `LIB-06`, `LIB-10`) and arrive via
//! [`zendia_browser_core::events::ViewRequest::LibraryVimAction`]. Row keys are strings made from core ids
//! (`history:42`, `download:7`, `space:3`), never Freya ids, and they only need to be unique within one section.

use crate::ui::core_view::CoreView;
use crate::ui::theme::{ResolvedScheme, ThemeRole, use_theme};
use freya::prelude::*;
use std::collections::BTreeMap;
use zendia_browser_core::commands::Command;
use zendia_browser_core::focus::HINT_ALPHABET;
use zendia_browser_core::library::LibrarySection;
use zendia_browser_core::model::SpaceId;

/// Outcome of typing one character during a library `s` jump-hint session (`LIB-03`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryHintOutcome {
    Narrowed,
    Matched(String),
    NoMatch,
}

/// Active `s` jump-hint session over visible library rows (`ZendiaHints`, `LIB-03`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryHintSession {
    pub entries: Vec<(String, String)>,
    pub typed: String,
}

impl LibraryHintSession {
    pub fn new(row_keys: Vec<String>) -> Self {
        let labels = assign_library_hint_labels(row_keys.len());
        let entries = row_keys.into_iter().zip(labels).collect();
        Self {
            entries,
            typed: String::new(),
        }
    }

    pub fn label_suffix<'a>(&'a self, row_key: &str) -> Option<&'a str> {
        let (_, label) = self.entries.iter().find(|(k, _)| k == row_key)?;
        label.strip_prefix(self.typed.as_str())
    }

    pub fn pop_char(&mut self) -> bool {
        self.typed.pop().is_some()
    }

    pub fn type_char(&mut self, ch: char) -> LibraryHintOutcome {
        let mut next = self.typed.clone();
        next.push(ch.to_ascii_lowercase());
        if let Some((key, _)) = self.entries.iter().find(|(_, label)| label == &next) {
            let matched = key.clone();
            self.typed = next;
            return LibraryHintOutcome::Matched(matched);
        }
        if self.entries.iter().any(|(_, label)| label.starts_with(&next)) {
            self.typed = next;
            LibraryHintOutcome::Narrowed
        } else {
            LibraryHintOutcome::NoMatch
        }
    }
}

/// Assigns prefix-free hint labels from `HINT_ALPHABET` (`ZendiaJumpLabels`).
pub fn assign_library_hint_labels(count: usize) -> Vec<String> {
    if count == 0 {
        return Vec::new();
    }
    let chars: Vec<char> = HINT_ALPHABET.chars().collect();
    let base = chars.len();
    if count <= base {
        return chars.into_iter().take(count).map(|c| c.to_string()).collect();
    }
    let mut groups: Vec<String> = chars.iter().map(|c| c.to_string()).collect();
    while groups.len() < count {
        let needed = count - groups.len();
        let expand = needed.div_ceil(base - 1).min(groups.len());
        let split_at = groups.len() - expand;
        let tail = groups.split_off(split_at);
        for prefix in tail {
            for &ch in &chars {
                groups.push(format!("{prefix}{ch}"));
                if groups.len() + (expand - 1) >= count && groups.len() >= count {
                    break;
                }
            }
        }
    }
    groups.truncate(count);
    groups
}

/// One row or tile the cursor can stop on.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryNavRow {
    pub key: String,
    /// What Enter / `o` does on this row when backed by a core command (`None` triggers view-local `activate_seq`).
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
    /// Accessibility id of an inline form text input (such as the Space Routing pattern field), if any.
    pub form_input_a11y_id: State<Option<AccessibilityId>>,
    /// Accessibility id of the library's list focus sink so Escape can pull focus out of the search box.
    pub list_focus_a11y_id: AccessibilityId,
    /// True while a section's filter panel is open (Escape/`q` closes it before closing the library).
    pub filter_panel_open: State<Option<State<bool>>>,
    /// True while a section has an inline subview (such as the Space Routing form) that Escape/`q` should close first.
    pub subview_open: State<bool>,
    /// Incremented whenever Escape/`q` is directed to the active section's inline subview.
    pub subview_escape_seq: State<u64>,
    /// Incremented when Enter/`o` activates a view-local row (filter chip, Done, or routing form control).
    pub activate_seq: State<u64>,
    /// Key of the most recently activated view-local row.
    pub activated_key: State<Option<String>>,
    /// Measured number of visible rows in the active section's scroll viewport (`LIB-08`).
    pub visible_rows: State<usize>,
    /// When true, switching to a section keeps focus in the list instead of auto-focusing its search box (`LIB-07`).
    pub stay_in_list: State<bool>,
    /// Reactive mirror of whether a text field inside the library currently holds keyboard focus (`INSERT` statusline, `LIB-07`).
    pub inserting: State<bool>,
    /// Active `s` jump-hint session over visible rows (`LIB-03`).
    pub hints: State<Option<LibraryHintSession>>,
    /// Window-coordinate bounds `(x, y, width, height)` of rendered rows for `s` visibility and `m` menu anchoring.
    pub row_areas: State<BTreeMap<String, (f32, f32, f32, f32)>>,
    /// Window-coordinate bounds `(x, y, width, height)` of the library body viewport.
    pub viewport_area: State<(f32, f32, f32, f32)>,
    /// True while the library holds keyboard focus (false after clicking on the page card).
    pub keyboard_focused: State<bool>,
}

impl LibraryNavState {
    /// Moves the cursor according to `motion` over the registered rows by `1` step.
    pub fn move_cursor(&self, motion: NavMove) {
        self.move_cursor_by(motion, 1);
    }

    /// Moves the cursor according to `motion` and `count` over the registered rows (`LIB-06`, `LIB-08`).
    pub fn move_cursor_by(&self, motion: NavMove, count: usize) {
        self.clear_hints();
        let Some(registered) = self.rows.peek().clone() else {
            return;
        };
        let current = self.cursor.peek().clone();
        let visible_rows = (*self.visible_rows.peek()).max(1);
        let next = step_cursor_with_rows(
            &registered.rows,
            registered.layout,
            current.as_deref(),
            motion,
            visible_rows,
            count.max(1),
        );
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

    /// Activates the row currently under the cursor (`<CR>` or `o`).
    ///
    /// On a space header (`space:{id}`), closes the library first and then switches to that space (`LIB-11`).
    /// On view-local rows (`activate == None`), bumps `activate_seq` so the owning component handles it (`LIB-05`).
    pub fn activate_cursor(&self, core: &CoreView) -> bool {
        self.clear_hints();
        let Some(registered) = self.rows.peek().clone() else {
            return false;
        };
        let Some(cursor_key) = self.cursor.peek().clone() else {
            return false;
        };
        let Some(row) = registered.rows.into_iter().find(|r| r.key == cursor_key) else {
            return false;
        };
        if let Some(id_raw) = cursor_key.strip_prefix("space:")
            && let Ok(raw_id) = id_raw.parse::<u64>()
        {
            core.invoke_or_log(Command::LibraryClose);
            core.invoke_or_log(Command::SpaceSwitch {
                space_id: SpaceId(raw_id),
            });
            return true;
        }
        if let Some(cmd) = row.activate {
            core.invoke_or_log(cmd);
            return true;
        }
        let mut key_state = self.activated_key;
        key_state.set(Some(cursor_key));
        let mut seq = self.activate_seq;
        let next = *seq.peek() + 1;
        seq.set(next);
        true
    }

    /// True when the active section's search box currently owns Freya keyboard focus and is not hidden by filters.
    pub fn is_search_focused(&self) -> bool {
        let filter_open = self.filter_panel_open.peek().is_some_and(|s| *s.peek());
        !filter_open && self.search_a11y_id.peek().is_some_and(|id| id.is_focused())
    }

    /// True when an inline form input (such as the Space Routing pattern input) currently owns focus.
    pub fn is_form_input_focused(&self) -> bool {
        self.form_input_a11y_id.peek().is_some_and(|id| id.is_focused())
    }

    /// Updates the reactive `inserting` flag from current field focus.
    pub fn sync_inserting(&self, extra_editing: bool) -> bool {
        let now = extra_editing || self.is_search_focused() || self.is_form_input_focused();
        let mut ins = self.inserting;
        ins.set_if_modified(now);
        now
    }

    /// Moves keyboard focus to the active section's search input, closing any open filter panel first.
    pub fn focus_search(&self) -> bool {
        self.clear_hints();
        if let Some(mut open) = *self.filter_panel_open.peek() {
            open.set_if_modified(false);
        }
        if let Some(id) = *self.search_a11y_id.peek() {
            let mut stay = self.stay_in_list;
            stay.set_if_modified(false);
            let mut ins = self.inserting;
            ins.set_if_modified(true);
            id.request_focus();
            return true;
        }
        false
    }

    /// Moves keyboard focus out of any field onto the library list sink.
    pub fn focus_list(&self) {
        self.clear_hints();
        let mut ins = self.inserting;
        ins.set_if_modified(false);
        self.list_focus_a11y_id.request_focus();
    }

    /// Closes the active section's filter panel if it is open. Returns `true` when it consumed the Escape/`q`.
    pub fn close_filter_panel_if_open(&self) -> bool {
        if let Some(mut open) = *self.filter_panel_open.peek()
            && *open.peek()
        {
            open.set(false);
            self.focus_list();
            return true;
        }
        false
    }

    /// Routes Escape/`q` to an open inline subview (such as the Space Routing add/edit form) if active.
    pub fn close_subview_if_open(&self) -> bool {
        if *self.subview_open.peek() {
            let mut seq = self.subview_escape_seq;
            let next = *seq.peek() + 1;
            seq.set(next);
            self.focus_list();
            return true;
        }
        false
    }

    /// Opens the active section's filter panel if the section exposes one, moving keys out of the search box (`LIB-05`).
    pub fn open_filter_panel(&self) -> bool {
        self.clear_hints();
        if let Some(mut open) = *self.filter_panel_open.peek() {
            open.set_if_modified(true);
            self.focus_list();
            return true;
        }
        false
    }

    /// Starts `s` jump-label hints over the rows currently visible on screen (`LIB-03`).
    pub fn start_jump_hints(&self) -> bool {
        let Some(registered) = self.rows.peek().clone() else {
            return false;
        };
        if registered.rows.is_empty() {
            return false;
        }
        let (vx, vy, vw, vh) = *self.viewport_area.peek();
        let areas = self.row_areas.peek();
        let mut visible_keys: Vec<String> = if vw > 1.0 && vh > 1.0 && !areas.is_empty() {
            registered
                .rows
                .iter()
                .filter(|row| {
                    areas.get(&row.key).is_some_and(|&(rx, ry, rw, rh)| {
                        rw > 0.5
                            && rh > 0.5
                            && rx + rw > vx + 1.0
                            && rx < vx + vw - 1.0
                            && ry + rh > vy + 1.0
                            && ry < vy + vh - 1.0
                    })
                })
                .map(|row| row.key.clone())
                .collect()
        } else {
            Vec::new()
        };
        if visible_keys.is_empty() {
            visible_keys = registered.rows.into_iter().map(|r| r.key).collect();
        }
        let mut hints = self.hints;
        hints.set(Some(LibraryHintSession::new(visible_keys)));
        true
    }

    /// Cancels any active `s` jump-label session.
    pub fn clear_hints(&self) {
        let mut hints = self.hints;
        if hints.peek().is_some() {
            hints.set(None);
        }
    }

    /// Returns the measured anchor position `(x, y)` for opening a context menu on `row_key` (`LIB-03`).
    pub fn menu_position_for(&self, row_key: &str) -> (f32, f32) {
        if let Some(&(x, y, w, h)) = self.row_areas.peek().get(row_key)
            && w > 0.0
            && h > 0.0
        {
            (x + (w * 0.5).min(24.0), y + h)
        } else {
            (220.0, 140.0)
        }
    }

    /// Updates the measured visible row count for `<C-d>` / `<C-u>` (`LIB-08`).
    pub fn set_visible_rows(&self, count: usize) {
        let clamped = count.max(1);
        let mut vis = self.visible_rows;
        if *vis.peek() != clamped {
            vis.set(clamped);
        }
    }

    /// Clears transient state when the library opens, closes, or suspends.
    pub fn reset_pending(&self) {
        self.clear_hints();
    }
}

pub(super) fn use_library_nav_state() -> LibraryNavState {
    LibraryNavState {
        rows: use_state(|| None),
        cursor: use_state(|| None),
        search_a11y_id: use_state(|| None),
        form_input_a11y_id: use_state(|| None),
        list_focus_a11y_id: use_hook(AccessibilityId::new_unique),
        filter_panel_open: use_state(|| None),
        subview_open: use_state(|| false),
        subview_escape_seq: use_state(|| 0u64),
        activate_seq: use_state(|| 0u64),
        activated_key: use_state(|| None),
        visible_rows: use_state(|| 16usize),
        stay_in_list: use_state(|| false),
        inserting: use_state(|| false),
        hints: use_state(|| None),
        row_areas: use_state(BTreeMap::new),
        viewport_area: use_state(|| (0.0f32, 0.0f32, 0.0f32, 0.0f32)),
        keyboard_focused: use_state(|| false),
    }
}

/// Registers the active section's walkable rows. Call it once, unconditionally, at the top of the section's
/// `render`. Returns the key under the cursor.
pub fn use_library_nav(section: LibrarySection, layout: LibraryNavLayout, rows: Vec<LibraryNavRow>) -> Option<String> {
    let state = super::try_use_library_state();
    use_drop(move || {
        if let Some(st) = state {
            let same_section = st.nav.rows.peek().as_ref().is_some_and(|r| r.section == section);
            if same_section {
                let mut fp = st.nav.filter_panel_open;
                fp.set_if_modified(None);
                let mut sid = st.nav.search_a11y_id;
                sid.set_if_modified(None);
                let mut fid = st.nav.form_input_a11y_id;
                fid.set_if_modified(None);
                let mut ins = st.nav.inserting;
                ins.set_if_modified(false);
            }
        }
    });
    let next = LibraryNavRows { section, layout, rows };
    if let Some(state) = state {
        let prev_section = state.nav.rows.peek().as_ref().map(|r| r.section);
        let section_changed = prev_section.is_some_and(|prev| prev != section);
        let rows_changed = state.nav.rows.peek().as_ref() != Some(&next);
        if rows_changed {
            let mut registered = state.nav.rows;
            registered.set(Some(next.clone()));
            let mut areas = state.nav.row_areas;
            let valid_keys: std::collections::HashSet<&str> = next.rows.iter().map(|r| r.key.as_str()).collect();
            areas.write().retain(|k, _| valid_keys.contains(k.as_str()));
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
            let mut form_id = state.nav.form_input_a11y_id;
            form_id.set_if_modified(None);
            let mut filter_panel = state.nav.filter_panel_open;
            filter_panel.set_if_modified(None);
            let mut subview = state.nav.subview_open;
            subview.set_if_modified(false);
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
    step_cursor_with_rows(rows, layout, current, motion, 12, 1)
}

/// Pure cursor movement over a flat row list with dynamic `visible_rows` and `count` (`LIB-06`, `LIB-08`).
pub fn step_cursor_with_rows(
    rows: &[LibraryNavRow],
    layout: LibraryNavLayout,
    current: Option<&str>,
    motion: NavMove,
    visible_rows: usize,
    count: usize,
) -> Option<String> {
    if rows.is_empty() {
        return None;
    }

    let current_idx = current.and_then(|key| rows.iter().position(|row| row.key == key));
    let half_rows = (visible_rows.max(1) / 2).max(1);
    let steps = count.max(1);

    let next_idx = match layout {
        LibraryNavLayout::List => step_list(rows.len(), current_idx, motion, half_rows, steps),
        LibraryNavLayout::Grid { columns } => {
            step_grid(rows.len(), columns.max(1), current_idx, motion, half_rows, steps)
        }
        LibraryNavLayout::Columns => step_columns(rows, current_idx, motion, half_rows, steps),
    };

    rows.get(next_idx).map(|row| row.key.clone())
}

fn step_list(len: usize, current: Option<usize>, motion: NavMove, half_rows: usize, count: usize) -> usize {
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
        NavMove::Down => idx.saturating_add(count).min(last),
        NavMove::Up => idx.saturating_sub(count),
        NavMove::Left | NavMove::Right => idx,
        NavMove::First => 0,
        NavMove::Last => last,
        NavMove::HalfPageDown => idx.saturating_add(half_rows.saturating_mul(count)).min(last),
        NavMove::HalfPageUp => idx.saturating_sub(half_rows.saturating_mul(count)),
    }
}

fn step_grid(
    len: usize,
    cols: usize,
    current: Option<usize>,
    motion: NavMove,
    half_rows: usize,
    count: usize,
) -> usize {
    let last = len - 1;
    let mut idx = match current {
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
            for _ in 0..count {
                if idx + cols < len {
                    idx += cols;
                }
            }
            idx
        }
        NavMove::Up => {
            for _ in 0..count {
                if idx >= cols {
                    idx -= cols;
                }
            }
            idx
        }
        NavMove::Right => {
            let row_end = ((idx / cols) + 1) * cols - 1;
            idx.saturating_add(count).min(row_end).min(last)
        }
        NavMove::Left => {
            let row_start = (idx / cols) * cols;
            idx.saturating_sub(count).max(row_start)
        }
        NavMove::First => 0,
        NavMove::Last => last,
        NavMove::HalfPageDown => {
            let total_steps = half_rows.saturating_mul(count);
            for _ in 0..total_steps {
                if idx + cols < len {
                    idx += cols;
                }
            }
            idx
        }
        NavMove::HalfPageUp => {
            let total_steps = half_rows.saturating_mul(count);
            for _ in 0..total_steps {
                if idx >= cols {
                    idx -= cols;
                }
            }
            idx
        }
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

fn step_columns(
    rows: &[LibraryNavRow],
    current: Option<usize>,
    motion: NavMove,
    half_rows: usize,
    count: usize,
) -> usize {
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
        NavMove::Down => (col, row.saturating_add(count).min(columns[col].len() - 1)),
        NavMove::Up => (col, row.saturating_sub(count)),
        NavMove::Right => {
            let nc = col.saturating_add(count).min(columns.len() - 1);
            (nc, row.min(columns[nc].len() - 1))
        }
        NavMove::Left => {
            let nc = col.saturating_sub(count);
            (nc, row.min(columns[nc].len() - 1))
        }
        NavMove::First => (col, 0),
        NavMove::Last => (col, columns[col].len() - 1),
        NavMove::HalfPageDown => (
            col,
            row.saturating_add(half_rows.saturating_mul(count))
                .min(columns[col].len() - 1),
        ),
        NavMove::HalfPageUp => (col, row.saturating_sub(half_rows.saturating_mul(count))),
    };

    columns[next_col][next_row]
}

fn is_bare_modifier(key: &Key) -> bool {
    matches!(
        key,
        Key::Named(
            NamedKey::Shift
                | NamedKey::Control
                | NamedKey::Alt
                | NamedKey::AltGraph
                | NamedKey::Meta
                | NamedKey::CapsLock
                | NamedKey::NumLock
                | NamedKey::ScrollLock
        )
    )
}

/// Handles global keyboard events while the library is open.
///
/// With vim mode enabled, only active `s` jump hints are handled here; all other list-mode keys pass through to
/// [`crate::ui::page::KeymapController`] so `KeyEngine` in mode `"l"` drives counts, `g` which-key, and `<leader>`
/// sequences (`LIB-02`, `LIB-06`, `LIB-10`). With vim mode off, standard arrow/Enter/Tab/Escape navigation runs here.
pub fn handle_library_key_down(e: &KeyboardEventData, core: &CoreView, library: &super::LibraryViewState) -> bool {
    if !*library.open.peek() || !*library.nav.keyboard_focused.peek() {
        return false;
    }

    // Active `s` jump-label session (`LIB-03`).
    if library.nav.hints.peek().is_some() {
        if is_bare_modifier(&e.key) {
            return false;
        }
        if e.modifiers
            .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
        {
            library.nav.clear_hints();
            return false;
        }
        match &e.key {
            Key::Named(NamedKey::Escape) => {
                library.nav.clear_hints();
                return true;
            }
            Key::Named(NamedKey::Backspace) => {
                let mut hints_state = library.nav.hints;
                let mut clear = false;
                if let Some(session) = hints_state.write().as_mut()
                    && !session.pop_char()
                {
                    clear = true;
                }
                if clear {
                    hints_state.set(None);
                }
                return true;
            }
            Key::Character(ch) if !ch.is_empty() => {
                let first = ch.chars().next().unwrap_or('\0');
                let mut hints_state = library.nav.hints;
                let outcome = hints_state.write().as_mut().map(|s| s.type_char(first));
                match outcome {
                    Some(LibraryHintOutcome::Matched(target_key)) => {
                        hints_state.set(None);
                        let mut cursor = library.nav.cursor;
                        cursor.set_if_modified(Some(target_key));
                    }
                    Some(LibraryHintOutcome::NoMatch) | None => {
                        hints_state.set(None);
                    }
                    Some(LibraryHintOutcome::Narrowed) => {}
                }
                return true;
            }
            _ => {
                library.nav.clear_hints();
                return true;
            }
        }
    }

    // While a search input or inline form input has focus, its own `on_pre_key_down` handles typing and navigation.
    if library.nav.is_search_focused() || library.nav.is_form_input_focused() {
        return false;
    }

    // In vim mode, route all library keys through `KeyEngine` (`mode = "l"`) so counts, `g` which-key, and `<leader>`
    // chords work identically to every other mode (`LIB-02`, `LIB-06`, `LIB-10`).
    if core.model().peek().vim_mode_enabled {
        return false;
    }

    let ctrl = e.modifiers.contains(Modifiers::CONTROL);
    let alt_or_meta = e.modifiers.contains(Modifiers::ALT) || e.modifiers.contains(Modifiers::META);
    let shift = e.modifiers.contains(Modifiers::SHIFT);

    if alt_or_meta || (ctrl && shift) {
        return false;
    }

    if ctrl {
        if let Key::Character(ch) = &e.key {
            match ch.to_ascii_lowercase().as_str() {
                "j" | "n" => {
                    library.nav.move_cursor(NavMove::Down);
                    return true;
                }
                "k" | "p" => {
                    library.nav.move_cursor(NavMove::Up);
                    return true;
                }
                _ => {}
            }
        }
        return false;
    }

    match &e.key {
        Key::Named(NamedKey::Escape) => {
            if !library.nav.close_subview_if_open() && !library.nav.close_filter_panel_if_open() {
                core.invoke_or_log(Command::LibraryClose);
            }
            true
        }
        Key::Named(NamedKey::ArrowDown) => {
            library.nav.move_cursor(NavMove::Down);
            true
        }
        Key::Named(NamedKey::ArrowUp) => {
            library.nav.move_cursor(NavMove::Up);
            true
        }
        Key::Named(NamedKey::ArrowLeft) => {
            library.nav.move_cursor(NavMove::Left);
            true
        }
        Key::Named(NamedKey::ArrowRight) => {
            library.nav.move_cursor(NavMove::Right);
            true
        }
        Key::Named(NamedKey::PageDown) => {
            library.nav.move_cursor(NavMove::HalfPageDown);
            true
        }
        Key::Named(NamedKey::PageUp) => {
            library.nav.move_cursor(NavMove::HalfPageUp);
            true
        }
        Key::Named(NamedKey::Home) => {
            library.nav.move_cursor(NavMove::First);
            true
        }
        Key::Named(NamedKey::End) => {
            library.nav.move_cursor(NavMove::Last);
            true
        }
        Key::Named(NamedKey::Enter) => {
            library.nav.activate_cursor(core);
            true
        }
        Key::Named(NamedKey::Tab) => {
            let delta = if shift { -1 } else { 1 };
            if let Some(next) = library.adjacent_visible_section(delta) {
                library.open_keeping_list_focus(Some(next));
            }
            true
        }
        _ => false,
    }
}

/// Draws the keyboard cursor's teal ring around one row or tile when `focused`, records its screen bounds for `s`/`m`,
/// and overlays its `s` jump-hint badge when active (`LIB-03`).
#[derive(Clone, PartialEq)]
pub struct LibraryCursorRing {
    pub focused: bool,
    pub corner_radius: f32,
    pub fill: bool,
    pub row_key: Option<String>,
    pub child: Element,
}

impl LibraryCursorRing {
    pub fn new(focused: bool, child: impl IntoElement) -> Self {
        Self {
            focused,
            corner_radius: 16.0,
            fill: true,
            row_key: None,
            child: child.into_element(),
        }
    }

    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = radius;
        self
    }

    /// Controls whether the focused state draws an 18% tinted fill inside the teal ring (disable for Space card
    /// headers and media tiles to match `zendia.css`).
    pub fn fill(mut self, fill: bool) -> Self {
        self.fill = fill;
        self
    }

    /// Associates this ring with its registered `LibraryNavRow` key so `s` jump labels and `m` menus know its bounds.
    pub fn row_key(mut self, key: impl Into<String>) -> Self {
        self.row_key = Some(key.into());
        self
    }
}

impl Component for LibraryCursorRing {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let library = super::try_use_library_state();
        let mut measured_height = use_state(|| 36.0f32);
        let teal = theme.rgba(ThemeRole::ModeLibrary);
        let (ring, fill) = if self.focused {
            let fill_color = if self.fill {
                let fill_alpha = match theme.scheme {
                    ResolvedScheme::Dark => 0.18,
                    ResolvedScheme::Light => 0.18,
                };
                teal.with_alpha(fill_alpha).to_color()
            } else {
                Color::TRANSPARENT
            };
            (teal.to_color(), fill_color)
        } else {
            (Color::TRANSPARENT, Color::TRANSPARENT)
        };

        let hint_suffix: Option<String> = self.row_key.as_deref().and_then(|rk| {
            let lib = library?;
            let session = lib.nav.hints.read();
            session
                .as_ref()
                .and_then(|s| s.label_suffix(rk))
                .filter(|suffix| !suffix.is_empty())
                .map(str::to_string)
        });

        let hint_badge = hint_suffix.map(|suffix| {
            let top = ((*measured_height.read() - 16.0) * 0.5).max(2.0);
            rect()
                .layer(Layer::Relative(8))
                .position(Position::new_absolute().right(4.0).top(top))
                .interactive(false)
                .min_width(Size::px(16.0))
                .height(Size::px(16.0))
                .padding(Gaps::new(0.0, 5.0, 0.0, 5.0))
                .corner_radius(CornerRadius::new_all(4.0))
                .background(theme.color(ThemeRole::Hint))
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .child(
                    label()
                        .text(suffix)
                        .font_size(11.0)
                        .font_weight(FontWeight::BOLD)
                        .font_family("monospace")
                        .color(theme.color(ThemeRole::HintText))
                        .max_lines(1),
                )
        });

        let tracked_key = self.row_key.clone();
        rect()
            .width(Size::fill())
            .corner_radius(CornerRadius::new_all(self.corner_radius))
            .background(fill)
            .border(Border::new().width(2.0).alignment(BorderAlignment::Inner).fill(ring))
            .maybe(tracked_key.is_some(), |r| {
                let rk = tracked_key.unwrap();
                r.on_sized(move |e: Event<SizedEventData>| {
                    let h = e.area.height();
                    if (*measured_height.peek() - h).abs() > 0.5 {
                        measured_height.set(h);
                    }
                    if let Some(lib) = library {
                        let next = (e.area.min_x(), e.area.min_y(), e.area.width(), h);
                        let mut areas = lib.nav.row_areas;
                        if areas.peek().get(&rk) != Some(&next) {
                            areas.write().insert(rk.clone(), next);
                        }
                    }
                })
            })
            .child(self.child.clone())
            .maybe_child(hint_badge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: &str) -> LibraryNavRow {
        LibraryNavRow::new(key, None)
    }

    #[test]
    fn list_movement_clamps_jumps_and_scales_with_count_and_visible_rows() {
        let rows: Vec<_> = (1..=20).map(|i| row(&format!("history:{i}"))).collect();
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::List, None, NavMove::Down),
            Some("history:1".to_string())
        );
        assert_eq!(
            step_cursor_with_rows(&rows, LibraryNavLayout::List, Some("history:1"), NavMove::Down, 10, 4),
            Some("history:5".to_string())
        );
        assert_eq!(
            step_cursor_with_rows(
                &rows,
                LibraryNavLayout::List,
                Some("history:1"),
                NavMove::HalfPageDown,
                10,
                2
            ),
            Some("history:11".to_string())
        );
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::List, Some("history:1"), NavMove::Last),
            Some("history:20".to_string())
        );
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::List, Some("history:20"), NavMove::First),
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
            row("space:add"),
        ];
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::Columns, Some("space:1"), NavMove::Down),
            Some("tab:10".to_string())
        );
        assert_eq!(
            step_cursor_with_rows(&rows, LibraryNavLayout::Columns, Some("space:1"), NavMove::Right, 10, 2),
            Some("space:add".to_string())
        );
        assert_eq!(
            step_cursor(&rows, LibraryNavLayout::Columns, Some("space:add"), NavMove::Left),
            Some("space:2".to_string())
        );
    }

    #[test]
    fn library_hint_session_narrows_and_matches_prefix_free_labels() {
        let keys: Vec<String> = (0..30).map(|i| format!("row:{i}")).collect();
        let mut session = LibraryHintSession::new(keys);
        assert_eq!(session.entries.len(), 30);
        let target_label = session.entries[29].1.clone();
        assert!(target_label.len() >= 2);
        let mut chars = target_label.chars();
        let first = chars.next().unwrap();
        assert_eq!(session.type_char(first), LibraryHintOutcome::Narrowed);
        let second = chars.next().unwrap();
        assert_eq!(
            session.type_char(second),
            LibraryHintOutcome::Matched("row:29".to_string())
        );
    }
}
