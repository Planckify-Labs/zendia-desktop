/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Vertical sidebar frame: top toolbar row (in single-toolbar layout), space header, vertical tab column, bottom row,
//! expand/collapse motion, and the interactive width resize handle.

use crate::ui::chrome::context_menu_seam;
use crate::ui::chrome::layout::{
    ChromePalette, RESIZE_HANDLE_WIDTH, SIDEBAR_EXPAND_DURATION, SIDEBAR_PADDING_COLLAPSED, SIDEBAR_PADDING_EXPANDED,
    SIDEBAR_TOP_ROW_HEIGHT, TAB_ROW_HEIGHT, TAB_ROW_RADIUS, ToolbarSlots, compute_resized_sidebar_width,
    show_window_controls_in_sidebar_top,
};
use crate::ui::chrome::toolbar::{ToolbarItemView, WindowControls};
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::theme::{ResolvedScheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{SIDEBAR_COLLAPSED_WIDTH, SidebarSide, Tab};

/// Vertical sidebar frame hosting top controls (single toolbar), space header, tab list, and bottom actions.
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

        let (layout, active_space_name, spaces, tabs, active_tab_id) = {
            let model = core.model().read();
            let space_name = model
                .active_space()
                .map(|space| space.name.clone())
                .unwrap_or_else(|| "Space".to_string());
            let spaces: Vec<(zendia_browser_core::model::SpaceId, String, String, bool)> = model
                .spaces
                .iter()
                .map(|space| {
                    (
                        space.id,
                        space.name.clone(),
                        space.icon.clone(),
                        model.active_space == Some(space.id),
                    )
                })
                .collect();
            (
                model.layout.clone(),
                space_name,
                spaces,
                model.tabs.clone(),
                model.active_tab,
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

        let is_collapsed_visual = !expanded && anim_factor < 0.35;
        let show_top_row = !layout.has_top_toolbar() && !is_collapsed_visual;
        let show_url_bar = !self.slots.sidebar_url_bar.is_empty() && !is_collapsed_visual;
        let show_sidebar_window_controls = show_window_controls_in_sidebar_top(&layout);

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

        let space_initial = active_space_name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "S".to_string());

        let space_header = if is_collapsed_visual {
            rect().width(Size::fill()).height(Size::px(34.0)).center().child(
                label()
                    .text(space_initial)
                    .font_size(13.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(palette.text_muted),
            )
        } else {
            rect()
                .width(Size::fill())
                .height(Size::px(34.0))
                .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .child(
                    label()
                        .text(active_space_name)
                        .font_size(12.5)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(palette.text)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
        };

        let hairline = rect().width(Size::fill()).padding(Gaps::new(0.0, 4.0, 4.0, 4.0)).child(
            rect()
                .width(Size::fill())
                .height(Size::px(1.0))
                .background(palette.divider),
        );

        let new_tab_row = NewTabRow {
            collapsed: is_collapsed_visual,
        };

        let tab_rows = tabs.into_iter().map(|tab| {
            let is_active = active_tab_id == Some(tab.id);
            let tab_id = tab.id;
            VerticalTabRow {
                tab,
                active: is_active,
                collapsed: is_collapsed_visual,
                key: DiffKey::from(&tab_id.0),
            }
            .into_element()
        });

        let bottom_items = self.slots.sidebar_bottom_for_side(layout.sidebar_side);
        let show_space_switcher = layout.has_top_toolbar() && !is_collapsed_visual && spaces.len() > 1;

        let bottom_bar = if is_collapsed_visual {
            rect()
                .width(Size::fill())
                .padding(Gaps::new(4.0, 0.0, 2.0, 0.0))
                .spacing(4.0)
                .cross_align(Alignment::center())
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
                .padding(Gaps::new(2.0, 2.0, 0.0, 2.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .main_align(Alignment::SpaceBetween)
                .cross_align(Alignment::center())
                .spacing(4.0)
                .maybe_child(first_item.map(|item| {
                    ToolbarItemView::new(item.clone(), location, location_a11y_id)
                        .dense(true)
                        .key(item.id.as_str().to_string())
                }))
                .maybe_child(show_space_switcher.then_some(SpaceSwitcherPlaceholder { spaces }))
                .child(
                    rect()
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(4.0)
                        .children(trailing_items.into_iter().map(|item| {
                            ToolbarItemView::new(item.clone(), location, location_a11y_id)
                                .dense(true)
                                .key(item.id.as_str().to_string())
                                .into_element()
                        })),
                )
        };

        let pad = if is_collapsed_visual {
            SIDEBAR_PADDING_COLLAPSED
        } else {
            SIDEBAR_PADDING_EXPANDED
        };
        let top_pad = if layout.has_top_toolbar() { 0.0 } else { 4.0 };

        rect()
            .width(Size::px(displayed_width))
            .height(Size::fill())
            .padding(Gaps::new(top_pad, pad, pad, pad))
            .content(Content::Flex)
            .overflow(Overflow::Clip)
            .maybe_child(sidebar_top_row)
            .maybe_child(url_bar_row)
            .child(space_header)
            .child(hairline)
            .child(new_tab_row)
            .child(
                ScrollView::new().width(Size::fill()).height(Size::flex(1.0)).child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(4.0, 0.0, 4.0, 0.0))
                        .spacing(2.0)
                        .cross_align(if is_collapsed_visual {
                            Alignment::center()
                        } else {
                            Alignment::Start
                        })
                        .children(tab_rows),
                ),
            )
            .child(bottom_bar)
    }
}

#[derive(Clone, PartialEq)]
struct NewTabRow {
    collapsed: bool,
}

impl Component for NewTabRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);

        let bg = if hovered() {
            palette.tab_hover_bg
        } else {
            Color::TRANSPARENT
        };

        let on_press = move |_: Event<PressEventData>| {
            core.invoke_or_log(Command::TabNew {
                url: None,
                after: None,
                activate: true,
            });
        };

        if self.collapsed {
            rect()
                .width(Size::fill())
                .center()
                .padding(Gaps::new(2.0, 0.0, 2.0, 0.0))
                .child(
                    rect()
                        .a11y_focusable(true)
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("New Tab")
                        .width(Size::px(38.0))
                        .height(Size::px(TAB_ROW_HEIGHT))
                        .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
                        .background(bg)
                        .center()
                        .cursor(CursorIcon::Pointer)
                        .on_pointer_enter(move |_| hovered.set_if_modified(true))
                        .on_pointer_leave(move |_| hovered.set_if_modified(false))
                        .on_press(on_press)
                        .child(Icon::new("plus").size(15.0).role(ThemeRole::TextMuted)),
                )
        } else {
            rect()
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("New Tab")
                .width(Size::fill())
                .height(Size::px(TAB_ROW_HEIGHT))
                .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                .spacing(9.0)
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
                .background(bg)
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| hovered.set_if_modified(true))
                .on_pointer_leave(move |_| hovered.set_if_modified(false))
                .on_press(on_press)
                .child(Icon::new("plus").size(14.0).role(ThemeRole::TextMuted))
                .child(
                    label()
                        .text("New Tab")
                        .font_size(12.5)
                        .color(palette.text_muted)
                        .max_lines(1),
                )
        }
    }
}

#[derive(Clone, PartialEq)]
struct VerticalTabRow {
    tab: Tab,
    active: bool,
    collapsed: bool,
    key: DiffKey,
}

impl KeyExt for VerticalTabRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for VerticalTabRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);
        let tab_id = self.tab.id;

        let bg = if self.active {
            palette.tab_selected_bg
        } else if hovered() {
            palette.tab_hover_bg
        } else {
            Color::TRANSPARENT
        };

        let activate_core = core.clone();
        let on_press = move |_: Event<PressEventData>| {
            activate_core.invoke_or_log(Command::TabActivate { tab_id });
        };
        let on_mouse_down = move |e: Event<MouseEventData>| {
            if e.button == Some(MouseButton::Right) {
                e.stop_propagation();
                context_menu_seam::open_tab_context_menu(tab_id, e.global_location);
            }
        };

        let favicon_name = self.tab.favicon.clone().unwrap_or_else(|| "globe".to_string());
        let favicon_role = if self.active {
            ThemeRole::Accent
        } else {
            ThemeRole::TextMuted
        };

        let selected_shadow = (self.active && theme.scheme == ResolvedScheme::Light).then(|| {
            Shadow::new()
                .x(0.0)
                .y(1.0)
                .blur(3.0)
                .color(Color::from_argb(24, 0, 0, 0))
        });

        let favicon_tile = rect()
            .width(Size::px(18.0))
            .height(Size::px(18.0))
            .corner_radius(CornerRadius::new_all(4.0))
            .center()
            .child(Icon::new(favicon_name).size(14.0).role(favicon_role));

        if self.collapsed {
            let row = rect()
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Tab)
                .a11y_alt(self.tab.display_title())
                .width(Size::px(38.0))
                .height(Size::px(TAB_ROW_HEIGHT))
                .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
                .background(bg)
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| hovered.set_if_modified(true))
                .on_pointer_leave(move |_| hovered.set_if_modified(false))
                .on_press(on_press)
                .on_mouse_down(on_mouse_down)
                .child(favicon_tile);
            return match selected_shadow {
                Some(shadow) => row.shadow(shadow),
                None => row,
            };
        }

        let title = if self.tab.is_loading {
            format!("{}…", self.tab.display_title())
        } else {
            self.tab.display_title()
        };

        let close_core = core.clone();
        let show_close = hovered();

        let row = rect()
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_alt(title.clone())
            .width(Size::fill())
            .height(Size::px(TAB_ROW_HEIGHT))
            .padding(Gaps::new(0.0, 6.0, 0.0, 8.0))
            .spacing(8.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
            .background(bg)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(on_press)
            .on_mouse_down(on_mouse_down)
            .child(favicon_tile)
            .child(
                label()
                    .text(title)
                    .width(Size::flex(1.0))
                    .font_size(12.5)
                    .font_weight(if self.active {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .color(if self.active { palette.text } else { palette.text_muted })
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(show_close.then(|| {
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Close Tab")
                    .width(Size::px(20.0))
                    .height(Size::px(20.0))
                    .corner_radius(CornerRadius::new_all(5.0))
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        let force = close_core.model().peek().tab(tab_id).is_some_and(|t| t.closing);
                        close_core.invoke_or_log(Command::TabClose {
                            tab_id: Some(tab_id),
                            force,
                        });
                    })
                    .child(Icon::new("close").size(11.0).role(ThemeRole::TextMuted))
            }));

        match selected_shadow {
            Some(shadow) => row.shadow(shadow),
            None => row,
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Compact space-switcher placeholder in the sidebar bottom row (full spaces UI comes in phase 2).
#[derive(Clone, PartialEq)]
struct SpaceSwitcherPlaceholder {
    spaces: Vec<(zendia_browser_core::model::SpaceId, String, String, bool)>,
}

impl Component for SpaceSwitcherPlaceholder {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);

        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(4.0)
            .children(self.spaces.iter().map(|(space_id, name, icon, is_active)| {
                let space_id = *space_id;
                let switch_core = core.clone();
                rect()
                    .key(space_id.0)
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(format!("Switch to {name} space"))
                    .width(Size::px(22.0))
                    .height(Size::px(22.0))
                    .corner_radius(CornerRadius::new_all(6.0))
                    .background(if *is_active {
                        palette.surface_pill
                    } else {
                        Color::TRANSPARENT
                    })
                    .opacity(if *is_active { 1.0 } else { 0.55 })
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |_| {
                        switch_core.invoke_or_log(Command::SpaceSwitch { space_id });
                    })
                    .child(Icon::new(icon.clone()).size(12.0).role(if *is_active {
                        ThemeRole::Text
                    } else {
                        ThemeRole::TextMuted
                    }))
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
