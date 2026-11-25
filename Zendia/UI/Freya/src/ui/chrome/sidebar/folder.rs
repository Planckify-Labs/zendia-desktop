/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Collapsible tab folders with Zen's 180 ms expand/collapse motion, nested indented tabs, active-child visibility
//! when collapsed, inline folder rename, and right-click folder context menu.

use crate::ui::chrome::layout::{ChromePalette, TAB_ROW_HEIGHT, TAB_ROW_RADIUS};
use crate::ui::chrome::menus::{open_folder_context_menu, try_use_menu_state};
use crate::ui::chrome::sidebar::state::try_use_sidebar_state;
use crate::ui::chrome::sidebar::tab_row::{InlineRenameField, VerticalTabRow};
use crate::ui::core_view::use_core;
use crate::ui::theme::{ThemeRole, motion, use_theme};
use crate::ui::vim::explorer::{apply_explorer_background, apply_explorer_border};
use crate::ui::vim::hints::hint_badge_for_row;
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::focus::FocusTarget;
use zendia_browser_core::model::{Folder, Tab, TabId, TabMoveTarget};

/// Folder expand/collapse duration matching Zen's `ZenFolders.mjs` (`0.18s`).
pub const FOLDER_TOGGLE_DURATION: Duration = Duration::from_millis(180);
/// Horizontal indentation of tabs nested inside a folder (matching `--zen-folder-indent: 14px`).
pub const FOLDER_INDENT_PX: f32 = 14.0;
const FOLDER_CHILD_SPACING: f32 = 2.0;

const ICON_FOLDER_CLOSED_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none"><path d="M3.2 5.2A1.7 1.7 0 0 1 4.9 3.5h3.05c.46 0 .9.18 1.22.5l.83.83c.32.32.76.5 1.22.5h3.88a1.7 1.7 0 0 1 1.7 1.7v7.8A1.7 1.7 0 0 1 15.1 16.5H4.9a1.7 1.7 0 0 1-1.7-1.7V5.2Z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M3.2 7.6h13.6" stroke="currentColor" stroke-opacity="0.45" stroke-width="1.2"/></svg>"#;

const ICON_FOLDER_OPEN_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none"><path d="M3.2 5.2A1.7 1.7 0 0 1 4.9 3.5h2.9c.45 0 .88.18 1.2.5l.8.8c.32.32.75.5 1.2.5h3.9a1.6 1.6 0 0 1 1.6 1.6v1.4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/><path d="M2.5 8.4c.16-.7.78-1.2 1.5-1.2h12.3c.88 0 1.55.8 1.38 1.66l-1.18 6.1A1.6 1.6 0 0 1 14.92 16.3H4.68a1.6 1.6 0 0 1-1.56-1.26L2.5 8.4Z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" fill="currentColor" fill-opacity="0.14"/></svg>"#;

/// One collapsible folder row and its animated nested tab list.
#[derive(Clone, PartialEq)]
pub struct FolderGroup {
    pub folder: Folder,
    pub tabs: Vec<Tab>,
    pub active_tab: Option<TabId>,
    pub collapsed_sidebar: bool,
    key: DiffKey,
}

impl KeyExt for FolderGroup {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl FolderGroup {
    pub fn new(folder: Folder, tabs: Vec<Tab>, active_tab: Option<TabId>, collapsed_sidebar: bool) -> Self {
        let key = DiffKey::from(&folder.id.0);
        Self {
            folder,
            tabs,
            active_tab,
            collapsed_sidebar,
            key,
        }
    }
}

impl Component for FolderGroup {
    fn render(&self) -> impl IntoElement {
        let is_expanded = !self.folder.collapsed;
        let active_child = self.active_tab.and_then(|active_id| {
            self.tabs
                .iter()
                .enumerate()
                .find(|(_, tab)| tab.id == active_id)
                .map(|(idx, tab)| (idx, tab.clone()))
        });

        let expand_anim = use_animation_with_dependencies(&is_expanded, move |conf, expanded| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *expanded { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(FOLDER_TOGGLE_DURATION)
                .ease(Ease::InOut)
                .function(motion::ease_out_fn())
        });

        let factor = expand_anim.get().value().clamp(0.0, 1.0);
        let folder_id = self.folder.id;
        let active_tab = self.active_tab;
        let collapsed_sidebar = self.collapsed_sidebar;
        let indent = if collapsed_sidebar { 0.0 } else { FOLDER_INDENT_PX };

        let full_height = if self.tabs.is_empty() {
            0.0
        } else {
            self.tabs.len() as f32 * TAB_ROW_HEIGHT
                + (self.tabs.len().saturating_sub(1)) as f32 * FOLDER_CHILD_SPACING
                + 2.0
        };

        let header = FolderHeaderRow {
            folder: self.folder.clone(),
            has_active_child: active_child.is_some(),
            collapsed_sidebar,
            tab_count: self.tabs.len(),
        };

        let children_section: Option<Element> = if factor > 0.01 {
            let current_height = (full_height * factor).max(0.0);
            let rows = self.tabs.iter().enumerate().map(|(idx, tab)| {
                VerticalTabRow::new(
                    tab.clone(),
                    active_tab == Some(tab.id),
                    collapsed_sidebar,
                    indent,
                    TabMoveTarget::Folder { folder_id, index: idx },
                )
                .into_element()
            });
            Some(
                rect()
                    .width(Size::fill())
                    .height(Size::px(current_height))
                    .overflow(Overflow::Clip)
                    .padding(Gaps::new(2.0, 0.0, 0.0, 0.0))
                    .spacing(FOLDER_CHILD_SPACING)
                    .opacity(factor)
                    .children(rows)
                    .into_element(),
            )
        } else if let Some((idx, active_tab_item)) = active_child {
            // Zen behavior: a collapsed folder that contains the active tab keeps that active tab visible below it.
            Some(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(2.0, 0.0, 0.0, 0.0))
                    .child(VerticalTabRow::new(
                        active_tab_item,
                        true,
                        collapsed_sidebar,
                        indent,
                        TabMoveTarget::Folder { folder_id, index: idx },
                    ))
                    .into_element(),
            )
        } else {
            None
        };

        rect().width(Size::fill()).child(header).maybe_child(children_section)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct FolderHeaderRow {
    folder: Folder,
    has_active_child: bool,
    collapsed_sidebar: bool,
    tab_count: usize,
}

impl Component for FolderHeaderRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let menu_state = try_use_menu_state();
        let sidebar_state = try_use_sidebar_state();
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let folder_id = self.folder.id;
        let is_renaming = sidebar_state.is_some_and(|s| *s.renaming_folder.read() == Some(folder_id));
        let is_drag_target = sidebar_state.is_some_and(|s| {
            matches!(
                s.drag_state.read().as_ref().and_then(|d| d.target),
                Some(TabMoveTarget::Folder { folder_id: target_id, .. }) if target_id == folder_id
            )
        });

        let toggle_core = core.clone();
        let on_press = move |_: Event<PressEventData>| {
            if is_renaming {
                return;
            }
            toggle_core.invoke_or_log(Command::FolderToggleCollapsed { folder_id });
        };

        let menu_core = core.clone();
        let on_mouse_down = move |e: Event<MouseEventData>| {
            if e.button == Some(MouseButton::Right) {
                e.stop_propagation();
                if let Some(menu_state) = &menu_state {
                    let model = menu_core.model().peek().clone();
                    let position = (e.global_location.x as f32, e.global_location.y as f32);
                    open_folder_context_menu(menu_state, &model, folder_id, position);
                }
            }
        };

        let (svg_key, svg_bytes) = if self.folder.collapsed {
            ("zendia-folder-closed", ICON_FOLDER_CLOSED_SVG)
        } else {
            ("zendia-folder-open", ICON_FOLDER_OPEN_SVG)
        };
        let icon_color = if self.has_active_child || !self.folder.collapsed || hovered() {
            palette.text
        } else {
            palette.text_muted
        };

        let folder_icon = SvgViewer::new((svg_key, Bytes::from_static(svg_bytes)))
            .width(Size::px(18.0))
            .height(Size::px(18.0))
            .color(icon_color)
            .show_loader(false);

        let bg = if is_renaming {
            palette.tab_selected_bg
        } else if hovered() {
            palette.tab_hover_bg
        } else {
            Color::TRANSPARENT
        };

        let border = if is_drag_target {
            Border::new()
                .width(1.25)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::Accent))
        } else if focus() == Focus::Keyboard {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::FocusRing))
        } else {
            Border::new().width(0.0).fill(Color::TRANSPARENT)
        };
        let focus_target = FocusTarget::Folder(folder_id);
        let (bg, border, hint_badge) = {
            let model = core.model().read();
            (
                apply_explorer_background(bg, &model, &focus_target, &theme),
                apply_explorer_border(border, &model, &focus_target, &theme),
                hint_badge_for_row(&model, &focus_target, TAB_ROW_HEIGHT, &theme),
            )
        };

        let tab_count = self.tab_count;
        let state_label = if self.folder.collapsed {
            format!("Folder {} (collapsed, {} tabs)", self.folder.name, tab_count)
        } else {
            format!("Folder {} (expanded, {} tabs)", self.folder.name, tab_count)
        };

        if self.collapsed_sidebar {
            return rect()
                .width(Size::fill())
                .center()
                .child(
                    rect()
                        .a11y_id(a11y_id)
                        .a11y_focusable(true)
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt(state_label)
                        .width(Size::px(38.0))
                        .height(Size::px(TAB_ROW_HEIGHT))
                        .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
                        .background(bg)
                        .border(border)
                        .center()
                        .cursor(CursorIcon::Pointer)
                        .on_pointer_enter(move |_| {
                            hovered.set_if_modified(true);
                            if let Some(state) = sidebar_state {
                                state.set_drag_target(TabMoveTarget::Folder { folder_id, index: 0 });
                            }
                        })
                        .on_pointer_leave(move |_| hovered.set_if_modified(false))
                        .on_press(on_press)
                        .on_mouse_down(on_mouse_down)
                        .child(folder_icon)
                        .maybe_child(hint_badge),
                )
                .into_element();
        }

        let center_el: Element = if is_renaming && let Some(state) = sidebar_state {
            InlineRenameField {
                draft: state.rename_draft,
                placeholder: "Folder name",
                on_commit: EventHandler::new({
                    let core = core.clone();
                    move |text: String| {
                        state.cancel_rename();
                        let trimmed = text.trim().to_string();
                        if !trimmed.is_empty() {
                            core.invoke_or_log(Command::FolderRename {
                                folder_id,
                                name: trimmed,
                            });
                        }
                    }
                }),
                on_cancel: EventHandler::new(move |_| {
                    state.cancel_rename();
                }),
                key: DiffKey::from(&("folder-rename", folder_id.0)),
            }
            .into_element()
        } else {
            label()
                .text(self.folder.name.clone())
                .width(Size::flex(1.0))
                .font_size(12.5)
                .font_weight(FontWeight::SEMI_BOLD)
                .color(palette.text)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .into_element()
        };

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(!is_renaming)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(state_label)
            .width(Size::fill())
            .height(Size::px(TAB_ROW_HEIGHT))
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .spacing(8.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
            .background(bg)
            .border(border)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| {
                hovered.set_if_modified(true);
                if let Some(state) = sidebar_state {
                    state.set_drag_target(TabMoveTarget::Folder { folder_id, index: 0 });
                }
            })
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(on_press)
            .on_mouse_down(on_mouse_down)
            .child(folder_icon)
            .child(center_el)
            .maybe_child(hint_badge)
            .into_element()
    }
}
