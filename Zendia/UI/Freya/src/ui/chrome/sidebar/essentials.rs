/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Shared Essentials tile grid and empty-essentials promo card (`ZenEssentialsPromo`) above the active space.

use crate::ui::chrome::layout::ChromePalette;
use crate::ui::chrome::menus::{open_tab_context_menu, try_use_menu_state};
use crate::ui::chrome::sidebar::state::{SidebarDragState, favicon_badge_colors, try_use_sidebar_state};
use crate::ui::chrome::sidebar::tab_row::VerticalDropIndicatorBar;
use crate::ui::core_view::use_core;
use crate::ui::kit::icons::prepared_icon_bytes;
use crate::ui::kit::{Icon, KitTooltip, Spinner};
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, use_theme};
use crate::ui::vim::explorer::{apply_explorer_background, apply_explorer_border};
use crate::ui::vim::hints::hint_badge_for_corner;
use freya::animation::*;
use freya::engine::prelude as sk;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::focus::FocusTarget;
use zendia_browser_core::model::{Tab, TabId, TabMoveTarget};

/// Height of an essential icon tile in expanded sidebar mode (`ESS-03`, matching `vertical-tabs.css`).
pub const ESSENTIAL_TILE_HEIGHT: f32 = 46.0;
/// Corner radius of an essential icon tile (`ESS-03`, matching `--border-radius-medium: 14px`).
pub const ESSENTIAL_TILE_RADIUS: f32 = 14.0;
const DRAG_THRESHOLD_PX: f32 = 6.0;

/// Computes responsive Essentials grid column count (`2..=4`) from the expanded sidebar width (`ESS-03`).
pub fn compute_essential_columns(sidebar_width: f32, tab_count: usize) -> usize {
    let inner = (sidebar_width - 16.0).max(80.0);
    // Matches `minmax(max(23.7%, 48px), 1fr)` with 4px gap: up to 4 columns when wide enough.
    let max_by_width = ((inner + 4.0) / 52.0).floor() as usize;
    let clamped = max_by_width.clamp(2, 4);
    if tab_count > 0 && tab_count < clamped {
        tab_count
    } else {
        clamped
    }
}

/// Grid of shared essential tab tiles (or the empty-essentials promo card) rendered above the active space.
#[derive(Clone, PartialEq)]
pub struct EssentialsGrid {
    pub tabs: Vec<Tab>,
    pub active_tab: Option<TabId>,
    pub collapsed: bool,
    pub show_promo: bool,
    pub sidebar_width: f32,
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
        let cols = compute_essential_columns(self.sidebar_width, self.tabs.len()).max(1);
        let rows = self.tabs.chunks(cols).enumerate().map(move |(row_idx, chunk)| {
            rect()
                .key(row_idx)
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .spacing(4.0)
                .children(chunk.iter().enumerate().map(|(col_idx, tab)| {
                    let index = row_idx * cols + col_idx;
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
        let mut press_origin = use_state(|| None::<(f32, f32)>);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let tab_id = self.tab.id;
        let index = self.index;
        let drop_target = TabMoveTarget::Essential { index };

        let (is_being_dragged, is_drop_target) = match sidebar_state.and_then(|s| s.drag_state.read().clone()) {
            Some(d) => (d.tab_id == tab_id, d.target == Some(drop_target) && d.tab_id != tab_id),
            None => (false, false),
        };

        let (tile_bg, tile_border) = match theme.scheme {
            ResolvedScheme::Dark => {
                let bg = if self.active {
                    Rgba::from_rgb_alpha(68, 64, 72, 0.85).to_color()
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
                    Color::TRANSPARENT
                };
                (bg, border)
            }
            ResolvedScheme::Light => {
                let bg = if self.active {
                    Rgba::from_rgb_alpha(255, 255, 255, 0.88).to_color()
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
                    Color::TRANSPARENT
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
            } else if e.button == Some(MouseButton::Middle) {
                // ESS-02: Middle-click resets/unloads essential tile per pinned tab close behavior.
                e.stop_propagation();
                menu_core.invoke_or_log(Command::TabPinnedCloseAction { tab_id: Some(tab_id) });
            }
        };

        // ESS-01 / DND-01: Drag initiation on EssentialTile hooked into SidebarDragState.
        let drag_title = self.tab.display_title();
        let drag_favicon = self.tab.favicon.clone();
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
                        target: Some(drop_target),
                        page_drop: None,
                        is_preview: false,
                    }));
                }
            }
        };
        let drop_core = core.clone();
        let on_global_pointer_up = move |_: Event<PointerEventData>| {
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

        let (badge_bg, badge_fg) = favicon_badge_colors(self.tab.favicon.as_deref(), &theme);
        let icon_name = self.tab.favicon.as_deref().unwrap_or("globe");
        let center_glyph: Element = if self.tab.is_loading {
            // ESS-04: Animated loading spinner on essential tile.
            Spinner::new(16.0, theme.color(ThemeRole::Accent)).into_element()
        } else {
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
            rect()
                .width(Size::px(18.0))
                .height(Size::px(18.0))
                .corner_radius(CornerRadius::new_all(5.0))
                .background(badge_bg)
                .center()
                .child(icon_el)
                .into_element()
        };

        let tile_width = if self.collapsed { Size::px(38.0) } else { Size::fill() };
        let tile_height_px = if self.collapsed { 38.0 } else { ESSENTIAL_TILE_HEIGHT };
        let tile_height = Size::px(tile_height_px);
        let focus_target = FocusTarget::Essential(tab_id);
        let (tile_bg, tile_border_style, hint_badge, is_unloaded) = {
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
                hint_badge_for_corner(&model, &focus_target, &theme),
                model.is_tab_unloaded(tab_id),
            )
        };

        // ESS-04: Clickable audio/mute corner badge when audible or muted.
        let mute_core = core.clone();
        let audio_badge = (self.tab.is_audible || self.tab.is_muted).then(|| {
            let icon = if self.tab.is_muted {
                "audio-muted"
            } else {
                "audio-playing"
            };
            rect()
                .position(Position::new_absolute().top(3.0).right(3.0))
                .width(Size::px(16.0))
                .height(Size::px(16.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(if theme.scheme.is_dark() {
                    Color::from_argb(190, 22, 24, 34)
                } else {
                    Color::from_argb(215, 255, 255, 255)
                })
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    mute_core.invoke_or_log(Command::TabToggleMute { tab_id: Some(tab_id) });
                })
                .child(Icon::new(icon).size(10.0).role(if self.tab.is_muted {
                    ThemeRole::TextMuted
                } else {
                    ThemeRole::Accent
                }))
        });

        // ESS-04: Active essential blurred radial favicon glow aura behind a 2px-inset inner plate.
        let is_active = self.active;
        let glow_rgb = (badge_bg.r(), badge_bg.g(), badge_bg.b());
        let is_dark = theme.scheme == ResolvedScheme::Dark;
        let radius = if self.collapsed { 10.0 } else { ESSENTIAL_TILE_RADIUS };

        let inner_content = if is_active {
            let aura_cb = RenderCallback::new(move |ctx| {
                let RenderTarget::Background { canvas, .. } = &mut ctx.target else {
                    return;
                };
                let w = ctx.size.width;
                let h = ctx.size.height;
                if w <= 1.0 || h <= 1.0 {
                    return;
                }
                let cx = w * 0.5;
                let cy = h * 0.5;
                let r = w.max(h) * 0.95;
                let a_center: u8 = if is_dark { 175 } else { 130 };
                let colors = [
                    sk::Color4f::from(sk::Color::from_argb(a_center, glow_rgb.0, glow_rgb.1, glow_rgb.2)),
                    sk::Color4f::from(sk::Color::from_argb(a_center / 2, glow_rgb.0, glow_rgb.1, glow_rgb.2)),
                    sk::Color4f::from(sk::Color::from_argb(0, glow_rgb.0, glow_rgb.1, glow_rgb.2)),
                ];
                let positions = [0.0_f32, 0.55_f32, 1.0_f32];
                let grad = sk::gradient::Gradient::new(
                    sk::gradient::Colors::new(&colors, Some(&positions), sk::TileMode::Clamp, None),
                    sk::gradient::Interpolation::default(),
                );
                let mut paint = sk::Paint::default();
                paint.set_anti_alias(true);
                if let Some(shader) = sk::gradient::shaders::radial_gradient((sk::Point::new(cx, cy), r), &grad, None) {
                    paint.set_shader(shader);
                }
                canvas.draw_rect(sk::Rect::from_xywh(0.0, 0.0, w, h), &paint);
            });
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .padding(Gaps::new_all(2.0))
                .background(Fill::Callback(aura_cb))
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::fill())
                        .corner_radius(CornerRadius::new_all((radius - 2.0).max(4.0)))
                        .background(tile_bg)
                        .center()
                        .child(center_glyph),
                )
        } else {
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .background(tile_bg)
                .center()
                .child(center_glyph)
        };

        let tile_opacity = if is_being_dragged {
            0.42
        } else if is_unloaded {
            0.60
        } else {
            1.0
        };

        let tile = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_alt(format!("Essential: {}", self.tab.display_title()))
            .width(tile_width)
            .height(tile_height)
            .corner_radius(CornerRadius::new_all(radius))
            .overflow(Overflow::Clip)
            .border(tile_border_style)
            .opacity(tile_opacity)
            .on_sized(move |e: Event<SizedEventData>| {
                if let Some(state) = sidebar_state {
                    state.set_tab_anchor(tab_id, crate::ui::chrome::menus::AnchorRect::from_area(e.area));
                }
            })
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| {
                hovered.set_if_modified(true);
                if let Some(state) = sidebar_state {
                    state.set_drag_target(drop_target);
                }
            })
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_pointer_down(on_pointer_down)
            .on_global_pointer_move(on_global_pointer_move)
            .on_global_pointer_up(on_global_pointer_up)
            .on_press(on_press)
            .on_mouse_down(on_mouse_down)
            .child(inner_content)
            .maybe_child(audio_badge)
            .maybe_child(hint_badge);

        let styled_tile = if self.active && theme.scheme == ResolvedScheme::Light {
            tile.shadow(
                Shadow::new()
                    .x(0.0)
                    .y(1.0)
                    .blur(3.0)
                    .color(Color::from_argb(26, 0, 0, 0)),
            )
        } else {
            tile
        };

        let wrapped = KitTooltip::new(self.tab.display_title())
            .enabled(!is_being_dragged)
            .child(styled_tile);

        if self.collapsed {
            wrapped.into_element()
        } else if is_drop_target {
            rect()
                .width(Size::flex(1.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .cross_align(Alignment::center())
                .spacing(2.0)
                .child(VerticalDropIndicatorBar {
                    height: tile_height_px - 6.0,
                })
                .child(rect().width(Size::flex(1.0)).child(wrapped))
                .into_element()
        } else {
            rect().width(Size::flex(1.0)).child(wrapped).into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Promo card shown when there are no essentials (Zen's `ZenEssentialsPromo`, `ESS-05`).
///
/// Uses `14px` corner radius, a `1px` dashed outline switching to solid on `dragover`, and a `100ms` background transition.
#[derive(Clone, PartialEq)]
pub struct EssentialsPromoCard {
    pub drag_over: bool,
}

impl Component for EssentialsPromoCard {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let accent = theme.rgba(ThemeRole::Accent);
        let is_dark = theme.scheme.is_dark();

        let drag_anim = use_animation_with_dependencies(&self.drag_over, move |conf, over| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *over { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(Duration::from_millis(100))
                .ease(Ease::InOut)
                .function(Function::Quad)
        });

        let t = drag_anim.get().value().clamp(0.0, 1.0);
        let base_a = if is_dark { 0.18 } else { 0.13 };
        let over_a = if is_dark { 0.34 } else { 0.24 };
        let bg = accent.with_alpha(base_a + (over_a - base_a) * t).to_color();
        let drag_over = self.drag_over;
        let stroke_sk = if drag_over {
            let c = accent.with_alpha(0.90).to_color();
            sk::Color::from_argb(c.a(), c.r(), c.g(), c.b())
        } else if is_dark {
            sk::Color::from_argb(145, 255, 255, 255)
        } else {
            sk::Color::from_argb(135, 24, 28, 42)
        };

        let bg_sk = sk::Color::from_argb(bg.a(), bg.r(), bg.g(), bg.b());
        let promo_cb = RenderCallback::new(move |ctx| {
            let RenderTarget::Background { canvas, .. } = &mut ctx.target else {
                return;
            };
            let w = ctx.size.width;
            let h = ctx.size.height;
            if w <= 2.0 || h <= 2.0 {
                return;
            }
            let fill_rr = sk::RRect::new_rect_xy(
                sk::Rect::from_xywh(0.0, 0.0, w, h),
                ESSENTIAL_TILE_RADIUS,
                ESSENTIAL_TILE_RADIUS,
            );
            let mut fill_paint = sk::Paint::default();
            fill_paint.set_anti_alias(true);
            fill_paint.set_style(sk::PaintStyle::Fill);
            fill_paint.set_color(bg_sk);
            canvas.draw_rrect(fill_rr, &fill_paint);

            let mut stroke_paint = sk::Paint::default();
            stroke_paint.set_anti_alias(true);
            stroke_paint.set_style(sk::PaintStyle::Stroke);
            stroke_paint.set_stroke_width(1.1);
            stroke_paint.set_color(stroke_sk);
            if !drag_over && let Some(effect) = sk::PathEffect::dash(&[4.0, 3.0], 0.0) {
                stroke_paint.set_path_effect(effect);
            }
            let stroke_rr = sk::RRect::new_rect_xy(
                sk::Rect::from_xywh(0.75, 0.75, w - 1.5, h - 1.5),
                ESSENTIAL_TILE_RADIUS,
                ESSENTIAL_TILE_RADIUS,
            );
            canvas.draw_rrect(stroke_rr, &stroke_paint);
        });

        rect()
            .a11y_role(AccessibilityRole::GenericContainer)
            .a11y_alt("Add to Essentials — Keep your favorite tabs just a click away")
            .width(Size::fill())
            .padding(Gaps::new(14.0, 10.0, 14.0, 10.0))
            .spacing(4.0)
            .cross_align(Alignment::center())
            .main_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(ESSENTIAL_TILE_RADIUS))
            .background(Fill::Callback(promo_cb))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn essential_columns_scale_responsively_up_to_four() {
        assert_eq!(compute_essential_columns(186.0, 6), 3);
        assert_eq!(compute_essential_columns(260.0, 6), 4);
        assert_eq!(compute_essential_columns(400.0, 8), 4);
        assert_eq!(compute_essential_columns(260.0, 2), 2);
    }
}
