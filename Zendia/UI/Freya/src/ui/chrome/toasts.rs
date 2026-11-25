/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Toasts at the top corner of the page card, away from the sidebar, as Zen places them.
//!
//! The core keeps the list (`toast.show`, `toast.dismiss`). Each toast pops in on Zen's spring, runs its own timeout
//! and dismisses itself through the core.

use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::toasts::Toast;

const POP_IN: Duration = Duration::from_millis(500);

/// Stacks the core's toasts. Place it over the page card, aligned to the corner away from the sidebar.
#[derive(Clone, PartialEq)]
pub struct ToastHost {
    /// Align to the left edge (the sidebar is on the right).
    pub align_left: bool,
}

impl Component for ToastHost {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let toasts = core.model().read().toasts.clone();
        rect()
            .spacing(6.0)
            .cross_align(if self.align_left {
                Alignment::Start
            } else {
                Alignment::End
            })
            .a11y_role(AccessibilityRole::Log)
            .children(toasts.into_iter().map(|toast| ToastCard { toast }.into_element()))
    }
}

/// One toast in Zen's look: bold text on a dark card, with an optional button and a close button.
#[derive(Clone, PartialEq)]
pub struct ToastCard {
    pub toast: Toast,
}

impl Component for ToastCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let toast_id = self.toast.id;
        let timeout = Duration::from_millis(self.toast.timeout_ms);

        let timeout_core = core.clone();
        use_hook(move || {
            spawn(async move {
                timer(timeout).await;
                timeout_core.invoke_or_log(Command::ToastDismiss { toast_id });
            });
        });
        let pop = use_animation(move |conf| {
            conf.on_creation(OnCreation::Run);
            AnimNum::new(0.0, 1.0)
                .duration(POP_IN)
                .ease(Ease::Out)
                .function(motion::spring_fn())
        });
        let scale = pop.get().value();

        let (bg, fg) = if theme.scheme.is_dark() {
            (Color::from_argb(240, 22, 22, 27), Color::from_rgb(244, 244, 248))
        } else {
            (Color::from_argb(245, 255, 255, 255), Color::from_rgb(24, 24, 30))
        };

        let action = self.toast.action.clone().map(|action| {
            let action_core = core.clone();
            rect()
                .padding(Gaps::new(4.0, 10.0, 4.0, 10.0))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(theme.color(ThemeRole::Accent))
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(action.label.clone())
                .a11y_focusable(true)
                .on_press(move |_| {
                    if let Ok(command) = Command::from_name_and_args(&action.command, &action.command_args) {
                        action_core.invoke_or_log(command);
                    }
                    action_core.invoke_or_log(Command::ToastDismiss { toast_id });
                })
                .child(
                    label()
                        .text(action.label.clone())
                        .font_size(12.0)
                        .font_weight(FontWeight::BOLD)
                        .color(theme.color(ThemeRole::TextOnAccent)),
                )
        });

        let close_core = core.clone();
        rect()
            .scale((scale.max(0.0), scale.max(0.0)))
            .opacity(scale.clamp(0.0, 1.0))
            .padding(Gaps::new(6.0, 6.0, 6.0, 10.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(bg)
            .shadow(Shadow::new().y(2.0).blur(10.0).color(Color::from_argb(70, 0, 0, 0)))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .a11y_role(AccessibilityRole::Alert)
            .a11y_alt(self.toast.message.clone())
            .child(
                label()
                    .text(self.toast.message.clone())
                    .font_size(13.0)
                    .font_weight(FontWeight::BOLD)
                    .color(fg),
            )
            .maybe_child(action)
            .child(
                rect()
                    .width(Size::px(20.0))
                    .height(Size::px(20.0))
                    .corner_radius(CornerRadius::new_all(5.0))
                    .center()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Dismiss")
                    .a11y_focusable(true)
                    .on_press(move |_| {
                        close_core.invoke_or_log(Command::ToastDismiss { toast_id });
                    })
                    .child(Icon::new("close").size(10.0).role(ThemeRole::TextMuted)),
            )
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.toast.id.0)
    }
}
