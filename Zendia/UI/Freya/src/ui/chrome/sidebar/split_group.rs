/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! A split group in the sidebar: one row holding a chip per pane, as Zen draws it.

use crate::ui::chrome::layout::ChromePalette;
use crate::ui::chrome::menus::{open_tab_context_menu, try_use_menu_state};
use crate::ui::chrome::sidebar::state::{SidebarDragState, try_use_sidebar_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, use_interactive_feedback};
use crate::ui::theme::{ThemeRole, use_theme};
use crate::ui::vim::explorer::{apply_explorer_background, apply_explorer_border};
use crate::ui::vim::hints::hint_badge_for_row;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::focus::FocusTarget;
use zendia_browser_core::model::{SplitGroup, Tab, TabId};

const ROW_HEIGHT: f32 = 36.0;
const DRAG_THRESHOLD_PX: f32 = 6.0;

#[derive(Clone, PartialEq)]
pub struct SplitGroupRow {
    pub group: SplitGroup,
    /// The group's tabs, in pane order.
    pub tabs: Vec<Tab>,
    pub active_tab: Option<TabId>,
    pub collapsed: bool,
}

impl Component for SplitGroupRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let group_active = self.tabs.iter().any(|tab| Some(tab.id) == self.active_tab);

        let divider_color = if theme.scheme.is_dark() {
            Color::from_argb(51, 255, 255, 255)
        } else {
            Color::from_argb(26, 0, 0, 0)
        };
        let mut chips: Vec<Element> = Vec::with_capacity(self.tabs.len() * 2);
        for (idx, tab) in self.tabs.iter().enumerate() {
            let this_active = Some(tab.id) == self.active_tab;
            if idx > 0 {
                let prev_active = Some(self.tabs[idx - 1].id) == self.active_tab;
                if !prev_active && !this_active {
                    let divider = if self.collapsed {
                        rect()
                            .key(("sep", idx))
                            .width(Size::px(16.0))
                            .height(Size::px(1.0))
                            .background(divider_color)
                    } else {
                        rect()
                            .key(("sep", idx))
                            .width(Size::px(1.0))
                            .height(Size::px(16.0))
                            .background(divider_color)
                    };
                    chips.push(divider.into_element());
                }
            }
            chips.push(
                SplitGroupChip {
                    tab: tab.clone(),
                    active: this_active,
                    group_active,
                    collapsed: self.collapsed,
                    allow_close_btn: self.tabs.len() <= 2,
                }
                .into_element(),
            );
        }

        let base_bg = if group_active {
            palette.tab_selected_bg
        } else {
            palette.tab_hover_bg
        };
        let focus_target = FocusTarget::SplitGroup(self.group.id);
        let (bg, border, hint_badge) = {
            let model = core.model().read();
            let base_border = Border::new().width(0.0).fill(Color::TRANSPARENT);
            (
                apply_explorer_background(base_bg, &model, &focus_target, &theme),
                apply_explorer_border(base_border, &model, &focus_target, &theme),
                hint_badge_for_row(&model, &focus_target, ROW_HEIGHT, &theme),
            )
        };

        let row = rect()
            .padding(Gaps::new_all(3.0))
            .spacing(3.0)
            .corner_radius(CornerRadius::new_all(8.0))
            .background(bg)
            .border(border)
            .a11y_role(AccessibilityRole::Group)
            .a11y_alt("Split view");
        if self.collapsed {
            row.cross_align(Alignment::center())
                .children(chips)
                .maybe_child(hint_badge)
        } else {
            row.width(Size::fill())
                .height(Size::px(ROW_HEIGHT))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .cross_align(Alignment::center())
                .children(chips)
                .maybe_child(hint_badge)
        }
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.group.id.0)
    }
}

#[derive(Clone, PartialEq)]
struct SplitGroupChip {
    tab: Tab,
    active: bool,
    group_active: bool,
    collapsed: bool,
    allow_close_btn: bool,
}

impl Component for SplitGroupChip {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = try_use_menu_state();
        let sidebar_state = try_use_sidebar_state();
        let feedback = use_interactive_feedback(true);
        let mut close_hovered = use_state(|| false);
        let mut press_origin = use_state(|| None::<(f32, f32)>);
        let tab_id = self.tab.id;
        let drag_title = self.tab.display_title();
        let drag_favicon = self.tab.favicon.clone();

        let mut fb_down = feedback.on_pointer_down();
        let on_pointer_down = move |e: Event<PointerEventData>| {
            let is_left = e.button() == Some(MouseButton::Left);
            let loc = (e.global_location().x as f32, e.global_location().y as f32);
            fb_down(e);
            if is_left {
                press_origin.set(Some(loc));
            }
        };
        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            let Some((ox, oy)) = *press_origin.peek() else {
                return;
            };
            let px = e.global_location().x as f32;
            let py = e.global_location().y as f32;
            let dist = ((px - ox).powi(2) + (py - oy).powi(2)).sqrt();
            if let Some(state) = sidebar_state {
                let mut drag = state.drag_state;
                let dragging_this = drag
                    .peek()
                    .as_ref()
                    .is_some_and(|d| d.tab_id == tab_id && !d.is_preview);
                if dragging_this {
                    if let Some(current) = drag.write().as_mut() {
                        current.pointer_x = px;
                        current.pointer_y = py;
                    }
                } else if dist >= DRAG_THRESHOLD_PX {
                    drag.set(Some(SidebarDragState {
                        tab_id,
                        tab_title: drag_title.clone(),
                        tab_favicon: drag_favicon.clone(),
                        pointer_x: px,
                        pointer_y: py,
                        target: None,
                        page_drop: None,
                        is_preview: false,
                    }));
                }
            }
        };
        let drop_core = core.clone();
        let mut fb_up = feedback.on_pointer_up();
        let on_global_pointer_up = move |e: Event<PointerEventData>| {
            fb_up(e);
            if press_origin.peek().is_none() {
                return;
            }
            press_origin.set(None);
            if let Some(state) = sidebar_state {
                let finished = {
                    let mut drag = state.drag_state;
                    if drag
                        .peek()
                        .as_ref()
                        .is_some_and(|d| d.tab_id == tab_id && !d.is_preview)
                    {
                        drag.write().take()
                    } else {
                        None
                    }
                };
                if let Some(drag) = finished {
                    state.clear_drag_extras();
                    if let Some(page_drop) = drag.page_drop {
                        drop_core.invoke_or_log(Command::SplitDrop {
                            tab_id,
                            target_tab_id: page_drop.target_tab,
                            zone: page_drop.zone,
                        });
                    } else if let Some(target) = drag.target {
                        drop_core.invoke_or_log(Command::TabMove { tab_id, target });
                    }
                }
            }
        };

        let (rest_bg, hover_bg, chip_border) = if self.active {
            if theme.scheme.is_dark() {
                (
                    Color::from_argb(46, 255, 255, 255),
                    Color::from_argb(56, 255, 255, 255),
                    Color::from_argb(36, 255, 255, 255),
                )
            } else {
                (
                    Color::from_argb(235, 255, 255, 255),
                    Color::from_argb(245, 255, 255, 255),
                    Color::from_argb(26, 0, 0, 0),
                )
            }
        } else if self.group_active {
            (
                theme.rgba(ThemeRole::Text).with_alpha(0.04).to_color(),
                theme.rgba(ThemeRole::Text).with_alpha(0.09).to_color(),
                Color::TRANSPARENT,
            )
        } else {
            (
                Color::TRANSPARENT,
                theme.rgba(ThemeRole::Text).with_alpha(0.06).to_color(),
                Color::TRANSPARENT,
            )
        };
        let press_bg = theme.rgba(ThemeRole::Text).with_alpha(0.16).to_color();
        let bg = feedback.background(rest_bg, hover_bg, press_bg);

        let menu_core = core.clone();
        let close_core = core.clone();

        let show_close = !self.collapsed && self.allow_close_btn && feedback.is_hovered();
        let close_btn = show_close.then(|| {
            rect()
                .width(Size::px(18.0))
                .height(Size::px(18.0))
                .corner_radius(CornerRadius::new_all(4.0))
                .background(if *close_hovered.read() {
                    theme.rgba(ThemeRole::Text).with_alpha(0.14).to_color()
                } else {
                    Color::TRANSPARENT
                })
                .center()
                .cursor(CursorIcon::Pointer)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Close pane tab")
                .on_pointer_enter(move |_| close_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| close_hovered.set_if_modified(false))
                .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    close_core.invoke_or_log(Command::TabClose {
                        tab_id: Some(tab_id),
                        force: false,
                    });
                })
                .child(Icon::new("close").size(10.0).role(ThemeRole::TextMuted))
        });

        let chip = rect()
            .height(Size::fill())
            .padding(Gaps::new(0.0, 6.0, 0.0, 6.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .border(
                Border::new()
                    .width(if self.active { 1.0 } else { 0.0 })
                    .alignment(BorderAlignment::Inner)
                    .fill(chip_border),
            )
            .scale(feedback.scale())
            .cursor(CursorIcon::Pointer)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(5.0)
            .overflow(Overflow::Clip)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_alt(self.tab.display_title())
            .a11y_focusable(true)
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(on_pointer_down)
            .on_global_pointer_move(on_global_pointer_move)
            .on_global_pointer_up(on_global_pointer_up)
            .on_press(move |_| {
                core.invoke_or_log(Command::TabActivate { tab_id });
            })
            .on_mouse_down(move |e: Event<MouseEventData>| {
                if e.button == Some(MouseButton::Right) {
                    e.stop_propagation();
                    if let Some(menu_state) = &menu_state {
                        let model = menu_core.model().peek().clone();
                        let position = (e.global_location.x as f32, e.global_location.y as f32);
                        open_tab_context_menu(menu_state, &model, tab_id, position);
                    }
                } else if e.button == Some(MouseButton::Middle) {
                    e.stop_propagation();
                    menu_core.invoke_or_log(Command::TabClose {
                        tab_id: Some(tab_id),
                        force: false,
                    });
                }
            })
            .child(
                Icon::new(self.tab.favicon.clone().unwrap_or_else(|| "globe".to_string()))
                    .size(14.0)
                    .role(ThemeRole::Text),
            );
        if self.collapsed {
            chip.width(Size::px(32.0))
                .height(Size::px(28.0))
                .main_align(Alignment::center())
        } else {
            chip.width(Size::flex(1.0))
                .child(
                    label()
                        .text(self.tab.display_title())
                        .width(Size::flex(1.0))
                        .font_size(12.5)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                        .color(theme.color(ThemeRole::Text)),
                )
                .maybe_child(close_btn)
        }
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.tab.id.0)
    }
}
