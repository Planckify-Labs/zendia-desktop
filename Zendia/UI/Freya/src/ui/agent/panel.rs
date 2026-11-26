/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Full AI agent side panel (`AgentPanel`): header, service-down callout, empty & no-agent states, conversation log
//! with collapsible step groups, Markdown answers, interactive tables & declared UI cards, confirmation cards,
//! follow-scroll jump pill, composer with attachments & media compatibility warnings, model card, and instructions.

use crate::ui::agent::confirm_card::AgentConfirmCard;
use crate::ui::agent::follow::{ConversationFollow, JumpToLatest, use_conversation_follow};
use crate::ui::agent::instructions::AgentInstructionsPage;
use crate::ui::agent::markdown::{MarkdownView, copy_to_clipboard};
use crate::ui::agent::model_card::{AgentModelCard, short_model_name};
use crate::ui::agent::motion::{WorkingRing, WorkingRow};
use crate::ui::agent::ui_view::{AgentDeclaredPageCard, AgentTableCard};
use crate::ui::agent::{
    agent_badge_emblem, agent_input_colors, agent_logo_element, chevron_down_icon, chevron_right_icon, chevron_up_icon,
    paperclip_icon,
};
use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, IconButton, KitTooltip, TextField};
use crate::ui::panels::{
    DictationState, DockLayoutControls, ResolvedDockPanel, active_agent_accent, try_use_dock_state,
};
use crate::ui::theme::{ResolvedScheme, ThemeRole, use_theme};
use freya::prelude::*;
use std::collections::HashSet;
use std::time::Duration;
use zendia_browser_core::agent::{
    AGENT_PANEL_ID, AttachmentKind, InstructionAttachment, SteerStatus, check_media_compatibility, format_file_size,
    seed_of, working_word,
};
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{SidePanelId, SidePanelLayout};
use zendia_browser_core::model::{
    AgentId, Conversation, ConversationId, ConversationStatus, ModelEffort, ModelId, ModelInfo, ServiceId,
    ServiceStatus, StepId, StepKind, StepStatus,
};

const WEBCHAT_PANEL_ID: &str = "zendia.webchat";

/// Main AI agent side panel hosted inside [`crate::ui::panels::DockFrame`].
#[derive(Clone, PartialEq)]
pub struct AgentPanel {
    pub resolved: ResolvedDockPanel,
    key: DiffKey,
}

impl KeyExt for AgentPanel {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl AgentPanel {
    pub fn new(resolved: ResolvedDockPanel) -> Self {
        Self {
            resolved,
            key: DiffKey::None,
        }
    }
}

impl Component for AgentPanel {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let page_ctx = try_consume_context::<crate::ui::page::PageSurfaceContext>();
        let scroll_controller = use_scroll_controller(ScrollConfig::default);
        let composer_a11y_id = use_a11y();
        let fallback_composer = use_state(String::new);
        let composer_text = dock_state.map(|ds| ds.composer_text).unwrap_or(fallback_composer);
        let expanded_groups = use_state(HashSet::<u64>::new);
        let copied_step = use_state(|| None::<u64>);
        let mut agent_picker_open = use_state(|| false);
        let mut follow = use_conversation_follow(scroll_controller);
        let mut last_shown = use_state(|| (None::<ConversationId>, None::<StepId>));

        let (
            active_tab_id,
            active_tab_title,
            active_agent_id,
            active_agent_name,
            available_agents,
            active_conv,
            other_convs,
            models,
            selected_model,
            effective_effort,
            active_context_id,
            instructions,
            enabled_instruction_count,
            captcha_finding,
            agent_server_status,
            agent_accent,
        ) = {
            let model = core.model().read();
            let active_tab = model.active_tab();
            let active_tab_id = active_tab.map(|t| t.id);
            let active_tab_title = active_tab
                .map(|t| t.display_title().to_string())
                .unwrap_or_else(|| "Current tab".to_string());
            let conv = active_tab_id
                .and_then(|tid| model.active_conversation_for_tab(tid))
                .cloned();
            let agent_id = conv
                .as_ref()
                .map(|c| c.agent_id.clone())
                .or_else(|| model.default_agent().map(|a| a.id.clone()))
                .unwrap_or_else(|| AgentId::new("claude-code"));
            let agent_name = model
                .agent(&agent_id)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Claude Code".to_string());
            let available_agents: Vec<_> = model.agents.iter().filter(|a| a.available).cloned().collect();
            let other_convs: Vec<_> = model
                .conversations
                .iter()
                .filter(|c| conv.as_ref().is_none_or(|cur| cur.id != c.id))
                .take(3)
                .cloned()
                .collect();
            let models = model.models.clone();
            let selected_model = model.effective_model_for_agent(&agent_id).cloned().unwrap_or_else(|| {
                ModelInfo::simple(
                    ModelId::new("claude-sonnet-4"),
                    "Claude Sonnet 4.6",
                    "Anthropic",
                    1_000_000,
                    ModelEffort::High,
                )
            });
            let effective_effort = model
                .agent_selections
                .get(&agent_id)
                .and_then(|s| s.effort)
                .unwrap_or(selected_model.effort);
            let active_context_id = model.agent_selections.get(&agent_id).and_then(|s| s.context_id.clone());
            let instructions = model.agent_instructions.clone();
            let enabled_instruction_count = instructions.iter().filter(|i| i.enabled).count();
            let captcha_finding = active_tab_id.and_then(|tid| model.tab_captcha_findings.get(&tid).cloned());
            let agent_server_status = model
                .services
                .iter()
                .find(|s| s.service == ServiceId::AgentServer)
                .map(|s| s.status.clone())
                .unwrap_or(ServiceStatus::Running);
            let agent_accent = active_agent_accent(&model, &theme);

            (
                active_tab_id,
                active_tab_title,
                agent_id,
                agent_name,
                available_agents,
                conv,
                other_convs,
                models,
                selected_model,
                effective_effort,
                active_context_id,
                instructions,
                enabled_instruction_count,
                captcha_finding,
                agent_server_status,
                agent_accent,
            )
        };

        // A reply finished while the panel was closed is seen once the panel shows it.
        let unseen_conversation = active_conv.as_ref().filter(|conv| conv.unseen).map(|conv| conv.id);
        use_side_effect_with_deps(&unseen_conversation, {
            let core = core.clone();
            move |conversation: &Option<ConversationId>| {
                if let Some(conversation_id) = *conversation {
                    core.invoke_or_log(Command::AgentMarkSeen { conversation_id });
                }
            }
        });

        // Another conversation starts at its end; a newly sent request glides to the top once it is laid out.
        let shown = (
            active_conv.as_ref().map(|conv| conv.id),
            active_conv.as_ref().and_then(|conv| {
                conv.steps
                    .iter()
                    .rev()
                    .find(|step| step.kind == StepKind::UserPrompt)
                    .map(|step| step.id)
            }),
        );
        use_side_effect_with_deps(&shown, move |shown: &(Option<ConversationId>, Option<StepId>)| {
            let previous = *last_shown.peek();
            if previous.0 != shown.0 {
                follow.reset();
            } else if previous.1 != shown.1 && shown.1.is_some() {
                follow.request_glide();
            }
            last_shown.set(*shown);
        });

        let theme_accent = theme.color(ThemeRole::Accent);
        let border_col = theme.color(ThemeRole::BorderSubtle);
        let pill_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(20, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(12, 20, 24, 38),
        };
        let composer_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(120, 36, 40, 59),
            ResolvedScheme::Light => Color::from_argb(225, 244, 245, 252),
        };

        let is_service_down = !matches!(agent_server_status, ServiceStatus::Running);
        let service_reason = match &agent_server_status {
            ServiceStatus::Stopped { reason } => reason
                .clone()
                .unwrap_or_else(|| "AgentServer is not running.".to_string()),
            ServiceStatus::Starting => "Starting AgentServer…".to_string(),
            ServiceStatus::Restarting => "Restarting AgentServer…".to_string(),
            ServiceStatus::Running => String::new(),
        };

        let pending_confirm = active_conv.as_ref().and_then(|c| {
            c.steps.iter().rev().find_map(|s| match &s.kind {
                StepKind::Confirmation { resolved: None, .. } => Some((c.id, s.id)),
                _ => None,
            })
        });
        let is_working = active_conv
            .as_ref()
            .is_some_and(|c| c.status == ConversationStatus::Working);

        // --- Header ---
        let core_all_agents = core.clone();
        let core_new = core.clone();
        let core_captcha = core.clone();
        let core_webchat = core.clone();
        let core_close = core.clone();

        let agent_chip_bg = Color::from_argb(
            match theme.scheme {
                ResolvedScheme::Dark => 34,
                ResolvedScheme::Light => 28,
            },
            agent_accent.r(),
            agent_accent.g(),
            agent_accent.b(),
        );
        let agent_chip_border = Color::from_argb(
            match theme.scheme {
                ResolvedScheme::Dark => 86,
                ResolvedScheme::Light => 78,
            },
            agent_accent.r(),
            agent_accent.g(),
            agent_accent.b(),
        );

        let mut other_stack = rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(3.0);
        for other in &other_convs {
            let cid = other.id;
            let core_sw = core.clone();
            let has_dot = other.status == ConversationStatus::Working || other.unseen;
            other_stack = other_stack.child(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(format!("Switch to chat: {}", other.title))
                    .width(Size::px(22.0))
                    .height(Size::px(22.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(pill_bg)
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        core_sw.invoke_or_log(Command::AgentSwitchConversation { conversation_id: cid });
                    })
                    .child(agent_logo_element(&other.agent_id.0, 12.0, false))
                    .maybe_child(has_dot.then(|| {
                        rect()
                            .position(Position::new_absolute().top(0.0).right(0.0))
                            .width(Size::px(6.0))
                            .height(Size::px(6.0))
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(agent_accent)
                    })),
            );
        }

        let header = rect()
            .width(Size::fill())
            .height(Size::px(44.0))
            .padding(Gaps::new(0.0, 8.0, 0.0, 10.0))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .main_align(Alignment::SpaceBetween)
            .spacing(6.0)
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Switch agent")
                            .height(Size::px(28.0))
                            .padding(Gaps::new(0.0, 9.0, 0.0, 4.0))
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(agent_chip_bg)
                            .border(
                                Border::new()
                                    .width(1.0)
                                    .alignment(BorderAlignment::Inner)
                                    .fill(agent_chip_border),
                            )
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(6.0)
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                let next = !*agent_picker_open.peek();
                                agent_picker_open.set(next);
                                if let Some(mut ds) = dock_state
                                    && *ds.model_card_open.peek()
                                {
                                    ds.model_card_open.set(false);
                                }
                            })
                            .child(agent_badge_emblem(&active_agent_id.0, 20.0, 13.0, 999.0, false))
                            .child(
                                label()
                                    .text(active_agent_name.clone())
                                    .font_size(12.0)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(theme.color(ThemeRole::Text))
                                    .max_lines(1),
                            )
                            .child(chevron_down_icon(10.0, theme.color(ThemeRole::TextMuted))),
                    )
                    .maybe_child((!other_convs.is_empty()).then_some(other_stack)),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(3.0)
                    .maybe_child(captcha_finding.as_ref().map(|_| {
                        let tab_for_cap = active_tab_id;
                        IconButton::new("captcha", "Summarize CAPTCHA on this page")
                            .tooltip("Summarize CAPTCHA on this page")
                            .size(24.0)
                            .icon_size(12.5)
                            .role(ThemeRole::Warning)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_captcha.invoke_or_log(Command::AgentSummarizeCaptcha { tab_id: tab_for_cap });
                            })
                    }))
                    .child(
                        IconButton::new("layers", "All agents")
                            .tooltip("All agents")
                            .size(24.0)
                            .icon_size(12.5)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_all_agents.invoke_or_log(Command::AgentList);
                            }),
                    )
                    .child(
                        IconButton::new("plus", "New chat")
                            .tooltip("New chat")
                            .size(24.0)
                            .icon_size(12.5)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_new.invoke_or_log(Command::AgentNew { tab_id: active_tab_id });
                            }),
                    )
                    .child(
                        IconButton::new("webchat", "Open web chat")
                            .tooltip("Open web chat")
                            .size(24.0)
                            .icon_size(12.5)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_webchat.invoke_or_log(Command::PanelToggle {
                                    panel_id: SidePanelId::new(WEBCHAT_PANEL_ID),
                                });
                            }),
                    )
                    .child(DockLayoutControls {
                        panel_id: self.resolved.panel_id.clone(),
                        prefs: self.resolved.prefs,
                        split_fell_back: self.resolved.split_fell_back,
                        accent: theme_accent,
                    })
                    .child(
                        IconButton::new("close", "Close agent panel")
                            .tooltip("Close")
                            .shortcut("Esc")
                            .size(24.0)
                            .icon_size(12.0)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_close.invoke_or_log(Command::PanelClose {
                                    panel_id: SidePanelId::new(AGENT_PANEL_ID),
                                });
                            }),
                    ),
            );

        // --- Service-down banner ---
        let service_banner = is_service_down.then(|| {
            let core_restart = core.clone();
            rect()
                .width(Size::fill())
                .margin(Gaps::new(8.0, 12.0, 0.0, 12.0))
                .padding(Gaps::new(9.0, 10.0, 9.0, 10.0))
                .corner_radius(CornerRadius::new_all(10.0))
                .background(theme.rgba(ThemeRole::Danger).with_alpha(0.14).to_color())
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.rgba(ThemeRole::Danger).with_alpha(0.45).to_color()),
                )
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .cross_align(Alignment::center())
                .main_align(Alignment::SpaceBetween)
                .spacing(8.0)
                .child(
                    rect()
                        .width(Size::flex(1.0))
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::center())
                        .spacing(8.0)
                        .child(Icon::new("warning").size(15.0).role(ThemeRole::Danger))
                        .child(
                            rect()
                                .width(Size::flex(1.0))
                                .spacing(2.0)
                                .child(
                                    label()
                                        .text("AgentServer unavailable")
                                        .font_size(12.0)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .color(theme.color(ThemeRole::Text)),
                                )
                                .child(
                                    label()
                                        .text(service_reason)
                                        .font_size(11.0)
                                        .color(theme.color(ThemeRole::TextMuted))
                                        .max_lines(2),
                                ),
                        ),
                )
                .child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("Restart AgentServer")
                        .height(Size::px(24.0))
                        .padding(Gaps::new(0.0, 9.0, 0.0, 9.0))
                        .corner_radius(CornerRadius::new_all(6.0))
                        .background(theme.rgba(ThemeRole::SurfaceRaised).with_alpha(0.85).to_color())
                        .center()
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_restart.invoke_or_log(Command::DebugToggleAgentService);
                        })
                        .child(
                            label()
                                .text("Retry")
                                .font_size(11.0)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(theme.color(ThemeRole::Text)),
                        ),
                )
        });

        // --- Main Body (No agents / Empty / Conversation log) ---
        let body: Element = if available_agents.is_empty() {
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .padding(Gaps::new(24.0, 28.0, 24.0, 28.0))
                .center()
                .spacing(12.0)
                .child(Icon::new("sparkles").size(36.0).role(ThemeRole::TextMuted))
                .child(
                    label()
                        .width(Size::fill())
                        .text("No local coding agent found")
                        .font_size(15.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .text_align(TextAlign::Center)
                        .color(theme.color(ThemeRole::Text)),
                )
                .child(
                    label()
                        .width(Size::fill())
                        .text("Install Claude Code, Codex, Cursor, OpenCode, or Kiro CLI and restart Zendia to chat with your browser.")
                        .font_size(12.0)
                        .line_height(1.48)
                        .text_align(TextAlign::Center)
                        .color(theme.color(ThemeRole::TextMuted)),
                )
                .into_element()
        } else if active_conv.as_ref().is_none_or(|c| c.steps.is_empty()) {
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .padding(Gaps::new(24.0, 36.0, 24.0, 36.0))
                .center()
                .spacing(12.0)
                .child(agent_badge_emblem(&active_agent_id.0, 48.0, 30.0, 14.0, true))
                .child(
                    label()
                        .width(Size::fill())
                        .text(format!("Ask {active_agent_name} about this tab"))
                        .font_size(14.5)
                        .font_weight(FontWeight::BOLD)
                        .text_align(TextAlign::Center)
                        .color(theme_accent),
                )
                .child(
                    rect().width(Size::fill()).max_width(Size::px(300.0)).child(
                        label()
                            .width(Size::fill())
                            .text("It reads the page, clicks, types and moves between pages for you, and asks before anything you have to allow.")
                            .font_size(12.0)
                            .line_height(1.48)
                            .text_align(TextAlign::Center)
                            .color(theme.color(ThemeRole::TextMuted)),
                    ),
                )
                .into_element()
        } else {
            let conv = active_conv.as_ref().unwrap();
            let trace_items = render_conversation_steps(
                conv,
                &active_agent_name,
                agent_accent,
                &theme,
                expanded_groups,
                copied_step,
                follow,
            );
            let room = *follow.room.read();

            let scroll_view = ScrollView::new_controlled(scroll_controller)
                .width(Size::fill())
                .height(Size::fill())
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(12.0, 14.0, 14.0, 14.0))
                        .spacing(12.0)
                        .on_wheel(move |_| follow.release())
                        .on_sized(move |e: Event<SizedEventData>| {
                            follow.content_sized(e.area.min_y(), e.area.height());
                        })
                        .children(trace_items)
                        .maybe_child((conv.status == ConversationStatus::Working).then(|| {
                            let seed = seed_of(&format!("{}:{}", conv.id.0, conv.steps.len()));
                            let word = working_word(seed, conv.elapsed_secs);
                            WorkingRow::new(word, conv.elapsed_secs, agent_accent)
                        }))
                        .child(rect().width(Size::fill()).height(Size::px(room))),
                );

            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .child(scroll_view)
                .child(JumpToLatest::new(follow))
                .into_element()
        };

        // --- Composer ---
        let staged_files = dock_state
            .map(|ds| ds.staged_attachments.read().clone())
            .unwrap_or_default();
        let staged_kinds: Vec<AttachmentKind> = staged_files.iter().map(|f| f.kind).collect();
        let media_issue = check_media_compatibility(&selected_model, &models, &staged_kinds);
        let dictation_state = dock_state.map(|ds| *ds.dictation.read()).unwrap_or_default();
        let model_card_open = dock_state.is_some_and(|ds| *ds.model_card_open.read());
        let instructions_open = dock_state.is_some_and(|ds| *ds.instructions_open.read());

        let attachments_row = (!staged_files.is_empty()).then(|| {
            let mut row = rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .spacing(6.0)
                .padding(Gaps::new(0.0, 0.0, 4.0, 0.0));
            for (idx, file) in staged_files.iter().enumerate() {
                let icon_name = if file.kind == AttachmentKind::Image {
                    "new-tab-image"
                } else {
                    "page-portrait"
                };
                row = row.child(
                    rect()
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(5.0)
                        .height(Size::px(24.0))
                        .padding(Gaps::new(0.0, 6.0, 0.0, 8.0))
                        .corner_radius(CornerRadius::new_all(8.0))
                        .background(pill_bg)
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(border_col),
                        )
                        .child(Icon::new(icon_name).size(11.0).role(ThemeRole::Accent))
                        .child(
                            label()
                                .text(format!("{} · {}", file.name, format_file_size(file.size_bytes)))
                                .font_size(10.5)
                                .color(theme.color(ThemeRole::Text))
                                .max_lines(1),
                        )
                        .child(
                            rect()
                                .a11y_role(AccessibilityRole::Button)
                                .a11y_alt("Remove attachment")
                                .width(Size::px(16.0))
                                .height(Size::px(16.0))
                                .center()
                                .cursor(CursorIcon::Pointer)
                                .on_press(move |e: Event<PressEventData>| {
                                    e.stop_propagation();
                                    if let Some(mut ds) = dock_state {
                                        let mut files = ds.staged_attachments.peek().clone();
                                        if idx < files.len() {
                                            files.remove(idx);
                                            ds.staged_attachments.set(files);
                                        }
                                    }
                                })
                                .child(Icon::new("close").size(9.0).role(ThemeRole::TextMuted)),
                        ),
                );
            }
            row
        });

        let media_warning_banner = media_issue.map(|issue| {
            let kind_names: Vec<&str> = issue
                .unsupported_kinds
                .iter()
                .map(|k| match k {
                    AttachmentKind::Image => "images",
                    AttachmentKind::Video => "video",
                    AttachmentKind::Pdf => "PDFs",
                    AttachmentKind::Document => "documents",
                })
                .collect();
            let core_switch = core.clone();
            let aid_sw = active_agent_id.clone();

            rect()
                .width(Size::fill())
                .margin(Gaps::new(0.0, 0.0, 6.0, 0.0))
                .padding(Gaps::new(7.0, 9.0, 7.0, 9.0))
                .corner_radius(CornerRadius::new_all(10.0))
                .background(theme.rgba(ThemeRole::Warning).with_alpha(0.14).to_color())
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.rgba(ThemeRole::Warning).with_alpha(0.45).to_color()),
                )
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .cross_align(Alignment::center())
                .main_align(Alignment::SpaceBetween)
                .spacing(6.0)
                .child(
                    rect()
                        .width(Size::flex(1.0))
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::center())
                        .spacing(6.0)
                        .child(Icon::new("warning").size(13.0).role(ThemeRole::Warning))
                        .child(
                            label()
                                .text(format!("{} cannot read {}", issue.model_name, kind_names.join(", ")))
                                .font_size(11.0)
                                .color(theme.color(ThemeRole::Text))
                                .max_lines(2),
                        ),
                )
                .maybe_child(
                    issue
                        .suggested_model_id
                        .clone()
                        .zip(issue.suggested_model_name.clone())
                        .map(|(target_id, target_name)| {
                            rect()
                                .a11y_role(AccessibilityRole::Button)
                                .a11y_alt(format!("Switch to {target_name}"))
                                .height(Size::px(22.0))
                                .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                                .corner_radius(CornerRadius::new_all(6.0))
                                .background(theme.color(ThemeRole::Warning))
                                .center()
                                .cursor(CursorIcon::Pointer)
                                .on_press(move |e: Event<PressEventData>| {
                                    e.stop_propagation();
                                    core_switch.invoke_or_log(Command::AgentModel {
                                        agent_id: Some(aid_sw.clone()),
                                        model_id: Some(target_id.clone()),
                                    });
                                })
                                .child(
                                    label()
                                        .text(format!("Use {target_name}"))
                                        .font_size(10.5)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .color(theme.color(ThemeRole::Ink)),
                                )
                        }),
                )
        });

        let send_action = {
            let core_send = core.clone();
            let conv_id_opt = active_conv.as_ref().map(|c| c.id);
            let aid_send = active_agent_id.clone();
            move || {
                let text = composer_text.peek().trim().to_string();
                let files = dock_state
                    .map(|ds| ds.staged_attachments.peek().clone())
                    .unwrap_or_default();
                if text.is_empty() && files.is_empty() {
                    return;
                }
                let mut ct = composer_text;
                ct.set(String::new());
                if let Some(mut ds) = dock_state {
                    ds.staged_attachments.set(Vec::new());
                }
                if files.is_empty() {
                    core_send.invoke_or_log(Command::AgentSend {
                        conversation_id: conv_id_opt,
                        agent_id: Some(aid_send.clone()),
                        prompt: text,
                    });
                } else {
                    let file_names = files.into_iter().map(|f| f.name).collect();
                    core_send.invoke_or_log(Command::AgentSendWithFiles {
                        conversation_id: conv_id_opt,
                        agent_id: Some(aid_send.clone()),
                        prompt: text,
                        files: file_names,
                    });
                }
            }
        };

        let send_for_key = send_action.clone();
        let send_for_btn = send_action;
        let core_key = core.clone();
        let core_stop = core.clone();
        let stop_conv_id = active_conv.as_ref().map(|c| c.id);
        let eff_layout = self.resolved.effective_layout;

        let placeholder_str = if is_working {
            format!("{active_agent_name} is working… type to steer it")
        } else {
            format!("Ask {active_agent_name} to do something on this tab")
        };

        // `TextField` handles Ctrl+W/U/H natively and stops editing chords from leaking to browser shortcuts.
        let composer_colors = agent_input_colors(&theme, true);
        let pref_color = |pref: Option<Preference<Color>>, fallback: Color| match pref {
            Some(Preference::Specific(c)) => c,
            _ => fallback,
        };
        let composer_input = rect().width(Size::fill()).font_size(13.0).child(
            TextField::new(composer_text)
                .a11y_id(composer_a11y_id)
                .auto_focus(true)
                .multiline(true)
                .placeholder(placeholder_str)
                .width(Size::fill())
                .height(Size::px(36.0))
                .color(pref_color(composer_colors.color, theme.color(ThemeRole::Text)))
                .placeholder_color(pref_color(
                    composer_colors.placeholder_color,
                    theme.color(ThemeRole::TextMuted),
                ))
                .padding(Gaps::new(2.0, 0.0, 2.0, 0.0))
                .corner_radius(CornerRadius::new_all(4.0))
                .on_pre_key_down(move |e: Event<KeyboardEventData>| {
                    let shift = e.modifiers.contains(Modifiers::SHIFT);
                    let ctrl = e.modifiers.contains(Modifiers::CONTROL);
                    let alt = e.modifiers.contains(Modifiers::ALT);
                    let meta = e.modifiers.contains(Modifiers::META);
                    let plain = !ctrl && !alt && !meta;
                    let empty = composer_text.peek().trim().is_empty();

                    match &e.key {
                        Key::Named(NamedKey::Enter) if !shift => {
                            e.stop_propagation();
                            e.prevent_default();
                            send_for_key();
                            false
                        }
                        Key::Named(NamedKey::Tab) if plain && !is_working => {
                            e.stop_propagation();
                            e.prevent_default();
                            let direction = if shift { -1 } else { 1 };
                            core_key.invoke_or_log(Command::AgentCycle { direction });
                            false
                        }
                        Key::Character(ch) if empty && plain && pending_confirm.is_some() => {
                            let lower = ch.to_ascii_lowercase();
                            if lower == "y" || lower == "n" {
                                e.stop_propagation();
                                e.prevent_default();
                                if let Some((cid, sid)) = pending_confirm {
                                    core_key.invoke_or_log(Command::AgentConfirm {
                                        conversation_id: cid,
                                        step_id: sid,
                                        allow: lower == "y",
                                    });
                                }
                                false
                            } else {
                                true
                            }
                        }
                        Key::Character(ch) if ctrl && !alt && !meta && ch.eq_ignore_ascii_case("c") && is_working => {
                            e.stop_propagation();
                            e.prevent_default();
                            core_key.invoke_or_log(Command::AgentStop {
                                conversation_id: stop_conv_id,
                            });
                            false
                        }
                        Key::Named(NamedKey::Escape) => {
                            e.stop_propagation();
                            e.prevent_default();
                            if let Some(mut ds) = dock_state {
                                if *ds.model_card_open.peek() {
                                    ds.model_card_open.set(false);
                                    return false;
                                }
                                if *ds.instructions_open.peek() {
                                    ds.instructions_open.set(false);
                                    return false;
                                }
                                if eff_layout == SidePanelLayout::Split {
                                    ds.set_focus(None);
                                    if let Some(ctx) = &page_ctx {
                                        ctx.request_focus_active_surface();
                                    }
                                    return false;
                                }
                            }
                            core_key.invoke_or_log(Command::PanelClose {
                                panel_id: SidePanelId::new(AGENT_PANEL_ID),
                            });
                            if let Some(ctx) = &page_ctx {
                                ctx.request_focus_active_surface();
                            }
                            false
                        }
                        _ => {
                            e.stop_propagation();
                            true
                        }
                    }
                }),
        );

        let has_sendable = !composer_text.read().trim().is_empty() || !staged_files.is_empty();
        let hint_text = if pending_confirm.is_some() {
            "y allow · n deny · Esc hide"
        } else if is_working {
            "Enter steer · Ctrl+C stop · Esc hide, it keeps working"
        } else {
            "Enter send · Shift+Enter new line · Tab switch agent"
        };

        let core_instr = core.clone();
        let model_btn_bg = if model_card_open { pill_bg } else { Color::TRANSPARENT };
        let instr_bg = if enabled_instruction_count > 0 || model_card_open {
            theme.rgba(ThemeRole::Accent).with_alpha(0.32).to_color()
        } else {
            theme.rgba(ThemeRole::Accent).with_alpha(0.24).to_color()
        };

        let composer_card = rect()
            .width(Size::fill())
            .padding(Gaps::new(4.0, 12.0, 10.0, 12.0))
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(10.0, 12.0, 9.0, 12.0))
                    .spacing(6.0)
                    .corner_radius(CornerRadius::new_all(18.0))
                    .background(composer_bg)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(border_col),
                    )
                    .child(
                        label()
                            .text(active_tab_title)
                            .font_size(11.0)
                            .color(theme.color(ThemeRole::TextMuted))
                            .max_lines(1),
                    )
                    .maybe_child(attachments_row)
                    .maybe_child(media_warning_banner)
                    .child(composer_input)
                    .child(
                        // Bottom controls row: paperclip left; model + sparkles + mic + send/stop grouped right.
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .main_align(Alignment::SpaceBetween)
                            .spacing(6.0)
                            .child(
                                KitTooltip::new("Attach file").child(
                                    rect()
                                        .a11y_role(AccessibilityRole::Button)
                                        .a11y_alt("Attach file")
                                        .width(Size::px(28.0))
                                        .height(Size::px(28.0))
                                        .corner_radius(CornerRadius::new_all(999.0))
                                        .center()
                                        .cursor(CursorIcon::Pointer)
                                        .on_press(move |e: Event<PressEventData>| {
                                            e.stop_propagation();
                                            if let Some(mut ds) = dock_state {
                                                let mut files = ds.staged_attachments.peek().clone();
                                                let next_idx = files.len() + 1;
                                                files.push(InstructionAttachment {
                                                    name: format!("capture-{next_idx}.png"),
                                                    path: format!("/tmp/capture-{next_idx}.png"),
                                                    size_bytes: 184_320,
                                                    kind: AttachmentKind::Image,
                                                });
                                                ds.staged_attachments.set(files);
                                            }
                                        })
                                        .child(paperclip_icon(15.0, theme.color(ThemeRole::TextMuted))),
                                ),
                            )
                            .child(
                                rect()
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(6.0)
                                    .child(
                                        // Model + effort trigger
                                        rect()
                                            .a11y_role(AccessibilityRole::Button)
                                            .a11y_alt("Select model and effort")
                                            .height(Size::px(28.0))
                                            .padding(Gaps::new(0.0, 8.0, 0.0, 5.0))
                                            .corner_radius(CornerRadius::new_all(999.0))
                                            .background(model_btn_bg)
                                            .direction(Direction::Horizontal)
                                            .cross_align(Alignment::center())
                                            .spacing(5.0)
                                            .cursor(CursorIcon::Pointer)
                                            .on_press(move |e: Event<PressEventData>| {
                                                e.stop_propagation();
                                                if let Some(mut ds) = dock_state {
                                                    let next = !*ds.model_card_open.peek();
                                                    ds.model_card_open.set(next);
                                                    if next {
                                                        ds.model_card_list_view.set(false);
                                                    }
                                                }
                                            })
                                            .child(agent_badge_emblem(&active_agent_id.0, 18.0, 12.0, 999.0, false))
                                            .child(
                                                label()
                                                    .text(short_model_name(&selected_model.name).to_string())
                                                    .font_size(11.5)
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .color(theme.color(ThemeRole::Text))
                                                    .max_lines(1),
                                            )
                                            .child(
                                                label()
                                                    .text(effective_effort.label())
                                                    .font_size(11.0)
                                                    .color(theme.color(ThemeRole::TextMuted))
                                                    .max_lines(1),
                                            ),
                                    )
                                    .child(
                                        // Instructions sparkles circle button
                                        rect()
                                            .a11y_role(AccessibilityRole::Button)
                                            .a11y_alt("Agent instructions")
                                            .width(Size::px(30.0))
                                            .height(Size::px(30.0))
                                            .corner_radius(CornerRadius::new_all(999.0))
                                            .background(instr_bg)
                                            .center()
                                            .cursor(CursorIcon::Pointer)
                                            .on_press(move |e: Event<PressEventData>| {
                                                e.stop_propagation();
                                                if let Some(mut ds) = dock_state {
                                                    ds.model_card_open.set(false);
                                                    ds.instructions_open.set(true);
                                                }
                                                core_instr.invoke_or_log(Command::AgentOpenInstructions);
                                            })
                                            .child(Icon::new("sparkles").size(13.5).role(ThemeRole::Text)),
                                    )
                                    .child(
                                        // Mic / Dictation button
                                        rect()
                                            .a11y_role(AccessibilityRole::Button)
                                            .a11y_alt("Voice dictation")
                                            .width(Size::px(28.0))
                                            .height(Size::px(28.0))
                                            .corner_radius(CornerRadius::new_all(999.0))
                                            .background(match dictation_state {
                                                DictationState::Idle => Color::TRANSPARENT,
                                                DictationState::Listening => {
                                                    theme.rgba(ThemeRole::Danger).with_alpha(0.24).to_color()
                                                }
                                                DictationState::Processing => {
                                                    theme.rgba(ThemeRole::Accent).with_alpha(0.24).to_color()
                                                }
                                            })
                                            .center()
                                            .cursor(CursorIcon::Pointer)
                                            .on_press(move |e: Event<PressEventData>| {
                                                e.stop_propagation();
                                                if let Some(mut ds) = dock_state {
                                                    let cur = *ds.dictation.peek();
                                                    match cur {
                                                        DictationState::Idle => {
                                                            ds.dictation.set(DictationState::Listening);
                                                        }
                                                        DictationState::Listening => {
                                                            ds.dictation.set(DictationState::Processing);
                                                            spawn(async move {
                                                                timer(Duration::from_millis(450)).await;
                                                                let mut ct = ds.composer_text;
                                                                let prev = ct.peek().clone();
                                                                let next = if prev.trim().is_empty() {
                                                                    "Summarize the key takeaways on this page"
                                                                        .to_string()
                                                                } else {
                                                                    format!("{prev} and highlight action items")
                                                                };
                                                                ct.set(next);
                                                                ds.dictation.set(DictationState::Idle);
                                                            });
                                                        }
                                                        DictationState::Processing => {
                                                            ds.dictation.set(DictationState::Idle);
                                                        }
                                                    }
                                                }
                                            })
                                            .child(
                                                Icon::new(if dictation_state == DictationState::Listening {
                                                    "microphone-fill"
                                                } else {
                                                    "microphone"
                                                })
                                                .size(13.5)
                                                .role(
                                                    match dictation_state {
                                                        DictationState::Idle => ThemeRole::TextMuted,
                                                        DictationState::Listening => ThemeRole::Danger,
                                                        DictationState::Processing => ThemeRole::Accent,
                                                    },
                                                ),
                                            ),
                                    )
                                    .child(if is_working && !has_sendable {
                                        // Stop button (accent-colored circle with dark/ink rounded square matching reference)
                                        rect()
                                            .a11y_role(AccessibilityRole::Button)
                                            .a11y_alt("Stop agent")
                                            .width(Size::px(30.0))
                                            .height(Size::px(30.0))
                                            .corner_radius(CornerRadius::new_all(999.0))
                                            .background(theme_accent)
                                            .center()
                                            .cursor(CursorIcon::Pointer)
                                            .on_press(move |e: Event<PressEventData>| {
                                                e.stop_propagation();
                                                core_stop.invoke_or_log(Command::AgentStop {
                                                    conversation_id: stop_conv_id,
                                                });
                                            })
                                            .child(
                                                rect()
                                                    .width(Size::px(9.5))
                                                    .height(Size::px(9.5))
                                                    .corner_radius(CornerRadius::new_all(2.0))
                                                    .background(theme.color(ThemeRole::TextOnAccent)),
                                            )
                                    } else {
                                        // Send button
                                        rect()
                                            .a11y_role(AccessibilityRole::Button)
                                            .a11y_alt("Send message")
                                            .width(Size::px(30.0))
                                            .height(Size::px(30.0))
                                            .corner_radius(CornerRadius::new_all(999.0))
                                            .background(if has_sendable { theme_accent } else { pill_bg })
                                            .center()
                                            .cursor(if has_sendable {
                                                CursorIcon::Pointer
                                            } else {
                                                CursorIcon::Default
                                            })
                                            .on_press(move |e: Event<PressEventData>| {
                                                e.stop_propagation();
                                                send_for_btn();
                                            })
                                            .child(chevron_up_icon(
                                                13.0,
                                                if has_sendable {
                                                    theme.color(ThemeRole::TextOnAccent)
                                                } else {
                                                    theme.color(ThemeRole::TextMuted)
                                                },
                                            ))
                                    }),
                            ),
                    )
                    .child(
                        label()
                            .text(hint_text)
                            .font_size(10.0)
                            .color(theme.color(ThemeRole::TextMuted))
                            .max_lines(1),
                    ),
            );

        let on_panel_press = move |_: Event<PointerEventData>| {
            if *agent_picker_open.peek() {
                agent_picker_open.set(false);
            }
            if let Some(mut ds) = dock_state
                && *ds.model_card_open.peek()
            {
                ds.model_card_open.set(false);
            }
        };

        let agent_picker_overlay = (*agent_picker_open.read()).then(|| {
            let picker_bg = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(248, 30, 34, 52),
                ResolvedScheme::Light => Color::from_argb(252, 249, 250, 255),
            };
            let selected_row_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.16).to_color();
            let mut rows = rect().width(Size::fill()).spacing(2.0);

            for ag in &available_agents {
                let is_cur = ag.id == active_agent_id;
                let aid = ag.id.clone();
                let core_sel = core.clone();
                rows = rows.child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt(format!("Switch to {}", ag.name))
                        .width(Size::fill())
                        .height(Size::px(32.0))
                        .padding(Gaps::new(0.0, 9.0, 0.0, 8.0))
                        .corner_radius(CornerRadius::new_all(8.0))
                        .background(if is_cur { selected_row_bg } else { Color::TRANSPARENT })
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::center())
                        .main_align(Alignment::SpaceBetween)
                        .spacing(8.0)
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            agent_picker_open.set(false);
                            core_sel.invoke_or_log(Command::AgentSelect {
                                agent_id: Some(aid.clone()),
                            });
                        })
                        .child(
                            rect()
                                .width(Size::flex(1.0))
                                .direction(Direction::Horizontal)
                                .cross_align(Alignment::center())
                                .spacing(8.0)
                                .child(agent_badge_emblem(&ag.id.0, 20.0, 12.5, 999.0, false))
                                .child(
                                    label()
                                        .text(ag.name.clone())
                                        .font_size(12.0)
                                        .font_weight(if is_cur {
                                            FontWeight::SEMI_BOLD
                                        } else {
                                            FontWeight::MEDIUM
                                        })
                                        .color(theme.color(ThemeRole::Text))
                                        .max_lines(1),
                                ),
                        )
                        .maybe_child(is_cur.then(|| Icon::new("check").size(12.0).role(ThemeRole::Accent))),
                );
            }

            rect()
                .layer(Layer::OverlayLevel(crate::ui::theme::POPUP_OVERLAY_LEVEL))
                .position(Position::new_absolute().top(44.0).left(10.0))
                .width(Size::px(208.0))
                .padding(Gaps::new_all(6.0))
                .corner_radius(CornerRadius::new_all(12.0))
                .overflow(Overflow::Clip)
                .background(picker_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(border_col),
                )
                .shadow(theme.shadows.card.to_shadow())
                .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                .child(rows)
        });

        let model_card_overlay = model_card_open.then(|| {
            AgentModelCard::new(
                active_agent_id.clone(),
                selected_model.clone(),
                effective_effort,
                active_context_id.clone(),
                models.to_vec(),
                theme_accent,
            )
        });

        if instructions_open {
            return rect()
                .width(Size::fill())
                .height(Size::fill())
                .child(AgentInstructionsPage::new(instructions, theme_accent));
        }

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .on_pointer_down(on_panel_press)
            .child(header)
            .child(rect().width(Size::fill()).height(Size::px(1.0)).background(border_col))
            .maybe_child(service_banner)
            .child(body)
            .child(composer_card)
            .maybe_child(agent_picker_overlay)
            .maybe_child(model_card_overlay)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Renders the full ordered trace of a conversation, folding consecutive tool/thinking/note steps into collapsible
/// step groups while showing accent-tinted user bubbles, Markdown answers, interactive tables, declared UI pages,
/// and confirmation cards.
fn render_conversation_steps(
    conv: &Conversation,
    agent_name: &str,
    agent_accent: Color,
    theme: &crate::ui::theme::ResolvedTheme,
    mut expanded_groups: State<HashSet<u64>>,
    mut copied_step: State<Option<u64>>,
    mut follow: ConversationFollow,
) -> Vec<Element> {
    let mut elements = Vec::new();
    let steps = &conv.steps;
    let latest_prompt = steps
        .iter()
        .rev()
        .find(|step| step.kind == StepKind::UserPrompt)
        .map(|step| step.id);
    let mut i = 0;

    let bubble_bg = theme
        .rgba(ThemeRole::Accent)
        .with_alpha(match theme.scheme {
            ResolvedScheme::Dark => 0.14,
            ResolvedScheme::Light => 0.16,
        })
        .to_color();
    let bubble_border = theme.rgba(ThemeRole::Accent).with_alpha(0.28).to_color();
    let border_col = theme.color(ThemeRole::BorderSubtle);

    while i < steps.len() {
        let step = &steps[i];
        match &step.kind {
            StepKind::ToolCall { .. } | StepKind::Thinking | StepKind::Note => {
                let group_start = i;
                let group_id = step.id.0;
                while i < steps.len()
                    && matches!(
                        steps[i].kind,
                        StepKind::ToolCall { .. } | StepKind::Thinking | StepKind::Note
                    )
                {
                    i += 1;
                }
                let group = &steps[group_start..i];
                let has_running = group.iter().any(|s| s.status == StepStatus::Running);
                let is_open = has_running || expanded_groups.read().contains(&group_id);
                let count = group.len();
                let header_text = if has_running {
                    group
                        .iter()
                        .rev()
                        .find(|s| s.status == StepStatus::Running)
                        .map(|s| s.summary.clone())
                        .unwrap_or_else(|| format!("{count} steps"))
                } else if count == 1 {
                    group[0].summary.clone()
                } else {
                    format!("{count} steps")
                };

                let mut group_box = rect().width(Size::fill()).spacing(4.0).child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("Toggle agent steps")
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(5.0)
                        .padding(Gaps::new(2.0, 4.0, 2.0, 0.0))
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            let mut set = expanded_groups.peek().clone();
                            if !set.remove(&group_id) {
                                set.insert(group_id);
                            }
                            expanded_groups.set(set);
                        })
                        .child(
                            label()
                                .text(header_text)
                                .font_size(11.5)
                                .font_weight(FontWeight::MEDIUM)
                                .color(theme.color(ThemeRole::TextMuted))
                                .max_lines(1),
                        )
                        .child(if is_open {
                            chevron_down_icon(10.0, theme.color(ThemeRole::TextMuted))
                        } else {
                            chevron_right_icon(10.0, theme.color(ThemeRole::TextMuted))
                        }),
                );

                if is_open {
                    let mut list = rect().width(Size::fill()).spacing(5.0);
                    for s in group {
                        let status_el: Element = match s.status {
                            StepStatus::Running => WorkingRing::new(12.0).color(agent_accent).into_element(),
                            StepStatus::Completed => {
                                Icon::new("check").size(11.0).role(ThemeRole::Success).into_element()
                            }
                            StepStatus::Failed => Icon::new("close").size(11.0).role(ThemeRole::Danger).into_element(),
                            StepStatus::Pending | StepStatus::WaitingForUser => {
                                Icon::new("info").size(11.0).role(ThemeRole::TextMuted).into_element()
                            }
                        };
                        list = list.child(
                            rect()
                                .width(Size::fill())
                                .direction(Direction::Horizontal)
                                .content(Content::Flex)
                                .cross_align(Alignment::center())
                                .spacing(7.0)
                                .child(status_el)
                                .child(
                                    rect().width(Size::flex(1.0)).child(
                                        label()
                                            .text(s.summary.clone())
                                            .font_size(11.5)
                                            .color(theme.color(ThemeRole::TextMuted))
                                            .max_lines(2),
                                    ),
                                ),
                        );
                    }
                    group_box = group_box.child(
                        rect()
                            .width(Size::fill())
                            .overflow(Overflow::Clip)
                            .padding(Gaps::new(2.0, 0.0, 2.0, 16.0))
                            .child(
                                rect()
                                    .position(Position::new_absolute().left(5.0).top(2.0))
                                    .width(Size::px(1.5))
                                    .height(Size::percent(100.0))
                                    .background(border_col),
                            )
                            .child(list),
                    );
                }
                elements.push(group_box.into_element());
            }
            StepKind::UserPrompt => {
                let steer_label = match step.steer {
                    Some(SteerStatus::Steering) => Some("Steering…"),
                    Some(SteerStatus::Queued) => Some("Queued"),
                    Some(SteerStatus::Delivered) | None => None,
                };
                let files_el = (!step.files.is_empty()).then(|| {
                    let mut row = rect()
                        .direction(Direction::Horizontal)
                        .spacing(5.0)
                        .padding(Gaps::new(0.0, 0.0, 4.0, 0.0));
                    for f in &step.files {
                        row = row.child(
                            rect()
                                .direction(Direction::Horizontal)
                                .cross_align(Alignment::center())
                                .spacing(4.0)
                                .padding(Gaps::new(2.0, 7.0, 2.0, 6.0))
                                .corner_radius(CornerRadius::new_all(6.0))
                                .background(theme.rgba(ThemeRole::SurfaceRaised).with_alpha(0.55).to_color())
                                .child(Icon::new("page-portrait").size(10.0).role(ThemeRole::Accent))
                                .child(
                                    label()
                                        .text(f.clone())
                                        .font_size(10.5)
                                        .color(theme.color(ThemeRole::Text)),
                                ),
                        );
                    }
                    row
                });

                let is_latest_prompt = latest_prompt == Some(step.id);
                elements.push(
                    rect()
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .main_align(Alignment::end())
                        .opacity(if step.steer.is_some() { 0.72 } else { 1.0 })
                        .on_sized(move |e: Event<SizedEventData>| {
                            if is_latest_prompt {
                                follow.prompt_sized(e.area.min_y());
                            }
                        })
                        .child(
                            rect()
                                .max_width(Size::percent(86.0))
                                .spacing(3.0)
                                .cross_align(Alignment::end())
                                .child(
                                    rect()
                                        .padding(Gaps::new(8.0, 12.0, 8.0, 12.0))
                                        .corner_radius((12.0, 12.0, 4.0, 12.0))
                                        .background(bubble_bg)
                                        .border(
                                            Border::new()
                                                .width(1.0)
                                                .alignment(BorderAlignment::Inner)
                                                .fill(bubble_border),
                                        )
                                        .spacing(4.0)
                                        .maybe_child(files_el)
                                        .child(
                                            label()
                                                .text(step.summary.clone())
                                                .font_size(12.5)
                                                .line_height(1.42)
                                                .color(theme.color(ThemeRole::Text)),
                                        ),
                                )
                                .maybe_child(
                                    steer_label.map(|s| {
                                        label().text(s).font_size(10.0).color(theme.color(ThemeRole::TextMuted))
                                    }),
                                ),
                        )
                        .into_element(),
                );
                i += 1;
            }
            StepKind::Response => {
                let sid = step.id.0;
                let is_copied = *copied_step.read() == Some(sid);
                let text_copy = step.summary.clone();
                elements.push(
                    rect()
                        .width(Size::fill())
                        .spacing(6.0)
                        .child(MarkdownView::new(step.summary.clone()).streaming(step.streaming))
                        .child(
                            rect()
                                .width(Size::fill())
                                .direction(Direction::Horizontal)
                                .main_align(Alignment::start())
                                .child(
                                    rect()
                                        .a11y_role(AccessibilityRole::Button)
                                        .a11y_alt("Copy answer")
                                        .direction(Direction::Horizontal)
                                        .cross_align(Alignment::center())
                                        .spacing(4.0)
                                        .padding(Gaps::new(2.0, 6.0, 2.0, 0.0))
                                        .corner_radius(CornerRadius::new_all(6.0))
                                        .cursor(CursorIcon::Pointer)
                                        .on_press(move |e: Event<PressEventData>| {
                                            e.stop_propagation();
                                            copy_to_clipboard(&text_copy);
                                            copied_step.set(Some(sid));
                                            spawn(async move {
                                                timer(Duration::from_millis(1400)).await;
                                                if *copied_step.peek() == Some(sid) {
                                                    copied_step.set(None);
                                                }
                                            });
                                        })
                                        .child(Icon::new(if is_copied { "check" } else { "copy" }).size(11.0).role(
                                            if is_copied {
                                                ThemeRole::Success
                                            } else {
                                                ThemeRole::TextMuted
                                            },
                                        ))
                                        .child(
                                            label()
                                                .text(if is_copied { "Copied" } else { "Copy" })
                                                .font_size(10.5)
                                                .color(if is_copied {
                                                    theme.color(ThemeRole::Success)
                                                } else {
                                                    theme.color(ThemeRole::TextMuted)
                                                }),
                                        ),
                                ),
                        )
                        .into_element(),
                );
                i += 1;
            }
            StepKind::Confirmation { prompt, resolved } => {
                elements.push(
                    AgentConfirmCard::new(conv.id, step.id, agent_name, prompt.clone(), *resolved)
                        .key(step.id.0)
                        .into_element(),
                );
                i += 1;
            }
            StepKind::AgentTable(table) => {
                elements.push(AgentTableCard::new(table.clone()).key(step.id.0).into_element());
                i += 1;
            }
            StepKind::DeclaredPage(page) => {
                elements.push(AgentDeclaredPageCard::new(page.clone()).key(step.id.0).into_element());
                i += 1;
            }
            StepKind::Error => {
                elements.push(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(9.0, 11.0, 9.0, 11.0))
                        .corner_radius(CornerRadius::new_all(10.0))
                        .background(theme.rgba(ThemeRole::Danger).with_alpha(0.14).to_color())
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(theme.rgba(ThemeRole::Danger).with_alpha(0.4).to_color()),
                        )
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::center())
                        .spacing(8.0)
                        .child(Icon::new("warning").size(14.0).role(ThemeRole::Danger))
                        .child(
                            rect().width(Size::flex(1.0)).child(
                                label()
                                    .text(step.summary.clone())
                                    .font_size(12.0)
                                    .line_height(1.4)
                                    .color(theme.color(ThemeRole::Text)),
                            ),
                        )
                        .into_element(),
                );
                i += 1;
            }
        }
    }

    elements
}
