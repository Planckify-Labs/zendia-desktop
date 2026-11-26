/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! View-local menu overlay state ([`MenuViewState`]), positioning/clamping helpers, and command dispatch.

use crate::ui::core_view::CoreView;
use crate::ui::kit::{PopupHorizontalAlign, anchor_popup_origin};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::menus::{ResolvedMenuItem, SelectDropdownItem};
use zendia_browser_core::model::TabId;

/// Logical window-space anchor rectangle (typically a toolbar button's bounds) for anchored menus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for AnchorRect {
    fn default() -> Self {
        Self {
            x: 8.0,
            y: 8.0,
            width: 28.0,
            height: 28.0,
        }
    }
}

impl AnchorRect {
    pub fn from_area(area: Area) -> Self {
        Self {
            x: area.min_x(),
            y: area.min_y(),
            width: area.width(),
            height: area.height(),
        }
    }
}

/// Active context menu popup state held by [`MenuViewState`].
#[derive(Clone, Debug, PartialEq)]
pub struct OpenContextMenu {
    pub position: (f32, f32),
    pub items: Vec<ResolvedMenuItem>,
    pub tab_id: Option<TabId>,
    /// Id of a submenu entry to show open as the menu appears.
    pub open_submenu: Option<String>,
}

/// Active HTML `<select>` dropdown popup state held by [`MenuViewState`].
#[derive(Clone, Debug, PartialEq)]
pub struct OpenSelectDropdown {
    pub position: (f32, f32),
    pub minimum_width: f32,
    pub items: Vec<SelectDropdownItem>,
    pub tab_id: Option<TabId>,
}

/// View-local state owning all open menus and the About dialog for a window.
///
/// Provided once at the window root via [`use_provide_menu_state`] and consumed by toolbar buttons (such as the app
/// menu trigger), Hub message handlers, and [`crate::ui::chrome::menus::MenuLayer`].
#[derive(Clone, Copy, PartialEq)]
pub struct MenuViewState {
    pub app_menu_open: State<bool>,
    pub app_menu_anchor: State<AnchorRect>,
    /// Whether the extensions menu (every installed extension, like Firefox's puzzle-piece panel) is open.
    pub extensions_menu_open: State<bool>,
    pub extensions_menu_anchor: State<AnchorRect>,
    /// Whether the sidebar bottom `+` Create New popup (`#zenCreateNewPopup`) is open.
    pub create_new_open: State<bool>,
    pub create_new_anchor: State<AnchorRect>,
    pub about_open: State<bool>,
    pub context_menu: State<Option<OpenContextMenu>>,
    pub select_dropdown: State<Option<OpenSelectDropdown>>,
    pub select_seq: State<u64>,
}

impl MenuViewState {
    /// Toggles the application menu, updating its anchor rectangle when provided.
    pub fn toggle_app_menu(&self, anchor: Option<AnchorRect>) {
        let mut open = self.app_menu_open;
        let next = !*open.peek();
        if next {
            if let Some(rect) = anchor {
                let mut anchor_state = self.app_menu_anchor;
                anchor_state.set(rect);
            }
            self.close_extensions_menu();
            self.close_create_new();
            self.close_context_menu();
            self.close_select_dropdown();
            self.close_about();
        }
        open.set(next);
    }

    /// Toggles the sidebar `+` Create New popup, closing any other open menu.
    pub fn toggle_create_new(&self, anchor: Option<AnchorRect>) {
        let mut open = self.create_new_open;
        let next = !*open.peek();
        if next {
            if let Some(rect) = anchor {
                let mut anchor_state = self.create_new_anchor;
                anchor_state.set(rect);
            }
            self.close_app_menu();
            self.close_extensions_menu();
            self.close_context_menu();
            self.close_select_dropdown();
            self.close_about();
        }
        open.set(next);
    }

    /// Closes the sidebar `+` Create New popup.
    pub fn close_create_new(&self) {
        let mut open = self.create_new_open;
        open.set_if_modified(false);
    }

    /// Opens the application menu anchored to `anchor`.
    #[allow(dead_code)]
    pub fn open_app_menu(&self, anchor: AnchorRect) {
        let mut anchor_state = self.app_menu_anchor;
        anchor_state.set(anchor);
        self.close_extensions_menu();
        self.close_create_new();
        self.close_context_menu();
        self.close_select_dropdown();
        self.close_about();
        let mut open = self.app_menu_open;
        open.set(true);
    }

    /// Closes the application menu.
    pub fn close_app_menu(&self) {
        let mut open = self.app_menu_open;
        open.set_if_modified(false);
    }

    /// Toggles the extensions menu, anchored where the extensions toolbar button last reported itself.
    pub fn toggle_extensions_menu(&self) {
        let mut open = self.extensions_menu_open;
        let next = !*open.peek();
        if next {
            self.close_app_menu();
            self.close_create_new();
            self.close_context_menu();
            self.close_select_dropdown();
            self.close_about();
        }
        open.set(next);
    }

    /// Closes the extensions menu.
    pub fn close_extensions_menu(&self) {
        let mut open = self.extensions_menu_open;
        open.set_if_modified(false);
    }

    /// Opens the About Zendia modal dialog, closing transient menus first.
    pub fn open_about(&self) {
        self.close_app_menu();
        self.close_extensions_menu();
        self.close_create_new();
        self.close_context_menu();
        self.close_select_dropdown();
        let mut open = self.about_open;
        open.set(true);
    }

    /// Closes the About Zendia modal dialog.
    pub fn close_about(&self) {
        let mut open = self.about_open;
        open.set_if_modified(false);
    }

    /// Opens a context menu at its requested window coordinates, closing any other open menu.
    pub fn open_context_menu(&self, menu: OpenContextMenu) {
        self.close_app_menu();
        self.close_extensions_menu();
        self.close_create_new();
        self.close_select_dropdown();
        self.close_about();
        let mut state = self.context_menu;
        state.set(Some(menu));
    }

    /// Closes any open context menu.
    pub fn close_context_menu(&self) {
        let mut state = self.context_menu;
        if state.peek().is_some() {
            state.set(None);
        }
    }

    /// Opens a `<select>` dropdown popup, closing any other open menu.
    pub fn open_select_dropdown(&self, dropdown: OpenSelectDropdown) {
        self.close_app_menu();
        self.close_extensions_menu();
        self.close_create_new();
        self.close_context_menu();
        self.close_about();
        let mut seq = self.select_seq;
        let next_seq = seq.peek().wrapping_add(1);
        seq.set(next_seq);
        let mut state = self.select_dropdown;
        state.set(Some(dropdown));
    }

    /// Closes any open `<select>` dropdown popup.
    pub fn close_select_dropdown(&self) {
        let mut state = self.select_dropdown;
        if state.peek().is_some() {
            state.set(None);
        }
    }

    /// Closes every open menu and dialog.
    #[allow(dead_code)]
    pub fn close_all(&self) {
        self.close_app_menu();
        self.close_extensions_menu();
        self.close_create_new();
        self.close_context_menu();
        self.close_select_dropdown();
        self.close_about();
    }

    /// Returns true if any menu or the About dialog is currently open.
    #[allow(dead_code)]
    pub fn has_open_overlay(&self) -> bool {
        *self.app_menu_open.read()
            || *self.extensions_menu_open.read()
            || *self.create_new_open.read()
            || *self.about_open.read()
            || self.context_menu.read().is_some()
            || self.select_dropdown.read().is_some()
    }
}

/// Installs [`MenuViewState`] into Freya context for the current window.
pub fn use_provide_menu_state() -> MenuViewState {
    let app_menu_open = use_state(|| false);
    let app_menu_anchor = use_state(AnchorRect::default);
    let extensions_menu_open = use_state(|| false);
    let extensions_menu_anchor = use_state(AnchorRect::default);
    let create_new_open = use_state(|| false);
    let create_new_anchor = use_state(AnchorRect::default);
    let about_open = use_state(|| false);
    let context_menu = use_state(|| None::<OpenContextMenu>);
    let select_dropdown = use_state(|| None::<OpenSelectDropdown>);
    let select_seq = use_state(|| 0u64);

    use_provide_context(|| MenuViewState {
        app_menu_open,
        app_menu_anchor,
        extensions_menu_open,
        extensions_menu_anchor,
        create_new_open,
        create_new_anchor,
        about_open,
        context_menu,
        select_dropdown,
        select_seq,
    })
}

/// Consumes the window's [`MenuViewState`] from Freya context.
pub fn use_menu_state() -> MenuViewState {
    use_consume::<MenuViewState>()
}

/// Consumes [`MenuViewState`] if a provider is installed in this window.
pub fn try_use_menu_state() -> Option<MenuViewState> {
    try_consume_context::<MenuViewState>()
}

/// Parses and invokes a menu entry's command through [`CoreView`].
pub fn invoke_menu_command(core: &CoreView, command: &str, args: &[(String, String)]) -> bool {
    if command.is_empty() {
        return false;
    }
    match Command::from_name_and_args(command, args) {
        Ok(parsed) => core.invoke_or_log(parsed),
        Err(error) => {
            eprintln!("zendia: menu command `{command}` failed to parse: {error}");
            false
        }
    }
}

/// Clamps a popup's top-left origin so it remains fully inside `window_size` with `margin` padding.
///
/// When `desired` would overflow the right or bottom edge, the popup shifts left or up (preferring to flip across the
/// click point when space permits) and never extends past the window origin.
pub fn clamp_popup_origin(
    desired: (f32, f32),
    popup_size: (f32, f32),
    window_size: (f32, f32),
    margin: f32,
) -> (f32, f32) {
    let max_x = (window_size.0 - popup_size.0 - margin).max(margin);
    let max_y = (window_size.1 - popup_size.1 - margin).max(margin);

    let x = if desired.0 + popup_size.0 + margin > window_size.0 {
        let flipped = desired.0 - popup_size.0;
        if flipped >= margin {
            flipped
        } else {
            desired.0.clamp(margin, max_x)
        }
    } else {
        desired.0.max(margin)
    };

    let y = if desired.1 + popup_size.1 + margin > window_size.1 {
        let flipped = desired.1 - popup_size.1;
        if flipped >= margin {
            flipped
        } else {
            desired.1.clamp(margin, max_y)
        }
    } else {
        desired.1.max(margin)
    };

    (x.clamp(margin, max_x), y.clamp(margin, max_y))
}

/// Computes the anchored origin for the application menu below (or above when near the bottom) `anchor`, right-aligned
/// when anchored on the right side of the window and clamped inside `window_size`.
pub fn compute_app_menu_origin(anchor: AnchorRect, popup_size: (f32, f32), window_size: (f32, f32)) -> (f32, f32) {
    anchor_popup_origin(anchor, popup_size, window_size, PopupHorizontalAlign::Auto, 6.0, 8.0)
}

/// Places a popup under its anchor, or above it when there is no room below (a button at the sidebar's bottom edge),
/// keeping it inside the window.
pub fn compute_popup_origin_above_or_below(
    anchor: AnchorRect,
    popup_size: (f32, f32),
    window_size: (f32, f32),
) -> (f32, f32) {
    anchor_popup_origin(anchor, popup_size, window_size, PopupHorizontalAlign::Auto, 6.0, 8.0)
}

/// Computes the floating origin for a cascading submenu next to a parent menu row, flipping left if near the right edge.
pub fn compute_submenu_origin(
    parent_origin: (f32, f32),
    parent_width: f32,
    row_y_offset: f32,
    submenu_size: (f32, f32),
    window_size: (f32, f32),
) -> (f32, f32) {
    let margin = 6.0;
    let right_x = parent_origin.0 + parent_width - 4.0;
    let left_x = parent_origin.0 - submenu_size.0 + 4.0;
    let x = if right_x + submenu_size.0 + margin <= window_size.0 {
        right_x
    } else if left_x >= margin {
        left_x
    } else {
        right_x.clamp(margin, (window_size.0 - submenu_size.0 - margin).max(margin))
    };

    let desired_y = parent_origin.1 + row_y_offset - 4.0;
    let max_y = (window_size.1 - submenu_size.1 - margin).max(margin);
    (x, desired_y.clamp(margin, max_y))
}

/// Returns the next enabled, non-separator item index in `items` moving by `delta` (`-1` or `+1`), wrapping around.
pub fn next_selectable_index(items: &[ResolvedMenuItem], current: Option<usize>, delta: isize) -> Option<usize> {
    let len = items.len();
    if len == 0 {
        return None;
    }
    let is_selectable = |idx: usize| !items[idx].is_separator() && items[idx].enabled;
    let start = match current {
        Some(idx) => ((idx as isize + delta).rem_euclid(len as isize)) as usize,
        None => {
            if delta >= 0 {
                0
            } else {
                len - 1
            }
        }
    };
    for step in 0..len {
        let idx = ((start as isize + delta * step as isize).rem_euclid(len as isize)) as usize;
        if is_selectable(idx) {
            return Some(idx);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_popup_origin_flips_and_clamps_within_window() {
        // Fits comfortably at click point.
        assert_eq!(
            clamp_popup_origin((100.0, 120.0), (220.0, 200.0), (1600.0, 1000.0), 6.0),
            (100.0, 120.0)
        );
        // Near right & bottom edges: flips left and up.
        assert_eq!(
            clamp_popup_origin((1520.0, 920.0), (220.0, 200.0), (1600.0, 1000.0), 6.0),
            (1300.0, 720.0)
        );
        // Oversized popup clamps to margin without negative coordinates.
        assert_eq!(
            clamp_popup_origin((10.0, 10.0), (500.0, 400.0), (300.0, 200.0), 6.0),
            (6.0, 6.0)
        );
    }

    #[test]
    fn popup_opens_above_a_button_at_the_bottom_edge() {
        let window = (1280.0, 860.0);
        let top_button = AnchorRect {
            x: 1200.0,
            y: 8.0,
            width: 28.0,
            height: 28.0,
        };
        // Top-right anchor right-aligns the popup to the button's right edge (1200 + 28 - 320 = 908).
        assert_eq!(
            compute_popup_origin_above_or_below(top_button, (320.0, 300.0), window),
            (908.0, 42.0)
        );
        let bottom_button = AnchorRect {
            x: 40.0,
            y: 820.0,
            width: 28.0,
            height: 28.0,
        };
        assert_eq!(
            compute_popup_origin_above_or_below(bottom_button, (320.0, 300.0), window),
            (40.0, 514.0)
        );
    }

    #[test]
    fn submenu_origin_flips_left_when_near_right_window_edge() {
        let right = compute_submenu_origin((200.0, 100.0), 220.0, 30.0, (200.0, 120.0), (1600.0, 1000.0));
        assert_eq!(right, (416.0, 126.0));

        let flipped = compute_submenu_origin((1300.0, 100.0), 220.0, 30.0, (200.0, 120.0), (1600.0, 1000.0));
        assert_eq!(flipped, (1104.0, 126.0));
    }

    #[test]
    fn next_selectable_index_skips_separators_and_disabled_items() {
        let items = vec![
            ResolvedMenuItem::action("a", "A", "cmd.a"),
            ResolvedMenuItem::separator("sep"),
            ResolvedMenuItem::action("b", "B", "cmd.b").with_enabled(false),
            ResolvedMenuItem::action("c", "C", "cmd.c"),
        ];
        assert_eq!(next_selectable_index(&items, None, 1), Some(0));
        assert_eq!(next_selectable_index(&items, Some(0), 1), Some(3));
        assert_eq!(next_selectable_index(&items, Some(3), 1), Some(0));
        assert_eq!(next_selectable_index(&items, Some(0), -1), Some(3));
    }
}
