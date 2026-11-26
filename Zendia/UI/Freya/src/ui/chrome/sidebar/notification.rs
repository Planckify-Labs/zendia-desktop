/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Sidebar notification card at the foot of the expanded sidebar (Zen's `ZenSidebarNotification`).

use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::engine::prelude as sk;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{NotificationId, SidebarNotification, SidebarNotificationLink};

const COUNTDOWN_TICK_MS: u64 = 33;

/// Renders a core-published [`SidebarNotification`] card above the sidebar foot row.
///
/// Matches Zen's `ZenSidebarNotification`: slide-in/slide-out entrance (`NOT-02`), `1px` bottom countdown progress bar
/// honoring `auto_hide_ms` with hover-pause (`NOT-01`), and `2s` shimmering gradient on `[special]` links (`NOT-03`).
#[derive(Clone, PartialEq)]
pub struct SidebarNotificationCard {
    pub notification: SidebarNotification,
    key: DiffKey,
}

impl KeyExt for SidebarNotificationCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl SidebarNotificationCard {
    pub fn new(notification: SidebarNotification) -> Self {
        let key = DiffKey::from(&notification.id.0);
        Self { notification, key }
    }
}

impl Component for SidebarNotificationCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut close_hovered = use_state(|| false);
        let mut card_hovered = use_state(|| false);
        let mut dismissing = use_state(|| false);
        let total_ms = self.notification.auto_hide_ms.filter(|ms| *ms > 0);
        let mut remaining_ms = use_state(move || total_ms.unwrap_or(0) as f32);
        let mut timer_started = use_state(|| false);
        let close_a11y = use_a11y();

        let notification_id = self.notification.id;

        // NOT-02: Slide-in entrance (opacity 0->1, y 50->0) and exit (opacity 1->0, y 0->10).
        let is_exiting = dismissing();
        let motion_anim = use_animation_with_dependencies(&is_exiting, move |conf, exiting| {
            conf.on_creation(OnCreation::Run);
            conf.on_change(OnChange::Rerun);
            if *exiting {
                (
                    AnimNum::new(1.0, 0.0)
                        .duration(Duration::from_millis(180))
                        .ease(Ease::In)
                        .function(Function::Quad),
                    AnimNum::new(0.0, 10.0)
                        .duration(Duration::from_millis(180))
                        .ease(Ease::In)
                        .function(Function::Quad),
                )
            } else {
                (
                    AnimNum::new(0.0, 1.0)
                        .duration(motion::SIDEBAR_NOTIF_SLIDE)
                        .ease(Ease::Out)
                        .function(motion::ease_out_fn()),
                    AnimNum::new(50.0, 0.0)
                        .duration(motion::SIDEBAR_NOTIF_SLIDE)
                        .ease(Ease::Out)
                        .function(motion::ease_out_fn()),
                )
            }
        });

        // NOT-01: Countdown task driven by `auto_hide_ms`, paused while pointer hovers the card.
        if let Some(total) = total_ms
            && !*timer_started.peek()
        {
            timer_started.set(true);
            remaining_ms.set(total as f32);
            let core_for_timer = core.clone();
            spawn(async move {
                loop {
                    timer(Duration::from_millis(COUNTDOWN_TICK_MS)).await;
                    if *dismissing.peek() {
                        break;
                    }
                    if *card_hovered.peek() {
                        continue;
                    }
                    let next = (*remaining_ms.peek() - COUNTDOWN_TICK_MS as f32).max(0.0);
                    remaining_ms.set(next);
                    if next <= 0.0 {
                        dismissing.set(true);
                        timer(Duration::from_millis(180)).await;
                        core_for_timer.invoke_or_log(Command::NotificationDismiss { notification_id });
                        break;
                    }
                }
            });
        }

        let (card_bg, card_border, header_divider, heading_color, shadow) = match theme.scheme {
            ResolvedScheme::Dark => (
                Rgba::from_rgb_alpha(12, 13, 18, 0.96).to_color(),
                Rgba::from_rgb_alpha(255, 255, 255, 0.14).to_color(),
                Rgba::from_rgb_alpha(255, 255, 255, 0.10).to_color(),
                Rgba::from_rgb_alpha(255, 255, 255, 0.92).to_color(),
                Shadow::new()
                    .x(0.0)
                    .y(4.0)
                    .blur(12.0)
                    .color(Color::from_argb(96, 0, 0, 0)),
            ),
            ResolvedScheme::Light => (
                Rgba::from_rgb_alpha(250, 250, 253, 0.96).to_color(),
                Rgba::from_rgb_alpha(0, 0, 0, 0.10).to_color(),
                Rgba::from_rgb_alpha(0, 0, 0, 0.09).to_color(),
                Rgba::from_rgb_alpha(20, 22, 32, 0.92).to_color(),
                Shadow::new()
                    .x(0.0)
                    .y(3.0)
                    .blur(10.0)
                    .color(Color::from_argb(36, 0, 0, 0)),
            ),
        };

        let dismiss_core = core.clone();
        let on_dismiss = move |_: Event<PressEventData>| {
            if *dismissing.peek() {
                return;
            }
            dismissing.set(true);
            let dismiss_core = dismiss_core.clone();
            spawn(async move {
                timer(Duration::from_millis(180)).await;
                dismiss_core.invoke_or_log(Command::NotificationDismiss { notification_id });
            });
        };

        let links = self.notification.links.iter().enumerate().map(|(idx, link)| {
            NotificationLinkRow {
                notification_id,
                link: link.clone(),
                key: DiffKey::from(&idx),
            }
            .into_element()
        });

        let progress_bar = total_ms.map(|total| {
            let pct = (*remaining_ms.read() / (total as f32)).clamp(0.0, 1.0) * 100.0;
            rect()
                .width(Size::fill())
                .height(Size::px(1.5))
                .background(Color::TRANSPARENT)
                .child(
                    rect()
                        .width(Size::percent(pct))
                        .height(Size::fill())
                        .background(theme.color(ThemeRole::Accent)),
                )
        });

        let (card_opacity, card_y) = {
            let anim = motion_anim.get();
            (anim.0.value().clamp(0.0, 1.0), anim.1.value())
        };

        rect()
            .a11y_role(AccessibilityRole::Alert)
            .a11y_alt(self.notification.heading.clone())
            .width(Size::fill())
            .corner_radius(CornerRadius::new_all(10.0))
            .background(card_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(card_border),
            )
            .shadow(shadow)
            .opacity(card_opacity)
            .offset_y(card_y)
            .overflow(Overflow::Clip)
            .on_pointer_enter(move |_| card_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| card_hovered.set_if_modified(false))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(34.0))
                    .padding(Gaps::new(0.0, 6.0, 0.0, 12.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(Color::TRANSPARENT),
                    )
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .main_align(Alignment::center())
                            .cross_align(Alignment::center())
                            .child(
                                label()
                                    .text(self.notification.heading.clone())
                                    .font_size(12.0)
                                    .font_weight(FontWeight::MEDIUM)
                                    .color(heading_color)
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis),
                            ),
                    )
                    .child(
                        rect()
                            .a11y_id(close_a11y)
                            .a11y_focusable(true)
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Dismiss notification")
                            .width(Size::px(20.0))
                            .height(Size::px(20.0))
                            .corner_radius(CornerRadius::new_all(5.0))
                            .background(if close_hovered() {
                                Rgba::from_rgb_alpha(128, 128, 140, 0.22).to_color()
                            } else {
                                Color::TRANSPARENT
                            })
                            .opacity(if close_hovered() { 0.95 } else { 0.5 })
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_pointer_enter(move |_| close_hovered.set_if_modified(true))
                            .on_pointer_leave(move |_| close_hovered.set_if_modified(false))
                            .on_press(on_dismiss)
                            .child(Icon::new("close").size(11.0).role(ThemeRole::Text)),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(1.0))
                    .background(header_divider),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(6.0, 6.0, 6.0, 6.0))
                    .spacing(2.0)
                    .children(links),
            )
            .maybe_child(progress_bar)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct NotificationLinkRow {
    notification_id: NotificationId,
    link: SidebarNotificationLink,
    key: DiffKey,
}

impl KeyExt for NotificationLinkRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for NotificationLinkRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();

        let notification_id = self.notification_id;
        let url = self.link.url.clone();
        let command = self.link.command.clone();
        let special = self.link.special;

        let text_color = if special {
            theme.color(ThemeRole::Accent)
        } else if hovered() {
            theme.color(ThemeRole::Text)
        } else {
            theme.color(ThemeRole::TextMuted)
        };
        let icon_role = if special {
            ThemeRole::Accent
        } else if hovered() {
            ThemeRole::Text
        } else {
            ThemeRole::TextMuted
        };

        let hover_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.05).to_color(),
        };

        let on_press = move |_: Event<PressEventData>| {
            if let Some(cmd_name) = &command
                && let Ok(parsed) = Command::from_name_and_args(cmd_name, &[])
            {
                core.invoke_or_log(parsed);
            } else if let Some(url) = &url {
                core.invoke_or_log(Command::TabNew {
                    url: Some(url.clone()),
                    after: None,
                    activate: true,
                });
            }
            core.invoke_or_log(Command::NotificationDismiss { notification_id });
        };

        let label_el: Element = if special {
            ShimmerSpecialLabel {
                text: self.link.label.clone(),
                accent: theme.color(ThemeRole::Accent),
                highlight: match theme.scheme {
                    ResolvedScheme::Dark => Color::from_rgb(255, 232, 248),
                    ResolvedScheme::Light => Color::from_rgb(138, 60, 212),
                },
            }
            .into_element()
        } else {
            label()
                .text(self.link.label.clone())
                .font_size(11.5)
                .font_weight(FontWeight::NORMAL)
                .color(text_color)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .into_element()
        };

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.link.label.clone())
            .width(Size::fill())
            .padding(Gaps::new(5.0, 6.0, 5.0, 6.0))
            .spacing(7.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(6.0))
            .background(if hovered() { hover_bg } else { Color::TRANSPARENT })
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(on_press)
            .child(Icon::new(self.link.icon.clone()).size(13.0).role(icon_role))
            .child(label_el)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// NOT-03: Shimmering `2s linear infinite` text gradient for `[special]` notification links.
#[derive(Clone, PartialEq)]
struct ShimmerSpecialLabel {
    text: String,
    accent: Color,
    highlight: Color,
}

impl Component for ShimmerSpecialLabel {
    fn render(&self) -> impl IntoElement {
        let shimmer = use_animation(|conf| {
            conf.on_creation(OnCreation::Run);
            conf.on_finish(OnFinish::Restart { delay: Duration::ZERO });
            AnimNum::new(0.0, 1.0)
                .duration(Duration::from_millis(2000))
                .ease(Ease::InOut)
                .function(Function::Linear)
        });
        let phase = shimmer.get().value();
        let text = self.text.clone();
        let accent = self.accent;
        let highlight = self.highlight;

        rect()
            .width(Size::flex(1.0))
            .height(Size::px(16.0))
            .overflow(Overflow::Clip)
            .background(Fill::Callback(RenderCallback::new(move |ctx| {
                let RenderTarget::Background { canvas, .. } = &mut ctx.target else {
                    return;
                };
                let w = ctx.size.width;
                let h = ctx.size.height;
                if w <= 1.0 || h <= 1.0 {
                    return;
                }
                let left = 0.0_f32;
                let top = 0.0_f32;
                let mgr = sk::FontMgr::default();
                let style = sk::FontStyle::new(sk::Weight::MEDIUM, sk::Width::NORMAL, sk::Slant::Upright);
                let tf = mgr
                    .match_family_style("Inter", style)
                    .or_else(|| mgr.match_family_style("sans-serif", style))
                    .or_else(|| mgr.legacy_make_typeface(None, style));
                let mut font = match tf {
                    Some(tf) => sk::Font::from_typeface(tf, 11.5),
                    None => {
                        let mut f = sk::Font::default();
                        f.set_size(11.5);
                        f
                    }
                };
                font.set_subpixel(true);
                let (_, metrics) = font.metrics();
                let baseline_y = top + h * 0.5 - (metrics.ascent + metrics.descent) * 0.5;

                let c_acc = sk::Color::from_argb(255, accent.r(), accent.g(), accent.b());
                let c_hi = sk::Color::from_argb(255, highlight.r(), highlight.g(), highlight.b());
                let shift = (phase * 2.0 - 0.5) * w;
                let colors = [
                    sk::Color4f::from(c_acc),
                    sk::Color4f::from(c_hi),
                    sk::Color4f::from(c_acc),
                ];
                let positions = [0.0_f32, 0.5_f32, 1.0_f32];
                let grad = sk::gradient::Gradient::new(
                    sk::gradient::Colors::new(&colors, Some(&positions), sk::TileMode::Clamp, None),
                    sk::gradient::Interpolation::default(),
                );
                let mut paint = sk::Paint::default();
                paint.set_anti_alias(true);
                if let Some(shader) = sk::gradient::shaders::linear_gradient(
                    (
                        sk::Point::new(left + shift - w * 0.5, top),
                        sk::Point::new(left + shift + w * 0.5, top),
                    ),
                    &grad,
                    None,
                ) {
                    paint.set_shader(shader);
                } else {
                    paint.set_color(c_acc);
                }
                canvas.save();
                canvas.clip_rect(sk::Rect::from_xywh(left, top, w, h), None, false);
                canvas.draw_str(&text, sk::Point::new(left, baseline_y), &font, &paint);
                canvas.restore();
            })))
    }
}
