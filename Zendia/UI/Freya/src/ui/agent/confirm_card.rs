/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Inline confirmation card for agent permission requests inside the conversation log.
//!
//! Pending confirmations rise in over `260 ms` (`motion::CONFIRM_RISE`) with an amber-tinted card
//! (`ThemeRole::AgentConfirm`), a `1.8 s` breathing shield icon, `"PERMISSION NEEDED"`, bolded action prompt, and
//! `Deny [N]` / `Allow [Y]` buttons with hover, `scale(0.97)` press, and keyboard focus rings. Resolved confirmations
//! collapse to a single quiet trace row (`✓ Allowed · ...` in success green or `✕ Declined · ...` in danger red).

use crate::ui::core_view::use_core;
use crate::ui::kit::{KeyLabel, focus_ring_border, mix_color, use_interactive_feedback};
use crate::ui::theme::{ResolvedScheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{ConversationId, StepId};

/// Inline permission prompt inside an agent conversation.
#[derive(Clone, PartialEq)]
pub struct AgentConfirmCard {
    pub conversation_id: ConversationId,
    pub step_id: StepId,
    pub agent_name: String,
    pub prompt: String,
    pub resolved: Option<bool>,
    pub static_snapshot: bool,
    key: DiffKey,
}

impl KeyExt for AgentConfirmCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl AgentConfirmCard {
    pub fn new(
        conversation_id: ConversationId,
        step_id: StepId,
        agent_name: impl Into<String>,
        prompt: impl Into<String>,
        resolved: Option<bool>,
    ) -> Self {
        Self {
            conversation_id,
            step_id,
            agent_name: agent_name.into(),
            prompt: prompt.into(),
            resolved,
            static_snapshot: false,
            key: DiffKey::None,
        }
    }

    pub fn static_snapshot(mut self, static_snapshot: bool) -> Self {
        self.static_snapshot = static_snapshot;
        self
    }
}

impl Component for AgentConfirmCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let is_pending = self.resolved.is_none();
        let animate_live = is_pending && !self.static_snapshot;

        // CC-01: 260ms rise entrance (`zendia-agent-rise`) when pending.
        let rise = use_animation_with_dependencies(&animate_live, |conf, live| {
            if *live {
                conf.on_creation(OnCreation::Run);
                conf.on_change(OnChange::Rerun);
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(0.0, 1.0)
                .duration(motion::CONFIRM_RISE)
                .ease(Ease::Out)
                .function(motion::ease_rise_fn())
        });

        // CC-01: 1.8s ease-in-out breathing shield while pending (stops once resolved or in static snapshots).
        let shield_pulse = use_animation_with_dependencies(&animate_live, |conf, live| {
            if *live {
                conf.on_creation(OnCreation::Run);
                conf.on_change(OnChange::Rerun);
                conf.on_finish(OnFinish::reverse());
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(1.0, 0.42)
                .duration(motion::CONFIRM_SHIELD_PULSE)
                .ease(Ease::InOut)
        });

        if let Some(allowed) = self.resolved {
            let (mark, role, verb) = if allowed {
                ("\u{2713}", ThemeRole::Success, "Allowed")
            } else {
                ("\u{2715}", ThemeRole::Danger, "Declined")
            };
            return rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .cross_align(Alignment::start())
                .spacing(7.0)
                .padding(Gaps::new(2.0, 0.0, 2.0, 0.0))
                .child(
                    label()
                        .text(mark)
                        .font_size(12.0)
                        .font_weight(FontWeight::BOLD)
                        .color(theme.color(role)),
                )
                .child(
                    rect().width(Size::flex(1.0)).child(
                        label()
                            .text(format!("{verb} \u{00b7} {}", self.prompt))
                            .font_size(12.0)
                            .color(theme.color(ThemeRole::TextMuted))
                            .max_lines(2),
                    ),
                )
                .into_element();
        }

        let rise_t = if animate_live {
            rise.get().value().clamp(0.0, 1.0)
        } else {
            1.0
        };
        let shield_opacity = if animate_live {
            shield_pulse.get().value().clamp(0.42, 1.0)
        } else {
            1.0
        };

        let amber = theme.rgba(ThemeRole::AgentConfirm);
        let text_rgba = theme.rgba(ThemeRole::Text);
        let head_color = amber.mix(text_rgba, 0.30).to_color();
        let card_bg = amber.with_alpha(0.12).to_color();
        let card_border = amber.with_alpha(0.55).to_color();
        let card_shadow = Shadow::new()
            .y(6.0)
            .blur(18.0)
            .spread(-12.0)
            .color(amber.with_alpha(0.70).to_color());

        let conv_id = self.conversation_id;
        let step_id = self.step_id;
        let core_deny = core.clone();
        let core_allow = core.clone();
        let action_text = self.prompt.trim_end_matches('?').to_string();

        // Shield SVG matching `zendia.css:3576`.
        const CONFIRM_SHIELD_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 2 4 5v6c0 5.25 3.4 10.15 8 11.5 4.6-1.35 8-6.25 8-11.5V5l-8-3Zm-1 6h2v6h-2V8Zm0 8h2v2h-2v-2Z" fill="currentColor"/></svg>"#;

        rect()
            .width(Size::fill())
            .offset_y((1.0 - rise_t) * 10.0)
            .opacity(rise_t)
            .padding(Gaps::new(12.0, 14.0, 12.0, 14.0))
            .spacing(8.0)
            .corner_radius(CornerRadius::new_all(14.0))
            .background(card_bg)
            .border(
                Border::new()
                    .width(0.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(card_border),
            )
            .shadow(card_shadow)
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(7.0)
                    .child(
                        rect().opacity(shield_opacity).child(
                            SvgViewer::new(("zendia-confirm-shield", Bytes::from_static(CONFIRM_SHIELD_SVG)))
                                .width(Size::px(14.0))
                                .height(Size::px(14.0))
                                .color(amber.to_color()),
                        ),
                    )
                    .child(
                        label()
                            .text("PERMISSION NEEDED")
                            .font_size(11.0)
                            .font_weight(FontWeight::BOLD)
                            .color(head_color),
                    ),
            )
            .child(
                paragraph()
                    .width(Size::fill())
                    .font_size(13.0)
                    .line_height(1.45)
                    .color(theme.color(ThemeRole::Text))
                    .span(Span::new(format!("Allow {} to ", self.agent_name)))
                    .span(Span::new(action_text).font_weight(FontWeight::SEMI_BOLD))
                    .span(Span::new("?")),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::end())
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .child(ConfirmActionButton {
                        label: "Deny",
                        key_cap: "N",
                        a11y_alt: "Deny permission",
                        primary: false,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_deny.invoke_or_log(Command::AgentConfirm {
                                conversation_id: conv_id,
                                step_id,
                                allow: false,
                            });
                        }),
                        key: DiffKey::None,
                    })
                    .child(ConfirmActionButton {
                        label: "Allow",
                        key_cap: "Y",
                        a11y_alt: "Allow permission",
                        primary: true,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_allow.invoke_or_log(Command::AgentConfirm {
                                conversation_id: conv_id,
                                step_id,
                                allow: true,
                            });
                        }),
                        key: DiffKey::None,
                    }),
            )
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct ConfirmActionButton {
    label: &'static str,
    key_cap: &'static str,
    a11y_alt: &'static str,
    primary: bool,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for ConfirmActionButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ConfirmActionButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let accent = theme.color(ThemeRole::Accent);
        let text_col = theme.color(ThemeRole::Text);
        let on_accent = match theme.scheme {
            ResolvedScheme::Light => Color::WHITE,
            ResolvedScheme::Dark => theme.color(ThemeRole::Ink),
        };

        let (bg, fg, base_border) = if self.primary {
            let hover_bg = mix_color(accent, Color::BLACK, 0.12);
            let press_bg = mix_color(accent, Color::BLACK, 0.20);
            (fb.background(accent, hover_bg, press_bg), on_accent, Border::new())
        } else {
            let hover_bg = theme.rgba(ThemeRole::Text).with_alpha(0.08).to_color();
            let press_bg = theme.rgba(ThemeRole::Text).with_alpha(0.14).to_color();
            (
                fb.background(Color::TRANSPARENT, hover_bg, press_bg),
                text_col,
                Border::new()
                    .width(0.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::Border)),
            )
        };

        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, base_border);
        let scale = 1.0 - 0.03 * fb.press_progress();
        let on_press = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.a11y_alt)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .padding(Gaps::new(5.0, 10.0, 5.0, 14.0))
            .corner_radius(CornerRadius::new_all(9.0))
            .background(bg)
            .border(border)
            .scale(scale)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| on_press.call(e))
            .child(
                label()
                    .text(self.label)
                    .font_size(12.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(fg),
            )
            .child(KeyLabel::new(self.key_cap))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
