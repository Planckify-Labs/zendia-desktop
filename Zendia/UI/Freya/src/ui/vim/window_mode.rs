/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Window mode overlay over split panes (`ZendiaWindowMode`): numbered pane chips, landing labels at each pane side
//! and split edge, and the WINDOW orange ring on the active pane.

use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::page::page_area::SPLIT_GAP;
use crate::ui::theme::{ResolvedScheme, ResolvedTheme, Rgba, ThemeRole, use_theme};
use crate::ui::vim::hints::inline_hint_badge;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::focus::{FocusSurface, FocusTarget, LandingKind, LandingTarget, WindowCarryState};
use zendia_browser_core::model::{BrowserModel, InputMode, TabId};
use zendia_browser_core::split::{self, DropZone, PaneRect};

const PANE_RADIUS: f32 = 8.0;
const CHIP_HEIGHT: f32 = 28.0;
const CHIP_TOP_INSET: f32 = 8.0;

/// Active pane border role: WINDOW orange while window mode is active, otherwise the standard accent ring.
pub fn active_pane_ring_role(model: &BrowserModel) -> ThemeRole {
    if model.vim_mode_enabled && model.input_mode == InputMode::Window {
        ThemeRole::ModeWindow
    } else {
        ThemeRole::Accent
    }
}

/// Overlay drawn over `PageArea` while vim window mode is active: numbered pane chips and carry landing labels.
#[derive(Clone, PartialEq)]
pub struct WindowModeOverlay {
    pub area: (f32, f32),
}

fn pane_pixels(rect: PaneRect, size: (f32, f32), in_split: bool) -> (f32, f32, f32, f32) {
    let (width, height) = size;
    if !in_split {
        return (0.0, 0.0, width, height);
    }
    let half = SPLIT_GAP / 2.0;
    let left = rect.x * width + if rect.x > 0.0 { half } else { 0.0 };
    let top = rect.y * height + if rect.y > 0.0 { half } else { 0.0 };
    let right = (rect.x + rect.width) * width - if rect.x + rect.width < 0.999 { half } else { 0.0 };
    let bottom = (rect.y + rect.height) * height - if rect.y + rect.height < 0.999 { half } else { 0.0 };
    (left, top, (right - left).max(0.0), (bottom - top).max(0.0))
}

impl Component for WindowModeOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let (width, height) = self.area;

        let model = core.model().read().clone();
        if !model.vim_mode_enabled || model.input_mode != InputMode::Window || width <= 0.0 || height <= 0.0 {
            return rect();
        }

        let group = model.active_split_group().cloned();
        let in_split = group.is_some();
        let panes: Vec<(TabId, PaneRect)> = match &group {
            Some(group) => split::pane_rects(group.direction, group.tab_ids.len(), &group.ratios)
                .into_iter()
                .zip(group.tab_ids.iter().copied())
                .map(|(rect, tab_id)| (tab_id, rect))
                .collect(),
            None => model
                .active_tab
                .map(|tab_id| {
                    (
                        tab_id,
                        PaneRect {
                            x: 0.0,
                            y: 0.0,
                            width: 1.0,
                            height: 1.0,
                        },
                    )
                })
                .into_iter()
                .collect(),
        };

        let mut overlay = rect()
            .layer(Layer::Relative(10))
            .position(Position::new_absolute().left(0.0).top(0.0))
            .width(Size::px(width))
            .height(Size::px(height));

        // Carry preview and landing labels (when carrying a pane in window mode).
        if let Some(carry) = &model.focus.window_carry {
            overlay = overlay.child(render_carry_layer(&panes, carry, (width, height), in_split, &theme));
        }

        // Numbered pane chips along the top center of each pane.
        let active_tab = model.active_tab;
        let window_hints = model
            .focus
            .hints
            .as_ref()
            .filter(|hints| hints.surface == FocusSurface::Window);

        for (index, (tab_id, pane_rect)) in panes.iter().enumerate() {
            let (px, py, pw, _) = pane_pixels(*pane_rect, (width, height), in_split);
            if pw < 48.0 {
                continue;
            }
            let (title, favicon) = model
                .tab(*tab_id)
                .map(|tab| (tab.display_title(), tab.favicon.clone()))
                .unwrap_or_else(|| ("Tab".to_string(), None));
            let is_active = Some(*tab_id) == active_tab;
            let hint_parts = window_hints
                .and_then(|hints| hints.label_parts(&FocusTarget::Pane(*tab_id)))
                .map(|(p, s)| (p.to_string(), s.to_string()));
            let pane_number = index + 1;
            let focus_core = core.clone();

            overlay = overlay.child(PaneChip {
                pane_number,
                title,
                favicon,
                active: is_active,
                hint_parts,
                pane_left: px,
                pane_top: py,
                pane_width: pw,
                on_focus: EventHandler::new(move |_| {
                    focus_core.invoke_or_log(Command::WindowModeFocusPane { pane: pane_number });
                }),
                key: DiffKey::from(&("pane-chip", tab_id.0)),
            });
        }

        overlay
    }
}

#[derive(Clone, PartialEq)]
struct PaneChip {
    pane_number: usize,
    title: String,
    favicon: Option<String>,
    active: bool,
    hint_parts: Option<(String, String)>,
    pane_left: f32,
    pane_top: f32,
    pane_width: f32,
    on_focus: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for PaneChip {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for PaneChip {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);

        let window_rgba = theme.rgba(ThemeRole::ModeWindow);
        let window_color = window_rgba.to_color();
        let (chip_bg, chip_border, chip_shadow) = match theme.scheme {
            ResolvedScheme::Dark => (
                if *hovered.read() {
                    Rgba::from_rgb_alpha(38, 42, 60, 0.96).to_color()
                } else {
                    Rgba::from_rgb_alpha(28, 31, 46, 0.92).to_color()
                },
                if self.active {
                    window_rgba.with_alpha(0.92).to_color()
                } else {
                    Rgba::from_rgb_alpha(255, 255, 255, 0.14).to_color()
                },
                Shadow::new().y(4.0).blur(14.0).color(Color::from_argb(110, 0, 0, 0)),
            ),
            ResolvedScheme::Light => (
                if *hovered.read() {
                    Rgba::from_rgb_alpha(255, 255, 255, 0.98).to_color()
                } else {
                    Rgba::from_rgb_alpha(250, 251, 255, 0.94).to_color()
                },
                if self.active {
                    window_rgba.with_alpha(0.92).to_color()
                } else {
                    Rgba::from_rgb_alpha(20, 24, 40, 0.14).to_color()
                },
                Shadow::new().y(3.0).blur(12.0).color(Color::from_argb(42, 20, 24, 40)),
            ),
        };

        let (num_bg, num_fg) = if self.active {
            let fg = match theme.scheme {
                ResolvedScheme::Dark => theme.color(ThemeRole::Ink),
                ResolvedScheme::Light => Color::WHITE,
            };
            (window_color, fg)
        } else {
            (window_rgba.with_alpha(0.20).to_color(), window_color)
        };

        let max_chip_width = (self.pane_width - 24.0).clamp(110.0, 260.0);
        let estimated_width = (self.title.len() as f32 * 6.8 + 76.0).clamp(110.0, max_chip_width);
        let chip_left = self.pane_left + ((self.pane_width - estimated_width) * 0.5).max(8.0);
        let chip_top = self.pane_top + CHIP_TOP_INSET;

        let on_focus = self.on_focus.clone();
        let hint_badge = self
            .hint_parts
            .as_ref()
            .map(|(prefix, suffix)| inline_hint_badge(prefix, suffix, &theme));
        let icon_name = self.favicon.clone().unwrap_or_else(|| "globe".to_string());

        rect()
            .position(Position::new_absolute().left(chip_left).top(chip_top))
            .width(Size::px(estimated_width))
            .height(Size::px(CHIP_HEIGHT))
            .padding(Gaps::new(0.0, 10.0, 0.0, 6.0))
            .spacing(6.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(8.0))
            .background(chip_bg)
            .border(
                Border::new()
                    .width(if self.active { 1.5 } else { 1.0 })
                    .alignment(BorderAlignment::Inner)
                    .fill(chip_border),
            )
            .shadow(chip_shadow)
            .overflow(Overflow::Clip)
            .cursor(CursorIcon::Pointer)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Pane {}: {}", self.pane_number, self.title))
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_focus.call(());
            })
            .maybe_child(hint_badge)
            .child(
                rect()
                    .width(Size::px(18.0))
                    .height(Size::px(18.0))
                    .corner_radius(CornerRadius::new_all(5.0))
                    .background(num_bg)
                    .center()
                    .child(
                        label()
                            .text(self.pane_number.to_string())
                            .font_size(11.0)
                            .font_weight(FontWeight::BOLD)
                            .font_family("monospace")
                            .color(num_fg)
                            .max_lines(1),
                    ),
            )
            .child(Icon::new(icon_name).size(13.0).role(ThemeRole::Text))
            .child(
                label()
                    .text(self.title.clone())
                    .width(Size::flex(1.0))
                    .font_size(12.0)
                    .font_weight(if self.active {
                        FontWeight::SEMI_BOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .color(theme.color(ThemeRole::Text))
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn render_carry_layer(
    panes: &[(TabId, PaneRect)],
    carry: &WindowCarryState,
    size: (f32, f32),
    in_split: bool,
    theme: &ResolvedTheme,
) -> Element {
    let (width, height) = size;
    let orange = theme.rgba(ThemeRole::ModeWindow);
    let orange_color = orange.to_color();
    let mut layer = rect()
        .position(Position::new_absolute().left(0.0).top(0.0))
        .width(Size::px(width))
        .height(Size::px(height))
        .interactive(false);

    // 1. Dim the pane currently being carried.
    if let Some((_, carried_rect)) = panes.iter().find(|(id, _)| *id == carry.carried_tab) {
        let (cx, cy, cw, ch) = pane_pixels(*carried_rect, size, in_split);
        let scrim = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(15, 17, 26, 0.45).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(240, 242, 248, 0.55).to_color(),
        };
        layer = layer.child(
            rect()
                .position(Position::new_absolute().left(cx).top(cy))
                .width(Size::px(cw))
                .height(Size::px(ch))
                .corner_radius(CornerRadius::new_all(PANE_RADIUS))
                .background(scrim)
                .center()
                .child(landing_pill("Carrying pane", false, true, theme)),
        );
    }

    // 2. Highlighted drop preview for the selected landing target.
    if let Some(selected) = carry.selected_target() {
        let px = selected.rect.x * width;
        let py = selected.rect.y * height;
        let pw = selected.rect.width * width;
        let ph = selected.rect.height * height;
        layer = layer.child(
            rect()
                .position(Position::new_absolute().left(px + 4.0).top(py + 4.0))
                .width(Size::px((pw - 8.0).max(0.0)))
                .height(Size::px((ph - 8.0).max(0.0)))
                .corner_radius(CornerRadius::new_all(PANE_RADIUS))
                .background(orange.with_alpha(0.18).to_color())
                .border(
                    Border::new()
                        .width(2.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(orange_color),
                )
                .center()
                .child(landing_pill(&selected.label, true, false, theme)),
        );
    }

    // 3. Landing labels at each side of every non-carried pane and at each outer split edge.
    for (idx, target) in carry.targets.iter().enumerate() {
        if idx == carry.selected {
            continue;
        }
        if let Some((lx, ly, caption)) = landing_label_anchor(panes, target, size, in_split) {
            layer = layer.child(
                rect()
                    .position(Position::new_absolute().left(lx).top(ly))
                    .child(landing_pill(caption, false, false, theme)),
            );
        }
    }

    layer.into_element()
}

fn landing_label_anchor(
    panes: &[(TabId, PaneRect)],
    target: &LandingTarget,
    size: (f32, f32),
    in_split: bool,
) -> Option<(f32, f32, &'static str)> {
    let (width, height) = size;
    let pill_h = 22.0;
    match target.kind {
        LandingKind::PaneSide { target_tab, zone } => {
            let (_, pane_rect) = panes.iter().find(|(id, _)| *id == target_tab)?;
            let (px, py, pw, ph) = pane_pixels(*pane_rect, size, in_split);
            if pw < 140.0 || ph < 110.0 {
                return None;
            }
            let (x, y, caption) = match zone {
                DropZone::Left => (px + 12.0, py + (ph - pill_h) * 0.5, "Left"),
                DropZone::Right => (px + pw - 62.0, py + (ph - pill_h) * 0.5, "Right"),
                DropZone::Top => (px + pw * 0.5 - 24.0, py + 44.0, "Top"),
                DropZone::Bottom => (px + pw * 0.5 - 32.0, py + ph - pill_h - 12.0, "Bottom"),
            };
            Some((x.max(4.0), y.max(4.0), caption))
        }
        LandingKind::PaneSwap { target_tab } => {
            let (_, pane_rect) = panes.iter().find(|(id, _)| *id == target_tab)?;
            let (px, py, pw, ph) = pane_pixels(*pane_rect, size, in_split);
            if pw < 100.0 || ph < 90.0 {
                return None;
            }
            Some((px + pw * 0.5 - 26.0, py + (ph - pill_h) * 0.5, "Swap"))
        }
        LandingKind::SplitEdge { zone } => {
            let (x, y, caption) = match zone {
                DropZone::Left => (8.0, (height - pill_h) * 0.5, "Left edge"),
                DropZone::Right => (width - 82.0, (height - pill_h) * 0.5, "Right edge"),
                DropZone::Top => (width * 0.5 - 38.0, 8.0, "Top edge"),
                DropZone::Bottom => (width * 0.5 - 46.0, height - pill_h - 8.0, "Bottom edge"),
            };
            Some((x.max(4.0), y.max(4.0), caption))
        }
    }
}

fn landing_pill(text: &str, active: bool, muted: bool, theme: &ResolvedTheme) -> Element {
    let orange = theme.rgba(ThemeRole::ModeWindow);
    let (bg, border, fg) = if active {
        let fg = match theme.scheme {
            ResolvedScheme::Dark => theme.color(ThemeRole::Ink),
            ResolvedScheme::Light => Color::WHITE,
        };
        (orange.to_color(), orange.to_color(), fg)
    } else if muted {
        (
            theme.rgba(ThemeRole::SurfaceRaised).with_alpha(0.90).to_color(),
            theme.color(ThemeRole::Border),
            theme.color(ThemeRole::TextMuted),
        )
    } else {
        let bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(26, 29, 44, 0.90).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(255, 255, 255, 0.92).to_color(),
        };
        (bg, orange.with_alpha(0.65).to_color(), theme.color(ThemeRole::Text))
    };

    rect()
        .height(Size::px(22.0))
        .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
        .corner_radius(CornerRadius::new_all(999.0))
        .background(bg)
        .border(Border::new().width(1.0).alignment(BorderAlignment::Inner).fill(border))
        .center()
        .overflow(Overflow::Clip)
        .child(
            label()
                .text(text.to_string())
                .font_size(11.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .color(fg)
                .max_lines(1),
        )
        .into_element()
}
