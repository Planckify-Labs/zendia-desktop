/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Glance: a page previewed in a card over the current page, with close, expand and split buttons beside it.
//!
//! The previewed page is a real tab the core keeps out of the sidebar. The card arcs out from the link that opened it
//! with Zen's spring while the page behind shrinks and fades, and reverses cleanly on close (see
//! [`PageArea`](super::page_area::PageArea)).

use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::page::page_area::{PaneMode, PlaceholderPage};
use crate::ui::page::surface::PageSurface;
use crate::ui::panels::try_use_dock_state;
use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::SidebarSide;
use zendia_browser_core::split::Glance;

/// How long Glance takes to open.
pub const GLANCE_OPEN: Duration = motion::GLANCE_OPEN;
/// How long Glance takes to close.
pub const GLANCE_CLOSE: Duration = motion::GLANCE_CLOSE;
/// How far the page behind Glance shrinks, as in Zen.
pub const GLANCE_BACKGROUND_SCALE: f32 = 0.97;
/// How faded the page behind Glance gets, as in Zen.
pub const GLANCE_BACKGROUND_OPACITY: f32 = 0.3;
/// Share of the page card's width the Glance card takes.
const GLANCE_WIDTH: f32 = 0.8;
/// Share of the page card's height the Glance card takes (matching Zen's `height: 100%` inside small outer inset).
const GLANCE_HEIGHT: f32 = 0.96;
const BUTTON_SIZE: f32 = 34.0;
const BUTTON_GAP: f32 = 12.0;
const BUTTON_TRANSITION: Duration = Duration::from_millis(50);
const MAX_ARC_HEIGHT: f32 = 30.0;
const ARC_HEIGHT_RATIO: f32 = 0.4;

/// Zen opens Glance on a spring with a subtle bounce.
pub const fn glance_spring() -> Function {
    motion::spring_fn()
}

/// Computes the parabolic vertical arc offset for Glance's curved trajectory between origin and target.
pub fn glance_arc_offset(dx: f32, dy: f32, progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    let distance = (dx * dx + dy * dy).sqrt();
    let arc_height = (distance * ARC_HEIGHT_RATIO).min(MAX_ARC_HEIGHT);
    arc_height * 4.0 * t * (1.0 - t)
}

#[derive(Clone, PartialEq)]
pub struct GlanceLayer {
    pub glance: Glance,
    /// Animation progress, from 0 (at the link) to 1 (open). The spring may pass 1 on open.
    pub progress: f32,
    /// True while playing the exit transition after `Command::GlanceClose`.
    pub closing: bool,
    /// The page card's size.
    pub area: (f32, f32),
    pub mode: PaneMode,
}

impl Component for GlanceLayer {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let tab_id = self.glance.tab_id;

        let mut cached_meta = use_state(|| (String::new(), String::new()));
        let (title, url, sidebar_side) = {
            let model = core.model().read();
            let meta = model.tab(tab_id).map(|tab| (tab.display_title(), tab.url.clone()));
            if let Some((t, u)) = meta {
                if !t.is_empty() && *cached_meta.peek() != (t.clone(), u.clone()) {
                    cached_meta.set((t.clone(), u.clone()));
                }
                (t, u, model.layout.sidebar_side)
            } else {
                let (t, u) = cached_meta.read().clone();
                (t, u, model.layout.sidebar_side)
            }
        };

        let (width, height) = self.area;
        let card_w = (width * GLANCE_WIDTH)
            .min(width - 2.0 * (BUTTON_SIZE + 2.0 * BUTTON_GAP))
            .max(0.0);
        let card_h = (height * GLANCE_HEIGHT).max(0.0);
        let card_x = (width - card_w) / 2.0;
        let card_y = (height - card_h) / 2.0;
        let progress = self.progress;
        let shown = progress.clamp(0.0, 1.0);

        // The card grows out of the link that opened it along a subtle curved arc.
        let (origin_x, origin_y) = self
            .glance
            .origin
            .map(|(x, y)| (x * width, y * height))
            .unwrap_or((width / 2.0, height / 2.0));
        let center_x = card_x + card_w / 2.0;
        let center_y = card_y + card_h / 2.0;
        let dx = origin_x - center_x;
        let dy = origin_y - center_y;
        let arc_lift = glance_arc_offset(dx, dy, shown);
        let offset_x = dx * (1.0 - progress);
        let offset_y = dy * (1.0 - progress) - arc_lift;
        let scale = (0.4 + 0.6 * progress).max(0.05);

        // On close, content fades over 150ms and buttons over 120ms; on open, buttons wait until the last 200ms.
        let content_opacity = if self.closing {
            ((shown - 0.30) / 0.70).clamp(0.0, 1.0)
        } else {
            (shown * 1.6).min(1.0)
        };
        let buttons_opacity = if self.closing {
            ((shown - 0.45) / 0.55).clamp(0.0, 1.0)
        } else {
            // Delayed button entrance over the last ~200ms of GLANCE_OPEN (420ms).
            ((shown - 0.52) / 0.48).clamp(0.0, 1.0)
        };

        let page: Element = match self.mode {
            PaneMode::Live => PageSurface::for_tab(tab_id)
                .key(tab_id.0)
                .width(Size::fill())
                .height(Size::fill())
                .into_element(),
            PaneMode::Placeholder => PlaceholderPage { title, url }.into_element(),
        };

        let card_border = rect()
            .layer(Layer::Relative(35))
            .position(Position::new_absolute().left(0.0).top(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .corner_radius(CornerRadius::new_all(10.0))
            .interactive(Interactive::No)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            );

        // Place position + scale on an outer UNCLIPPED wrapper so Freya does not clip the inner card's left edge
        // during the opening/closing scale transition (GL-02), and draw the card border on an overlay above nested
        // `PageSurface` frames.
        let card = rect()
            .position(Position::new_absolute().left(card_x + offset_x).top(card_y + offset_y))
            .width(Size::px(card_w))
            .height(Size::px(card_h))
            .scale((scale, scale))
            .opacity(content_opacity)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .corner_radius(CornerRadius::new_all(10.0))
                    .overflow(Overflow::Clip)
                    .shadow(
                        Shadow::new()
                            .x(0.0)
                            .y(8.0)
                            .blur(28.0)
                            .color(Color::from_argb(90, 0, 0, 0)),
                    )
                    .a11y_role(AccessibilityRole::Dialog)
                    .a11y_alt("Glance")
                    .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
                    .child(page)
                    .child(card_border),
            );

        // Mirror action buttons to the left of the Glance card when the sidebar sits on the right (GL-04).
        let buttons_x = if sidebar_side == SidebarSide::Right {
            (card_x - BUTTON_GAP - BUTTON_SIZE).max(8.0)
        } else {
            card_x + card_w + BUTTON_GAP
        };
        let buttons = rect()
            .position(Position::new_absolute().left(buttons_x).top(card_y + BUTTON_GAP + 8.0))
            .opacity(buttons_opacity)
            .spacing(BUTTON_GAP)
            .child(GlanceButton {
                icon: "close",
                label: "Close Glance",
                command: GlanceAction::Close,
            })
            .child(GlanceButton {
                icon: "fullscreen",
                label: "Expand Glance into a tab",
                command: GlanceAction::Expand,
            })
            .child(GlanceButton {
                icon: "split",
                label: "Split Glance with this tab",
                command: GlanceAction::Split,
            });

        let closing = self.closing;
        let close_core = core.clone();
        let escape_core = core.clone();
        let mut root = rect()
            .layer(Layer::Relative(8))
            .position(Position::new_absolute().left(0.0).top(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .interactive(if closing { Interactive::No } else { Interactive::Yes });

        if !closing {
            root = root
                .on_mouse_down(move |_| {
                    close_core.invoke_or_log(Command::GlanceClose);
                })
                .on_global_key_down(move |e: Event<KeyboardEventData>| {
                    if e.key == Key::Named(NamedKey::Escape) {
                        // XIX-03: Yield Escape to any focused side panel so the topmost focused surface handles it first.
                        if dock_state.is_some_and(|ds| ds.focused_panel.peek().is_some()) {
                            return;
                        }
                        e.stop_propagation();
                        e.prevent_default();
                        escape_core.invoke_or_log(Command::GlanceClose);
                    }
                });
        }

        root.child(card).child(buttons)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum GlanceAction {
    Close,
    Expand,
    Split,
}

#[derive(Clone, PartialEq)]
struct GlanceButton {
    icon: &'static str,
    label: &'static str,
    command: GlanceAction,
}

impl Component for GlanceButton {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let mut pressed = use_state(|| false);
        let a11y_id = use_hook(AccessibilityId::new_unique);
        let focus = use_focus(a11y_id);

        let target_scale = if *pressed.read() {
            0.98f32
        } else if *hovered.read() {
            1.02f32
        } else {
            1.0f32
        };
        let mut prev_scale = use_state(|| target_scale);
        let from_scale = *prev_scale.peek();
        let scale_anim = use_animation_with_dependencies(&target_scale.to_bits(), move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(from_scale, target_scale)
                .duration(BUTTON_TRANSITION)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        if (*prev_scale.peek() - target_scale).abs() > f32::EPSILON {
            prev_scale.set(target_scale);
        }
        let current_scale = scale_anim.get().value();

        // Light circles with dark icons, as Zen draws them.
        let bg = if *hovered.read() {
            theme.rgba(ThemeRole::Text).mix(theme.rgba(ThemeRole::Surface), 0.2)
        } else {
            theme.rgba(ThemeRole::Text).mix(theme.rgba(ThemeRole::Surface), 0.12)
        };
        let command = self.command;
        let run = move || {
            core.invoke_or_log(match command {
                GlanceAction::Close => Command::GlanceClose,
                GlanceAction::Expand => Command::GlanceExpand,
                GlanceAction::Split => Command::GlanceSplit,
            });
        };
        let run_key = run.clone();

        let mut button = rect()
            .width(Size::px(BUTTON_SIZE))
            .height(Size::px(BUTTON_SIZE))
            .scale(current_scale)
            .corner_radius(CornerRadius::new_all(BUTTON_SIZE))
            .background(bg.to_color())
            .shadow(Shadow::new().y(2.0).blur(6.0).color(Color::from_argb(60, 0, 0, 0)))
            .center()
            .cursor(CursorIcon::Pointer)
            .a11y_id(a11y_id)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label)
            .a11y_focusable(true)
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| {
                hovered.set(false);
                pressed.set(false);
            })
            .on_pointer_down(move |e: Event<PointerEventData>| {
                e.stop_propagation();
                pressed.set(true);
            })
            .on_global_pointer_up(move |_| {
                pressed.set_if_modified(false);
            })
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_press(move |_| run())
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Enter)) || e.key == Key::Character(" ".into()) {
                    e.stop_propagation();
                    e.prevent_default();
                    run_key();
                }
            })
            .child(Icon::new(self.icon).size(14.0).role(ThemeRole::Surface));
        if focus() != Focus::Not {
            button = button.border(
                Border::new()
                    .width(2.0)
                    .alignment(BorderAlignment::Outer)
                    .fill(theme.color(ThemeRole::FocusRing)),
            );
        }
        button
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glance_arc_peaks_midway_and_is_zero_at_endpoints() {
        assert_eq!(glance_arc_offset(200.0, 100.0, 0.0), 0.0);
        assert_eq!(glance_arc_offset(200.0, 100.0, 1.0), 0.0);
        let mid = glance_arc_offset(200.0, 100.0, 0.5);
        assert!((mid - MAX_ARC_HEIGHT).abs() < 1e-4);
    }
}
