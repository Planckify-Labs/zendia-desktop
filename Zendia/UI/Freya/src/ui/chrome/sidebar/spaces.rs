/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Space header above the active space's tabs (`REN-04`, `SPC-04`) and the space switcher icon strip in the sidebar
//! foot row (`SPC-01`, `SPC-02`, `SPC-06`, `DND-01`, `DND-02`).

use crate::ui::chrome::layout::ChromePalette;
use crate::ui::chrome::menus::{open_space_context_menu, try_use_menu_state};
use crate::ui::chrome::sidebar::state::try_use_sidebar_state;
use crate::ui::chrome::sidebar::tab_row::InlineRenameField;
use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, KitTooltip};
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::{Duration, Instant};
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{Space, SpaceId};

const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);
const SPACE_DRAG_THRESHOLD_PX: f32 = 5.0;

fn is_emoji_str(s: &str) -> bool {
    !s.is_empty() && !s.is_ascii()
}

/// Header above the space's pinned tabs and folders displaying the active space's icon, name, collapsible pinned-tabs
/// chevron, and hover-revealed `...` space actions button (`REN-04`, `SPC-04`).
#[derive(Clone, PartialEq)]
pub struct SpaceHeader {
    pub space: Option<Space>,
    pub collapsed: bool,
    pub has_pinned_tabs: bool,
}

impl Component for SpaceHeader {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let menu_state = try_use_menu_state();
        let sidebar_state = try_use_sidebar_state();
        let mut hovered = use_state(|| false);
        let mut actions_hovered = use_state(|| false);
        let mut actions_pos = use_state(|| (140.0_f32, 180.0_f32));
        let mut last_click = use_state(|| None::<Instant>);
        let a11y_id = use_a11y();

        let space_id = self.space.as_ref().map(|s| s.id);
        let name = self
            .space
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "Space".to_string());
        let icon = self.space.as_ref().map(|s| s.icon.clone()).unwrap_or_default();
        let is_renaming = space_id.is_some() && sidebar_state.is_some_and(|s| *s.renaming_space.read() == space_id);
        let pinned_collapsed = space_id
            .zip(sidebar_state)
            .is_some_and(|(id, st)| st.is_pinned_collapsed(id));

        // SPC-04: Hover fades in the `...` actions button over 100ms (`zen-workspaces.css:280-322`).
        let show_actions = hovered() && !is_renaming && !self.collapsed && space_id.is_some();
        let actions_fade = use_animation_with_dependencies(&show_actions, move |conf, shown| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *shown { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(Duration::from_millis(100))
                .ease(Ease::InOut)
                .function(Function::Quad)
        });

        // SPC-04: Chevron rotates 90deg (expanded) <-> 0deg (collapsed) over 100ms.
        let chevron_rot = use_animation_with_dependencies(&pinned_collapsed, move |conf, col| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *col { (90.0, 0.0) } else { (0.0, 90.0) };
            AnimNum::new(from, to)
                .duration(Duration::from_millis(100))
                .ease(Ease::Out)
                .function(Function::Quad)
        });

        let menu_core = core.clone();
        let on_mouse_down = move |e: Event<MouseEventData>| {
            if e.button == Some(MouseButton::Right)
                && let Some(id) = space_id
            {
                e.stop_propagation();
                if let Some(menu_state) = &menu_state {
                    let model = menu_core.model().peek().clone();
                    let position = (e.global_location.x as f32, e.global_location.y as f32);
                    open_space_context_menu(menu_state, &model, id, position);
                }
            }
        };

        if self.collapsed {
            let glyph: Element = if is_emoji_str(&icon) {
                label().text(icon).font_size(13.0).color(palette.text).into_element()
            } else if !icon.is_empty() {
                Icon::new(icon).size(14.0).role(ThemeRole::TextMuted).into_element()
            } else {
                Icon::new("library-spaces")
                    .size(14.0)
                    .role(ThemeRole::TextMuted)
                    .into_element()
            };
            return rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(format!("Space: {name}"))
                .width(Size::fill())
                .height(Size::px(32.0))
                .corner_radius(CornerRadius::new_all(8.0))
                .background(if hovered() {
                    palette.tab_hover_bg
                } else {
                    Color::TRANSPARENT
                })
                .center()
                .on_pointer_enter(move |_| hovered.set_if_modified(true))
                .on_pointer_leave(move |_| hovered.set_if_modified(false))
                .on_mouse_down(on_mouse_down)
                .child(glyph)
                .into_element();
        }

        let name_for_dbl = name.clone();
        let on_press = move |_: Event<PressEventData>| {
            if is_renaming {
                return;
            }
            let now = Instant::now();
            let is_double = last_click
                .peek()
                .is_some_and(|prev| now.duration_since(prev) <= DOUBLE_CLICK_WINDOW);
            if is_double && let (Some(id), Some(state)) = (space_id, sidebar_state) {
                last_click.set(None);
                state.start_space_rename(id, name_for_dbl.clone());
            } else {
                last_click.set(Some(now));
            }
        };

        // SPC-04: Leading icon / collapsible pinned-tabs chevron on hover when pinned tabs exist.
        let show_chevron = self.has_pinned_tabs && (hovered() || pinned_collapsed);
        let leading_el: Option<Element> = if show_chevron && let (Some(id), Some(state)) = (space_id, sidebar_state) {
            let deg = chevron_rot.get().value();
            Some(
                KitTooltip::new(if pinned_collapsed {
                    "Show pinned tabs"
                } else {
                    "Hide pinned tabs"
                })
                .child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("Toggle pinned tabs")
                        .width(Size::px(18.0))
                        .height(Size::px(18.0))
                        .corner_radius(CornerRadius::new_all(4.0))
                        .center()
                        .cursor(CursorIcon::Pointer)
                        .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            state.toggle_pinned_collapsed(id);
                        })
                        .child(
                            rect()
                                .rotate(deg)
                                .center()
                                .child(Icon::new("chevron-right").size(12.0).role(ThemeRole::TextMuted)),
                        ),
                )
                .into_element(),
            )
        } else if is_emoji_str(&icon) {
            Some(label().text(icon).font_size(13.0).color(palette.text).into_element())
        } else if !icon.is_empty() {
            Some(Icon::new(icon).size(13.5).role(ThemeRole::Text).into_element())
        } else {
            None
        };

        let title_el: Element = if is_renaming && let (Some(id), Some(state)) = (space_id, sidebar_state) {
            InlineRenameField {
                draft: state.rename_draft,
                placeholder: "Space name",
                on_commit: EventHandler::new({
                    let core = core.clone();
                    move |text: String| {
                        state.cancel_rename();
                        let trimmed = text.trim().to_string();
                        if !trimmed.is_empty() {
                            core.invoke_or_log(Command::SpaceRename {
                                space_id: id,
                                name: trimmed,
                            });
                        }
                    }
                }),
                on_cancel: EventHandler::new(move |_| {
                    state.cancel_rename();
                }),
                key: DiffKey::from(&("space-rename", id.0)),
            }
            .into_element()
        } else {
            label()
                .text(name.clone())
                .width(Size::flex(1.0))
                .font_size(12.5)
                .font_weight(FontWeight::SEMI_BOLD)
                .color(palette.text)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .into_element()
        };

        let actions_opacity = actions_fade.get().value().clamp(0.0, 1.0);
        let actions_core = core.clone();
        let actions_btn = (actions_opacity > 0.02 && space_id.is_some()).then(|| {
            let id = space_id.expect("checked");
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Space actions")
                .width(Size::px(22.0))
                .height(Size::px(22.0))
                .corner_radius(CornerRadius::new_all(5.0))
                .background(if actions_hovered() {
                    palette.surface_pill
                } else {
                    Color::TRANSPARENT
                })
                .opacity(actions_opacity)
                .center()
                .cursor(CursorIcon::Pointer)
                .on_sized(move |e: Event<SizedEventData>| {
                    actions_pos.set_if_modified((e.area.min_x(), e.area.max_y() + 4.0));
                })
                .on_pointer_enter(move |_| actions_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| actions_hovered.set_if_modified(false))
                .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    if let Some(ms) = &menu_state {
                        let model = actions_core.model().peek().clone();
                        open_space_context_menu(ms, &model, id, *actions_pos.peek());
                    }
                })
                .child(Icon::new("more-horizontal").size(13.0).role(if actions_hovered() {
                    ThemeRole::Text
                } else {
                    ThemeRole::TextMuted
                }))
        });

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(!is_renaming)
            .a11y_role(AccessibilityRole::Heading)
            .a11y_alt(format!("Space: {name}"))
            .width(Size::fill())
            .height(Size::px(32.0))
            .padding(Gaps::new(0.0, 6.0, 0.0, 8.0))
            .spacing(6.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(8.0))
            .background(if hovered() || is_renaming {
                palette.tab_hover_bg
            } else {
                Color::TRANSPARENT
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(on_press)
            .on_mouse_down(on_mouse_down)
            .maybe_child(leading_el)
            .child(title_el)
            .maybe_child(actions_btn)
            .into_element()
    }
}

/// Interactive space switcher icons in the sidebar bottom row (`SPC-01`, `SPC-02`, `SPC-06`, `DND-02`).
#[derive(Clone, PartialEq)]
pub struct SpaceSwitcher {
    pub spaces: Vec<Space>,
    pub active_space: Option<SpaceId>,
    pub compact: bool,
    pub vertical: bool,
    /// Horizontal width budget available between leading and trailing bottom-bar buttons.
    pub max_width: Option<f32>,
}

impl Component for SpaceSwitcher {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let active_space = self.active_space;
        let compact = self.compact;
        let base_size = if compact { 19.0 } else { 24.0 };
        let normal_gap = if compact { 2.0 } else { 4.0 };
        let full_needed_w =
            self.spaces.len() as f32 * base_size + self.spaces.len().saturating_sub(1) as f32 * normal_gap;
        let overflow_mode = !self.vertical
            && (self.max_width.is_some_and(|limit| full_needed_w > limit)
                || self.spaces.len() > 3
                || (compact && self.spaces.len() > 2));
        let active_w = if overflow_mode && compact { 18.0 } else { base_size };
        let dot_w = if overflow_mode && self.spaces.len() > 1 {
            if let Some(limit) = self.max_width {
                ((limit - active_w) / (self.spaces.len() - 1) as f32)
                    .floor()
                    .clamp(6.0, 10.0)
            } else {
                10.0
            }
        } else {
            10.0
        };

        let wheel_core = core.clone();
        let on_wheel = move |e: Event<WheelEventData>| {
            // SPC-02: Wheel gesture over space switcher steps through spaces smoothly.
            let delta = if e.delta_x.abs() >= e.delta_y.abs() {
                e.delta_x
            } else {
                e.delta_y
            };
            if delta.abs() >= 0.2 {
                e.stop_propagation();
                if delta < 0.0 {
                    wheel_core.invoke_or_log(Command::SpaceNext);
                } else {
                    wheel_core.invoke_or_log(Command::SpacePrevious);
                }
            }
        };

        let container = if self.vertical {
            rect()
                .direction(Direction::Vertical)
                .cross_align(Alignment::center())
                .spacing(3.0)
        } else {
            rect()
                .width(Size::flex(1.0))
                .overflow(Overflow::Clip)
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .main_align(Alignment::center())
                .spacing(if overflow_mode { 0.0 } else { normal_gap })
                .on_wheel(on_wheel)
        };

        container.children(self.spaces.iter().enumerate().map(|(idx, space)| {
            SpaceSwitcherButton {
                space: space.clone(),
                index: idx,
                active: active_space == Some(space.id),
                compact,
                overflow_mode,
                active_w,
                dot_w,
                key: DiffKey::from(&space.id.0),
            }
            .into_element()
        }))
    }
}

#[derive(Clone, PartialEq)]
struct SpaceSwitcherButton {
    space: Space,
    index: usize,
    active: bool,
    compact: bool,
    overflow_mode: bool,
    active_w: f32,
    dot_w: f32,
    key: DiffKey,
}

impl KeyExt for SpaceSwitcherButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SpaceSwitcherButton {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let menu_state = try_use_menu_state();
        let sidebar_state = try_use_sidebar_state();
        let mut hovered = use_state(|| false);
        let mut press_origin = use_state(|| None::<(f32, f32)>);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let space_id = self.space.id;
        let index = self.index;
        let is_hovered = hovered();

        let (reorder_active, is_dragged_space) = match sidebar_state.and_then(|s| *s.dragging_space.read()) {
            Some((drag_id, _)) => (true, drag_id == space_id),
            None => (false, false),
        };

        // SPC-06 & SPC-01: Smooth 180ms opacity transition (0.7 rest, 1.0 active/hover/dragged, 0.2 dimmed during reorder).
        let target_opacity = if reorder_active && !is_dragged_space {
            0.20
        } else if self.active || is_hovered || is_dragged_space {
            1.0
        } else {
            0.70
        };
        let opacity_anim = use_animation_with_dependencies(&((target_opacity * 100.0) as i32), move |conf, pct| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let to = (*pct as f32) / 100.0;
            AnimNum::new(to, to)
                .duration(motion::SPACE_ICON_TRANSITION)
                .ease(Ease::Out)
                .function(Function::Quad)
        });

        // SPC-02: In overflow dot mode, inactive unhovered spaces collapse to compact dots and expand over 150ms.
        let collapsed_to_dot = self.overflow_mode && !self.active && !is_hovered && !is_dragged_space;
        let base_size = self.active_w;
        let dot_size = self.dot_w;
        let width_anim = use_animation_with_dependencies(&collapsed_to_dot, move |conf, dot_mode| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *dot_mode {
                (base_size, dot_size)
            } else {
                (dot_size, base_size)
            };
            AnimNum::new(from, to)
                .duration(Duration::from_millis(150))
                .ease(Ease::InOut)
                .function(Function::Quad)
        });

        let switch_core = core.clone();
        let on_press = move |_: Event<PressEventData>| {
            switch_core.invoke_or_log(Command::SpaceSwitch { space_id });
        };

        let menu_core = core.clone();
        let on_mouse_down = move |e: Event<MouseEventData>| {
            if e.button == Some(MouseButton::Right) {
                e.stop_propagation();
                if let Some(menu_state) = &menu_state {
                    let model = menu_core.model().peek().clone();
                    let position = (e.global_location.x as f32, e.global_location.y as f32);
                    open_space_context_menu(menu_state, &model, space_id, position);
                }
            }
        };

        // SPC-01: Drag-to-reorder space icons (5px threshold).
        let on_pointer_down = move |e: Event<PointerEventData>| {
            if e.button() == Some(MouseButton::Left) {
                press_origin.set(Some((e.global_location().x as f32, e.global_location().y as f32)));
            }
        };
        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            let Some((ox, oy)) = *press_origin.peek() else {
                return;
            };
            let px = e.global_location().x as f32;
            let py = e.global_location().y as f32;
            let dist = ((px - ox).powi(2) + (py - oy).powi(2)).sqrt();
            if dist >= SPACE_DRAG_THRESHOLD_PX
                && let Some(state) = sidebar_state
            {
                let mut ds = state.dragging_space;
                if ds.peek().is_none() {
                    ds.set(Some((space_id, index)));
                }
            }
        };
        let reorder_core = core.clone();
        let on_global_pointer_up = move |_: Event<PointerEventData>| {
            if press_origin.peek().is_none() {
                return;
            }
            press_origin.set(None);
            if let Some(state) = sidebar_state {
                let mut ds = state.dragging_space;
                let current_drag = *ds.peek();
                if let Some((drag_id, target_idx)) = current_drag
                    && drag_id == space_id
                {
                    ds.set(None);
                    if target_idx != index {
                        reorder_core.invoke_or_log(Command::SpaceReorder {
                            space_id: drag_id,
                            index: target_idx,
                        });
                    }
                }
            }
        };

        // DND-02: Dragging a tab over a space switcher icon immediately switches to that space!
        let drag_switch_core = core.clone();
        let on_pointer_enter = move |_| {
            hovered.set_if_modified(true);
            if let Some(state) = sidebar_state {
                let mut ds = state.dragging_space;
                let current_drag = *ds.peek();
                if let Some((drag_id, _)) = current_drag
                    && drag_id != space_id
                {
                    ds.set(Some((drag_id, index)));
                }
                let tab_dragging = state.drag_state.peek().as_ref().is_some_and(|d| !d.is_preview);
                if tab_dragging {
                    drag_switch_core.invoke_or_log(Command::SpaceSwitch { space_id });
                }
            }
        };

        let bg = if self.active || is_dragged_space {
            palette.surface_pill
        } else if is_hovered {
            palette.tab_hover_bg
        } else {
            Color::TRANSPARENT
        };

        let dot_color = match theme.scheme {
            ResolvedScheme::Dark => {
                Rgba::from_rgb_alpha(255, 255, 255, if self.active { 0.90 } else { 0.45 }).to_color()
            }
            ResolvedScheme::Light => Rgba::from_rgb_alpha(24, 28, 42, if self.active { 0.85 } else { 0.40 }).to_color(),
        };

        let icon_el: Element = if collapsed_to_dot {
            rect()
                .width(Size::px(4.0))
                .height(Size::px(4.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(dot_color)
                .into_element()
        } else if is_emoji_str(&self.space.icon) {
            label()
                .text(self.space.icon.clone())
                .font_size(if self.compact { 11.0 } else { 12.5 })
                .color(palette.text)
                .into_element()
        } else if !self.space.icon.is_empty() {
            Icon::new(self.space.icon.clone())
                .size(if self.compact { 11.5 } else { 13.0 })
                .role(if self.active || is_hovered {
                    ThemeRole::Text
                } else {
                    ThemeRole::TextMuted
                })
                .into_element()
        } else {
            rect()
                .width(Size::px(if self.active { 7.0 } else { 6.0 }))
                .height(Size::px(if self.active { 7.0 } else { 6.0 }))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(dot_color)
                .into_element()
        };

        let focus_border = if focus() == Focus::Keyboard {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::FocusRing))
        } else {
            Border::new().width(0.0).fill(Color::TRANSPARENT)
        };

        let rendered_w = if self.overflow_mode {
            width_anim.get().value().clamp(dot_size, base_size)
        } else {
            base_size
        };
        let rendered_h = if self.compact { 20.0 } else { 24.0 };
        let rendered_opacity = opacity_anim.get().value().clamp(0.15, 1.0);

        KitTooltip::new(self.space.name.clone()).enabled(!reorder_active).child(
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(format!("Switch to {} space", self.space.name))
                .width(Size::px(rendered_w))
                .height(Size::px(rendered_h))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(bg)
                .border(focus_border)
                .opacity(rendered_opacity)
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(on_pointer_enter)
                .on_pointer_leave(move |_| hovered.set_if_modified(false))
                .on_pointer_down(on_pointer_down)
                .on_global_pointer_move(on_global_pointer_move)
                .on_global_pointer_up(on_global_pointer_up)
                .on_press(on_press)
                .on_mouse_down(on_mouse_down)
                .child(icon_el),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
