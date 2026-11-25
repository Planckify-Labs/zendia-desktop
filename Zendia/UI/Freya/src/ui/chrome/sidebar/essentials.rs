/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Shared Essentials tile grid and empty-essentials promo card (`ZenEssentialsPromo`) above the active space.

use crate::ui::chrome::layout::ChromePalette;
use crate::ui::chrome::menus::{open_tab_context_menu, try_use_menu_state};
use crate::ui::chrome::sidebar::state::{favicon_badge_colors, try_use_sidebar_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::kit::icons::prepared_icon_bytes;
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, use_theme};
use crate::ui::vim::explorer::{apply_explorer_background, apply_explorer_border};
use crate::ui::vim::hints::hint_badge_for_row;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::focus::FocusTarget;
use zendia_browser_core::model::{Tab, TabId, TabMoveTarget};

/// Height of an essential icon tile in expanded sidebar mode.
pub const ESSENTIAL_TILE_HEIGHT: f32 = 38.0;
/// Corner radius of an essential icon tile.
pub const ESSENTIAL_TILE_RADIUS: f32 = 9.0;

/// Grid of shared essential tab tiles (or the empty-essentials promo card) rendered above the active space.
#[derive(Clone, PartialEq)]
pub struct EssentialsGrid {
    pub tabs: Vec<Tab>,
    pub active_tab: Option<TabId>,
    pub collapsed: bool,
    pub show_promo: bool,
}

impl Component for EssentialsGrid {
    fn render(&self) -> impl IntoElement {
        let sidebar_state = try_use_sidebar_state();
        let is_dragging = sidebar_state.is_some_and(|s| s.drag_state.read().is_some());
        let drop_targeted = sidebar_state.is_some_and(|s| {
            matches!(
                s.drag_state.read().as_ref().and_then(|d| d.target),
                Some(TabMoveTarget::Essential { .. })
            )
        });

        if self.tabs.is_empty() {
            if !self.collapsed && (self.show_promo || is_dragging) {
                return rect()
                    .width(Size::fill())
                    .padding(Gaps::new(4.0, 2.0, 6.0, 2.0))
                    .on_pointer_enter(move |_| {
                        if let Some(state) = sidebar_state {
                            state.set_drag_target(TabMoveTarget::Essential { index: 0 });
                        }
                    })
                    .child(EssentialsPromoCard {
                        drag_over: drop_targeted,
                    })
                    .into_element();
            }
            return rect().into_element();
        }

        if self.collapsed {
            return rect()
                .width(Size::fill())
                .padding(Gaps::new(4.0, 0.0, 4.0, 0.0))
                .spacing(4.0)
                .cross_align(Alignment::center())
                .children(self.tabs.iter().enumerate().map(|(idx, tab)| {
                    EssentialTile {
                        tab: tab.clone(),
                        active: self.active_tab == Some(tab.id),
                        collapsed: true,
                        index: idx,
                        key: DiffKey::from(&tab.id.0),
                    }
                    .into_element()
                }))
                .into_element();
        }

        let active_tab = self.active_tab;
        let rows = self.tabs.chunks(3).enumerate().map(move |(row_idx, chunk)| {
            rect()
                .key(row_idx)
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .spacing(4.0)
                .children(chunk.iter().enumerate().map(|(col_idx, tab)| {
                    let index = row_idx * 3 + col_idx;
                    EssentialTile {
                        tab: tab.clone(),
                        active: active_tab == Some(tab.id),
                        collapsed: false,
                        index,
                        key: DiffKey::from(&tab.id.0),
                    }
                    .into_element()
                }))
        });

        rect()
            .width(Size::fill())
            .padding(Gaps::new(4.0, 2.0, 4.0, 2.0))
            .spacing(4.0)
            .children(rows)
            .into_element()
    }
}

#[derive(Clone, PartialEq)]
struct EssentialTile {
    tab: Tab,
    active: bool,
    collapsed: bool,
    index: usize,
    key: DiffKey,
}

impl KeyExt for EssentialTile {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for EssentialTile {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = try_use_menu_state();
        let sidebar_state = try_use_sidebar_state();
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let tab_id = self.tab.id;
        let index = self.index;

        let is_drop_target = sidebar_state.is_some_and(|s| {
            s.drag_state
                .read()
                .as_ref()
                .is_some_and(|d| d.target == Some(TabMoveTarget::Essential { index }) && d.tab_id != tab_id)
        });

        let (tile_bg, tile_border) = match theme.scheme {
            ResolvedScheme::Dark => {
                let bg = if self.active {
                    Rgba::from_rgb_alpha(255, 255, 255, 0.20).to_color()
                } else if hovered() {
                    Rgba::from_rgb_alpha(255, 255, 255, 0.14).to_color()
                } else {
                    Rgba::from_rgb_alpha(255, 255, 255, 0.085).to_color()
                };
                let border = if is_drop_target || focus() == Focus::Keyboard {
                    theme.color(ThemeRole::Accent)
                } else if self.active {
                    Rgba::from_rgb_alpha(255, 255, 255, 0.22).to_color()
                } else {
                    Rgba::from_rgb_alpha(255, 255, 255, 0.05).to_color()
                };
                (bg, border)
            }
            ResolvedScheme::Light => {
                let bg = if self.active {
                    Rgba::from_rgb_alpha(255, 255, 255, 0.92).to_color()
                } else if hovered() {
                    Rgba::from_rgb_alpha(255, 255, 255, 0.62).to_color()
                } else {
                    Rgba::from_rgb_alpha(0, 0, 0, 0.055).to_color()
                };
                let border = if is_drop_target || focus() == Focus::Keyboard {
                    theme.color(ThemeRole::Accent)
                } else if self.active {
                    Rgba::from_rgb_alpha(0, 0, 0, 0.10).to_color()
                } else {
                    Rgba::from_rgb_alpha(0, 0, 0, 0.04).to_color()
                };
                (bg, border)
            }
        };

        let activate_core = core.clone();
        let on_press = move |_: Event<PressEventData>| {
            activate_core.invoke_or_log(Command::TabActivate { tab_id });
        };

        let menu_core = core.clone();
        let on_mouse_down = move |e: Event<MouseEventData>| {
            if e.button == Some(MouseButton::Right) {
                e.stop_propagation();
                if let Some(menu_state) = &menu_state {
                    let model = menu_core.model().peek().clone();
                    let position = (e.global_location.x as f32, e.global_location.y as f32);
                    open_tab_context_menu(menu_state, &model, tab_id, position);
                }
            }
        };

        let (badge_bg, badge_fg) = favicon_badge_colors(self.tab.favicon.as_deref(), &theme);
        let icon_name = self.tab.favicon.as_deref().unwrap_or("globe");
        let icon_el: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(icon_name) {
            SvgViewer::new((("zendia-icon", canonical), bytes))
                .width(Size::px(12.0))
                .height(Size::px(12.0))
                .color(badge_fg)
                .show_loader(false)
                .into_element()
        } else {
            Icon::new("globe").size(12.0).role(ThemeRole::Text).into_element()
        };

        let tile_width = if self.collapsed {
            Size::px(38.0)
        } else {
            Size::flex(1.0)
        };
        let tile_height_px = if self.collapsed { 36.0 } else { ESSENTIAL_TILE_HEIGHT };
        let tile_height = Size::px(tile_height_px);
        let focus_target = FocusTarget::Essential(tab_id);
        let (tile_bg, tile_border_style, hint_badge) = {
            let model = core.model().read();
            let base_border = Border::new()
                .width(if is_drop_target || focus() == Focus::Keyboard {
                    1.5
                } else {
                    1.0
                })
                .alignment(BorderAlignment::Inner)
                .fill(tile_border);
            (
                apply_explorer_background(tile_bg, &model, &focus_target, &theme),
                apply_explorer_border(base_border, &model, &focus_target, &theme),
                hint_badge_for_row(&model, &focus_target, tile_height_px, &theme),
            )
        };

        let tile = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_alt(format!("Essential: {}", self.tab.display_title()))
            .width(tile_width)
            .height(tile_height)
            .corner_radius(CornerRadius::new_all(ESSENTIAL_TILE_RADIUS))
            .background(tile_bg)
            .border(tile_border_style)
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| {
                hovered.set_if_modified(true);
                if let Some(state) = sidebar_state {
                    state.set_drag_target(TabMoveTarget::Essential { index });
                }
            })
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(on_press)
            .on_mouse_down(on_mouse_down)
            .child(
                rect()
                    .width(Size::px(20.0))
                    .height(Size::px(20.0))
                    .corner_radius(CornerRadius::new_all(5.0))
                    .background(badge_bg)
                    .center()
                    .child(icon_el),
            )
            .maybe_child(hint_badge);

        if self.active && theme.scheme == ResolvedScheme::Light {
            tile.shadow(
                Shadow::new()
                    .x(0.0)
                    .y(1.0)
                    .blur(3.0)
                    .color(Color::from_argb(26, 0, 0, 0)),
            )
        } else {
            tile
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Promo card shown when there are no essentials (Zen's `ZenEssentialsPromo`).
#[derive(Clone, PartialEq)]
pub struct EssentialsPromoCard {
    pub drag_over: bool,
}

impl Component for EssentialsPromoCard {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let accent = theme.rgba(ThemeRole::Accent);

        let bg = if self.drag_over {
            accent
                .with_alpha(if theme.scheme.is_dark() { 0.34 } else { 0.24 })
                .to_color()
        } else {
            accent
                .with_alpha(if theme.scheme.is_dark() { 0.18 } else { 0.13 })
                .to_color()
        };
        let border = accent.with_alpha(if self.drag_over { 0.85 } else { 0.55 }).to_color();

        rect()
            .a11y_role(AccessibilityRole::GenericContainer)
            .a11y_alt("Add to Essentials — Keep your favorite tabs just a click away")
            .width(Size::fill())
            .padding(Gaps::new(14.0, 10.0, 14.0, 10.0))
            .spacing(4.0)
            .cross_align(Alignment::center())
            .main_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(9.0))
            .background(bg)
            .border(Border::new().width(1.0).alignment(BorderAlignment::Inner).fill(border))
            .child(Icon::new("heart").size(16.0).role(ThemeRole::Text))
            .child(
                label()
                    .width(Size::fill())
                    .text("Add to Essentials")
                    .font_size(12.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .text_align(TextAlign::Center)
                    .color(palette.text),
            )
            .child(
                label()
                    .width(Size::fill())
                    .text("Keep your favorite tabs just a click away")
                    .font_size(10.5)
                    .text_align(TextAlign::Center)
                    .color(palette.text_muted),
            )
    }
}
