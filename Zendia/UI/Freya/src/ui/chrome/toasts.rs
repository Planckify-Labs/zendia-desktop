/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Toasts at the top corner of the page card, away from the sidebar, as Zen places them.
//!
//! The core keeps the list (`toast.show`, `toast.dismiss`). Each toast pops in on Zen's spring, pauses its auto-dismiss
//! timer while hovered, bounces when a duplicate message reuses it, and fades/scales out over 200 ms on dismiss.

use crate::ui::core_view::{CoreView, use_core};
use crate::ui::kit::focus_ring_border;
use crate::ui::theme::{Rgba, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::toasts::{Toast, ToastId};

/// Entry tracked by [`ToastHost`] so dismissed toasts stay mounted during their 200 ms exit animation.
#[derive(Clone, PartialEq)]
struct VisibleToast {
    toast: Toast,
    dismissing: bool,
}

/// Stacks the core's toasts. Place it over the page card, aligned to the corner away from the sidebar.
#[derive(Clone, PartialEq)]
pub struct ToastHost {
    /// Align to the left edge (the sidebar is on the right).
    pub align_left: bool,
}

impl Component for ToastHost {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let active_toasts = core.model().read().toasts.clone();
        let mut visible = use_state(|| {
            active_toasts
                .iter()
                .cloned()
                .map(|toast| VisibleToast {
                    toast,
                    dismissing: false,
                })
                .collect::<Vec<_>>()
        });

        // Reconcile model toasts against the animated visible list: newly removed toasts flip to `dismissing` and stay
        // mounted until their exit fade/scale completes.
        let mut newly_dismissed: Vec<ToastId> = Vec::new();
        {
            let current = visible.peek().clone();
            let mut next: Vec<VisibleToast> = Vec::with_capacity(current.len().max(active_toasts.len()));
            for entry in current {
                if let Some(live) = active_toasts.iter().find(|t| t.id == entry.toast.id) {
                    next.push(VisibleToast {
                        toast: live.clone(),
                        dismissing: false,
                    });
                } else {
                    if !entry.dismissing {
                        newly_dismissed.push(entry.toast.id);
                    }
                    next.push(VisibleToast {
                        toast: entry.toast,
                        dismissing: true,
                    });
                }
            }
            for live in &active_toasts {
                if !next.iter().any(|entry| entry.toast.id == live.id) {
                    next.push(VisibleToast {
                        toast: live.clone(),
                        dismissing: false,
                    });
                }
            }
            if *visible.peek() != next {
                visible.set(next);
            }
        }

        for dismissed_id in newly_dismissed {
            spawn(async move {
                timer(motion::TOAST_EXIT + Duration::from_millis(25)).await;
                let mut list = visible.write();
                list.retain(|entry| !(entry.toast.id == dismissed_id && entry.dismissing));
            });
        }

        let items = visible.read().clone();
        rect()
            .spacing(6.0)
            .cross_align(if self.align_left {
                Alignment::Start
            } else {
                Alignment::End
            })
            .a11y_role(AccessibilityRole::Log)
            .children(items.into_iter().map(|entry| {
                ToastCard {
                    toast: entry.toast,
                    dismissing: entry.dismissing,
                }
                .into_element()
            }))
    }
}

fn schedule_toast_dismiss(
    mut timer_gen: State<u64>,
    hovered: State<bool>,
    core: CoreView,
    toast_id: ToastId,
    timeout: Duration,
) {
    let next_gen = timer_gen.peek().wrapping_add(1);
    timer_gen.set(next_gen);
    spawn(async move {
        timer(timeout).await;
        if *timer_gen.peek() == next_gen && !*hovered.peek() {
            core.invoke_or_log(Command::ToastDismiss { toast_id });
        }
    });
}

/// One toast in Zen's look: accent-gradient 14 px pill, bold white text, optional description, interactive button, and
/// hover-pausing auto-dismiss with spring pop-in, duplicate-reuse bounce, and 200 ms fade/scale exit.
#[derive(Clone, PartialEq)]
pub struct ToastCard {
    pub toast: Toast,
    pub dismissing: bool,
}

impl Component for ToastCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let toast_id = self.toast.id;
        let timeout = Duration::from_millis(self.toast.timeout_ms);
        let bounce_seq = self.toast.bounce_seq;

        let mut hovered = use_state(|| false);
        let mut timer_gen = use_state(|| 0u64);
        let mut last_bounce_seq = use_state(|| bounce_seq);
        let mut action_hovered = use_state(|| false);
        let action_a11y = use_a11y();
        let action_focus = use_focus(action_a11y);

        // Start initial auto-dismiss timer once on mount.
        let init_core = core.clone();
        use_hook(move || {
            schedule_toast_dismiss(timer_gen, hovered, init_core, toast_id, timeout);
        });

        // Reset auto-dismiss timer when a duplicate message reuses this toast.
        if *last_bounce_seq.peek() != bounce_seq {
            last_bounce_seq.set(bounce_seq);
            if !*hovered.peek() && !self.dismissing {
                schedule_toast_dismiss(timer_gen, hovered, core.clone(), toast_id, timeout);
            }
        }

        let pop = use_animation(move |conf| {
            conf.on_creation(OnCreation::Run);
            AnimNum::new(0.0, 1.0)
                .duration(motion::TOAST_POP_IN)
                .ease(Ease::Out)
                .function(motion::spring_fn())
        });
        let exit = use_animation_with_dependencies(&self.dismissing, move |conf, dismissing| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *dismissing { (1.0, 0.0) } else { (0.0, 1.0) };
            AnimNum::new(from, to)
                .duration(motion::TOAST_EXIT)
                .ease(Ease::In)
                .function(motion::ease_in_fn())
        });
        let bounce = use_animation_with_dependencies(&bounce_seq, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.2, 1.0)
                .duration(motion::TOAST_POP_IN)
                .ease(Ease::Out)
                .function(motion::spring_fn())
        });

        let pop_val = pop.get().value().max(0.0);
        let exit_val = if self.dismissing {
            exit.get().value().clamp(0.0, 1.0)
        } else {
            1.0
        };
        let bounce_val = if bounce_seq > 0 {
            bounce.get().value().max(0.2)
        } else {
            1.0
        };
        let exit_scale = 0.5 + 0.5 * exit_val;
        let combined_scale = (pop_val * exit_scale * bounce_val).max(0.0);
        let combined_opacity = (pop_val.clamp(0.0, 1.0) * exit_val).clamp(0.0, 1.0);

        // Zen's toast pill gradient (`zen-popup.css:343`): deep charcoal in dark mode, royal indigo-purple in light mode.
        let (grad_top, grad_bottom, border_color) = if theme.scheme.is_dark() {
            (
                Rgba::rgb(26, 26, 30),
                Rgba::rgb(18, 18, 22),
                Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
            )
        } else {
            let base = theme.palette.purple.mix(Rgba::rgb(28, 18, 64), 0.35);
            (
                base,
                base.mix(Rgba::rgb(15, 15, 15), 0.20),
                Rgba::from_rgb_alpha(0, 0, 0, 0.08).to_color(),
            )
        };
        let gradient = LinearGradient::new()
            .angle(180.0)
            .stop((grad_top.to_color(), 0.0))
            .stop((grad_bottom.to_color(), 100.0));
        let fg = Color::from_rgb(245, 245, 248);
        let desc_fg = Color::from_argb(185, 255, 255, 255);
        let shadow_color = if theme.scheme.is_dark() {
            Color::from_argb(102, 0, 0, 0)
        } else {
            Color::from_argb(38, 0, 0, 0)
        };

        let action = self.toast.action.clone().map(|action| {
            let run_action = {
                let action_core = core.clone();
                let cmd_name = action.command.clone();
                let cmd_args = action.command_args.clone();
                move || {
                    if let Ok(command) = Command::from_name_and_args(&cmd_name, &cmd_args) {
                        action_core.invoke_or_log(command);
                    }
                    action_core.invoke_or_log(Command::ToastDismiss { toast_id });
                }
            };
            let run_action_key = run_action.clone();
            let btn_bg = if action_hovered() {
                Color::from_argb(72, 255, 255, 255)
            } else {
                Color::from_argb(42, 255, 255, 255)
            };
            rect()
                .a11y_id(action_a11y)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(action.label.clone())
                .a11y_focusable(true)
                .padding(Gaps::new(5.0, 10.0, 5.0, 10.0))
                .corner_radius(CornerRadius::new_all(8.0))
                .background(btn_bg)
                .border(focus_ring_border(
                    action_focus() == Focus::Keyboard,
                    &theme,
                    Border::new(),
                ))
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| action_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| action_hovered.set_if_modified(false))
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    run_action();
                })
                .on_key_down(move |e: Event<KeyboardEventData>| {
                    if matches!(&e.key, Key::Named(NamedKey::Enter)) || matches!(&e.key, Key::Character(c) if c == " ")
                    {
                        e.stop_propagation();
                        e.prevent_default();
                        run_action_key();
                    }
                })
                .child(
                    label()
                        .text(action.label.clone())
                        .font_size(12.5)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(fg),
                )
        });

        let text_col = rect()
            .spacing(2.0)
            .child(
                label()
                    .text(self.toast.message.clone())
                    .font_size(14.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(fg)
                    .max_lines(1),
            )
            .maybe_child(self.toast.description.as_ref().map(|desc| {
                label()
                    .text(desc.clone())
                    .font_size(12.0)
                    .font_weight(FontWeight::MEDIUM)
                    .color(desc_fg)
                    .max_lines(1)
            }));

        let dismissing = self.dismissing;
        let leave_core = core.clone();

        rect()
            .scale((combined_scale, combined_scale))
            .opacity(combined_opacity)
            .interactive(if dismissing { Interactive::No } else { Interactive::Yes })
            .min_height(Size::px(42.0))
            .padding(Gaps::new(6.0, 14.0, 6.0, 14.0))
            .corner_radius(CornerRadius::new_all(14.0))
            .background(gradient)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .shadow(Shadow::new().x(0.0).y(0.0).blur(22.0).spread(2.0).color(shadow_color))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .a11y_role(AccessibilityRole::Alert)
            .a11y_alt(self.toast.message.clone())
            .on_pointer_over(move |_| {
                hovered.set_if_modified(true);
                // Cancel any pending dismiss timer while the pointer rests over the toast.
                let next_gen = timer_gen.peek().wrapping_add(1);
                timer_gen.set(next_gen);
            })
            .on_pointer_out(move |_| {
                hovered.set_if_modified(false);
                if !dismissing {
                    schedule_toast_dismiss(timer_gen, hovered, leave_core.clone(), toast_id, timeout);
                }
            })
            .child(text_col)
            .maybe_child(action)
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.toast.id.0)
    }
}
