/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Space header above the active space's tabs and the space switcher icon strip in the sidebar foot row.

use crate::ui::chrome::layout::ChromePalette;
use crate::ui::chrome::menus::{open_space_context_menu, try_use_menu_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{Space, SpaceId};

/// Header above the space's pinned tabs and folders displaying the active space's icon and name.
#[derive(Clone, PartialEq)]
pub struct SpaceHeader {
    pub space: Option<Space>,
    pub collapsed: bool,
}

impl Component for SpaceHeader {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let menu_state = try_use_menu_state();
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();

        let space_id = self.space.as_ref().map(|s| s.id);
        let name = self
            .space
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "Space".to_string());
        let icon = self.space.as_ref().map(|s| s.icon.clone()).unwrap_or_default();

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
            let initial = name
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "S".to_string());
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
                .child(if !icon.is_empty() {
                    Icon::new(icon).size(14.0).role(ThemeRole::TextMuted).into_element()
                } else {
                    label()
                        .text(initial)
                        .font_size(13.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(palette.text_muted)
                        .into_element()
                })
                .into_element();
        }

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Heading)
            .a11y_alt(format!("Space: {name}"))
            .width(Size::fill())
            .height(Size::px(32.0))
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .spacing(6.0)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(8.0))
            .background(if hovered() {
                palette.tab_hover_bg
            } else {
                Color::TRANSPARENT
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_mouse_down(on_mouse_down)
            .maybe_child((!icon.is_empty()).then(|| Icon::new(icon).size(13.5).role(ThemeRole::Text)))
            .child(
                label()
                    .text(name)
                    .font_size(12.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(palette.text)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .into_element()
    }
}

/// Interactive space switcher icons in the sidebar bottom row.
#[derive(Clone, PartialEq)]
pub struct SpaceSwitcher {
    pub spaces: Vec<Space>,
    pub active_space: Option<SpaceId>,
    pub compact: bool,
    pub vertical: bool,
}

impl Component for SpaceSwitcher {
    fn render(&self) -> impl IntoElement {
        let active_space = self.active_space;
        let compact = self.compact;

        let container = if self.vertical {
            rect()
                .direction(Direction::Vertical)
                .cross_align(Alignment::center())
                .spacing(3.0)
        } else {
            rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .main_align(Alignment::center())
                .spacing(if compact { 2.0 } else { 4.0 })
        };

        container.children(self.spaces.iter().map(|space| {
            SpaceSwitcherButton {
                space: space.clone(),
                active: active_space == Some(space.id),
                compact,
                key: DiffKey::from(&space.id.0),
            }
            .into_element()
        }))
    }
}

#[derive(Clone, PartialEq)]
struct SpaceSwitcherButton {
    space: Space,
    active: bool,
    compact: bool,
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
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let space_id = self.space.id;
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

        let size = if self.compact { 20.0 } else { 24.0 };
        let bg = if self.active {
            palette.surface_pill
        } else if hovered() {
            palette.tab_hover_bg
        } else {
            Color::TRANSPARENT
        };

        let dot_color = match theme.scheme {
            ResolvedScheme::Dark => {
                Rgba::from_rgb_alpha(255, 255, 255, if self.active { 0.90 } else { 0.40 }).to_color()
            }
            ResolvedScheme::Light => Rgba::from_rgb_alpha(24, 28, 42, if self.active { 0.85 } else { 0.35 }).to_color(),
        };

        let icon_el: Element = if !self.space.icon.is_empty() {
            Icon::new(self.space.icon.clone())
                .size(if self.compact { 11.5 } else { 13.0 })
                .role(if self.active || hovered() {
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

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Switch to {} space", self.space.name))
            .width(Size::px(size))
            .height(Size::px(size))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .border(focus_border)
            .opacity(if self.active || hovered() { 1.0 } else { 0.65 })
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(on_press)
            .on_mouse_down(on_mouse_down)
            .child(icon_el)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
