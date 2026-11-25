/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Vertical sidebar frame and all phase 2 sidebar surfaces: essentials grid, space header, pinned tabs, collapsible
//! folders, pinned separator with "Clear", vertical tabs with inline renamer & drag reorder, sidebar notification
//! card, space switcher, and interactive width resize handle.

pub mod essentials;
pub mod folder;
pub mod media_controls;
pub mod notification;
pub mod pinned_separator;
pub mod spaces;
pub mod split_group;
pub mod state;
pub mod tab_row;

pub use state::{PageDrop, SidebarDragState, SidebarViewState, try_use_sidebar_state, use_provide_sidebar_state};

use crate::ui::chrome::layout::{
    ChromePalette, RESIZE_HANDLE_WIDTH, SIDEBAR_EXPAND_DURATION, SIDEBAR_PADDING_COLLAPSED, SIDEBAR_PADDING_EXPANDED,
    SIDEBAR_TOP_ROW_HEIGHT, ToolbarSlots, compute_resized_sidebar_width, show_window_controls_in_sidebar_top,
};
use crate::ui::chrome::sidebar::essentials::EssentialsGrid;
use crate::ui::chrome::sidebar::folder::FolderGroup;
use crate::ui::chrome::sidebar::media_controls::SidebarMediaControls;
use crate::ui::chrome::sidebar::notification::SidebarNotificationCard;
use crate::ui::chrome::sidebar::pinned_separator::PinnedSeparator;
use crate::ui::chrome::sidebar::spaces::{SpaceHeader, SpaceSwitcher};
use crate::ui::chrome::sidebar::split_group::SplitGroupRow;
use crate::ui::chrome::sidebar::tab_row::{DragGhostOverlay, NewTabRow, VerticalTabRow};
use crate::ui::chrome::toolbar::{ToolbarItemView, WindowControls};
use crate::ui::core_view::use_core;
use crate::ui::spaces::{
    SpaceAddSwipeOverlay, SpaceCreationForm, SpaceSwipeIndicator, SpaceSwipeMode, handle_sidebar_wheel,
    try_use_space_editing_state,
};
use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{SIDEBAR_COLLAPSED_WIDTH, SidebarSide, TabMoveTarget};

/// Vertical sidebar frame hosting top controls (single toolbar), essentials, space header, pinned/folder/normal tabs,
/// sidebar notification card, space switcher, and bottom actions.
#[derive(Clone, PartialEq)]
pub struct SidebarFrame {
    pub slots: ToolbarSlots,
    pub location: State<String>,
    pub location_a11y_id: AccessibilityId,
    pub interactive_window_controls: bool,
    pub is_resizing: State<bool>,
}

impl Component for SidebarFrame {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let _palette = ChromePalette::for_scheme(theme.scheme);
        let sidebar_state = try_use_sidebar_state();
        let space_editing = try_use_space_editing_state();
        let create_form_open = space_editing.is_some_and(|s| *s.create_form_open.read());
        let swipe = space_editing.map(|s| s.swipe.read().clone()).unwrap_or_default();

        let (
            layout,
            active_space,
            spaces,
            essentials,
            pinned_tabs,
            folders_with_tabs,
            normal_tabs,
            active_tab_id,
            notifications,
            show_essentials_promo,
            split_groups,
        ) = {
            let model = core.model().read();
            let active_space_id = model.active_space;
            let active_space = model.active_space().cloned();
            let essentials = model.essential_tabs().into_iter().cloned().collect::<Vec<_>>();
            let pinned_tabs: Vec<_> = active_space_id
                .map(|id| model.pinned_tabs_in_space(id).into_iter().cloned().collect())
                .unwrap_or_default();
            let folders_with_tabs = active_space_id
                .map(|id| {
                    model
                        .folders_in_space(id)
                        .into_iter()
                        .map(|folder| {
                            let tabs = model.folder_tabs(folder.id).into_iter().cloned().collect::<Vec<_>>();
                            (folder.clone(), tabs)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let normal_tabs: Vec<_> = active_space_id
                .map(|id| model.normal_tabs_in_space(id).into_iter().cloned().collect())
                .unwrap_or_default();
            (
                model.layout.clone(),
                active_space,
                model.spaces.clone(),
                essentials,
                pinned_tabs,
                folders_with_tabs,
                normal_tabs,
                model.active_tab,
                model.sidebar_notifications.clone(),
                model.show_essentials_promo,
                model.split_groups.clone(),
            )
        };

        let expanded = layout.sidebar_expanded;
        let expanded_width = layout.sidebar_width;
        let resizing = *self.is_resizing.read();

        let expand_anim = use_animation_with_dependencies(&expanded, move |conf, is_expanded| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *is_expanded { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(SIDEBAR_EXPAND_DURATION)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let anim_factor = expand_anim.get().value().clamp(0.0, 1.0);
        let displayed_width = if resizing {
            layout.effective_sidebar_width()
        } else {
            SIDEBAR_COLLAPSED_WIDTH + (expanded_width - SIDEBAR_COLLAPSED_WIDTH) * anim_factor
        };

        let is_collapsed_visual = !expanded && anim_factor < 0.35 && !create_form_open;
        let show_top_row = !layout.has_top_toolbar() && !is_collapsed_visual;
        let show_url_bar = !self.slots.sidebar_url_bar.is_empty() && !is_collapsed_visual && !create_form_open;
        let show_sidebar_window_controls = show_window_controls_in_sidebar_top(&layout);
        let active_space_id = active_space.as_ref().map(|s| s.id);

        let location = self.location;
        let location_a11y_id = self.location_a11y_id;

        let sidebar_top_row = show_top_row.then(|| {
            let tight_top = show_sidebar_window_controls && expanded_width < 230.0;
            let start_buttons = self.slots.sidebar_top_start.iter().map(|item| {
                ToolbarItemView::new(item.clone(), location, location_a11y_id)
                    .dense(true)
                    .tight(tight_top)
                    .key(item.id.as_str().to_string())
                    .into_element()
            });
            let end_buttons = self.slots.sidebar_top_end.iter().map(|item| {
                ToolbarItemView::new(item.clone(), location, location_a11y_id)
                    .dense(true)
                    .tight(tight_top)
                    .key(item.id.as_str().to_string())
                    .into_element()
            });

            let row = rect()
                .width(Size::fill())
                .height(Size::px(SIDEBAR_TOP_ROW_HEIGHT))
                .padding(Gaps::new(2.0, 1.0, 0.0, 1.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .main_align(Alignment::SpaceBetween)
                .cross_align(Alignment::center())
                .child(
                    rect()
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(if tight_top { 1.0 } else { 2.0 })
                        .children(start_buttons),
                )
                .child(
                    rect()
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(1.0)
                        .children(end_buttons)
                        .maybe_child(show_sidebar_window_controls.then_some(WindowControls {
                            interactive: self.interactive_window_controls,
                            compact: true,
                        })),
                );
            if self.interactive_window_controls {
                row.window_drag()
            } else {
                row
            }
        });

        let url_bar_row = show_url_bar.then(|| {
            rect()
                .width(Size::fill())
                .padding(Gaps::new(2.0, 2.0, 4.0, 2.0))
                .children(self.slots.sidebar_url_bar.iter().map(|item| {
                    ToolbarItemView::new(item.clone(), location, location_a11y_id)
                        .compact_urlbar(true)
                        .key(item.id.as_str().to_string())
                        .into_element()
                }))
        });

        let essentials_section = EssentialsGrid {
            tabs: essentials,
            active_tab: active_tab_id,
            collapsed: is_collapsed_visual,
            show_promo: show_essentials_promo,
        };

        let space_header = SpaceHeader {
            space: active_space,
            collapsed: is_collapsed_visual,
        };

        let has_normal_tabs = !normal_tabs.is_empty();
        let pinned_rows = pinned_tabs.into_iter().enumerate().map(|(idx, tab)| {
            VerticalTabRow::new(
                tab.clone(),
                active_tab_id == Some(tab.id),
                is_collapsed_visual,
                0.0,
                TabMoveTarget::Pinned {
                    space_id: active_space_id,
                    index: idx,
                },
            )
            .into_element()
        });

        let folder_rows = folders_with_tabs
            .into_iter()
            .map(|(folder, tabs)| FolderGroup::new(folder, tabs, active_tab_id, is_collapsed_visual).into_element());

        let separator = PinnedSeparator {
            space_id: active_space_id,
            has_normal_tabs,
            collapsed: is_collapsed_visual,
        };

        let new_tab_row = NewTabRow {
            collapsed: is_collapsed_visual,
        };

        // A split group draws as one row where its first tab sits; its other tabs are left out of the list.
        let mut drawn_groups = Vec::new();
        let mut normal_rows = Vec::new();
        for (idx, tab) in normal_tabs.iter().enumerate() {
            if let Some(group) = split_groups.iter().find(|group| group.tab_ids.contains(&tab.id)) {
                if drawn_groups.contains(&group.id) {
                    continue;
                }
                drawn_groups.push(group.id);
                let tabs = group
                    .tab_ids
                    .iter()
                    .filter_map(|id| normal_tabs.iter().find(|tab| tab.id == *id).cloned())
                    .collect();
                normal_rows.push(
                    SplitGroupRow {
                        group: group.clone(),
                        tabs,
                        active_tab: active_tab_id,
                        collapsed: is_collapsed_visual,
                    }
                    .into_element(),
                );
                continue;
            }
            normal_rows.push(
                VerticalTabRow::new(
                    tab.clone(),
                    active_tab_id == Some(tab.id),
                    is_collapsed_visual,
                    0.0,
                    TabMoveTarget::Normal {
                        space_id: active_space_id,
                        index: idx,
                    },
                )
                .into_element(),
            );
        }

        let notification_stack = (!is_collapsed_visual && !notifications.is_empty()).then(|| {
            rect()
                .width(Size::fill())
                .padding(Gaps::new(4.0, 2.0, 4.0, 2.0))
                .spacing(6.0)
                .children(
                    notifications
                        .into_iter()
                        .map(|notif| SidebarNotificationCard::new(notif).into_element()),
                )
        });

        let bottom_items = self.slots.sidebar_bottom_for_side(layout.sidebar_side);
        let show_space_switcher = spaces.len() > 1;
        let tight_bottom = !layout.has_top_toolbar() && expanded_width < 220.0;

        let swipe_target_meta = swipe.target_space_id.and_then(|tid| {
            spaces
                .iter()
                .find(|s| s.id == tid)
                .map(|s| (s.name.clone(), s.icon.clone()))
        });
        let side_factor = if layout.sidebar_side == SidebarSide::Right {
            -1.0
        } else {
            1.0
        };
        let swipe_strip_offset = match swipe.mode {
            SpaceSwipeMode::SwitchSpace => swipe.offset_x * 0.45,
            SpaceSwipeMode::AddSpace => -swipe.add_progress * displayed_width * 0.25 * side_factor,
            SpaceSwipeMode::Idle => 0.0,
        };

        let wheel_core = core.clone();
        let on_sidebar_wheel = move |e: Event<WheelEventData>| {
            if let Some(editing) = space_editing {
                handle_sidebar_wheel(&e, &wheel_core, editing, displayed_width);
            }
        };

        let bottom_bar = if is_collapsed_visual {
            rect()
                .width(Size::fill())
                .padding(Gaps::new(4.0, 0.0, 2.0, 0.0))
                .spacing(4.0)
                .cross_align(Alignment::center())
                .maybe_child(show_space_switcher.then_some(SpaceSwitcher {
                    spaces,
                    active_space: active_space_id,
                    compact: true,
                    vertical: true,
                }))
                .children(self.slots.sidebar_bottom.iter().map(|item| {
                    ToolbarItemView::new(item.clone(), location, location_a11y_id)
                        .dense(true)
                        .key(item.id.as_str().to_string())
                        .into_element()
                }))
        } else {
            let first_item = bottom_items.first().cloned();
            let trailing_items: Vec<_> = bottom_items.into_iter().skip(1).collect();

            rect()
                .width(Size::fill())
                .height(Size::px(36.0))
                .padding(Gaps::new(2.0, 1.0, 0.0, 1.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .main_align(Alignment::SpaceBetween)
                .cross_align(Alignment::center())
                .spacing(if tight_bottom { 2.0 } else { 4.0 })
                .maybe_child(first_item.map(|item| {
                    ToolbarItemView::new(item.clone(), location, location_a11y_id)
                        .dense(true)
                        .tight(tight_bottom)
                        .key(item.id.as_str().to_string())
                }))
                .maybe_child(show_space_switcher.then_some(SpaceSwitcher {
                    spaces,
                    active_space: active_space_id,
                    compact: tight_bottom,
                    vertical: false,
                }))
                .child(
                    rect()
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(if tight_bottom { 2.0 } else { 4.0 })
                        .children(trailing_items.into_iter().map(|item| {
                            ToolbarItemView::new(item.clone(), location, location_a11y_id)
                                .dense(true)
                                .tight(tight_bottom)
                                .key(item.id.as_str().to_string())
                                .into_element()
                        })),
                )
        };

        let active_drag = sidebar_state.and_then(|s| s.drag_state.read().clone());

        let pad = if is_collapsed_visual {
            SIDEBAR_PADDING_COLLAPSED
        } else {
            SIDEBAR_PADDING_EXPANDED
        };
        let top_pad = if layout.has_top_toolbar() && !layout.compact_hides_toolbar() {
            0.0
        } else {
            4.0
        };

        let sidebar_body: Element = if let Some(editing) = space_editing.filter(|_| create_form_open) {
            SpaceCreationForm { state: editing }.into_element()
        } else {
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .content(Content::Flex)
                .overflow(Overflow::Clip)
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::fill())
                        .content(Content::Flex)
                        .offset_x(swipe_strip_offset)
                        .child(essentials_section)
                        .child(space_header)
                        .child(
                            ScrollView::new().width(Size::fill()).height(Size::flex(1.0)).child(
                                rect()
                                    .width(Size::fill())
                                    .padding(Gaps::new(2.0, 0.0, 4.0, 0.0))
                                    .spacing(2.0)
                                    .cross_align(if is_collapsed_visual {
                                        Alignment::center()
                                    } else {
                                        Alignment::Start
                                    })
                                    .children(pinned_rows)
                                    .children(folder_rows)
                                    .child(separator)
                                    .child(new_tab_row)
                                    .children(normal_rows),
                            ),
                        ),
                )
                .maybe_child(
                    (swipe.mode == SpaceSwipeMode::SwitchSpace)
                        .then_some(swipe_target_meta)
                        .flatten()
                        .map(|(target_name, target_icon)| SpaceSwipeIndicator {
                            target_name,
                            target_icon,
                            offset_x: swipe.offset_x,
                        }),
                )
                .maybe_child(
                    (swipe.mode == SpaceSwipeMode::AddSpace).then_some(SpaceAddSwipeOverlay {
                        progress: swipe.add_progress,
                        sidebar_side: layout.sidebar_side,
                    }),
                )
                .into_element()
        };

        rect()
            .width(Size::px(displayed_width))
            .height(Size::fill())
            .padding(Gaps::new(top_pad, pad, pad, pad))
            .content(Content::Flex)
            .overflow(Overflow::Clip)
            .on_wheel(on_sidebar_wheel)
            .maybe_child(sidebar_top_row)
            .maybe_child(url_bar_row)
            .child(sidebar_body)
            .maybe_child((!create_form_open).then_some(SidebarMediaControls {
                collapsed: is_collapsed_visual,
            }))
            .maybe_child((!create_form_open).then_some(notification_stack).flatten())
            .child(bottom_bar)
            .maybe_child(active_drag.map(|drag| DragGhostOverlay {
                drag,
                sidebar_width: displayed_width,
            }))
    }
}

/// Interactive resize handle positioned between the vertical sidebar and the inset page card.
#[derive(Clone, PartialEq)]
pub struct SidebarResizeHandle {
    pub side: SidebarSide,
    pub expanded: bool,
    pub current_width: f32,
    pub is_resizing: State<bool>,
}

impl Component for SidebarResizeHandle {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let mut drag_anchor = use_state(|| None::<(f32, f32)>);

        let expanded = self.expanded;
        let side = self.side;
        let current_width = self.current_width;
        let mut is_resizing = self.is_resizing;

        let on_pointer_down = move |e: Event<PointerEventData>| {
            if !expanded || e.button() != Some(MouseButton::Left) {
                return;
            }
            drag_anchor.set(Some((e.global_location().x as f32, current_width)));
            is_resizing.set_if_modified(true);
        };

        let move_core = core.clone();
        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            let Some((start_x, start_width)) = *drag_anchor.peek() else {
                return;
            };
            let next_width = compute_resized_sidebar_width(start_width, start_x, e.global_location().x as f32, side);
            if (move_core.model().peek().layout.sidebar_width - next_width).abs() >= 0.5 {
                move_core.invoke_or_log(Command::SidebarSetWidth { width: next_width });
            }
        };

        let on_global_pointer_up = move |_: Event<PointerEventData>| {
            if drag_anchor.peek().is_some() {
                drag_anchor.set(None);
                is_resizing.set_if_modified(false);
            }
        };

        let active = drag_anchor.read().is_some();
        let show_bar = expanded && (hovered() || active);

        rect()
            .width(Size::px(RESIZE_HANDLE_WIDTH))
            .height(Size::fill())
            .center()
            .cursor(if expanded {
                CursorIcon::ColResize
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_pointer_down(on_pointer_down)
            .on_global_pointer_move(on_global_pointer_move)
            .on_global_pointer_up(on_global_pointer_up)
            .maybe_child(show_bar.then(|| {
                rect()
                    .width(Size::px(2.0))
                    .height(Size::px(48.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(if active {
                        theme.color(ThemeRole::Accent)
                    } else {
                        theme.color(ThemeRole::Border)
                    })
            }))
    }
}
