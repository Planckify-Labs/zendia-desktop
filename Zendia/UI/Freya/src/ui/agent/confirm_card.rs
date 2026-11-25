/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Inline confirmation card for agent permission requests inside the conversation log.
//!
//! Pending confirmations render an amber-tinted card (`ThemeRole::AgentConfirm`) with `"PERMISSION NEEDED"`,
//! shield icon, prompt, and `Deny [N]` / `Allow [Y]` buttons. Resolved confirmations collapse to a single quiet
//! trace row (`✓ Allowed · ...` or `✕ Declined · ...`).

use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, KeyLabel};
use crate::ui::theme::{ResolvedScheme, ThemeRole, use_theme};
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
            key: DiffKey::None,
        }
    }
}

impl Component for AgentConfirmCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut deny_hovered = use_state(|| false);
        let mut allow_hovered = use_state(|| false);

        if let Some(allowed) = self.resolved {
            let (icon_name, role, verb) = if allowed {
                ("check", ThemeRole::Success, "Allowed")
            } else {
                ("close", ThemeRole::TextMuted, "Declined")
            };
            return rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(6.0)
                .padding(Gaps::new(4.0, 8.0, 4.0, 6.0))
                .child(Icon::new(icon_name).size(12.0).role(role))
                .child(
                    label()
                        .text(format!("{verb} · {}", self.prompt))
                        .font_size(11.5)
                        .color(theme.color(ThemeRole::TextMuted))
                        .max_lines(2),
                );
        }

        let amber = theme.rgba(ThemeRole::AgentConfirm);
        let accent = theme.rgba(ThemeRole::Accent);
        let card_bg = match theme.scheme {
            ResolvedScheme::Dark => amber.with_alpha(0.09).to_color(),
            ResolvedScheme::Light => amber.with_alpha(0.11).to_color(),
        };
        let card_border = amber.with_alpha(0.38).to_color();
        let deny_bg = if *deny_hovered.read() {
            theme.color(ThemeRole::SurfaceRaised)
        } else {
            theme.rgba(ThemeRole::SurfaceRaised).with_alpha(0.6).to_color()
        };
        let allow_bg = if *allow_hovered.read() {
            accent.with_alpha(1.0).to_color()
        } else {
            accent.with_alpha(0.92).to_color()
        };

        let conv_id = self.conversation_id;
        let step_id = self.step_id;
        let core_deny = core.clone();
        let core_allow = core.clone();

        rect()
            .width(Size::fill())
            .padding(Gaps::new(11.0, 12.0, 11.0, 12.0))
            .spacing(8.0)
            .corner_radius(CornerRadius::new_all(12.0))
            .background(card_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(card_border),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .child(Icon::new("security").size(13.0).role(ThemeRole::AgentConfirm))
                    .child(
                        label()
                            .text("PERMISSION NEEDED")
                            .font_size(10.0)
                            .font_weight(FontWeight::BOLD)
                            .color(theme.color(ThemeRole::AgentConfirm)),
                    ),
            )
            .child(
                label()
                    .text(format!(
                        "Allow {} to {}?",
                        self.agent_name,
                        self.prompt.trim_end_matches('?')
                    ))
                    .font_size(12.5)
                    .font_weight(FontWeight::MEDIUM)
                    .line_height(1.4)
                    .color(theme.color(ThemeRole::Text)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::end())
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Deny permission")
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(6.0)
                            .height(Size::px(28.0))
                            .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
                            .corner_radius(CornerRadius::new_all(8.0))
                            .background(deny_bg)
                            .border(
                                Border::new()
                                    .width(1.0)
                                    .alignment(BorderAlignment::Inner)
                                    .fill(theme.color(ThemeRole::BorderSubtle)),
                            )
                            .cursor(CursorIcon::Pointer)
                            .on_pointer_enter(move |_| deny_hovered.set_if_modified(true))
                            .on_pointer_leave(move |_| deny_hovered.set_if_modified(false))
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_deny.invoke_or_log(Command::AgentConfirm {
                                    conversation_id: conv_id,
                                    step_id,
                                    allow: false,
                                });
                            })
                            .child(
                                label()
                                    .text("Deny")
                                    .font_size(12.0)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(theme.color(ThemeRole::Text)),
                            )
                            .child(KeyLabel::new("N")),
                    )
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Allow permission")
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(6.0)
                            .height(Size::px(28.0))
                            .padding(Gaps::new(0.0, 11.0, 0.0, 11.0))
                            .corner_radius(CornerRadius::new_all(8.0))
                            .background(allow_bg)
                            .cursor(CursorIcon::Pointer)
                            .on_pointer_enter(move |_| allow_hovered.set_if_modified(true))
                            .on_pointer_leave(move |_| allow_hovered.set_if_modified(false))
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_allow.invoke_or_log(Command::AgentConfirm {
                                    conversation_id: conv_id,
                                    step_id,
                                    allow: true,
                                });
                            })
                            .child(
                                label()
                                    .text("Allow")
                                    .font_size(12.0)
                                    .font_weight(FontWeight::BOLD)
                                    .color(theme.color(ThemeRole::TextOnAccent)),
                            )
                            .child(KeyLabel::new("Y")),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
