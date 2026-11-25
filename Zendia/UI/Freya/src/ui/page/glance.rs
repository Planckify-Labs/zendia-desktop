/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Glance: a page previewed in a card over the current page, with close, expand and split buttons beside it.
//!
//! The previewed page is a real tab the core keeps out of the sidebar. The card grows from the link that opened it
//! with Zen's spring, while the page behind shrinks and fades (see [`PageArea`](super::page_area::PageArea)).

use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::page::page_area::{PaneMode, PlaceholderPage};
use crate::ui::page::surface::PageSurface;
use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::split::Glance;

/// How long Glance takes to open.
pub const GLANCE_OPEN: Duration = Duration::from_millis(420);
/// How far the page behind Glance shrinks, as in Zen.
pub const GLANCE_BACKGROUND_SCALE: f32 = 0.97;
/// How faded the page behind Glance gets, as in Zen.
pub const GLANCE_BACKGROUND_OPACITY: f32 = 0.3;
/// Share of the page card's width the Glance card takes.
const GLANCE_WIDTH: f32 = 0.8;
const BUTTON_SIZE: f32 = 30.0;
const BUTTON_GAP: f32 = 14.0;

/// Zen opens Glance on a spring with a bounce of 0.2.
pub const fn glance_spring() -> Function {
    motion::spring_fn()
}

#[derive(Clone, PartialEq)]
pub struct GlanceLayer {
    pub glance: Glance,
    /// Open animation progress, from 0 (at the link) to 1 (open). The spring may pass 1.
    pub progress: f32,
    /// The page card's size.
    pub area: (f32, f32),
    pub mode: PaneMode,
}

impl Component for GlanceLayer {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let tab_id = self.glance.tab_id;

        let (title, url) = {
            let model = core.model().read();
            model
                .tab(tab_id)
                .map(|tab| (tab.display_title(), tab.url.clone()))
                .unwrap_or_default()
        };

        let (width, height) = self.area;
        let card_w = (width * GLANCE_WIDTH)
            .min(width - 2.0 * (BUTTON_SIZE + 2.0 * BUTTON_GAP))
            .max(0.0);
        let card_h = height;
        let card_x = (width - card_w) / 2.0;
        let progress = self.progress;
        let shown = progress.clamp(0.0, 1.0);

        // The card grows out of the link that opened it.
        let (origin_x, origin_y) = self
            .glance
            .origin
            .map(|(x, y)| (x * width, y * height))
            .unwrap_or((width / 2.0, height / 2.0));
        let center_x = card_x + card_w / 2.0;
        let center_y = card_h / 2.0;
        let offset_x = (origin_x - center_x) * (1.0 - progress);
        let offset_y = (origin_y - center_y) * (1.0 - progress);
        let scale = 0.4 + 0.6 * progress;

        let page: Element = match self.mode {
            PaneMode::Live => PageSurface::for_tab(tab_id)
                .key(tab_id.0)
                .width(Size::fill())
                .height(Size::fill())
                .into_element(),
            PaneMode::Placeholder => PlaceholderPage { title, url }.into_element(),
        };

        let card = rect()
            .position(Position::new_absolute().left(card_x).top(0.0))
            .width(Size::px(card_w))
            .height(Size::px(card_h))
            .offset_x(offset_x)
            .offset_y(offset_y)
            .scale((scale, scale))
            .opacity((shown * 1.6).min(1.0))
            .corner_radius(CornerRadius::new_all(10.0))
            .overflow(Overflow::Clip)
            .shadow(
                Shadow::new()
                    .x(0.0)
                    .y(8.0)
                    .blur(28.0)
                    .color(Color::from_argb(90, 0, 0, 0)),
            )
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .a11y_role(AccessibilityRole::Dialog)
            .a11y_alt("Glance")
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .child(page);

        let buttons = rect()
            .position(
                Position::new_absolute()
                    .left(card_x + card_w + BUTTON_GAP)
                    .top(BUTTON_GAP + 14.0),
            )
            .opacity(shown)
            .spacing(BUTTON_GAP)
            .child(GlanceButton {
                icon: "close",
                label: "Close Glance",
                command: GlanceAction::Close,
            })
            .child(GlanceButton {
                icon: "arrows-expand",
                label: "Expand Glance into a tab",
                command: GlanceAction::Expand,
            })
            .child(GlanceButton {
                icon: "split",
                label: "Split Glance with this tab",
                command: GlanceAction::Split,
            });

        let close_core = core.clone();
        let escape_core = core.clone();
        rect()
            .layer(Layer::Relative(8))
            .position(Position::new_absolute().left(0.0).top(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .on_mouse_down(move |_| {
                close_core.invoke_or_log(Command::GlanceClose);
            })
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if e.key == Key::Named(NamedKey::Escape) {
                    escape_core.invoke_or_log(Command::GlanceClose);
                }
            })
            .child(card)
            .child(buttons)
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
        let a11y_id = use_hook(AccessibilityId::new_unique);
        let focus = use_focus(a11y_id);

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
            .corner_radius(CornerRadius::new_all(BUTTON_SIZE))
            .background(bg.to_color())
            .shadow(Shadow::new().y(2.0).blur(6.0).color(Color::from_argb(60, 0, 0, 0)))
            .center()
            .a11y_id(a11y_id)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label)
            .a11y_focusable(true)
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_press(move |_| run())
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Enter)) || e.key == Key::Character(" ".into()) {
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
