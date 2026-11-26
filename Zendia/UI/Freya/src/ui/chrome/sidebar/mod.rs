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
    SIDEBAR_TOP_ROW_HEIGHT, TAB_ROW_GAP, TAB_ROW_HEIGHT, ToolbarSlots, compute_raw_resized_sidebar_width,
    compute_resized_sidebar_width_for_layout, show_window_controls_in_sidebar_top,
};
use crate::ui::chrome::menus::{open_sidebar_context_menu, try_use_menu_state};
use crate::ui::chrome::sidebar::essentials::EssentialsGrid;
use crate::ui::chrome::sidebar::folder::FolderGroup;
use crate::ui::chrome::sidebar::media_controls::SidebarMediaControls;
use crate::ui::chrome::sidebar::notification::SidebarNotificationCard;
use crate::ui::chrome::sidebar::pinned_separator::PinnedSeparator;
use crate::ui::chrome::sidebar::spaces::{SpaceHeader, SpaceSwitcher};
use crate::ui::chrome::sidebar::split_group::SplitGroupRow;
use crate::ui::chrome::sidebar::tab_row::{DragGhostOverlay, NewTabRow, VerticalTabRow};
use crate::ui::chrome::toolbar::{
    COMPACT_WINDOW_CONTROLS_WIDTH, OverflowMenuRow, ToolbarItemView, WindowControls, partition_sidebar_top_row,
};
use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, KitTooltip, scroll_item_into_view, use_interactive_feedback};
use crate::ui::spaces::{
    SpaceAddSwipeOverlay, SpaceCreationForm, SpaceLibrarySwipeOverlay, SpaceSwipeMode, handle_sidebar_wheel,
    try_use_space_editing_state,
};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::{Duration, Instant};
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::ToolbarItem;
use zendia_browser_core::focus::FocusTarget;
use zendia_browser_core::model::{
    InputMode, SIDEBAR_COLLAPSED_WIDTH, SIDEBAR_DEFAULT_WIDTH, SidebarSide, TabId, TabMoveTarget,
};
use zendia_browser_core::shell_layout::{
    effective_sidebar_width_for_window, min_sidebar_width_for_layout, should_drag_trigger_compact,
};

const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(350);

/// Estimates the rendered width of one toolbar item inside the sidebar bottom bar for overflow partitioning.
fn estimate_bottom_item_width(item: &ToolbarItem, tight: bool) -> f32 {
    match item.id.as_str() {
        "zendia.chat" => {
            if tight {
                55.0
            } else {
                68.0
            }
        }
        "zendia.agents" => {
            if tight {
                26.0
            } else {
                30.0
            }
        }
        _ => {
            if tight {
                20.0
            } else {
                26.0
            }
        }
    }
}

/// Partitions sidebar bottom-bar middle items into `(visible_middle, overflow_items, switcher_budget_w)`.
///
/// Matches Zen + Zendia (`ZendiaChatButton.sys.mjs:109`, `ZendiaAgentsButton.sys.mjs:124`, `ZenSpaceManager.mjs:3678`):
/// `Agents` and `Chat` stay inline rather than spilling into an overflow button when spaces grow, while the middle
/// space switcher receives the remaining horizontal budget and collapses inactive spaces into compact dots.
fn partition_sidebar_bottom_row(
    bottom_items: &[ToolbarItem],
    show_space_switcher: bool,
    displayed_width: f32,
    tight: bool,
) -> (
    Option<ToolbarItem>,
    Vec<ToolbarItem>,
    Option<ToolbarItem>,
    Vec<ToolbarItem>,
    f32,
) {
    let gap = if tight { 2.0 } else { 4.0 };
    let available_w = (displayed_width - SIDEBAR_PADDING_EXPANDED * 2.0 - 2.0).max(80.0);

    let first_item = bottom_items.first().cloned();
    let last_item = (bottom_items.len() >= 2)
        .then(|| bottom_items.last().cloned())
        .flatten();
    let middle_items: Vec<ToolbarItem> = if bottom_items.len() > 2 {
        bottom_items[1..bottom_items.len() - 1].to_vec()
    } else {
        Vec::new()
    };

    // Reserve at least one active space slot so the space switcher never disappears, without letting many spaces push
    // Agents or Chat into overflow.
    let min_switcher_w = if show_space_switcher {
        if tight { 19.0 + gap } else { 24.0 + gap }
    } else {
        0.0
    };

    let mut fixed_w = 0.0;
    if let Some(ref f) = first_item {
        fixed_w += estimate_bottom_item_width(f, tight) + gap;
    }
    if let Some(ref l) = last_item {
        fixed_w += estimate_bottom_item_width(l, tight) + gap;
    }

    let mut visible_middle: Vec<ToolbarItem> = Vec::new();
    let mut overflow_items: Vec<ToolbarItem> = Vec::new();
    let overflow_btn_w = if tight { 22.0 } else { 24.0 } + gap;

    for (i, item) in middle_items.iter().enumerate() {
        let w = estimate_bottom_item_width(item, tight) + gap;
        let has_more_after = i + 1 < middle_items.len();
        let remaining_w: f32 = middle_items[i + 1..]
            .iter()
            .map(|m| estimate_bottom_item_width(m, tight) + gap)
            .sum();
        let used_with_min_switcher = fixed_w + min_switcher_w;
        let fits_all_rest = used_with_min_switcher + w + remaining_w <= available_w;
        let fits_with_overflow = used_with_min_switcher + w + overflow_btn_w <= available_w;
        if fits_all_rest
            || (fits_with_overflow && overflow_items.is_empty())
            || (!has_more_after && overflow_items.is_empty() && used_with_min_switcher + w <= available_w)
        {
            fixed_w += w;
            visible_middle.push(item.clone());
        } else {
            overflow_items.push(item.clone());
        }
    }

    let overflow_used = if overflow_items.is_empty() { 0.0 } else { overflow_btn_w };
    let switcher_budget_w = if show_space_switcher {
        (available_w - fixed_w - overflow_used - gap).max(18.0)
    } else {
        0.0
    };

    (first_item, visible_middle, last_item, overflow_items, switcher_budget_w)
}

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
        let palette = ChromePalette::for_scheme(theme.scheme);
        let menu_state = try_use_menu_state();
        let sidebar_state = try_use_sidebar_state();
        let space_editing = try_use_space_editing_state();
        let create_form_open = space_editing.is_some_and(|s| *s.create_form_open.read());
        let swipe = space_editing.map(|s| s.swipe.read().clone()).unwrap_or_default();
        let mut tab_scroll = use_scroll_controller(ScrollConfig::default);
        let mut tab_viewport_h = use_state(|| 420.0f32);
        let mut last_scrolled_tab = use_state(|| None::<TabId>);
        let mut last_scrolled_focus = use_state(|| None::<FocusTarget>);
        let mut sidebar_x_bounds = use_state(|| (0.0_f32, SIDEBAR_DEFAULT_WIDTH));
        let mut last_space_idx = use_state(|| 0usize);
        let mut space_slide_dir = use_state(|| 0.0_f32);
        let mut bottom_overflow_open = use_state(|| false);
        let mut top_overflow_open = use_state(|| false);
        // Where the `»` menu and its button are, as `(x, y, width, height)`.
        let mut top_overflow_area = use_state(|| (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32));
        let mut top_overflow_button_area = use_state(|| (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32));
        let top_overflow_fb = use_interactive_feedback(true);
        let overflow_btn_feedback = use_interactive_feedback(true);

        let (
            layout,
            settings,
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
            explorer_cursor,
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
                            let depth = model.folder_depth(folder.id);
                            (folder.clone(), tabs, depth)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let normal_tabs: Vec<_> = active_space_id
                .map(|id| model.normal_tabs_in_space(id).into_iter().cloned().collect())
                .unwrap_or_default();
            let ex_cursor = (model.vim_mode_enabled && model.input_mode == InputMode::Explorer)
                .then(|| model.focus.cursor.clone())
                .flatten();
            (
                model.layout.clone(),
                model.settings.clone(),
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
                ex_cursor,
            )
        };

        let root_window_w = Platform::get().current_window().root_size.read().width;
        let expanded = layout.sidebar_expanded;
        let expanded_width = effective_sidebar_width_for_window(
            &zendia_browser_core::model::LayoutSettings {
                sidebar_expanded: true,
                ..layout.clone()
            },
            root_window_w,
        );
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
        // SB-05: opening Space Creation while collapsed temporarily expands the sidebar so the form is never squished.
        let displayed_width = if create_form_open {
            expanded_width
        } else if resizing {
            effective_sidebar_width_for_window(&layout, root_window_w)
        } else {
            SIDEBAR_COLLAPSED_WIDTH + (expanded_width - SIDEBAR_COLLAPSED_WIDTH) * anim_factor
        };

        let is_collapsed_visual = !expanded && anim_factor < 0.35 && !create_form_open;
        // With a top toolbar the sidebar has a top row only while it holds the Compact Mode button.
        let show_top_row =
            (!layout.has_top_toolbar() || !self.slots.sidebar_top_start.is_empty()) && !is_collapsed_visual;
        let show_url_bar = !self.slots.sidebar_url_bar.is_empty() && !is_collapsed_visual;
        let url_bar_fade = use_animation_with_dependencies(&create_form_open, move |conf, creating| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *creating { (1.0, 0.0) } else { (0.0, 1.0) };
            AnimNum::new(from, to)
                .duration(motion::SPACE_SWITCH_SLIDE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let url_bar_opacity = url_bar_fade.get().value().clamp(0.0, 1.0);
        let show_sidebar_window_controls = show_window_controls_in_sidebar_top(&layout);
        let active_space_id = active_space.as_ref().map(|s| s.id);

        let location = self.location;
        let location_a11y_id = self.location_a11y_id;

        let (top_vis_start, top_vis_end, top_overflow_items) = if show_top_row {
            partition_sidebar_top_row(
                &self.slots.sidebar_top_start,
                &self.slots.sidebar_top_end,
                show_sidebar_window_controls,
                displayed_width,
            )
        } else {
            (Vec::new(), Vec::new(), Vec::new())
        };
        if top_overflow_items.is_empty() && *top_overflow_open.peek() {
            top_overflow_open.set(false);
        }

        let sidebar_top_row = show_top_row.then(|| {
            let start_buttons = top_vis_start.iter().map(|item| {
                ToolbarItemView::new(item.clone(), location, location_a11y_id)
                    .dense(true)
                    .key(item.id.as_str().to_string())
                    .into_element()
            });
            let end_buttons = top_vis_end.iter().map(|item| {
                ToolbarItemView::new(item.clone(), location, location_a11y_id)
                    .dense(true)
                    .key(item.id.as_str().to_string())
                    .into_element()
            });
            let top_overflow_btn = (!top_overflow_items.is_empty()).then(|| {
                let open_now = *top_overflow_open.read();
                let bg = top_overflow_fb.background(
                    if open_now {
                        palette.surface_pill
                    } else {
                        Color::TRANSPARENT
                    },
                    palette.surface_pill,
                    palette.surface_pill_hover,
                );
                KitTooltip::new("More toolbar controls").enabled(!open_now).child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("More toolbar controls")
                        .width(Size::px(26.0))
                        .height(Size::px(26.0))
                        .corner_radius(CornerRadius::new_all(6.0))
                        .background(bg)
                        .scale(top_overflow_fb.scale())
                        .center()
                        .cursor(CursorIcon::Pointer)
                        .on_sized(move |e: Event<SizedEventData>| {
                            let area = (e.area.min_x(), e.area.min_y(), e.area.width(), e.area.height());
                            top_overflow_button_area.set_if_modified(area);
                        })
                        .on_pointer_enter(top_overflow_fb.on_pointer_enter())
                        .on_pointer_leave(top_overflow_fb.on_pointer_leave())
                        .on_pointer_down(top_overflow_fb.on_pointer_down())
                        .on_global_pointer_up(top_overflow_fb.on_pointer_up())
                        // Keep the sidebar from closing the menu on the way down, so this press can toggle it.
                        .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            let next = !*top_overflow_open.peek();
                            top_overflow_open.set(next);
                        })
                        .child(label().text("»").font_size(18.0).color(palette.text_muted)),
                )
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
                        .spacing(2.0)
                        .children(start_buttons),
                )
                .child(
                    rect()
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(2.0)
                        .children(end_buttons)
                        .maybe_child(
                            top_overflow_btn.map(|button| rect().padding(Gaps::new(0.0, 5.0, 0.0, 0.0)).child(button)),
                        )
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

        let top_overflow_popover = (*top_overflow_open.read() && !top_overflow_items.is_empty()).then(|| {
            let pop_bg = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(255, 43, 42, 51),
                ResolvedScheme::Light => Color::from_argb(255, 243, 243, 243),
            };
            rect()
                .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                .position(
                    Position::new_absolute()
                        .top(SIDEBAR_TOP_ROW_HEIGHT - 4.0)
                        .right(COMPACT_WINDOW_CONTROLS_WIDTH + 7.0),
                )
                .width(Size::px(202.0))
                .padding(Gaps::new(8.0, 0.0, 8.0, 0.0))
                .corner_radius(CornerRadius::new_all(2.0))
                .background(pop_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(palette.page_card_border),
                )
                .shadow(Shadow::new().y(4.0).blur(14.0).color(Color::from_argb(72, 0, 0, 0)))
                .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
                .on_sized(move |e: Event<SizedEventData>| {
                    let area = (e.area.min_x(), e.area.min_y(), e.area.width(), e.area.height());
                    top_overflow_area.set_if_modified(area);
                })
                // A press anywhere else closes the menu; the `»` button toggles it itself.
                .on_global_pointer_down(move |e: Event<PointerEventData>| {
                    let at = (e.global_location().x as f32, e.global_location().y as f32);
                    let inside = |(x, y, width, height): (f32, f32, f32, f32)| {
                        (x..=x + width).contains(&at.0) && (y..=y + height).contains(&at.1)
                    };
                    if !inside(*top_overflow_area.peek()) && !inside(*top_overflow_button_area.peek()) {
                        top_overflow_open.set_if_modified(false);
                    }
                })
                .children(top_overflow_items.into_iter().map(|item| {
                    OverflowMenuRow {
                        item,
                        menu_open: top_overflow_open,
                    }
                    .into_element()
                }))
        });

        let url_bar_row = (show_url_bar && url_bar_opacity > 0.02).then(|| {
            use zendia_browser_core::contributions::{ContributionSource, ToolbarItemKind};
            let url_bars: Vec<_> = self
                .slots
                .sidebar_url_bar
                .iter()
                .filter(|item| item.kind == ToolbarItemKind::UrlBar)
                .cloned()
                .collect();
            let pinned_exts: Vec<_> = self
                .slots
                .sidebar_url_bar
                .iter()
                .filter(|item| matches!(item.source, ContributionSource::Extension { .. }))
                .cloned()
                .collect();
            // Matches Zen's `#zen-overflow-extensions-list`: pinned extensions in single-toolbar mode render in a
            // compact horizontal strip directly beneath the sidebar URL bar instead of crowding the bottom bar.
            let pinned_ext_strip = (!pinned_exts.is_empty()).then(|| {
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(6.0, 2.0, 0.0, 2.0))
                    .spacing(6.0)
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .cross_align(Alignment::center())
                    .children(pinned_exts.into_iter().map(|item| {
                        rect()
                            .height(Size::px(26.0))
                            .min_width(Size::px(32.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .background(palette.surface_pill)
                            .center()
                            .child(
                                ToolbarItemView::new(item.clone(), location, location_a11y_id)
                                    .dense(true)
                                    .key(item.id.as_str().to_string()),
                            )
                            .into_element()
                    }))
            });
            rect()
                .width(Size::fill())
                .padding(Gaps::new(2.0, 2.0, 4.0, 2.0))
                .opacity(url_bar_opacity)
                .interactive(if create_form_open {
                    Interactive::No
                } else {
                    Interactive::Yes
                })
                .children(url_bars.into_iter().map(|item| {
                    ToolbarItemView::new(item.clone(), location, location_a11y_id)
                        .compact_urlbar(true)
                        .key(item.id.as_str().to_string())
                        .into_element()
                }))
                .maybe_child(pinned_ext_strip)
        });

        let essentials_section = EssentialsGrid {
            tabs: essentials,
            active_tab: active_tab_id,
            collapsed: is_collapsed_visual,
            show_promo: show_essentials_promo,
            sidebar_width: displayed_width,
        };

        let pinned_collapsed_for_space = active_space_id
            .zip(sidebar_state)
            .is_some_and(|(sid, st)| st.is_pinned_collapsed(sid));

        // SPC-03: 250ms horizontal spring slide on the tab strip whenever active_space_id changes.
        let current_space_idx = active_space_id
            .and_then(|id| spaces.iter().position(|s| s.id == id))
            .unwrap_or(0);
        if *last_space_idx.peek() != current_space_idx {
            let dir = if current_space_idx > *last_space_idx.peek() {
                1.0
            } else {
                -1.0
            };
            last_space_idx.set(current_space_idx);
            space_slide_dir.set(dir);
        }
        let slide_from_x = *space_slide_dir.read() * 26.0;
        let space_switch_anim = use_animation_with_dependencies(&active_space_id, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(slide_from_x, 0.0)
                .duration(motion::SPACE_SWITCH_SLIDE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let space_switch_offset_x = space_switch_anim.get().value();

        let space_header = SpaceHeader {
            space: active_space,
            collapsed: is_collapsed_visual,
            has_pinned_tabs: !pinned_tabs.is_empty() || !folders_with_tabs.is_empty(),
        };

        // XSB-02 & VIM-EX-05: Compute row Y-spans inside the vertical ScrollView so switching tabs and moving the
        // Explorer focus cursor both keep their target row scrolled into view.
        let show_new_tab_row = settings.show_new_tab_on_tab_list;
        let new_tab_on_top = settings.new_tab_button_top;
        let row_step = TAB_ROW_HEIGHT + TAB_ROW_GAP;
        let mut cursor_y = 2.0f32;
        let mut active_tab_y: Option<f32> = None;
        let mut explorer_focus_y: Option<f32> = None;

        if !pinned_collapsed_for_space {
            for tab in &pinned_tabs {
                if active_tab_id == Some(tab.id) {
                    active_tab_y = Some(cursor_y);
                }
                if explorer_cursor.as_ref() == Some(&FocusTarget::PinnedTab(tab.id)) {
                    explorer_focus_y = Some(cursor_y);
                }
                cursor_y += row_step;
            }
            for (folder, tabs, _) in &folders_with_tabs {
                if explorer_cursor.as_ref() == Some(&FocusTarget::Folder(folder.id)) {
                    explorer_focus_y = Some(cursor_y);
                }
                cursor_y += row_step; // Folder header row
                if !folder.collapsed {
                    for tab in tabs {
                        if active_tab_id == Some(tab.id) {
                            active_tab_y = Some(cursor_y);
                        }
                        if explorer_cursor.as_ref() == Some(&FocusTarget::Tab(tab.id)) {
                            explorer_focus_y = Some(cursor_y);
                        }
                        cursor_y += row_step;
                    }
                } else if let Some(active_in_folder) = tabs.iter().find(|t| active_tab_id == Some(t.id)) {
                    active_tab_y = Some(cursor_y);
                    if explorer_cursor.as_ref() == Some(&FocusTarget::Tab(active_in_folder.id)) {
                        explorer_focus_y = Some(cursor_y);
                    }
                    cursor_y += row_step;
                }
            }
        }
        cursor_y += 24.0; // PinnedSeparator
        if show_new_tab_row && new_tab_on_top {
            cursor_y += row_step;
        }

        let has_normal_tabs = !normal_tabs.is_empty();
        let pinned_count = pinned_tabs.len();
        let normal_count = normal_tabs.len();
        let active_tab_favicon = active_tab_id.and_then(|aid| {
            pinned_tabs
                .iter()
                .chain(normal_tabs.iter())
                .find(|t| t.id == aid)
                .and_then(|t| t.favicon.clone())
        });
        let pinned_rows: Vec<Element> = if pinned_collapsed_for_space {
            Vec::new()
        } else {
            pinned_tabs
                .into_iter()
                .enumerate()
                .map(|(idx, tab)| {
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
                })
                .collect()
        };

        let folder_rows: Vec<Element> = if pinned_collapsed_for_space {
            Vec::new()
        } else {
            folders_with_tabs
                .into_iter()
                .enumerate()
                .map(|(f_idx, (folder, tabs, depth))| {
                    FolderGroup::new(folder, tabs, active_tab_id, is_collapsed_visual)
                        .with_nesting(depth as f32 * 12.0, f_idx)
                        .into_element()
                })
                .collect()
        };

        let separator = PinnedSeparator {
            space_id: active_space_id,
            has_normal_tabs,
            collapsed: is_collapsed_visual,
        };

        let new_tab_row_top = (show_new_tab_row && new_tab_on_top).then_some(NewTabRow {
            collapsed: is_collapsed_visual,
        });
        let new_tab_row_bottom = (show_new_tab_row && !new_tab_on_top).then_some(NewTabRow {
            collapsed: is_collapsed_visual,
        });

        // A split group draws as one row where its first tab sits; its other tabs are left out of the list.
        let mut drawn_groups = Vec::new();
        let mut normal_rows = Vec::new();
        for (idx, tab) in normal_tabs.iter().enumerate() {
            if let Some(group) = split_groups.iter().find(|group| group.tab_ids.contains(&tab.id)) {
                if drawn_groups.contains(&group.id) {
                    continue;
                }
                drawn_groups.push(group.id);
                if active_tab_id.is_some_and(|aid| group.tab_ids.contains(&aid)) {
                    active_tab_y = Some(cursor_y);
                }
                if explorer_cursor.as_ref() == Some(&FocusTarget::SplitGroup(group.id)) {
                    explorer_focus_y = Some(cursor_y);
                }
                cursor_y += row_step;
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
            if active_tab_id == Some(tab.id) {
                active_tab_y = Some(cursor_y);
            }
            if explorer_cursor.as_ref() == Some(&FocusTarget::Tab(tab.id)) {
                explorer_focus_y = Some(cursor_y);
            }
            cursor_y += row_step;
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
        if show_new_tab_row && !new_tab_on_top {
            cursor_y += row_step;
        }
        let total_tab_content_h = cursor_y + 6.0;

        let viewport_h = (*tab_viewport_h.peek()).max(120.0);
        if *last_scrolled_focus.peek() != explorer_cursor {
            last_scrolled_focus.set(explorer_cursor.clone());
            if let Some(row_top) = explorer_focus_y {
                scroll_item_into_view(
                    &mut tab_scroll,
                    row_top,
                    TAB_ROW_HEIGHT,
                    viewport_h,
                    total_tab_content_h,
                );
            }
        }
        if *last_scrolled_tab.peek() != active_tab_id {
            last_scrolled_tab.set(active_tab_id);
            if explorer_focus_y.is_none()
                && let Some(item_y) = active_tab_y
            {
                scroll_item_into_view(&mut tab_scroll, item_y, TAB_ROW_HEIGHT, viewport_h, total_tab_content_h);
            }
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

        let bottom_items = self.slots.sidebar_bottom.clone();
        let show_space_switcher = spaces.len() > 1;
        let tight_bottom = !layout.has_top_toolbar() && displayed_width < 230.0;

        // SWP-01: Build adjacent incoming space preview strip when swiping between spaces.
        let swipe_incoming_space = (swipe.mode == SpaceSwipeMode::SwitchSpace)
            .then_some(swipe.target_space_id)
            .flatten()
            .and_then(|tid| {
                let model = core.model().read();
                let sp = model.space(tid)?.clone();
                let p_tabs: Vec<_> = model.pinned_tabs_in_space(tid).into_iter().cloned().collect();
                let n_tabs: Vec<_> = model.normal_tabs_in_space(tid).into_iter().cloned().collect();
                Some((sp, p_tabs, n_tabs))
            });
        let side_factor = if layout.sidebar_side == SidebarSide::Right {
            -1.0
        } else {
            1.0
        };
        let track_width = (displayed_width - SIDEBAR_PADDING_EXPANDED * 2.0).max(100.0);
        let swipe_strip_offset = match swipe.mode {
            SpaceSwipeMode::SwitchSpace => swipe.offset_x,
            SpaceSwipeMode::AddSpace => -swipe.add_progress * displayed_width * 0.25 * side_factor,
            SpaceSwipeMode::OpenLibrary => swipe.add_progress * displayed_width * 0.25 * side_factor,
            SpaceSwipeMode::Idle => 0.0,
        };

        let wheel_core = core.clone();
        let on_sidebar_wheel = move |e: Event<WheelEventData>| {
            if let Some(editing) = space_editing {
                handle_sidebar_wheel(&e, &wheel_core, editing, displayed_width);
            }
        };

        // XSB-07: Partition bottom bar items into visible + overflow (`»`) while keeping Agents & Chat whole at 186px.
        let mut overflow_items: Vec<ToolbarItem> = Vec::new();
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
                    max_width: None,
                }))
                .children(self.slots.sidebar_bottom.iter().map(|item| {
                    ToolbarItemView::new(item.clone(), location, location_a11y_id)
                        .dense(true)
                        .key(item.id.as_str().to_string())
                        .into_element()
                }))
        } else {
            let gap = if tight_bottom { 2.0 } else { 4.0 };
            let (first_item, visible_middle, last_item, next_overflow, switcher_budget_w) =
                partition_sidebar_bottom_row(&bottom_items, show_space_switcher, displayed_width, tight_bottom);
            overflow_items = next_overflow;

            let has_overflow = !overflow_items.is_empty();
            if !has_overflow && *bottom_overflow_open.peek() {
                bottom_overflow_open.set(false);
            }
            let explorer_cursor_in_overflow = matches!(
                explorer_cursor.as_ref(),
                Some(FocusTarget::SidebarFooter(id)) if overflow_items.iter().any(|item| &item.id == id)
            );

            let mut trailing_visible = visible_middle;
            if let Some(l) = last_item {
                trailing_visible.push(l);
            }

            let overflow_button = has_overflow.then(|| {
                let open_now = *bottom_overflow_open.read() || explorer_cursor_in_overflow;
                let rest_bg = if open_now {
                    palette.surface_pill
                } else {
                    Color::TRANSPARENT
                };
                let hover_bg = if open_now {
                    palette.surface_pill
                } else {
                    palette.tab_hover_bg
                };
                let bg = overflow_btn_feedback.background(rest_bg, hover_bg, palette.surface_pill_hover);
                let btn = rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("More sidebar tools")
                    .width(Size::px(if tight_bottom { 22.0 } else { 24.0 }))
                    .height(Size::px(if tight_bottom { 22.0 } else { 24.0 }))
                    .corner_radius(CornerRadius::new_all(6.0))
                    .background(bg)
                    .scale(overflow_btn_feedback.scale())
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(overflow_btn_feedback.on_pointer_enter())
                    .on_pointer_leave(overflow_btn_feedback.on_pointer_leave())
                    .on_pointer_down(overflow_btn_feedback.on_pointer_down())
                    .on_global_pointer_up(overflow_btn_feedback.on_pointer_up())
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        let next = !*bottom_overflow_open.peek();
                        bottom_overflow_open.set(next);
                    })
                    .child(Icon::new("chevron").size(12.0).role(ThemeRole::TextMuted));
                KitTooltip::new("More sidebar tools").enabled(!open_now).child(btn)
            });

            let leading_el = first_item.map(|item| {
                ToolbarItemView::new(item.clone(), location, location_a11y_id)
                    .dense(true)
                    .tight(tight_bottom)
                    .key(item.id.as_str().to_string())
            });
            let switcher_el = show_space_switcher.then_some(SpaceSwitcher {
                spaces,
                active_space: active_space_id,
                compact: tight_bottom,
                vertical: false,
                max_width: Some(switcher_budget_w),
            });
            // Match Zen's `flex-direction: row-reverse` on `#zen-sidebar-foot-buttons` when the sidebar is on the
            // right: `[+] [Chat] [Agents]` — `[SpaceSwitcher]` — `[Library]` (`vertical-tabs.css:488`).
            let row = rect()
                .width(Size::fill())
                .height(Size::px(36.0))
                .padding(Gaps::new(2.0, 1.0, 0.0, 1.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .main_align(Alignment::SpaceBetween)
                .cross_align(Alignment::center())
                .spacing(gap);

            if layout.sidebar_side == SidebarSide::Right {
                let trailing_group = rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(gap)
                    .children(trailing_visible.into_iter().rev().map(|item| {
                        ToolbarItemView::new(item.clone(), location, location_a11y_id)
                            .dense(true)
                            .tight(tight_bottom)
                            .key(item.id.as_str().to_string())
                            .into_element()
                    }))
                    .maybe_child(overflow_button);
                row.child(trailing_group)
                    .maybe_child(switcher_el)
                    .maybe_child(leading_el)
            } else {
                let trailing_group = rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(gap)
                    .maybe_child(overflow_button)
                    .children(trailing_visible.into_iter().map(|item| {
                        ToolbarItemView::new(item.clone(), location, location_a11y_id)
                            .dense(true)
                            .tight(tight_bottom)
                            .key(item.id.as_str().to_string())
                            .into_element()
                    }));
                row.maybe_child(leading_el)
                    .maybe_child(switcher_el)
                    .child(trailing_group)
            }
        };

        let explorer_cursor_in_overflow = matches!(
            explorer_cursor.as_ref(),
            Some(FocusTarget::SidebarFooter(id)) if overflow_items.iter().any(|item| &item.id == id)
        );
        let overflow_popover =
            ((*bottom_overflow_open.read() || explorer_cursor_in_overflow) && !overflow_items.is_empty()).then(|| {
                let pop_bg = match theme.scheme {
                    ResolvedScheme::Dark => Color::from_argb(242, 26, 28, 40),
                    ResolvedScheme::Light => Color::from_argb(246, 248, 249, 252),
                };
                rect()
                    .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                    .position(Position::new_absolute().bottom(42.0).right(6.0))
                    .padding(Gaps::new(6.0, 8.0, 6.0, 8.0))
                    .corner_radius(CornerRadius::new_all(10.0))
                    .background(pop_bg)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(palette.page_card_border),
                    )
                    .shadow(Shadow::new().y(4.0).blur(14.0).color(Color::from_argb(72, 0, 0, 0)))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
                    .children(overflow_items.into_iter().map(|item| {
                        ToolbarItemView::new(item.clone(), location, location_a11y_id)
                            .dense(true)
                            .key(format!("overflow:{}", item.id.as_str()))
                            .into_element()
                    }))
            });

        let active_drag = sidebar_state.and_then(|s| s.drag_state.read().clone());

        let pad = if is_collapsed_visual {
            SIDEBAR_PADDING_COLLAPSED
        } else {
            SIDEBAR_PADDING_EXPANDED
        };
        let top_pad = if layout.has_top_toolbar() && !layout.compact_hides_toolbar() && !show_top_row {
            0.0
        } else {
            4.0
        };

        // XSB-03 & XCM-04: Right-clicking empty sidebar space opens the sidebar/toolbar context menu.
        let ctx_core = core.clone();
        let on_sidebar_mouse_down = move |e: Event<MouseEventData>| {
            if *bottom_overflow_open.peek() {
                bottom_overflow_open.set(false);
            }
            if *top_overflow_open.peek() {
                top_overflow_open.set(false);
            }
            if e.button == Some(MouseButton::Right)
                && let Some(ms) = &menu_state
            {
                e.stop_propagation();
                let model = ctx_core.model().peek().clone();
                let position = (e.global_location.x as f32, e.global_location.y as f32);
                open_sidebar_context_menu(ms, &model, position);
            }
        };

        // SPC-05: Show floating "Scroll to active tab" pill when the active tab is scrolled out of view.
        let (_, scroll_y_i32): (i32, i32) = tab_scroll.into();
        let scroll_top = (-scroll_y_i32 as f32).max(0.0);
        let scroll_to_tab_pill = (!is_collapsed_visual && total_tab_content_h > viewport_h + 4.0)
            .then(|| {
                let item_y = active_tab_y?;
                let dir_up = item_y + TAB_ROW_HEIGHT < scroll_top + 4.0;
                let dir_down = item_y > scroll_top + viewport_h - 4.0;
                if !dir_up && !dir_down {
                    return None;
                }
                let pill_pos = if dir_up {
                    Position::new_absolute().top(6.0).left(0.0).right(0.0)
                } else {
                    Position::new_absolute().bottom(6.0).left(0.0).right(0.0)
                };
                let pill_bg = match theme.scheme {
                    ResolvedScheme::Dark => theme
                        .rgba(ThemeRole::Accent)
                        .mix(crate::ui::theme::Rgba::rgb(12, 14, 20), 0.55)
                        .to_color(),
                    ResolvedScheme::Light => theme
                        .rgba(ThemeRole::Accent)
                        .mix(crate::ui::theme::Rgba::rgb(255, 255, 255), 0.85)
                        .to_color(),
                };
                let target_scroll_y = item_y;
                Some(
                    rect()
                        .position(pill_pos)
                        .main_align(Alignment::center())
                        .cross_align(Alignment::center())
                        .child(
                            rect()
                                .a11y_role(AccessibilityRole::Button)
                                .a11y_alt("Scroll to active tab")
                                .padding(Gaps::new(5.0, 9.0, 5.0, 8.0))
                                .spacing(5.0)
                                .direction(Direction::Horizontal)
                                .cross_align(Alignment::center())
                                .corner_radius(CornerRadius::new_all(999.0))
                                .background(pill_bg)
                                .shadow(Shadow::new().y(2.0).blur(10.0).color(Color::from_argb(65, 0, 0, 0)))
                                .cursor(CursorIcon::Pointer)
                                .on_press(move |e: Event<PressEventData>| {
                                    e.stop_propagation();
                                    scroll_item_into_view(
                                        &mut tab_scroll,
                                        target_scroll_y,
                                        TAB_ROW_HEIGHT,
                                        viewport_h,
                                        total_tab_content_h,
                                    );
                                })
                                .child(
                                    Icon::new(active_tab_favicon.clone().unwrap_or_else(|| "globe".to_string()))
                                        .size(13.0)
                                        .role(ThemeRole::Text),
                                )
                                .child(
                                    Icon::new(if dir_up { "chevron-up" } else { "chevron-down" })
                                        .size(12.0)
                                        .role(ThemeRole::Text),
                                ),
                        ),
                )
            })
            .flatten();

        let is_tab_dragging = active_drag.as_ref().is_some_and(|d| !d.is_preview);
        let pinned_tail_drop = is_tab_dragging.then(|| {
            let tail_target = TabMoveTarget::Pinned {
                space_id: active_space_id,
                index: pinned_count,
            };
            let is_targeted = active_drag.as_ref().is_some_and(|d| d.target == Some(tail_target));
            rect()
                .width(Size::fill())
                .height(Size::px(8.0))
                .on_pointer_enter(move |_| {
                    if let Some(state) = sidebar_state {
                        state.set_drag_target(tail_target);
                    }
                })
                .maybe_child(
                    is_targeted.then_some(crate::ui::chrome::sidebar::tab_row::DropIndicatorBar { indent: 0.0 }),
                )
        });
        let normal_tail_drop = is_tab_dragging.then(|| {
            let tail_target = TabMoveTarget::Normal {
                space_id: active_space_id,
                index: normal_count,
            };
            let is_targeted = active_drag.as_ref().is_some_and(|d| d.target == Some(tail_target));
            rect()
                .width(Size::fill())
                .height(Size::px(12.0))
                .on_pointer_enter(move |_| {
                    if let Some(state) = sidebar_state {
                        state.set_drag_target(tail_target);
                    }
                })
                .maybe_child(
                    is_targeted.then_some(crate::ui::chrome::sidebar::tab_row::DropIndicatorBar { indent: 0.0 }),
                )
        });

        let incoming_space_pane = swipe_incoming_space.map(|(sp, p_tabs, n_tabs)| {
            let incoming_x = if swipe.offset_x < 0.0 {
                track_width + swipe_strip_offset
            } else {
                -track_width + swipe_strip_offset
            };
            let inc_header = SpaceHeader {
                space: Some(sp.clone()),
                collapsed: false,
                has_pinned_tabs: !p_tabs.is_empty(),
            };
            let inc_tabs = p_tabs.into_iter().chain(n_tabs).take(8).map(|t| {
                rect()
                    .key(format!("inc-tab-{}", t.id.0))
                    .width(Size::fill())
                    .height(Size::px(TAB_ROW_HEIGHT))
                    .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                    .spacing(8.0)
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .child(
                        Icon::new(t.favicon.clone().unwrap_or_else(|| "globe".to_string()))
                            .size(13.0)
                            .role(ThemeRole::TextMuted),
                    )
                    .child(
                        label()
                            .text(t.display_title())
                            .font_size(12.5)
                            .color(palette.text_muted)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Clip),
                    )
                    .into_element()
            });
            rect()
                .position(Position::new_absolute().left(incoming_x).top(0.0))
                .width(Size::px(track_width))
                .height(Size::percent(100.0))
                .interactive(Interactive::No)
                .child(inc_header)
                .child(rect().width(Size::fill()).spacing(2.0).children(inc_tabs))
        });

        let sidebar_body: Element = if let Some(editing) = space_editing.filter(|_| create_form_open) {
            SpaceCreationForm { state: editing }.into_element()
        } else {
            let tab_strip_viewport = rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .content(Content::Flex)
                .overflow(Overflow::Clip)
                .on_wheel(move |e: Event<WheelEventData>| {
                    // SWP-05: Space swipe wheel listener attached ONLY to the expanded tab-list region.
                    if !is_collapsed_visual {
                        on_sidebar_wheel(e);
                    }
                })
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::fill())
                        .content(Content::Flex)
                        .offset_x(swipe_strip_offset + space_switch_offset_x)
                        .child(space_header)
                        .child(
                            rect()
                                .width(Size::fill())
                                .height(Size::flex(1.0))
                                .on_sized(move |e: Event<SizedEventData>| {
                                    let h = e.area.height();
                                    if h > 20.0 && (*tab_viewport_h.peek() - h).abs() > 1.0 {
                                        tab_viewport_h.set(h);
                                    }
                                })
                                .child(
                                    ScrollView::new_controlled(tab_scroll)
                                        .width(Size::fill())
                                        .height(Size::fill())
                                        .child(
                                            rect()
                                                .width(Size::fill())
                                                .padding(Gaps::new(4.0, 0.0, 4.0, 0.0))
                                                .spacing(TAB_ROW_GAP)
                                                .cross_align(if is_collapsed_visual {
                                                    Alignment::center()
                                                } else {
                                                    Alignment::Start
                                                })
                                                .children(pinned_rows)
                                                .maybe_child(pinned_tail_drop)
                                                .children(folder_rows)
                                                .child(separator)
                                                .maybe_child(new_tab_row_top)
                                                .children(normal_rows)
                                                .maybe_child(normal_tail_drop)
                                                .maybe_child(new_tab_row_bottom)
                                                .maybe_child(self.interactive_window_controls.then(|| {
                                                    // WIN-04: Empty sidebar space below tabs is window-draggable.
                                                    rect()
                                                        .width(Size::fill())
                                                        .min_height(Size::px(48.0))
                                                        .height(Size::flex(1.0))
                                                        .window_drag()
                                                })),
                                        ),
                                )
                                .maybe_child(scroll_to_tab_pill),
                        ),
                )
                .maybe_child(incoming_space_pane)
                .maybe_child(
                    (swipe.mode == SpaceSwipeMode::AddSpace).then_some(SpaceAddSwipeOverlay {
                        progress: swipe.add_progress,
                        sidebar_side: layout.sidebar_side,
                    }),
                )
                .maybe_child(
                    (swipe.mode == SpaceSwipeMode::OpenLibrary).then_some(SpaceLibrarySwipeOverlay {
                        progress: swipe.add_progress,
                        sidebar_side: layout.sidebar_side,
                    }),
                );

            // SWP-01: Keep EssentialsGrid anchored outside the horizontal space-swipe track.
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .content(Content::Flex)
                .overflow(Overflow::Clip)
                .child(essentials_section)
                .child(tab_strip_viewport)
                .into_element()
        };

        let edge_core = core.clone();
        rect()
            .width(Size::px(displayed_width))
            .height(Size::fill())
            .padding(Gaps::new(top_pad, pad, pad, pad))
            .content(Content::Flex)
            .overflow(Overflow::Clip)
            .on_sized(move |e: Event<SizedEventData>| {
                sidebar_x_bounds.set_if_modified((e.area.min_x(), e.area.max_x()));
            })
            .on_pointer_move(move |e: Event<PointerEventData>| {
                // DND-02: Dragging a tab within 20px of sidebar left/right edge switches spaces after 1000ms.
                let Some(state) = sidebar_state else {
                    return;
                };
                if !state.drag_state.peek().as_ref().is_some_and(|d| !d.is_preview) {
                    return;
                }
                let px = e.global_location().x as f32;
                let (min_x, max_x) = *sidebar_x_bounds.peek();
                let near_left = px >= min_x && px <= min_x + 20.0;
                let near_right = px <= max_x && px >= max_x - 20.0;
                if near_left || near_right {
                    let mut seq_state = state.edge_switch_seq;
                    let seq = seq_state.peek().wrapping_add(1);
                    seq_state.set(seq);
                    let edge_core = edge_core.clone();
                    spawn(async move {
                        timer(Duration::from_millis(1000)).await;
                        if *seq_state.peek() == seq && state.drag_state.peek().as_ref().is_some_and(|d| !d.is_preview) {
                            edge_core.invoke_or_log(if near_left {
                                Command::SpacePrevious
                            } else {
                                Command::SpaceNext
                            });
                        }
                    });
                }
            })
            .on_pointer_enter(move |_| {
                if let Some(state) = sidebar_state {
                    let mut hov = state.sidebar_hovered;
                    hov.set_if_modified(true);
                }
            })
            .on_pointer_leave(move |_| {
                if let Some(state) = sidebar_state {
                    let mut hov = state.sidebar_hovered;
                    hov.set_if_modified(false);
                }
            })
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if let Some(state) = sidebar_state {
                    let mut ctrl = state.ctrl_held;
                    ctrl.set_if_modified(
                        e.modifiers.contains(Modifiers::CONTROL) || matches!(e.key, Key::Named(NamedKey::Control)),
                    );
                    let mut shift = state.shift_held;
                    shift.set_if_modified(
                        e.modifiers.contains(Modifiers::SHIFT) || matches!(e.key, Key::Named(NamedKey::Shift)),
                    );
                    if matches!(e.key, Key::Named(NamedKey::Escape)) {
                        let mut drag = state.drag_state;
                        if drag.peek().as_ref().is_some_and(|d| !d.is_preview) {
                            drag.set(None);
                        }
                    }
                }
            })
            .on_global_key_up(move |e: Event<KeyboardEventData>| {
                if let Some(state) = sidebar_state {
                    let mut ctrl = state.ctrl_held;
                    if matches!(e.key, Key::Named(NamedKey::Control)) || !e.modifiers.contains(Modifiers::CONTROL) {
                        ctrl.set_if_modified(false);
                    }
                    let mut shift = state.shift_held;
                    if matches!(e.key, Key::Named(NamedKey::Shift)) || !e.modifiers.contains(Modifiers::SHIFT) {
                        shift.set_if_modified(false);
                    }
                }
            })
            .on_mouse_down(on_sidebar_mouse_down)
            .maybe_child(sidebar_top_row.map(|row| {
                if create_form_open {
                    rect()
                        .width(Size::fill())
                        .opacity(0.35)
                        .interactive(Interactive::No)
                        .child(row)
                } else {
                    row
                }
            }))
            .maybe_child(url_bar_row)
            .child(sidebar_body)
            .maybe_child((!create_form_open).then_some(SidebarMediaControls {
                collapsed: is_collapsed_visual,
            }))
            .maybe_child((!create_form_open).then_some(notification_stack).flatten())
            .child(if create_form_open {
                // CRF-04: Dim and disable conflicting bottom-bar controls while Space Creation Form is open.
                rect()
                    .width(Size::fill())
                    .opacity(0.35)
                    .interactive(Interactive::No)
                    .child(bottom_bar)
            } else {
                bottom_bar
            })
            .maybe_child(top_overflow_popover)
            .maybe_child(overflow_popover)
            .maybe_child(active_drag.map(|drag| DragGhostOverlay {
                drag,
                sidebar_width: displayed_width,
            }))
    }
}

/// Interactive resize handle positioned between the vertical sidebar and the inset page card.
///
/// Matches Zen's `#zen-sidebar-splitter`: full-height (`calc(100% - 16px)`), `5px` wide accent bar fading in
/// (`0 -> 0.8`) after `200ms` hover delay (`200ms ease-in-out`), double-click resets width to default (`XSB-01`),
/// and dragging past the narrow threshold enters compact mode (`XSB-06`).
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
        let mut hover_revealed = use_state(|| false);
        let mut hover_seq = use_state(|| 0u64);
        let mut drag_anchor = use_state(|| None::<(f32, f32)>);
        let mut drag_collapse_armed = use_state(|| false);
        let mut last_click = use_state(|| None::<Instant>);

        let expanded = self.expanded;
        let side = self.side;
        let current_width = self.current_width;
        let mut is_resizing = self.is_resizing;

        let active = drag_anchor.read().is_some();
        let bar_visible = expanded && (active || *hover_revealed.read());

        // SB-01: 200ms ease-in-out fade to 0.8 opacity after 200ms hover delay.
        let fade_anim = use_animation_with_dependencies(&bar_visible, move |conf, shown| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *shown { (0.0, 0.8) } else { (0.8, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::SIDEBAR_SPLITTER_FADE)
                .ease(Ease::InOut)
                .function(Function::Quad)
        });

        let dbl_core = core.clone();
        let on_pointer_down = move |e: Event<PointerEventData>| {
            if !expanded || e.button() != Some(MouseButton::Left) {
                return;
            }
            let now = Instant::now();
            let is_double = last_click
                .peek()
                .is_some_and(|prev| now.duration_since(prev) <= DOUBLE_CLICK_WINDOW);
            if is_double {
                // XSB-01: double-click resets sidebar width to default (respecting layout min width).
                last_click.set(None);
                drag_anchor.set(None);
                drag_collapse_armed.set_if_modified(false);
                is_resizing.set_if_modified(false);
                let layout = dbl_core.model().peek().layout.clone();
                let default_w = SIDEBAR_DEFAULT_WIDTH.max(min_sidebar_width_for_layout(&layout));
                dbl_core.invoke_or_log(Command::SidebarSetWidth { width: default_w });
                return;
            }
            last_click.set(Some(now));
            drag_anchor.set(Some((e.global_location().x as f32, current_width)));
            drag_collapse_armed.set_if_modified(false);
            is_resizing.set_if_modified(true);
        };

        let move_core = core.clone();
        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            let Some((start_x, start_width)) = *drag_anchor.peek() else {
                return;
            };
            let px = e.global_location().x as f32;
            let raw = compute_raw_resized_sidebar_width(start_width, start_x, px, side);
            drag_collapse_armed.set_if_modified(should_drag_trigger_compact(raw));
            let layout = move_core.model().peek().layout.clone();
            let next_width = compute_resized_sidebar_width_for_layout(&layout, start_width, start_x, px);
            if (layout.sidebar_width - next_width).abs() >= 0.5 {
                move_core.invoke_or_log(Command::SidebarSetWidth { width: next_width });
            }
        };

        let up_core = core.clone();
        let on_global_pointer_up = move |_: Event<PointerEventData>| {
            let anchor = *drag_anchor.peek();
            if let Some((_, start_width)) = anchor {
                drag_anchor.set(None);
                is_resizing.set_if_modified(false);
                if *drag_collapse_armed.peek() {
                    drag_collapse_armed.set(false);
                    up_core.invoke_or_log(Command::SidebarSetWidth { width: start_width });
                    up_core.invoke_or_log(Command::LayoutSetCompactMode { enabled: true });
                }
            }
        };

        let bar_opacity = fade_anim.get().value().clamp(0.0, 0.8);

        rect()
            .width(Size::px(RESIZE_HANDLE_WIDTH))
            .height(Size::fill())
            .padding(Gaps::new(8.0, 0.0, 8.0, 0.0))
            .center()
            .cursor(if expanded {
                CursorIcon::ColResize
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(move |_| {
                let seq = hover_seq.peek().wrapping_add(1);
                hover_seq.set(seq);
                spawn(async move {
                    timer(motion::SIDEBAR_SPLITTER_DELAY).await;
                    if *hover_seq.peek() == seq {
                        hover_revealed.set_if_modified(true);
                    }
                });
            })
            .on_pointer_leave(move |_| {
                let seq = hover_seq.peek().wrapping_add(1);
                hover_seq.set(seq);
                hover_revealed.set_if_modified(false);
            })
            .on_pointer_down(on_pointer_down)
            .on_global_pointer_move(on_global_pointer_move)
            .on_global_pointer_up(on_global_pointer_up)
            .maybe_child((bar_opacity > 0.01).then(|| {
                rect()
                    .width(Size::px(5.0))
                    .height(Size::fill())
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(theme.color(ThemeRole::Accent))
                    .opacity(bar_opacity)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zendia_browser_core::BrowserCore;
    use zendia_browser_core::model::{LayoutSettings, ToolbarLayout};
    use zendia_browser_core::stub::StubBackend;

    #[test]
    fn bottom_row_keeps_agents_and_chat_inline_at_default_width_with_spaces() {
        let core = StubBackend::new();
        let items = core.model().contributions.toolbar_items;
        let single = LayoutSettings {
            toolbar_layout: ToolbarLayout::Single,
            sidebar_expanded: true,
            ..LayoutSettings::default()
        };
        let slots = ToolbarSlots::resolve(&items, &single);
        assert_eq!(
            slots.sidebar_bottom.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            vec!["builtin.library", "zendia.agents", "zendia.chat", "builtin.new_tab"]
        );

        for show_switcher in [false, true] {
            let (first, vis_mid, last, overflow, switcher_budget) =
                partition_sidebar_bottom_row(&slots.sidebar_bottom, show_switcher, SIDEBAR_DEFAULT_WIDTH, true);
            assert_eq!(first.as_ref().map(|i| i.id.as_str()), Some("builtin.library"));
            assert_eq!(last.as_ref().map(|i| i.id.as_str()), Some("builtin.new_tab"));
            assert_eq!(
                vis_mid.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
                vec!["zendia.agents", "zendia.chat"]
            );
            assert!(overflow.is_empty(), "Agents and Chat must never overflow at 186px");
            if show_switcher {
                assert!(
                    switcher_budget >= 40.0,
                    "space switcher needs >=40px budget at 186px, got {switcher_budget}"
                );
            }
        }
    }
}
