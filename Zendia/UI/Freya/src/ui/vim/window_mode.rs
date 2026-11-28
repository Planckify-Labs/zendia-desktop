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
use crate::ui::theme::{ResolvedScheme, ResolvedTheme, Rgba, ThemeRole, motion, use_theme};
use crate::ui::vim::hints::inline_hint_badge;
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::focus::{FocusSurface, FocusTarget, WindowCarryState};
use zendia_browser_core::model::{BrowserModel, InputMode, TabId};
use zendia_browser_core::split::{self, PaneRect};
use zendia_browser_core::split_actions::SplitAction;

const PANE_RADIUS: f32 = 8.0;
const CHIP_HEIGHT: f32 = 30.0;
const CHIP_TOP_INSET: f32 = 4.0;

/// Active pane border role: keeps the pink accent outline even in Window mode while the outer window chrome turns
/// orange (`VIM-WM-05`, matching `window-mode-active-dark.png`).
pub fn active_pane_ring_role(_model: &BrowserModel) -> ThemeRole {
    ThemeRole::Accent
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
        let zoomed_tab = group
            .as_ref()
            .and_then(|g| g.zoomed_tab.filter(|z| g.tab_ids.contains(z)));
        let panes: Vec<(TabId, PaneRect)> = match &group {
            Some(group) => {
                let by_id = split::group_pane_rects(group);
                split::panes_in_reading_order(group)
                    .into_iter()
                    .filter_map(|id| by_id.iter().copied().find(|(tid, _)| *tid == id))
                    .collect()
            }
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

        // Clicking outside chips and menus leaves Window mode (`ZendiaWindowMode.sys.mjs:104-116`, `VimParity.md:761`).
        let backdrop_core = core.clone();
        let menu_state = crate::ui::chrome::menus::try_use_menu_state();
        let mut overlay = rect()
            .layer(Layer::Relative(10))
            .position(Position::new_absolute().left(0.0).top(0.0))
            .width(Size::px(width))
            .height(Size::px(height))
            .on_mouse_down(move |_| {
                if !menu_state.is_some_and(|m| m.has_open_overlay()) {
                    backdrop_core.invoke_or_log(Command::WindowModeClose);
                }
            });

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
            let is_zoomed = zoomed_tab == Some(*tab_id);
            if zoomed_tab.is_some() && !is_zoomed {
                continue;
            }
            let (px, py, pw, _) = if is_zoomed {
                (0.0, 0.0, width, height)
            } else {
                pane_pixels(*pane_rect, (width, height), in_split)
            };
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
            let zoom_core = core.clone();
            let detach_core = core.clone();
            let close_core = core.clone();
            let target_tab = *tab_id;

            overlay = overlay.child(PaneChip {
                pane_number,
                title,
                favicon,
                active: is_active,
                in_split,
                zoomed: is_zoomed,
                hint_parts,
                pane_left: px,
                pane_top: py,
                pane_width: pw,
                on_focus: EventHandler::new(move |_| {
                    focus_core.invoke_or_log(Command::WindowModeFocusPane { pane: pane_number });
                }),
                on_zoom: EventHandler::new(move |_| {
                    zoom_core.invoke_or_log(Command::Splits(SplitAction::Zoom {
                        tab_id: Some(target_tab),
                    }));
                }),
                on_detach: EventHandler::new(move |_| {
                    detach_core.invoke_or_log(Command::SplitRemoveTab { tab_id: target_tab });
                }),
                on_close: EventHandler::new(move |_| {
                    close_core.invoke_or_log(Command::TabClose {
                        tab_id: Some(target_tab),
                        force: false,
                    });
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
    in_split: bool,
    zoomed: bool,
    hint_parts: Option<(String, String)>,
    pane_left: f32,
    pane_top: f32,
    pane_width: f32,
    on_focus: EventHandler<()>,
    on_zoom: EventHandler<()>,
    on_detach: EventHandler<()>,
    on_close: EventHandler<()>,
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
        let zoom_hovered = use_state(|| false);
        let detach_hovered = use_state(|| false);
        let close_hovered = use_state(|| false);

        // `120ms` ease-out entrance (`translateY(-4px) scale(0.96) -> scale(1)`, `zen-split-view.css:246-251`, `VIM-WM-02`).
        let entrance = use_animation(|conf| {
            conf.on_creation(OnCreation::Run);
            AnimNum::new(0.0, 1.0)
                .duration(motion::SPLIT_CHIP_IN)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let t = entrance.get().value().clamp(0.0, 1.0);
        let chip_scale = 0.96 + 0.04 * t;
        let chip_offset_y = -4.0 * (1.0 - t);

        let (chip_bg, chip_border, chip_shadow, btn_hover_bg) = match theme.scheme {
            ResolvedScheme::Dark => (
                if *hovered.read() {
                    Rgba::from_rgb_alpha(38, 42, 60, 0.96).to_color()
                } else {
                    Rgba::from_rgb_alpha(28, 31, 46, 0.92).to_color()
                },
                Rgba::from_rgb_alpha(255, 255, 255, 0.14).to_color(),
                Shadow::new().y(2.0).blur(10.0).color(Color::from_argb(46, 0, 0, 0)),
                Rgba::from_rgb_alpha(255, 255, 255, 0.12).to_color(),
            ),
            ResolvedScheme::Light => (
                if *hovered.read() {
                    Rgba::from_rgb_alpha(255, 255, 255, 0.98).to_color()
                } else {
                    Rgba::from_rgb_alpha(250, 251, 255, 0.94).to_color()
                },
                Rgba::from_rgb_alpha(20, 24, 40, 0.14).to_color(),
                Shadow::new().y(2.0).blur(10.0).color(Color::from_argb(46, 20, 24, 40)),
                Rgba::from_rgb_alpha(0, 0, 0, 0.08).to_color(),
            ),
        };

        let actions_w = if self.in_split { 76.0 } else { 26.0 };
        let hint_w = if self.hint_parts.is_some() { 26.0 } else { 0.0 };
        let max_chip_width = (self.pane_width - 16.0).clamp(130.0, 340.0);
        let estimated_width = (self.title.len() as f32 * 6.5 + 52.0 + actions_w + hint_w).clamp(130.0, max_chip_width);
        let chip_left = self.pane_left + ((self.pane_width - estimated_width) * 0.5).max(8.0);
        let chip_top = self.pane_top + CHIP_TOP_INSET;

        let on_focus = self.on_focus.clone();
        let on_zoom = self.on_zoom.clone();
        let on_detach = self.on_detach.clone();
        let on_close = self.on_close.clone();
        let hint_badge = self
            .hint_parts
            .as_ref()
            .map(|(prefix, suffix)| inline_hint_badge(prefix, suffix, &theme));
        let icon_name = self.favicon.clone().unwrap_or_else(|| "globe".to_string());

        let action_btn =
            |icon: &'static str, alt: &'static str, is_hov: bool, mut hov_state: State<bool>, cb: EventHandler<()>| {
                rect()
                    .width(Size::px(24.0))
                    .height(Size::px(24.0))
                    .corner_radius(CornerRadius::new_all(6.0))
                    .background(if is_hov { btn_hover_bg } else { Color::TRANSPARENT })
                    .opacity(if is_hov { 1.0 } else { 0.75 })
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(alt)
                    .on_pointer_enter(move |_| hov_state.set_if_modified(true))
                    .on_pointer_leave(move |_| hov_state.set_if_modified(false))
                    .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        cb.call(());
                    })
                    .child(Icon::new(icon).size(13.0).role(ThemeRole::Text))
            };

        let zoom_btn = self.in_split.then(|| {
            action_btn(
                if self.zoomed {
                    "fullscreen-exit"
                } else {
                    "arrows-expand"
                },
                if self.zoomed {
                    "Show all panes"
                } else {
                    "Fill the view with this pane"
                },
                *zoom_hovered.read(),
                zoom_hovered,
                on_zoom,
            )
        });
        let detach_btn = self.in_split.then(|| {
            action_btn(
                "split",
                "Detach pane from split",
                *detach_hovered.read(),
                detach_hovered,
                on_detach,
            )
        });
        let close_btn = action_btn("close", "Close pane", *close_hovered.read(), close_hovered, on_close);

        rect()
            .position(Position::new_absolute().left(chip_left).top(chip_top))
            .width(Size::px(estimated_width))
            .height(Size::px(CHIP_HEIGHT))
            .padding(Gaps::new_all(3.0))
            .spacing(2.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(9.0))
            .background(chip_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(chip_border),
            )
            .shadow(chip_shadow)
            .opacity(t)
            .offset_y(chip_offset_y)
            .scale((chip_scale, chip_scale))
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
                    .height(Size::px(24.0))
                    .center()
                    .opacity(0.65)
                    .child(Icon::new("drag-indicator").size(12.0).role(ThemeRole::TextMuted)),
            )
            .child(
                rect()
                    .margin(Gaps::new(0.0, 4.0, 0.0, 2.0))
                    .center()
                    .child(Icon::new(icon_name).size(14.0).role(ThemeRole::Text)),
            )
            .child(
                label()
                    .text(self.title.clone())
                    .width(Size::flex(1.0))
                    .font_size(12.0)
                    .font_weight(if self.active {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .color(theme.rgba(ThemeRole::Text).with_alpha(0.85).to_color())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(zoom_btn)
            .maybe_child(detach_btn)
            .child(close_btn)
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

    // 1. Dim the carried pane to ~0.35 perceived visibility without a cluttering pill (`VIM-WM-03`, `zen-split-view.css:272`).
    if let Some((_, carried_rect)) = panes.iter().find(|(id, _)| *id == carry.carried_tab) {
        let (cx, cy, cw, ch) = pane_pixels(*carried_rect, size, in_split);
        let scrim = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(15, 17, 26, 0.65).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(240, 242, 248, 0.65).to_color(),
        };
        layer = layer.child(
            rect()
                .position(Position::new_absolute().left(cx).top(cy))
                .width(Size::px(cw))
                .height(Size::px(ch))
                .corner_radius(CornerRadius::new_all(PANE_RADIUS))
                .background(scrim),
        );
    }

    // 2. Render ONLY the single active drop-preview highlight box + label (`VIM-WM-03`).
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
                .child(landing_pill(&selected.label, theme)),
        );
    }

    layer.into_element()
}

fn landing_pill(text: &str, theme: &ResolvedTheme) -> Element {
    let orange = theme.rgba(ThemeRole::ModeWindow);
    let fg = match theme.scheme {
        ResolvedScheme::Dark => theme.color(ThemeRole::Ink),
        ResolvedScheme::Light => Color::WHITE,
    };

    rect()
        .height(Size::px(22.0))
        .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
        .corner_radius(CornerRadius::new_all(999.0))
        .background(orange.to_color())
        .border(
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(orange.to_color()),
        )
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
