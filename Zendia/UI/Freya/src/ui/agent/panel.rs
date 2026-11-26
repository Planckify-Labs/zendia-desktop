/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Full AI agent side panel (`AgentPanel`): header with agent chip, same-tab other-chats stack, ordered action buttons,
//! service-down callout, empty & no-agent install states, conversation log with animated collapsible step groups,
//! Markdown answers, interactive tables & declared UI cards, confirmation cards, follow-scroll jump pill, auto-growing
//! composer with `72×72` media tiles & media compatibility warning/nudge, 4-state dictation UI (`<A-Space>` hold),
//! floating model card, and animated slide-in instructions overlay.

use crate::ui::agent::confirm_card::AgentConfirmCard;
use crate::ui::agent::follow::{ConversationFollow, JumpToLatest, use_conversation_follow};
use crate::ui::agent::instructions::{AgentInstructionsPage, estimate_autogrow_height};
use crate::ui::agent::markdown::{MarkdownView, copy_to_clipboard};
use crate::ui::agent::model_card::{AgentModelCard, short_model_name};
use crate::ui::agent::motion::{RollingText, WorkingRing, WorkingRow};
use crate::ui::agent::ui_view::{AgentDeclaredPageCard, AgentTableCard};
use crate::ui::agent::{
    agent_badge_emblem, agent_badge_emblem_with_glow, agent_input_colors, agent_logo_element, chevron_down_icon,
    chevron_right_icon, colored_zen_icon, dashed_rounded_box_fill, paperclip_icon,
};
use crate::ui::core_view::use_core;
use crate::ui::kit::{
    Icon, IconButton, KitTooltip, ListNavAction, ListNavConfig, PopupPresence, TextField, classify_list_key,
    focus_ring_border, mix_color, use_interactive_feedback,
};
use crate::ui::panels::{
    DictationState, DockLayoutControls, ResolvedDockPanel, active_agent_accent, try_use_dock_state,
};
use crate::ui::theme::{ResolvedScheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::engine::prelude::{Paint, PaintStyle, SkColor};
use freya::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use zendia_browser_core::agent::{
    AGENT_PANEL_ID, AttachmentKind, InstructionAttachment, MediaCompatibilityIssue, SteerStatus,
    check_media_compatibility, format_duration_secs, format_file_size, seed_of, working_word,
};
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{SidePanelId, SidePanelLayout};
use zendia_browser_core::model::{
    Agent, AgentId, Conversation, ConversationId, ConversationStatus, ConversationStep, ModelEffort, ModelId,
    ModelInfo, ServiceId, ServiceStatus, StepId, StepKind, StepStatus,
};

const WEBCHAT_PANEL_ID: &str = "zendia.webchat";

/// Visual 4-state dictation phase matching `ZendiaAgentDock.sys.mjs` (`idle`, `connecting`, `listening`, `finishing`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgentDictationPhase {
    Idle,
    Connecting,
    Listening,
    Finishing,
}

fn agent_install_command(agent_id: &str) -> &'static str {
    match agent_id {
        "claude-code" | "claude" => "npm i -g @anthropic-ai/claude-code",
        "codex" => "npm i -g @openai/codex",
        "cursor" => "curl https://cursor.com/install -fsS | bash",
        "antigravity" => "npm i -g @google/antigravity",
        "opencode" => "npm i -g opencode-ai",
        "kiro" => "curl -fsSL https://cli.kiro.dev/install | bash",
        _ => "npm i -g @anthropic-ai/claude-code",
    }
}

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
        let composer_focus = use_focus(composer_a11y_id);
        let fallback_composer = use_state(String::new);
        let composer_text = dock_state.map(|ds| ds.composer_text).unwrap_or(fallback_composer);
        let mut agent_picker_open = use_state(|| false);
        let mut others_picker_open = use_state(|| false);
        let mut composer_width = use_state(|| 360.0f32);
        let mut drafts = use_state(HashMap::<Option<ConversationId>, String>::new);
        let mut last_draft_conv = use_state(|| None::<Option<ConversationId>>);
        let mut media_ack = use_state(|| None::<String>);
        let mut media_nudge_seq = use_state(|| 0u64);
        let mut notice_text = use_state(|| None::<String>);
        let mut dictation_connecting = use_state(|| false);
        let mut dictation_seq = use_state(|| 0u64);
        let mut send_after_dictation = use_state(|| false);
        let mut hold_active = use_state(|| false);
        let drag_over = use_state(|| false);
        let mut follow = use_conversation_follow(scroll_controller);
        let mut last_shown = use_state(|| (None::<ConversationId>, None::<StepId>, None::<StepId>));

        let (
            active_tab_id,
            active_tab_title,
            active_agent_id,
            active_agent_name,
            all_agents,
            available_agents,
            active_conv,
            same_tab_convs,
            other_convs,
            models,
            selected_model,
            explicit_model_id,
            explicit_effort,
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
            let all_agents = model.agents.clone();
            let available_agents: Vec<_> = model.agents.iter().filter(|a| a.available).cloned().collect();
            // AP-03: Scope other chats strictly to the active tab (`chatsOn(browser)`).
            let same_tab_convs: Vec<_> = model
                .conversations
                .iter()
                .filter(|c| active_tab_id.is_some() && c.tab_id == active_tab_id)
                .cloned()
                .collect();
            let other_convs: Vec<_> = same_tab_convs
                .iter()
                .filter(|c| conv.as_ref().is_none_or(|cur| cur.id != c.id))
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
            let explicit_model_id = model.agent_selections.get(&agent_id).and_then(|s| s.model_id.clone());
            let explicit_effort = model.agent_selections.get(&agent_id).and_then(|s| s.effort);
            let effective_effort = explicit_effort.unwrap_or(selected_model.effort);
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
                all_agents,
                available_agents,
                conv,
                same_tab_convs,
                other_convs,
                models,
                selected_model,
                explicit_model_id,
                explicit_effort,
                effective_effort,
                active_context_id,
                instructions,
                enabled_instruction_count,
                captcha_finding,
                agent_server_status,
                agent_accent,
            )
        };

        // AP-14: Save and restore per-conversation composer drafts when switching tabs or chats.
        let current_conv_key = active_conv.as_ref().map(|c| c.id);
        if *last_draft_conv.peek() != Some(current_conv_key) {
            if let Some(prev_key) = *last_draft_conv.peek() {
                let mut map = drafts.peek().clone();
                let current_text = composer_text.peek().clone();
                if current_text.is_empty() {
                    map.remove(&prev_key);
                } else {
                    map.insert(prev_key, current_text);
                }
                let restored = map.get(&current_conv_key).cloned().unwrap_or_default();
                drafts.set(map);
                let mut ct = composer_text;
                if *ct.peek() != restored {
                    ct.set(restored);
                }
            }
            last_draft_conv.set(Some(current_conv_key));
        }

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

        // Another conversation (or replaced fixture) snaps immediately to its end (`AP-15`); a newly sent request in
        // the same conversation glides to the top.
        let shown = (
            active_conv.as_ref().map(|conv| conv.id),
            active_conv.as_ref().and_then(|conv| conv.steps.first().map(|s| s.id)),
            active_conv.as_ref().and_then(|conv| {
                conv.steps
                    .iter()
                    .rev()
                    .find(|step| step.kind == StepKind::UserPrompt)
                    .map(|step| step.id)
            }),
        );
        use_side_effect_with_deps(
            &shown,
            move |shown: &(Option<ConversationId>, Option<StepId>, Option<StepId>)| {
                let previous = *last_shown.peek();
                if previous.0 != shown.0 || previous.1 != shown.1 {
                    follow.reset();
                } else if previous.2 != shown.2 && previous.2.is_some() && shown.2.is_some() {
                    follow.request_glide();
                }
                last_shown.set(*shown);
            },
        );

        let model_card_open = dock_state.is_some_and(|ds| *ds.model_card_open.read());
        let instructions_open = dock_state.is_some_and(|ds| *ds.instructions_open.read());
        let static_snapshot = dock_state.is_some_and(|ds| ds.pinned_ring_phase.read().is_some());

        // AP-10: Auto-focus composer when panel gains focus and no sub-overlay owns focus.
        let focus_composer_dep = (
            self.resolved.focused,
            current_conv_key,
            model_card_open,
            instructions_open,
        );
        use_side_effect_with_deps(&focus_composer_dep, move |(focused, _, mc, instr)| {
            if *focused && !*mc && !*instr && !static_snapshot {
                composer_a11y_id.request_focus();
            }
        });

        let theme_accent = theme.color(ThemeRole::Accent);
        let border_col = theme.color(ThemeRole::BorderSubtle);

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

        // Compute 4-state dictation phase (`DC-01`).
        let dock_dictation = dock_state.map(|ds| *ds.dictation.read()).unwrap_or_default();
        let dictation_phase = if *dictation_connecting.read() {
            AgentDictationPhase::Connecting
        } else {
            match dock_dictation {
                DictationState::Idle => AgentDictationPhase::Idle,
                DictationState::Listening => AgentDictationPhase::Listening,
                DictationState::Processing => AgentDictationPhase::Finishing,
            }
        };
        let dictation_active = dictation_phase != AgentDictationPhase::Idle;

        let staged_files = dock_state
            .map(|ds| ds.staged_attachments.read().clone())
            .unwrap_or_default();
        let staged_kinds: Vec<AttachmentKind> = staged_files.iter().map(|f| f.kind).collect();
        let media_issue = check_media_compatibility(&selected_model, &models, &staged_kinds);
        let media_issue_key = media_issue.as_ref().map(|issue| {
            let kinds_str: Vec<&str> = issue
                .unsupported_kinds
                .iter()
                .map(|k| match k {
                    AttachmentKind::Image => "image",
                    AttachmentKind::Video => "video",
                    AttachmentKind::Pdf => "pdf",
                    AttachmentKind::Document => "doc",
                })
                .collect();
            format!("{}|{}", selected_model.id.0, kinds_str.join(","))
        });

        // Clear stale media ack whenever issue disappears or changes.
        if *media_ack.peek() != media_issue_key && media_issue_key.is_none() {
            media_ack.set(None);
        }

        // --- Header (`AP-01`, `AP-02`, `AP-03`) ---
        let core_all_agents = core.clone();
        let core_new = core.clone();
        let core_captcha = core.clone();
        let core_webchat = core.clone();
        let core_close = core.clone();

        let others_trigger = (!other_convs.is_empty()).then(|| {
            let core_sw = core.clone();
            let others_clone = other_convs.clone();
            let agent_names_map: HashMap<String, String> =
                all_agents.iter().map(|a| (a.id.0.clone(), a.name.clone())).collect();
            OtherChatsButton {
                others: others_clone.clone(),
                agent_names: agent_names_map,
                on_press: EventHandler::new(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    if others_clone.len() == 1 {
                        others_picker_open.set_if_modified(false);
                        core_sw.invoke_or_log(Command::AgentSwitchConversation {
                            conversation_id: others_clone[0].id,
                        });
                    } else {
                        let next = !*others_picker_open.peek();
                        others_picker_open.set(next);
                        agent_picker_open.set_if_modified(false);
                        if let Some(mut ds) = dock_state {
                            ds.model_card_open.set_if_modified(false);
                        }
                    }
                }),
                key: DiffKey::None,
            }
        });

        // AP-01 exact fork header order: Agents, New chat, CAPTCHA, Web chat, Layouts, Close.
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
                    .child(AgentChipButton {
                        agent_id: active_agent_id.0.clone(),
                        agent_name: active_agent_name.clone(),
                        agent_accent,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            let next = !*agent_picker_open.peek();
                            agent_picker_open.set(next);
                            others_picker_open.set_if_modified(false);
                            if let Some(mut ds) = dock_state
                                && *ds.model_card_open.peek()
                            {
                                ds.model_card_open.set(false);
                            }
                        }),
                        key: DiffKey::None,
                    })
                    .maybe_child(others_trigger),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(2.0)
                    .child(
                        IconButton::new("layers", "All agents")
                            .tooltip("All agents")
                            .size(24.0)
                            .icon_size(14.0)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_all_agents.invoke_or_log(Command::AgentList);
                            }),
                    )
                    .child(
                        IconButton::new("plus", "New chat")
                            .tooltip("New chat")
                            .size(24.0)
                            .icon_size(14.0)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_new.invoke_or_log(Command::AgentNew { tab_id: active_tab_id });
                            }),
                    )
                    .maybe_child(captcha_finding.as_ref().map(|_| {
                        let tab_for_cap = active_tab_id;
                        IconButton::new("captcha", "Summarize CAPTCHA on this page")
                            .tooltip("Summarize CAPTCHA on this page")
                            .size(24.0)
                            .icon_size(14.0)
                            .role(ThemeRole::Warning)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_captcha.invoke_or_log(Command::AgentSummarizeCaptcha { tab_id: tab_for_cap });
                            })
                    }))
                    .child(
                        IconButton::new("webchat", "Web chat")
                            .tooltip("Web chat")
                            .size(24.0)
                            .icon_size(14.0)
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
                        IconButton::new("close", "Close")
                            .tooltip("Close")
                            .shortcut("Esc")
                            .size(24.0)
                            .icon_size(14.0)
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
                .child(SmallActionButton {
                    label: "Retry".to_string(),
                    tooltip: Some("Restart AgentServer".to_string()),
                    primary: false,
                    on_press: EventHandler::new(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        core_restart.invoke_or_log(Command::DebugToggleAgentService);
                    }),
                    key: DiffKey::None,
                })
        });

        // --- Main Body (No agents `AP-04` / Empty `AP-05` / Conversation log `AP-06..08`, `AP-15`) ---
        let body: Element = if available_agents.is_empty() {
            let core_rescan = core.clone();
            let install_agents: Vec<(String, String, &'static str)> = if all_agents.is_empty() {
                vec![
                    (
                        "claude-code".into(),
                        "Claude Code".into(),
                        agent_install_command("claude-code"),
                    ),
                    ("codex".into(), "Codex".into(), agent_install_command("codex")),
                    ("cursor".into(), "Cursor".into(), agent_install_command("cursor")),
                    ("opencode".into(), "OpenCode".into(), agent_install_command("opencode")),
                    ("kiro".into(), "Kiro".into(), agent_install_command("kiro")),
                ]
            } else {
                all_agents
                    .iter()
                    .map(|a| (a.id.0.clone(), a.name.clone(), agent_install_command(&a.id.0)))
                    .collect()
            };

            let mut install_list = rect().width(Size::fill()).spacing(6.0);
            for (aid, name, cmd) in install_agents {
                install_list = install_list.child(
                    rect()
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::center())
                        .spacing(12.0)
                        .child(
                            rect()
                                .width(Size::px(118.0))
                                .direction(Direction::Horizontal)
                                .cross_align(Alignment::center())
                                .spacing(8.0)
                                .child(agent_badge_emblem(&aid, 18.0, 11.5, 999.0, false))
                                .child(
                                    label()
                                        .text(name)
                                        .font_size(12.0)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .color(theme.color(ThemeRole::Text))
                                        .max_lines(1),
                                ),
                        )
                        .child(
                            rect().width(Size::flex(1.0)).opacity(0.70).child(
                                label()
                                    .text(cmd)
                                    .font_size(11.5)
                                    .font_family("monospace")
                                    .color(theme.color(ThemeRole::Text))
                                    .max_lines(1),
                            ),
                        ),
                );
            }

            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .padding(Gaps::new(20.0, 18.0, 20.0, 18.0))
                .spacing(14.0)
                .child(
                    label()
                        .width(Size::fill())
                        .text("No coding agent found on this computer. zendia works with the one you already use: install one, then look again.")
                        .font_size(13.0)
                        .line_height(1.5)
                        .color(theme.color(ThemeRole::Text)),
                )
                .child(install_list)
                .child(
                    rect().direction(Direction::Horizontal).child(SmallActionButton {
                        label: "Look again".to_string(),
                        tooltip: Some("Look again for installed agents".to_string()),
                        primary: true,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_rescan.invoke_or_log(Command::AgentNew { tab_id: active_tab_id });
                        }),
                        key: DiffKey::None,
                    }),
                )
                .into_element()
        } else if active_conv.as_ref().is_none_or(|c| c.steps.is_empty()) {
            // AP-05: 44x44 circle with agent-accent drop shadow (35%), 15px SemiBold accent title, 13px 1.45 detail.
            let glow = Color::from_argb(89, agent_accent.r(), agent_accent.g(), agent_accent.b());
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .padding(Gaps::new(24.0, 24.0, 24.0, 24.0))
                .center()
                .spacing(6.0)
                .child(
                    rect()
                        .margin(Gaps::new(0.0, 0.0, 6.0, 0.0))
                        .child(agent_badge_emblem_with_glow(&active_agent_id.0, 44.0, 28.0, 999.0, Some(glow))),
                )
                .child(
                    label()
                        .width(Size::fill())
                        .text(format!("Ask {active_agent_name} about this tab"))
                        .font_size(15.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .text_align(TextAlign::Center)
                        .color(theme_accent),
                )
                .child(
                    rect().width(Size::fill()).max_width(Size::px(300.0)).child(
                        label()
                            .width(Size::fill())
                            .text("It reads the page, clicks, types and moves between pages for you, and asks before anything you have to allow.")
                            .font_size(13.0)
                            .line_height(1.45)
                            .text_align(TextAlign::Center)
                            .color(theme.color(ThemeRole::TextMuted)),
                    ),
                )
                .into_element()
        } else {
            let conv = active_conv.as_ref().unwrap();
            let trace_items =
                render_conversation_steps(conv, &active_agent_name, agent_accent, &theme, follow, static_snapshot);
            let room = *follow.room.read();

            let scroll_view = ScrollView::new_controlled(scroll_controller)
                .width(Size::fill())
                .height(Size::fill())
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(14.0, 16.0, 12.0, 16.0))
                        .spacing(10.0)
                        .on_wheel(move |e: Event<WheelEventData>| {
                            if e.delta_y > 0.0 {
                                follow.release();
                            }
                        })
                        .on_sized(move |e: Event<SizedEventData>| {
                            follow.content_sized(e.area.min_y(), e.area.height());
                        })
                        .children(trace_items)
                        .maybe_child((conv.status == ConversationStatus::Working).then(|| {
                            let seed = seed_of(&conv.id.0.to_string());
                            let word = if conv.elapsed_secs == 0 {
                                "Starting…".to_string()
                            } else {
                                working_word(seed, conv.elapsed_secs)
                            };
                            WorkingRow::new(word, conv.elapsed_secs, agent_accent)
                        }))
                        .child(rect().width(Size::fill()).height(Size::px(room.max(32.0)))),
                );

            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .child(scroll_view)
                .child(JumpToLatest::new(follow, conv.status == ConversationStatus::Working))
                .into_element()
        };

        // --- Staged attachments (`AP-11`: 72x72 rounded media tiles + pill chips for docs) ---
        let attachments_row = (!staged_files.is_empty()).then(|| {
            let mut row = rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .spacing(6.0)
                .padding(Gaps::new(0.0, 0.0, 4.0, 0.0));
            for (idx, file) in staged_files.iter().enumerate() {
                if matches!(file.kind, AttachmentKind::Image | AttachmentKind::Video) {
                    row = row.child(StagedMediaTile {
                        file: file.clone(),
                        on_remove: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            if let Some(mut ds) = dock_state {
                                let mut files = ds.staged_attachments.peek().clone();
                                if idx < files.len() {
                                    files.remove(idx);
                                    ds.staged_attachments.set(files);
                                }
                            }
                        }),
                        key: DiffKey::from(&format!("tile-{idx}-{}", file.name)),
                    });
                } else {
                    row = row.child(StagedFileChip {
                        file: file.clone(),
                        on_remove: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            if let Some(mut ds) = dock_state {
                                let mut files = ds.staged_attachments.peek().clone();
                                if idx < files.len() {
                                    files.remove(idx);
                                    ds.staged_attachments.set(files);
                                }
                            }
                        }),
                        key: DiffKey::from(&format!("chip-{idx}-{}", file.name)),
                    });
                }
            }
            row
        });

        // Helper closure to execute a send (with 2-step media warning ack & nudge).
        let dispatch_send = {
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
                let mut map = drafts.peek().clone();
                map.remove(&conv_id_opt);
                drafts.set(map);
                media_ack.set_if_modified(None);
                notice_text.set_if_modified(None);
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

        // Helper closure to finish dictation and insert transcript (auto-sending if requested).
        let finish_dictation = {
            let send_after = dispatch_send.clone();
            move || {
                let Some(mut ds) = dock_state else { return };
                dictation_connecting.set_if_modified(false);
                ds.dictation.set(DictationState::Processing);
                let seq = dictation_seq.peek().wrapping_add(1);
                dictation_seq.set(seq);
                let mut send_now = send_after.clone();
                spawn(async move {
                    timer(Duration::from_millis(450)).await;
                    if *dictation_seq.peek() != seq {
                        return;
                    }
                    let mut ct = ds.composer_text;
                    let prev = ct.peek().clone();
                    let next = if prev.trim().is_empty() {
                        "Summarize the key takeaways on this page".to_string()
                    } else {
                        format!("{prev} and highlight action items")
                    };
                    ct.set(next);
                    ds.dictation.set(DictationState::Idle);
                    if *send_after_dictation.peek() {
                        send_after_dictation.set(false);
                        send_now();
                    }
                });
            }
        };

        let mut start_dictation = move || {
            let Some(mut ds) = dock_state else { return };
            notice_text.set_if_modified(None);
            send_after_dictation.set_if_modified(false);
            dictation_connecting.set(true);
            let seq = dictation_seq.peek().wrapping_add(1);
            dictation_seq.set(seq);
            spawn(async move {
                timer(Duration::from_millis(220)).await;
                if *dictation_seq.peek() == seq && *dictation_connecting.peek() {
                    dictation_connecting.set(false);
                    ds.dictation.set(DictationState::Listening);
                }
            });
        };

        let mut cancel_dictation = move || {
            let next_seq = dictation_seq.peek().wrapping_add(1);
            dictation_seq.set(next_seq);
            dictation_connecting.set_if_modified(false);
            send_after_dictation.set_if_modified(false);
            if let Some(mut ds) = dock_state {
                ds.dictation.set_if_modified(DictationState::Idle);
            }
        };

        let send_action = {
            let mut do_send = dispatch_send.clone();
            let mut finish_dict = finish_dictation.clone();
            let issue_key = media_issue_key.clone();
            let issue_model = media_issue.as_ref().map(|i| i.model_name.clone());
            move || {
                // DC-01: Enter while dictating finishes dictation and auto-sends.
                if *dictation_connecting.peek() {
                    cancel_dictation();
                } else if dock_state.is_some_and(|ds| *ds.dictation.peek() != DictationState::Idle) {
                    send_after_dictation.set(true);
                    if dock_state.is_some_and(|ds| *ds.dictation.peek() == DictationState::Listening) {
                        finish_dict();
                    }
                    return;
                }
                let text = composer_text.peek().trim().to_string();
                let files = dock_state
                    .map(|ds| ds.staged_attachments.peek().clone())
                    .unwrap_or_default();
                if text.is_empty() && files.is_empty() {
                    return;
                }
                // AP-11: First Enter with incompatible media shows warning & nudges; second sends.
                if let Some(ref key) = issue_key
                    && media_ack.peek().as_deref() != Some(key.as_str())
                {
                    media_ack.set(Some(key.clone()));
                    let next_nudge = media_nudge_seq.peek().wrapping_add(1);
                    media_nudge_seq.set(next_nudge);
                    let m = issue_model.as_deref().unwrap_or("This model");
                    notice_text.set(Some(format!(
                        "{m} can't read these \u{00b7} Enter again to send anyway"
                    )));
                    return;
                }
                do_send();
            }
        };

        // --- Media compatibility warning banner (`AP-11`) ---
        let media_warning_banner = media_issue.as_ref().map(|issue| {
            let core_switch = core.clone();
            let aid_sw = active_agent_id.clone();
            let mut do_send_anyway = dispatch_send.clone();
            let issue_key_for_anyway = media_issue_key.clone();
            let nudged = media_ack.read().as_deref() == media_issue_key.as_deref() && media_issue_key.is_some();
            MediaWarningBanner {
                issue: issue.clone(),
                nudged,
                nudge_seq: *media_nudge_seq.read(),
                on_use_best: EventHandler::new(move |target_id: ModelId| {
                    media_ack.set_if_modified(None);
                    notice_text.set_if_modified(None);
                    core_switch.invoke_or_log(Command::AgentModel {
                        agent_id: Some(aid_sw.clone()),
                        model_id: Some(target_id),
                    });
                    composer_a11y_id.request_focus();
                }),
                on_pick: EventHandler::new(move |_| {
                    if let Some(mut ds) = dock_state {
                        ds.model_card_open.set(true);
                        ds.model_card_list_view.set(true);
                    }
                }),
                on_send_anyway: EventHandler::new(move |_| {
                    media_ack.set(issue_key_for_anyway.clone());
                    do_send_anyway();
                }),
                key: DiffKey::None,
            }
        });

        // Stop action that also restores any undelivered steer text into an empty composer (`AP-14`).
        let stop_action = {
            let core_stop = core.clone();
            let conv_id = active_conv.as_ref().map(|c| c.id);
            let unsent_steer = active_conv.as_ref().and_then(|c| {
                c.steps.iter().rev().find_map(|s| {
                    (s.kind == StepKind::UserPrompt
                        && matches!(s.steer, Some(SteerStatus::Queued | SteerStatus::Steering)))
                    .then(|| s.summary.clone())
                })
            });
            move || {
                if let Some(ref text) = unsent_steer
                    && composer_text.peek().trim().is_empty()
                {
                    let mut ct = composer_text;
                    ct.set(text.clone());
                }
                core_stop.invoke_or_log(Command::AgentStop {
                    conversation_id: conv_id,
                });
            }
        };

        let mut send_for_key = send_action.clone();
        let mut send_for_btn = send_action;
        let stop_for_key = stop_action.clone();
        let stop_for_btn = stop_action;
        let core_key = core.clone();
        let eff_layout = self.resolved.effective_layout;

        let current_composer_val = composer_text.read().clone();
        let placeholder_str = if dictation_phase == AgentDictationPhase::Listening && current_composer_val.is_empty() {
            "Listening… start speaking".to_string()
        } else if is_working {
            format!("{active_agent_name} is working… type to steer it")
        } else {
            format!("Ask {active_agent_name} to do something on this tab")
        };

        // AP-09: Dynamic auto-grow height from 24px up to 160px (`14px` font, `20px` line-height).
        let input_height = estimate_autogrow_height(
            &current_composer_val,
            *composer_width.read(),
            7.8,
            20.0,
            4.0,
            24.0,
            160.0,
        );

        let composer_colors = agent_input_colors(&theme, true);
        let pref_color = |pref: Option<Preference<Color>>, fallback: Color| match pref {
            Some(Preference::Specific(c)) => c,
            _ => fallback,
        };
        let composer_input = rect()
            .width(Size::fill())
            .height(Size::px(input_height))
            .font_size(14.0)
            .on_sized(move |e: Event<SizedEventData>| {
                let w = e.area.width();
                if w > 40.0 && (*composer_width.peek() - w).abs() > 1.0 {
                    composer_width.set(w);
                }
            })
            .child(
                TextField::new(composer_text)
                    .a11y_id(composer_a11y_id)
                    .auto_focus(true)
                    .enabled(!dictation_active)
                    .multiline(true)
                    .placeholder(placeholder_str)
                    .width(Size::fill())
                    .height(Size::px(input_height))
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
                            Key::Character(ch)
                                if ctrl && !alt && !meta && ch.eq_ignore_ascii_case("c") && is_working =>
                            {
                                e.stop_propagation();
                                e.prevent_default();
                                stop_for_key();
                                false
                            }
                            Key::Named(NamedKey::Escape) => {
                                e.stop_propagation();
                                e.prevent_default();
                                if *agent_picker_open.peek() {
                                    agent_picker_open.set(false);
                                    return false;
                                }
                                if *others_picker_open.peek() {
                                    others_picker_open.set(false);
                                    return false;
                                }
                                if let Some(mut ds) = dock_state {
                                    if *ds.model_card_open.peek() {
                                        // Handled by AgentModelCard (submenu -> list view -> close card).
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

        let has_sendable = !current_composer_val.trim().is_empty() || !staged_files.is_empty();
        let dictating_sendable = matches!(
            dictation_phase,
            AgentDictationPhase::Listening | AgentDictationPhase::Finishing
        );
        let send_enabled = is_working || has_sendable || dictating_sendable;

        // AP-13 & DC-01: Layout-aware Esc hint + dictation status + warning notice.
        let esc_label = if eff_layout == SidePanelLayout::Split {
            "back to the page"
        } else {
            "close"
        };
        let default_hint = if pending_confirm.is_some() {
            "y allow \u{00b7} n deny \u{00b7} Esc hide".to_string()
        } else if is_working {
            "Enter steer \u{00b7} Ctrl+C stop \u{00b7} Esc hide, it keeps working".to_string()
        } else {
            format!("Enter send \u{00b7} Shift+Enter new line \u{00b7} Tab switch agent \u{00b7} Esc {esc_label}")
        };
        let dictation_hint = match dictation_phase {
            AgentDictationPhase::Idle => None,
            AgentDictationPhase::Connecting => Some("Connecting to microphone…".to_string()),
            AgentDictationPhase::Listening => {
                let base = if *hold_active.read() {
                    "Speak now \u{00b7} let go of Alt+Space to finish"
                } else {
                    "Speak now \u{00b7} mic finishes \u{00b7} Enter sends"
                };
                Some(if *send_after_dictation.read() {
                    format!("{base} \u{00b7} sends when done")
                } else {
                    base.to_string()
                })
            }
            AgentDictationPhase::Finishing => Some(if *send_after_dictation.read() {
                "Writing it down… \u{00b7} sends when done".to_string()
            } else {
                "Writing it down…".to_string()
            }),
        };
        let active_notice = notice_text.read().clone();
        let (hint_text, hint_role) = if let Some(warn) = active_notice {
            (warn, ThemeRole::Warning)
        } else if let Some(dmsg) = dictation_hint {
            (dmsg, ThemeRole::Accent)
        } else {
            (default_hint, ThemeRole::TextMuted)
        };

        let core_instr = core.clone();
        let instructions_active = enabled_instruction_count > 0;
        let composer_is_focused = composer_focus().is_focused();
        let composer_bg = theme.rgba(ThemeRole::SurfaceRaised).with_alpha(0.55).to_color();
        let (composer_border_w, composer_border_col) = if composer_is_focused {
            (1.0, theme.rgba(ThemeRole::Accent).with_alpha(0.70).to_color())
        } else {
            (0.5, theme.rgba(ThemeRole::Border).with_alpha(0.80).to_color())
        };

        let model_chip_label = if explicit_model_id.is_some() {
            short_model_name(&selected_model.name).to_string()
        } else {
            "Default".to_string()
        };
        let model_chip_effort = explicit_effort.map(|e| e.label().to_string());
        let model_text_only = !selected_model.supports_images;

        let mut finish_for_mic = finish_dictation.clone();
        let mut finish_for_hold = finish_dictation;
        let is_stop_mode = is_working && !has_sendable;

        // AP-09 & AP-12: Composer card (`radius: 22px`, `padding: 12 10 8 14`, `margin: 0 10 10`, `32x32` buttons, `gap: 8`).
        let composer_card = rect()
            .width(Size::fill())
            .padding(Gaps::new(0.0, 10.0, 10.0, 10.0))
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(12.0, 10.0, 8.0, 14.0))
                    .spacing(6.0)
                    .corner_radius(CornerRadius::new_all(22.0))
                    .background(composer_bg)
                    .border(
                        Border::new()
                            .width(composer_border_w)
                            .alignment(BorderAlignment::Inner)
                            .fill(composer_border_col),
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
                        // Bottom controls row: attach left; model + instructions + mic + send/stop grouped right.
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(2.0, 0.0, 0.0, 0.0))
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .main_align(Alignment::SpaceBetween)
                            .spacing(8.0)
                            .child(ComposerAttachButton {
                                on_press: EventHandler::new(move |e: Event<PressEventData>| {
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
                                }),
                                key: DiffKey::None,
                            })
                            .child(
                                rect()
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(8.0)
                                    .child(ComposerModelChip {
                                        agent_id: active_agent_id.0.clone(),
                                        model_name: model_chip_label,
                                        effort_label: model_chip_effort,
                                        text_only: model_text_only,
                                        expanded: model_card_open,
                                        on_sized: EventHandler::new(move |bounds: (f32, f32, f32, f32)| {
                                            if let Some(mut ds) = dock_state {
                                                let next = Some(bounds);
                                                if *ds.model_chip_anchor.peek() != next {
                                                    ds.model_chip_anchor.set(next);
                                                }
                                            }
                                        }),
                                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                                            e.stop_propagation();
                                            agent_picker_open.set_if_modified(false);
                                            others_picker_open.set_if_modified(false);
                                            if let Some(mut ds) = dock_state {
                                                let next = !*ds.model_card_open.peek();
                                                ds.model_card_open.set(next);
                                                if next {
                                                    ds.model_card_list_view.set(false);
                                                }
                                            }
                                        }),
                                        key: DiffKey::None,
                                    })
                                    .child(ComposerInstructionsButton {
                                        active: instructions_active,
                                        active_count: enabled_instruction_count,
                                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                                            e.stop_propagation();
                                            agent_picker_open.set_if_modified(false);
                                            others_picker_open.set_if_modified(false);
                                            if let Some(mut ds) = dock_state {
                                                ds.model_card_open.set(false);
                                                ds.instructions_open.set(true);
                                            }
                                            core_instr.invoke_or_log(Command::AgentOpenInstructions);
                                        }),
                                        key: DiffKey::None,
                                    })
                                    .child(ComposerMicButton {
                                        phase: dictation_phase,
                                        static_snapshot,
                                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                                            e.stop_propagation();
                                            match dictation_phase {
                                                AgentDictationPhase::Idle => start_dictation(),
                                                AgentDictationPhase::Connecting => cancel_dictation(),
                                                AgentDictationPhase::Listening => finish_for_mic(),
                                                AgentDictationPhase::Finishing => cancel_dictation(),
                                            }
                                        }),
                                        key: DiffKey::None,
                                    })
                                    .child(ComposerSendButton {
                                        is_stop: is_stop_mode,
                                        is_steering: is_working && has_sendable,
                                        is_dictating: dictating_sendable,
                                        send_after_dictation: *send_after_dictation.read(),
                                        enabled: send_enabled,
                                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                                            e.stop_propagation();
                                            if is_stop_mode {
                                                stop_for_btn();
                                            } else {
                                                send_for_btn();
                                            }
                                        }),
                                        key: DiffKey::None,
                                    }),
                            ),
                    )
                    .child(
                        rect().padding(Gaps::new(0.0, 0.0, 0.0, 4.0)).child(
                            label()
                                .text(hint_text)
                                .font_size(11.0)
                                .color(theme.color(hint_role))
                                .max_lines(1),
                        ),
                    ),
            );

        let on_panel_press = move |_: Event<PointerEventData>| {
            if *agent_picker_open.peek() {
                agent_picker_open.set(false);
            }
            if *others_picker_open.peek() {
                others_picker_open.set(false);
            }
            if let Some(mut ds) = dock_state
                && *ds.model_card_open.peek()
            {
                ds.model_card_open.set(false);
            }
        };

        // AP-02: Enhanced agent switcher popover with all installed + dimmed uninstalled agents, hover, keyboard nav & Esc.
        let agent_picker_overlay = AgentSwitcherPopover {
            open: *agent_picker_open.read(),
            agents: all_agents.clone(),
            active_agent_id: active_agent_id.clone(),
            composer_a11y_id,
            on_select: EventHandler::new({
                let core_sel = core.clone();
                move |aid: AgentId| {
                    agent_picker_open.set(false);
                    core_sel.invoke_or_log(Command::AgentSelect { agent_id: Some(aid) });
                    composer_a11y_id.request_focus();
                }
            }),
            on_close: EventHandler::new({
                let core_keep = core.clone();
                move |_| {
                    agent_picker_open.set(false);
                    core_keep.invoke_or_log(Command::PanelOpen {
                        panel_id: SidePanelId::new(AGENT_PANEL_ID),
                        focus: true,
                    });
                    composer_a11y_id.request_focus();
                }
            }),
            key: DiffKey::None,
        };

        // AP-03: "Chats on this tab" popover when >1 other chat exists on the active tab.
        let others_picker_overlay = OtherChatsPopover {
            open: *others_picker_open.read() && other_convs.len() > 1,
            convs: same_tab_convs.clone(),
            active_conv_id: current_conv_key,
            agents: all_agents.clone(),
            on_select: EventHandler::new({
                let core_sw = core.clone();
                move |cid: ConversationId| {
                    others_picker_open.set(false);
                    core_sw.invoke_or_log(Command::AgentSwitchConversation { conversation_id: cid });
                    composer_a11y_id.request_focus();
                }
            }),
            on_close: EventHandler::new({
                let core_keep = core.clone();
                move |_| {
                    others_picker_open.set(false);
                    core_keep.invoke_or_log(Command::PanelOpen {
                        panel_id: SidePanelId::new(AGENT_PANEL_ID),
                        focus: true,
                    });
                    composer_a11y_id.request_focus();
                }
            }),
            key: DiffKey::None,
        };

        let model_card_overlay = AgentModelCard::new(
            model_card_open,
            active_agent_id.clone(),
            active_agent_name.clone(),
            selected_model.clone(),
            effective_effort,
            models.to_vec(),
            composer_a11y_id,
        )
        .explicit_model_id(explicit_model_id.clone())
        .explicit_effort(explicit_effort)
        .active_context_id(active_context_id.clone())
        .staged_kinds(staged_kinds.clone())
        .accent(theme_accent)
        .static_snapshot(static_snapshot);

        // IP-01: Render AgentInstructionsPage as an animated overlay rather than early-returning.
        let instructions_overlay = AgentInstructionsPage::new(instructions_open, instructions, theme_accent)
            .composer_a11y_id(composer_a11y_id)
            .static_snapshot(static_snapshot);

        // AP-11: Dashed "Drop to attach" overlay when dragging files over the panel.
        let drop_overlay = (*drag_over.read()).then(|| {
            let accent = theme.color(ThemeRole::Accent);
            let bg = mix_color(theme.color(ThemeRole::Surface), accent, 0.16);
            rect()
                .layer(Layer::Relative(9))
                .position(Position::new_absolute().top(8.0).left(8.0).right(8.0).bottom(8.0))
                .corner_radius(CornerRadius::new_all(14.0))
                .background(dashed_rounded_box_fill(bg, accent, 2.0, 14.0, 6.0, 4.0))
                .center()
                .child(
                    label()
                        .text("Drop to attach")
                        .font_size(15.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(accent),
                )
        });

        // DC-01: Global <A-Space> (Alt+Space) hold-to-dictate keyboard handler.
        let panel_focused = self.resolved.focused;
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .on_pointer_down(on_panel_press)
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if !panel_focused {
                    return;
                }
                let is_alt_space = e.modifiers.contains(Modifiers::ALT)
                    && !e.modifiers.intersects(Modifiers::CONTROL | Modifiers::META)
                    && (e.code == Code::Space || matches!(&e.key, Key::Character(c) if c == " "));
                if is_alt_space {
                    e.stop_propagation();
                    e.prevent_default();
                    if !*hold_active.peek() && dictation_phase == AgentDictationPhase::Idle {
                        hold_active.set(true);
                        start_dictation();
                    }
                }
            })
            .on_global_key_up(move |e: Event<KeyboardEventData>| {
                if !*hold_active.peek() {
                    return;
                }
                let released = e.code == Code::Space
                    || matches!(&e.key, Key::Named(NamedKey::Alt))
                    || matches!(&e.key, Key::Character(c) if c == " ");
                if released {
                    hold_active.set(false);
                    if *dictation_connecting.peek() {
                        cancel_dictation();
                        notice_text.set(Some(
                            "Hold Alt+Space until it says \u{201c}Speak now\u{201d}.".to_string(),
                        ));
                    } else if dock_state.is_some_and(|ds| *ds.dictation.peek() == DictationState::Listening) {
                        finish_for_hold();
                    }
                }
            })
            .child(header)
            .child(rect().width(Size::fill()).height(Size::px(1.0)).background(border_col))
            .maybe_child(service_banner)
            .child(body)
            .child(composer_card)
            .child(agent_picker_overlay)
            .child(others_picker_overlay)
            .child(model_card_overlay)
            .child(instructions_overlay)
            .maybe_child(drop_overlay)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Renders the full ordered trace of a conversation, folding consecutive tool/thinking steps into animated
/// `StepGroupView` cards while rendering `StepKind::Note` centered outside groups, accent-tinted `UserMessageBubble`
/// with hover Copy & bottom sent files, `AnswerMessageView` with streaming-aware Copy, tables, declared UI pages,
/// and confirmation cards.
fn render_conversation_steps(
    conv: &Conversation,
    agent_name: &str,
    agent_accent: Color,
    theme: &crate::ui::theme::ResolvedTheme,
    mut follow: ConversationFollow,
    static_snapshot: bool,
) -> Vec<Element> {
    let mut elements = Vec::new();
    let steps = &conv.steps;
    let latest_prompt = steps
        .iter()
        .rev()
        .find(|step| step.kind == StepKind::UserPrompt)
        .map(|step| step.id);
    let mut i = 0;

    while i < steps.len() {
        let step = &steps[i];
        match &step.kind {
            // AP-07: Only ToolCall and Thinking fold into step groups; Note renders centered outside groups.
            StepKind::ToolCall { .. } | StepKind::Thinking => {
                let group_start = i;
                let group_id = step.id.0;
                while i < steps.len() && matches!(steps[i].kind, StepKind::ToolCall { .. } | StepKind::Thinking) {
                    i += 1;
                }
                let group_steps: Vec<ConversationStep> = steps[group_start..i].to_vec();
                let is_trailing_live = i == steps.len() && conv.status == ConversationStatus::Working;
                let has_running = is_trailing_live || group_steps.iter().any(|s| s.status == StepStatus::Running);
                elements.push(
                    StepGroupView {
                        steps: group_steps,
                        live: has_running,
                        agent_accent,
                        static_snapshot,
                        key: DiffKey::from(&format!("step-group-{group_id}")),
                    }
                    .into_element(),
                );
            }
            StepKind::Note => {
                elements.push(
                    rect()
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .main_align(Alignment::center())
                        .child(
                            label()
                                .text(step.summary.clone())
                                .font_size(11.0)
                                .text_align(TextAlign::Center)
                                .color(theme.color(ThemeRole::TextMuted)),
                        )
                        .into_element(),
                );
                i += 1;
            }
            StepKind::UserPrompt => {
                let is_latest_prompt = latest_prompt == Some(step.id);
                elements.push(
                    rect()
                        .width(Size::fill())
                        .on_sized(move |e: Event<SizedEventData>| {
                            if is_latest_prompt {
                                follow.prompt_sized(e.area.min_y());
                            }
                        })
                        .child(UserMessageBubble {
                            text: step.summary.clone(),
                            files: step.files.clone(),
                            steer: step.steer,
                            key: DiffKey::from(&format!("user-{}", step.id.0)),
                        })
                        .into_element(),
                );
                i += 1;
            }
            StepKind::Response => {
                elements.push(
                    AnswerMessageView {
                        text: step.summary.clone(),
                        streaming: step.streaming,
                        key: DiffKey::from(&format!("answer-{}", step.id.0)),
                    }
                    .into_element(),
                );
                i += 1;
            }
            StepKind::Confirmation { prompt, resolved } => {
                elements.push(
                    AgentConfirmCard::new(conv.id, step.id, agent_name, prompt.clone(), *resolved)
                        .static_snapshot(static_snapshot)
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
                                    .color(theme.color(ThemeRole::Danger)),
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

// ============================================================================
// Step Group (`AP-07`)
// ============================================================================

#[derive(Clone, PartialEq)]
struct StepGroupView {
    steps: Vec<ConversationStep>,
    live: bool,
    agent_accent: Color,
    static_snapshot: bool,
    key: DiffKey,
}

impl KeyExt for StepGroupView {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for StepGroupView {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut chosen = use_state(|| None::<bool>);
        let mut was_live = use_state(|| self.live);
        let started_at = use_state(|| self.live.then(Instant::now));
        let mut ended_at = use_state(|| None::<Instant>);
        let mut measured_body_h = use_state(|| (self.steps.len() as f32 * 24.0).clamp(24.0, 240.0));
        let head_fb = use_interactive_feedback(true);
        let head_a11y = use_a11y();
        let head_focus = use_focus(head_a11y);

        if *was_live.peek() && !self.live {
            was_live.set(false);
            if ended_at.peek().is_none() {
                ended_at.set(Some(Instant::now()));
            }
        } else if !*was_live.peek() && self.live {
            was_live.set(true);
        }

        // AP-07: User click overrides open/closed state EVEN while live (`group.chosen ?? group.live`).
        let is_open = chosen.read().unwrap_or(self.live);

        let fold_anim = use_animation_with_dependencies(&(is_open, self.static_snapshot), |conf, (open, snap)| {
            if *snap {
                conf.on_creation(OnCreation::Finish);
            } else {
                conf.on_creation(OnCreation::Finish);
                conf.on_change(OnChange::Rerun);
            }
            let (from, to) = if *open { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::STEP_FOLD)
                .ease(Ease::Out)
                .function(motion::css_ease_step_fold_fn())
        });

        let live_shimmer_active = self.live && !self.static_snapshot;
        let shimmer_anim = use_animation_with_dependencies(&live_shimmer_active, |conf, active| {
            if *active {
                conf.on_creation(OnCreation::Run);
                conf.on_change(OnChange::Rerun);
                conf.on_finish(OnFinish::reverse());
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(0.0, 1.0)
                .duration(Duration::from_millis(1700))
                .ease(Ease::InOut)
        });

        let fold_t = if self.static_snapshot {
            if is_open { 1.0 } else { 0.0 }
        } else {
            fold_anim.get().value().clamp(0.0, 1.0)
        };

        let header_text = if self.live {
            self.steps
                .last()
                .map(|s| match &s.kind {
                    StepKind::ToolCall { .. } => format!("Used {}", s.summary),
                    _ => s.summary.clone(),
                })
                .unwrap_or_else(|| format!("{} steps", self.steps.len()))
        } else {
            let count = self.steps.len();
            let failed = self.steps.iter().filter(|s| s.status == StepStatus::Failed).count();
            let mut parts: Vec<String> = Vec::new();
            if let (Some(start), Some(end)) = (*started_at.read(), *ended_at.read()) {
                let secs = end.saturating_duration_since(start).as_secs().max(1);
                parts.push(format!("Worked for {}", format_duration_secs(secs)));
            }
            parts.push(if count == 1 {
                "1 step".to_string()
            } else {
                format!("{count} steps")
            });
            if failed > 0 {
                parts.push(format!("{failed} failed"));
            }
            parts.join(" \u{00b7} ")
        };

        let base_head_col = mix_color(
            theme.color(ThemeRole::TextMuted),
            theme.color(ThemeRole::Text),
            head_fb.hover_progress(),
        );
        let head_col = if live_shimmer_active {
            let s = shimmer_anim.get().value().clamp(0.0, 1.0);
            mix_color(base_head_col, theme.color(ThemeRole::Text), s * 0.65)
        } else {
            base_head_col
        };
        let head_border = focus_ring_border(matches!(head_focus(), Focus::Keyboard), &theme, Border::new());
        let hairline_col = theme.rgba(ThemeRole::Text).with_alpha(0.12).to_color();

        let body_visible = is_open || fold_t > 0.01;
        let target_h = (*measured_body_h.read()).min(240.0);
        let animated_h = (target_h * fold_t).max(0.0);
        let chevron_rot = fold_t * 90.0;

        let mut list = rect()
            .width(Size::fill())
            .spacing(4.0)
            .on_sized(move |e: Event<SizedEventData>| {
                let h = e.area.height().clamp(20.0, 240.0);
                if (*measured_body_h.peek() - h).abs() > 1.0 {
                    measured_body_h.set(h);
                }
            });
        for s in &self.steps {
            list = list.child(StepRowView {
                step: s.clone(),
                live: self.live,
                agent_accent: self.agent_accent,
                static_snapshot: self.static_snapshot,
                key: DiffKey::from(&format!("step-row-{}", s.id.0)),
            });
        }

        let fold_section = body_visible.then(|| {
            let needs_scroll = *measured_body_h.read() >= 238.0;
            let content_el: Element = if needs_scroll {
                ScrollView::new()
                    .width(Size::fill())
                    .height(Size::px(240.0))
                    .child(list)
                    .into_element()
            } else {
                list.into_element()
            };
            rect()
                .width(Size::fill())
                .height(if is_open && fold_t >= 0.99 {
                    if needs_scroll { Size::px(240.0) } else { Size::Inner }
                } else {
                    Size::px(animated_h)
                })
                .max_height(Size::px(240.0))
                .overflow(Overflow::Clip)
                .opacity(fold_t)
                .margin(Gaps::new(0.0, 0.0, 0.0, 5.0))
                .padding(Gaps::new(2.0, 0.0, 2.0, 11.0))
                .child(
                    rect()
                        .position(Position::new_absolute().left(0.0).top(0.0))
                        .width(Size::px(1.0))
                        .height(Size::percent(100.0))
                        .background(hairline_col),
                )
                .child(content_el)
        });

        rect()
            .width(Size::fill())
            .spacing(2.0)
            .child(
                rect()
                    .a11y_id(head_a11y)
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Toggle agent steps")
                    .height(Size::px(26.0))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .corner_radius(CornerRadius::new_all(6.0))
                    .border(head_border)
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(head_fb.on_pointer_enter())
                    .on_pointer_leave(head_fb.on_pointer_leave())
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        chosen.set(Some(!is_open));
                    })
                    .child(
                        RollingText::new(header_text)
                            .font_size(12.0)
                            .font_weight(FontWeight::MEDIUM)
                            .color(head_col),
                    )
                    .child(
                        rect()
                            .opacity(0.55)
                            .rotate(chevron_rot)
                            .child(chevron_right_icon(10.0, head_col)),
                    ),
            )
            .maybe_child(fold_section)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct StepRowView {
    step: ConversationStep,
    live: bool,
    agent_accent: Color,
    static_snapshot: bool,
    key: DiffKey,
}

impl KeyExt for StepRowView {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for StepRowView {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let animate_fresh = self.live && !self.static_snapshot;
        let fresh_anim = use_animation_with_dependencies(&animate_fresh, |conf, fresh| {
            if *fresh {
                conf.on_creation(OnCreation::Run);
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(0.0, 1.0)
                .duration(motion::STEP_FRESH_IN)
                .ease(Ease::Out)
                .function(motion::ease_rise_fn())
        });
        let t = if animate_fresh {
            fresh_anim.get().value().clamp(0.0, 1.0)
        } else {
            1.0
        };

        let status_el: Element = match self.step.status {
            StepStatus::Running => WorkingRing::new(12.0).color(self.agent_accent).into_element(),
            StepStatus::Completed => label()
                .text("\u{2713}")
                .font_size(11.0)
                .color(theme.color(ThemeRole::Success))
                .into_element(),
            StepStatus::Failed => label()
                .text("\u{2715}")
                .font_size(11.0)
                .color(theme.color(ThemeRole::Danger))
                .into_element(),
            StepStatus::Pending | StepStatus::WaitingForUser => label()
                .text("\u{00b7}")
                .font_size(12.0)
                .color(theme.color(ThemeRole::TextMuted))
                .into_element(),
        };

        let mut text_col = rect().width(Size::flex(1.0)).spacing(1.0).child(
            label()
                .text(self.step.summary.clone())
                .font_size(12.0)
                .color(theme.color(ThemeRole::TextMuted))
                .max_lines(2),
        );
        if let Some(detail) = &self.step.detail
            && !detail.trim().is_empty()
        {
            text_col = text_col.child(
                label()
                    .text(detail.clone())
                    .font_size(11.0)
                    .color(theme.rgba(ThemeRole::TextMuted).with_alpha(0.75).to_color())
                    .max_lines(2),
            );
        }

        rect()
            .width(Size::fill())
            .offset_y((1.0 - t) * -6.0)
            .opacity(t)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::start())
            .spacing(8.0)
            .padding(Gaps::new(1.0, 0.0, 1.0, 0.0))
            .child(rect().padding(Gaps::new(2.0, 0.0, 0.0, 0.0)).child(status_el))
            .child(text_col)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

// ============================================================================
// User Bubble (`AP-06`) & Answer View (`AP-08`)
// ============================================================================

#[derive(Clone, PartialEq)]
struct UserMessageBubble {
    text: String,
    files: Vec<String>,
    steer: Option<SteerStatus>,
    key: DiffKey,
}

impl KeyExt for UserMessageBubble {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for UserMessageBubble {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let mut copied = use_state(|| false);
        let copy_fb = use_interactive_feedback(true);

        let show_copy = *hovered.read() || *copied.read() || copy_fb.is_hovered();
        let copy_fade = use_animation_with_dependencies(&show_copy, |conf, show| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *show { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::HOVER)
                .ease(Ease::Out)
                .function(motion::css_ease_default_fn())
        });

        let user_rgba = theme.rgba(ThemeRole::AgentUser);
        let bubble_bg = user_rgba.with_alpha(0.20).to_color();
        let bubble_border = user_rgba.with_alpha(0.40).to_color();
        let chip_bg = user_rgba.with_alpha(0.22).to_color();

        // AP-06: Dim ONLY while Queued or Steering (`0.55`), never when Delivered.
        let is_pending_steer = matches!(self.steer, Some(SteerStatus::Queued | SteerStatus::Steering));
        let steer_label = match self.steer {
            Some(SteerStatus::Steering) => Some("Steering…"),
            Some(SteerStatus::Queued) => Some("Queued"),
            Some(SteerStatus::Delivered) | None => None,
        };

        // AP-06: Sent files BELOW text (`margin-top: 6px`).
        let files_el = (!self.files.is_empty()).then(|| {
            let mut row = rect()
                .direction(Direction::Horizontal)
                .spacing(4.0)
                .margin(Gaps::new(6.0, 0.0, 0.0, 0.0));
            for f in &self.files {
                row = row.child(
                    rect()
                        .padding(Gaps::new(1.0, 9.0, 1.0, 9.0))
                        .corner_radius(CornerRadius::new_all(999.0))
                        .background(chip_bg)
                        .child(
                            label()
                                .text(f.clone())
                                .font_size(11.5)
                                .color(theme.color(ThemeRole::Text))
                                .max_lines(1),
                        ),
                );
            }
            row
        });

        let copy_opacity = copy_fade.get().value().clamp(0.0, 1.0);
        let is_copied = *copied.read();
        let copy_text = self.text.clone();
        let copy_bg = copy_fb.background(
            theme.rgba(ThemeRole::Surface).with_alpha(0.80).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.16).to_color(),
        );

        rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .main_align(Alignment::end())
            .opacity(if is_pending_steer { 0.55 } else { 1.0 })
            .child(
                rect()
                    .max_width(Size::percent(85.0))
                    .spacing(3.0)
                    .cross_align(Alignment::end())
                    .on_pointer_enter(move |_| hovered.set_if_modified(true))
                    .on_pointer_leave(move |_| hovered.set_if_modified(false))
                    .child(
                        rect()
                            .padding(Gaps::new(7.0, 12.0, 7.0, 12.0))
                            .corner_radius((14.0, 14.0, 4.0, 14.0))
                            .background(bubble_bg)
                            .border(
                                Border::new()
                                    .width(0.5)
                                    .alignment(BorderAlignment::Inner)
                                    .fill(bubble_border),
                            )
                            .child(
                                label()
                                    .text(self.text.clone())
                                    .font_size(13.0)
                                    .line_height(1.5)
                                    .color(theme.color(ThemeRole::Text)),
                            )
                            .maybe_child(files_el)
                            .maybe_child((copy_opacity > 0.01).then(|| {
                                rect()
                                    .layer(Layer::Relative(3))
                                    .position(Position::new_absolute().top(-9.0).right(8.0))
                                    .opacity(copy_opacity)
                                    .child(
                                        KitTooltip::new(if is_copied { "Copied" } else { "Copy" }).child(
                                            rect()
                                                .a11y_role(AccessibilityRole::Button)
                                                .a11y_alt("Copy")
                                                .padding(Gaps::new_all(3.0))
                                                .corner_radius(CornerRadius::new_all(6.0))
                                                .background(copy_bg)
                                                .cursor(CursorIcon::Pointer)
                                                .on_pointer_enter(copy_fb.on_pointer_enter())
                                                .on_pointer_leave(copy_fb.on_pointer_leave())
                                                .on_press(move |e: Event<PressEventData>| {
                                                    e.stop_propagation();
                                                    copy_to_clipboard(&copy_text);
                                                    copied.set(true);
                                                    spawn(async move {
                                                        timer(Duration::from_millis(1400)).await;
                                                        copied.set(false);
                                                    });
                                                })
                                                .child(
                                                    Icon::new(if is_copied { "check" } else { "copy" })
                                                        .size(13.0)
                                                        .role(if is_copied {
                                                            ThemeRole::Success
                                                        } else if copy_fb.is_hovered() {
                                                            ThemeRole::Text
                                                        } else {
                                                            ThemeRole::TextMuted
                                                        }),
                                                ),
                                        ),
                                    )
                            })),
                    )
                    .maybe_child(
                        steer_label.map(|s| label().text(s).font_size(10.0).color(theme.color(ThemeRole::TextMuted))),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct AnswerMessageView {
    text: String,
    streaming: bool,
    key: DiffKey,
}

impl KeyExt for AnswerMessageView {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for AnswerMessageView {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut msg_hovered = use_state(|| false);
        let mut copied = use_state(|| false);
        let btn_fb = use_interactive_feedback(true);
        let btn_a11y = use_a11y();
        let btn_focus = use_focus(btn_a11y);

        let emphasized = *msg_hovered.read() || *copied.read() || btn_fb.is_hovered() || btn_focus().is_focused();
        let opacity_anim = use_animation_with_dependencies(&emphasized, |conf, active| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *active { (0.55, 1.0) } else { (1.0, 0.55) };
            AnimNum::new(from, to)
                .duration(motion::HOVER)
                .ease(Ease::Out)
                .function(motion::css_ease_default_fn())
        });

        let show_copy = !self.streaming && !self.text.trim().is_empty();
        let is_copied = *copied.read();
        let copy_text = self.text.clone();
        let btn_bg = btn_fb.background(
            Color::TRANSPARENT,
            theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.16).to_color(),
        );
        let btn_role = if is_copied {
            ThemeRole::Success
        } else if btn_fb.is_hovered() {
            ThemeRole::Text
        } else {
            ThemeRole::TextMuted
        };
        let btn_border = focus_ring_border(matches!(btn_focus(), Focus::Keyboard), &theme, Border::new());

        rect()
            .width(Size::fill())
            .spacing(4.0)
            .on_pointer_enter(move |_| msg_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| msg_hovered.set_if_modified(false))
            .child(MarkdownView::new(self.text.clone()).streaming(self.streaming))
            .maybe_child(show_copy.then(|| {
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::start())
                    .margin(Gaps::new(0.0, 0.0, 0.0, -5.0))
                    .opacity(opacity_anim.get().value().clamp(0.55, 1.0))
                    .child(
                        rect()
                            .a11y_id(btn_a11y)
                            .a11y_focusable(true)
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Copy answer")
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(5.0)
                            .padding(Gaps::new(2.0, 7.0, 2.0, 5.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .background(btn_bg)
                            .border(btn_border)
                            .cursor(CursorIcon::Pointer)
                            .on_pointer_enter(btn_fb.on_pointer_enter())
                            .on_pointer_leave(btn_fb.on_pointer_leave())
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                copy_to_clipboard(&copy_text);
                                copied.set(true);
                                spawn(async move {
                                    timer(Duration::from_millis(1400)).await;
                                    copied.set(false);
                                });
                            })
                            .child(
                                Icon::new(if is_copied { "check" } else { "copy" })
                                    .size(13.0)
                                    .role(btn_role),
                            )
                            .child(
                                label()
                                    .text(if is_copied { "Copied" } else { "Copy" })
                                    .font_size(11.0)
                                    .color(theme.color(btn_role)),
                            ),
                    )
            }))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

// ============================================================================
// Header Chips & Switcher Popovers (`AP-02`, `AP-03`)
// ============================================================================

#[derive(Clone, PartialEq)]
struct AgentChipButton {
    agent_id: String,
    agent_name: String,
    agent_accent: Color,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for AgentChipButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for AgentChipButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let rest_bg = Color::from_argb(46, self.agent_accent.r(), self.agent_accent.g(), self.agent_accent.b());
        let hover_bg = Color::from_argb(71, self.agent_accent.r(), self.agent_accent.g(), self.agent_accent.b());
        let press_bg = Color::from_argb(90, self.agent_accent.r(), self.agent_accent.g(), self.agent_accent.b());
        let border_col = Color::from_argb(97, self.agent_accent.r(), self.agent_accent.g(), self.agent_accent.b());

        let base_border = Border::new()
            .width(0.5)
            .alignment(BorderAlignment::Inner)
            .fill(border_col);
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, base_border);
        let on_press = self.on_press.clone();

        KitTooltip::new("Switch agent").child(
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Switch agent")
                .padding(Gaps::new(3.0, 9.0, 3.0, 5.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(fb.background(rest_bg, hover_bg, press_bg))
                .border(border)
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(7.0)
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| on_press.call(e))
                .child(agent_badge_emblem(&self.agent_id, 20.0, 13.0, 999.0, false))
                .child(
                    label()
                        .text(self.agent_name.clone())
                        .font_size(12.5)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(theme.color(ThemeRole::Text))
                        .max_lines(1),
                )
                .child(
                    rect()
                        .opacity(0.55)
                        .child(chevron_down_icon(10.0, theme.color(ThemeRole::Text))),
                ),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct OtherChatsButton {
    others: Vec<Conversation>,
    agent_names: HashMap<String, String>,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for OtherChatsButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for OtherChatsButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let describe = |conv: &Conversation| -> String {
            let name = self
                .agent_names
                .get(&conv.agent_id.0)
                .map(|s| s.as_str())
                .unwrap_or("Agent");
            let working = if conv.status == ConversationStatus::Working {
                " (working)"
            } else {
                ""
            };
            let title = if conv.title.trim().is_empty() {
                "New conversation"
            } else {
                conv.title.as_str()
            };
            format!("{name}{working}: {title}")
        };
        let tip = if self.others.len() == 1 {
            format!("Switch to {}", describe(&self.others[0]))
        } else {
            let lines: Vec<String> = self.others.iter().map(describe).collect();
            format!("Also on this tab:\n{}", lines.join("\n"))
        };

        let bg = fb.background(
            Color::TRANSPARENT,
            theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.16).to_color(),
        );
        let logo_opacity = 0.70 + 0.30 * fb.hover_progress();
        let text_col = mix_color(
            theme.color(ThemeRole::TextMuted),
            theme.color(ThemeRole::Text),
            fb.hover_progress(),
        );
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let on_press = self.on_press.clone();

        let mut stack = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(tip.clone())
            .height(Size::px(24.0))
            .padding(Gaps::new(0.0, 6.0, 0.0, 4.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(bg)
            .border(border)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| on_press.call(e));

        for (idx, other) in self.others.iter().take(2).enumerate() {
            let is_work = other.status == ConversationStatus::Working;
            let ring_col = theme.agent_color(&other.agent_id.0);
            let mut badge = rect()
                .width(Size::px(18.0))
                .height(Size::px(18.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(Color::WHITE)
                .opacity(logo_opacity)
                .center()
                .child(agent_logo_element(&other.agent_id.0, 12.0, false));
            if idx > 0 {
                badge = badge.margin(Gaps::new(0.0, 0.0, 0.0, -6.0));
            }
            if is_work {
                badge = badge.border(
                    Border::new()
                        .width(1.5)
                        .alignment(BorderAlignment::Outer)
                        .fill(ring_col),
                );
            }
            stack = stack.child(badge);
        }

        if self.others.len() > 2 {
            stack = stack.child(
                rect().margin(Gaps::new(0.0, 0.0, 0.0, 4.0)).child(
                    label()
                        .text(format!("+{}", self.others.len() - 2))
                        .font_size(11.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(text_col),
                ),
            );
        }

        KitTooltip::new(tip).child(stack)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct AgentSwitcherPopover {
    open: bool,
    agents: Vec<Agent>,
    active_agent_id: AgentId,
    composer_a11y_id: AccessibilityId,
    on_select: EventHandler<AgentId>,
    on_close: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for AgentSwitcherPopover {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for AgentSwitcherPopover {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut cursor = use_state(|| 0usize);
        let mut was_open = use_state(|| false);

        if self.open && !*was_open.peek() {
            was_open.set(true);
            let active_idx = self
                .agents
                .iter()
                .position(|a| a.id == self.active_agent_id && a.available)
                .or_else(|| self.agents.iter().position(|a| a.available))
                .unwrap_or(0);
            cursor.set(active_idx);
        } else if !self.open && *was_open.peek() {
            was_open.set(false);
        }

        let is_open = self.open;
        let agents_for_key = self.agents.clone();
        let on_select_key = self.on_select.clone();
        let on_close_key = self.on_close.clone();

        let picker_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(248, 30, 34, 52),
            ResolvedScheme::Light => Color::from_argb(252, 249, 250, 255),
        };
        let border_col = theme.color(ThemeRole::BorderSubtle);
        let active_cursor = *cursor.read();

        let mut rows = rect().width(Size::fill()).spacing(2.0);
        for (idx, ag) in self.agents.iter().enumerate() {
            let is_cur = ag.id == self.active_agent_id;
            let is_highlighted = idx == active_cursor && ag.available;
            let aid = ag.id.clone();
            let on_sel = self.on_select.clone();
            rows = rows.child(AgentSwitcherRow {
                agent: ag.clone(),
                is_current: is_cur,
                highlighted: is_highlighted,
                on_press: EventHandler::new(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    on_sel.call(aid.clone());
                }),
                key: DiffKey::from(&ag.id.0),
            });
        }

        rect()
            .layer(Layer::OverlayLevel(crate::ui::theme::POPUP_OVERLAY_LEVEL))
            .position(Position::new_absolute().top(44.0).left(10.0))
            .child(
                PopupPresence::inline(self.open).child(
                    rect()
                        .width(Size::px(236.0))
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
                        .on_global_key_down(move |e: Event<KeyboardEventData>| {
                            if !is_open {
                                return;
                            }
                            let selectable: Vec<usize> = agents_for_key
                                .iter()
                                .enumerate()
                                .filter(|(_, a)| a.available)
                                .map(|(i, _)| i)
                                .collect();
                            match classify_list_key(&e, ListNavConfig::menu()) {
                                Some(ListNavAction::MoveNext) => {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    if let Some(pos) = selectable.iter().position(|&i| i == *cursor.peek()) {
                                        cursor.set(selectable[(pos + 1) % selectable.len()]);
                                    } else if let Some(&first) = selectable.first() {
                                        cursor.set(first);
                                    }
                                }
                                Some(ListNavAction::MovePrev) => {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    if let Some(pos) = selectable.iter().position(|&i| i == *cursor.peek()) {
                                        cursor.set(selectable[(pos + selectable.len() - 1) % selectable.len()]);
                                    } else if let Some(&last) = selectable.last() {
                                        cursor.set(last);
                                    }
                                }
                                Some(ListNavAction::First) => {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    if let Some(&first) = selectable.first() {
                                        cursor.set(first);
                                    }
                                }
                                Some(ListNavAction::Last) => {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    if let Some(&last) = selectable.last() {
                                        cursor.set(last);
                                    }
                                }
                                Some(ListNavAction::Activate) => {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    if let Some(ag) = agents_for_key.get(*cursor.peek())
                                        && ag.available
                                    {
                                        on_select_key.call(ag.id.clone());
                                    }
                                }
                                Some(ListNavAction::Dismiss) => {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    on_close_key.call(());
                                }
                                _ => {}
                            }
                        })
                        .child(rows),
                ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct AgentSwitcherRow {
    agent: Agent,
    is_current: bool,
    highlighted: bool,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for AgentSwitcherRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for AgentSwitcherRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let enabled = self.agent.available;
        let fb = use_interactive_feedback(enabled);

        let rest_bg = if self.highlighted {
            theme.rgba(ThemeRole::Accent).with_alpha(0.18).to_color()
        } else if self.is_current {
            theme.rgba(ThemeRole::Accent).with_alpha(0.10).to_color()
        } else {
            Color::TRANSPARENT
        };
        let hover_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.18).to_color();
        let press_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.26).to_color();
        let bg = fb.background(rest_bg, hover_bg, press_bg);
        let on_press = self.on_press.clone();

        let row = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_enabled(enabled)
            .a11y_alt(format!("Switch to {}", self.agent.name))
            .width(Size::fill())
            .height(Size::px(32.0))
            .padding(Gaps::new(0.0, 9.0, 0.0, 8.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(bg)
            .opacity(if enabled { 1.0 } else { 0.48 })
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .main_align(Alignment::SpaceBetween)
            .spacing(8.0)
            .cursor(if enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                if enabled {
                    on_press.call(e);
                }
            })
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .child(agent_badge_emblem(&self.agent.id.0, 20.0, 12.5, 999.0, false))
                    .child(
                        label()
                            .text(self.agent.name.clone())
                            .font_size(12.0)
                            .font_weight(if self.is_current {
                                FontWeight::SEMI_BOLD
                            } else {
                                FontWeight::MEDIUM
                            })
                            .color(theme.color(ThemeRole::Text))
                            .max_lines(1),
                    ),
            )
            .maybe_child(if self.is_current {
                Some(
                    rect()
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(5.0)
                        .child(
                            label()
                                .text("current")
                                .font_size(10.5)
                                .color(theme.color(ThemeRole::TextMuted)),
                        )
                        .child(Icon::new("check").size(12.0).role(ThemeRole::Accent))
                        .into_element(),
                )
            } else if !enabled {
                Some(
                    label()
                        .text("not installed")
                        .font_size(10.5)
                        .color(theme.color(ThemeRole::TextMuted))
                        .into_element(),
                )
            } else {
                None
            });

        if !enabled {
            KitTooltip::new(format!("Install: {}", agent_install_command(&self.agent.id.0)))
                .child(row)
                .into_element()
        } else {
            row.into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct OtherChatsPopover {
    open: bool,
    convs: Vec<Conversation>,
    active_conv_id: Option<ConversationId>,
    agents: Vec<Agent>,
    on_select: EventHandler<ConversationId>,
    on_close: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for OtherChatsPopover {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for OtherChatsPopover {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut cursor = use_state(|| 0usize);
        let mut was_open = use_state(|| false);

        if self.open && !*was_open.peek() {
            was_open.set(true);
            let idx = self
                .convs
                .iter()
                .position(|c| Some(c.id) != self.active_conv_id)
                .unwrap_or(0);
            cursor.set(idx);
        } else if !self.open && *was_open.peek() {
            was_open.set(false);
        }

        let picker_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(248, 30, 34, 52),
            ResolvedScheme::Light => Color::from_argb(252, 249, 250, 255),
        };
        let border_col = theme.color(ThemeRole::BorderSubtle);
        let is_open = self.open;
        let convs_for_key = self.convs.clone();
        let on_sel_key = self.on_select.clone();
        let on_close_key = self.on_close.clone();

        let mut rows = rect().width(Size::fill()).spacing(2.0).child(
            rect().padding(Gaps::new(2.0, 8.0, 4.0, 8.0)).child(
                label()
                    .text("Chats on this tab")
                    .font_size(10.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(theme.color(ThemeRole::TextMuted)),
            ),
        );
        let active_cursor = *cursor.read();
        for (idx, conv) in self.convs.iter().enumerate() {
            let agent_name = self
                .agents
                .iter()
                .find(|a| a.id == conv.agent_id)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| conv.agent_id.0.clone());
            let mut detail_parts = vec![agent_name];
            if conv.status == ConversationStatus::Working {
                detail_parts.push("working".to_string());
            }
            if Some(conv.id) == self.active_conv_id {
                detail_parts.push("shown".to_string());
            }
            let cid = conv.id;
            let on_sel = self.on_select.clone();
            rows = rows.child(OtherChatRow {
                agent_id: conv.agent_id.0.clone(),
                title: if conv.title.trim().is_empty() {
                    "New conversation".to_string()
                } else {
                    conv.title.clone()
                },
                detail: detail_parts.join(" \u{00b7} "),
                highlighted: idx == active_cursor,
                is_current: Some(conv.id) == self.active_conv_id,
                on_press: EventHandler::new(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    on_sel.call(cid);
                }),
                key: DiffKey::from(&format!("other-chat-{}", conv.id.0)),
            });
        }

        rect()
            .layer(Layer::OverlayLevel(crate::ui::theme::POPUP_OVERLAY_LEVEL))
            .position(Position::new_absolute().top(44.0).left(120.0))
            .child(
                PopupPresence::inline(self.open).child(
                    rect()
                        .width(Size::px(250.0))
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
                        .on_global_key_down(move |e: Event<KeyboardEventData>| {
                            if !is_open || convs_for_key.is_empty() {
                                return;
                            }
                            let len = convs_for_key.len();
                            match classify_list_key(&e, ListNavConfig::menu()) {
                                Some(ListNavAction::MoveNext) => {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    let next = (*cursor.peek() + 1) % len;
                                    cursor.set(next);
                                }
                                Some(ListNavAction::MovePrev) => {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    let prev = (*cursor.peek() + len - 1) % len;
                                    cursor.set(prev);
                                }
                                Some(ListNavAction::First) => {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    cursor.set(0);
                                }
                                Some(ListNavAction::Last) => {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    cursor.set(len - 1);
                                }
                                Some(ListNavAction::Activate) => {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    if let Some(c) = convs_for_key.get(*cursor.peek()) {
                                        on_sel_key.call(c.id);
                                    }
                                }
                                Some(ListNavAction::Dismiss) => {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    on_close_key.call(());
                                }
                                _ => {}
                            }
                        })
                        .child(rows),
                ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct OtherChatRow {
    agent_id: String,
    title: String,
    detail: String,
    highlighted: bool,
    is_current: bool,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for OtherChatRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for OtherChatRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let rest_bg = if self.highlighted {
            theme.rgba(ThemeRole::Accent).with_alpha(0.16).to_color()
        } else {
            Color::TRANSPARENT
        };
        let bg = fb.background(
            rest_bg,
            theme.rgba(ThemeRole::Accent).with_alpha(0.18).to_color(),
            theme.rgba(ThemeRole::Accent).with_alpha(0.26).to_color(),
        );
        let on_press = self.on_press.clone();

        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.title.clone())
            .width(Size::fill())
            .padding(Gaps::new(6.0, 8.0, 6.0, 8.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(bg)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| on_press.call(e))
            .child(agent_badge_emblem(&self.agent_id, 20.0, 12.5, 999.0, false))
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .spacing(1.0)
                    .child(
                        label()
                            .text(self.title.clone())
                            .font_size(12.0)
                            .font_weight(if self.is_current {
                                FontWeight::SEMI_BOLD
                            } else {
                                FontWeight::MEDIUM
                            })
                            .color(theme.color(ThemeRole::Text))
                            .max_lines(1),
                    )
                    .child(
                        label()
                            .text(self.detail.clone())
                            .font_size(10.5)
                            .color(theme.color(ThemeRole::TextMuted))
                            .max_lines(1),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

// ============================================================================
// Staged Attachments (`AP-11`) & Media Warning Banner
// ============================================================================

#[derive(Clone, PartialEq)]
struct StagedMediaTile {
    file: InstructionAttachment,
    on_remove: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for StagedMediaTile {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for StagedMediaTile {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let remove_fb = use_interactive_feedback(true);
        let is_video = self.file.kind == AttachmentKind::Video;
        let tip = format!("{} \u{00b7} {}", self.file.name, format_file_size(self.file.size_bytes));
        let on_remove = self.on_remove.clone();
        let remove_bg = remove_fb.background(
            Color::from_argb(153, 0, 0, 0),
            Color::from_argb(217, 0, 0, 0),
            Color::from_argb(235, 0, 0, 0),
        );

        KitTooltip::new(tip).child(
            rect()
                .width(Size::px(72.0))
                .height(Size::px(72.0))
                .corner_radius(CornerRadius::new_all(16.0))
                .overflow(Overflow::Clip)
                .background(theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color())
                .border(
                    Border::new()
                        .width(0.5)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.rgba(ThemeRole::Border).with_alpha(0.80).to_color()),
                )
                .center()
                .spacing(3.0)
                .child(Icon::new("new-tab-image").size(22.0).role(ThemeRole::Accent))
                .child(
                    rect().padding(Gaps::new(0.0, 5.0, 0.0, 5.0)).child(
                        label()
                            .text(self.file.name.clone())
                            .font_size(9.5)
                            .color(theme.color(ThemeRole::TextMuted))
                            .max_lines(1),
                    ),
                )
                .maybe_child(is_video.then(|| {
                    rect()
                        .position(Position::new_absolute().top(23.0).left(23.0))
                        .width(Size::px(26.0))
                        .height(Size::px(26.0))
                        .corner_radius(CornerRadius::new_all(999.0))
                        .background(Color::from_argb(140, 0, 0, 0))
                        .center()
                        .child(colored_zen_icon("media-play", 12.0, Color::WHITE))
                }))
                .child(
                    rect()
                        .layer(Layer::Relative(2))
                        .position(Position::new_absolute().top(4.0).right(4.0))
                        .child(
                            KitTooltip::new(format!("Remove {}", self.file.name)).child(
                                rect()
                                    .a11y_role(AccessibilityRole::Button)
                                    .a11y_alt(format!("Remove {}", self.file.name))
                                    .width(Size::px(20.0))
                                    .height(Size::px(20.0))
                                    .corner_radius(CornerRadius::new_all(999.0))
                                    .background(remove_bg)
                                    .center()
                                    .cursor(CursorIcon::Pointer)
                                    .on_pointer_enter(remove_fb.on_pointer_enter())
                                    .on_pointer_leave(remove_fb.on_pointer_leave())
                                    .on_press(move |e: Event<PressEventData>| on_remove.call(e))
                                    .child(colored_zen_icon("close", 10.0, Color::WHITE)),
                            ),
                        ),
                ),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct StagedFileChip {
    file: InstructionAttachment,
    on_remove: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for StagedFileChip {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for StagedFileChip {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let remove_fb = use_interactive_feedback(true);
        let on_remove = self.on_remove.clone();
        let x_bg = remove_fb.background(
            Color::TRANSPARENT,
            theme.rgba(ThemeRole::Text).with_alpha(0.16).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.24).to_color(),
        );

        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(6.0)
            .padding(Gaps::new(3.0, 4.0, 3.0, 10.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color())
            .child(
                label()
                    .text(self.file.name.clone())
                    .font_size(12.0)
                    .color(theme.color(ThemeRole::Text))
                    .max_lines(1),
            )
            .child(
                label()
                    .text(format_file_size(self.file.size_bytes))
                    .font_size(11.0)
                    .color(theme.color(ThemeRole::TextMuted))
                    .max_lines(1),
            )
            .child(
                KitTooltip::new(format!("Remove {}", self.file.name)).child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt(format!("Remove {}", self.file.name))
                        .width(Size::px(18.0))
                        .height(Size::px(18.0))
                        .corner_radius(CornerRadius::new_all(999.0))
                        .background(x_bg)
                        .center()
                        .cursor(CursorIcon::Pointer)
                        .on_pointer_enter(remove_fb.on_pointer_enter())
                        .on_pointer_leave(remove_fb.on_pointer_leave())
                        .on_press(move |e: Event<PressEventData>| on_remove.call(e))
                        .child(Icon::new("close").size(10.0).role(if remove_fb.is_hovered() {
                            ThemeRole::Text
                        } else {
                            ThemeRole::TextMuted
                        })),
                ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct MediaWarningBanner {
    issue: MediaCompatibilityIssue,
    nudged: bool,
    nudge_seq: u64,
    on_use_best: EventHandler<ModelId>,
    on_pick: EventHandler<()>,
    on_send_anyway: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for MediaWarningBanner {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for MediaWarningBanner {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let nudge_anim = use_animation_with_dependencies(&self.nudge_seq, |conf, seq| {
            if *seq == 0 {
                conf.on_creation(OnCreation::Finish);
            } else {
                conf.on_creation(OnCreation::Finish);
                conf.on_change(OnChange::Rerun);
            }
            AnimNum::new(0.0, 1.0)
                .duration(motion::MEDIA_NUDGE)
                .function(Function::Linear)
        });

        let nudge_x = if self.nudge_seq == 0 {
            0.0
        } else {
            let p = nudge_anim.get().value().clamp(0.0, 1.0);
            if p < 0.25 {
                -3.0 * (p / 0.25)
            } else if p < 0.75 {
                -3.0 + 6.0 * ((p - 0.25) / 0.50)
            } else {
                3.0 * (1.0 - (p - 0.75) / 0.25)
            }
        };

        let things: Vec<&str> = self
            .issue
            .unsupported_kinds
            .iter()
            .map(|k| match k {
                AttachmentKind::Image => "images",
                AttachmentKind::Video => "videos",
                AttachmentKind::Pdf => "PDFs",
                AttachmentKind::Document => "documents",
            })
            .collect();
        let things_text = things.join(" or ");

        let border_w = if self.nudged { 1.5 } else { 0.5 };
        let border_col = if self.nudged {
            theme.color(ThemeRole::Warning)
        } else {
            theme.rgba(ThemeRole::Warning).with_alpha(0.55).to_color()
        };

        let on_use = self.on_use_best.clone();
        let on_pick = self.on_pick.clone();
        let on_anyway = self.on_send_anyway.clone();

        rect()
            .width(Size::fill())
            .offset_x(nudge_x)
            .padding(Gaps::new(8.0, 10.0, 8.0, 10.0))
            .spacing(6.0)
            .corner_radius(CornerRadius::new_all(12.0))
            .background(theme.rgba(ThemeRole::Warning).with_alpha(0.15).to_color())
            .border(
                Border::new()
                    .width(border_w)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_col),
            )
            .child(
                paragraph()
                    .width(Size::fill())
                    .font_size(12.0)
                    .line_height(1.4)
                    .color(theme.color(ThemeRole::Text))
                    .span(Span::new(self.issue.model_name.clone()).font_weight(FontWeight::SEMI_BOLD))
                    .span(Span::new(format!(
                        " can't read {things_text}. The agent would only get the file's name, so it can't tell what is in it."
                    ))),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .maybe_child(
                        self.issue
                            .suggested_model_id
                            .clone()
                            .zip(self.issue.suggested_model_name.clone())
                            .map(|(target_id, target_name)| SmallActionButton {
                                label: format!("Use {target_name}"),
                                tooltip: Some("Switch to a model that can read it".to_string()),
                                primary: true,
                                on_press: EventHandler::new(move |e: Event<PressEventData>| {
                                    e.stop_propagation();
                                    on_use.call(target_id.clone());
                                }),
                                key: DiffKey::None,
                            }),
                    )
                    .child(SmallActionButton {
                        label: "Pick…".to_string(),
                        tooltip: Some("Choose another model".to_string()),
                        primary: false,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            on_pick.call(());
                        }),
                        key: DiffKey::None,
                    })
                    .child(SmallActionButton {
                        label: "Send anyway".to_string(),
                        tooltip: None,
                        primary: false,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            on_anyway.call(());
                        }),
                        key: DiffKey::None,
                    }),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SmallActionButton {
    label: String,
    tooltip: Option<String>,
    primary: bool,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for SmallActionButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SmallActionButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let accent = theme.color(ThemeRole::Accent);
        let on_accent = match theme.scheme {
            ResolvedScheme::Light => Color::WHITE,
            ResolvedScheme::Dark => theme.color(ThemeRole::Ink),
        };
        let (bg, fg, base_border) = if self.primary {
            (
                fb.background(
                    accent,
                    mix_color(accent, Color::BLACK, 0.12),
                    mix_color(accent, Color::BLACK, 0.20),
                ),
                on_accent,
                Border::new(),
            )
        } else {
            (
                fb.background(
                    Color::TRANSPARENT,
                    theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color(),
                    theme.rgba(ThemeRole::Text).with_alpha(0.16).to_color(),
                ),
                theme.color(ThemeRole::Text),
                Border::new()
                    .width(0.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::Border)),
            )
        };
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, base_border);
        let on_press = self.on_press.clone();

        let btn = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label.clone())
            .padding(Gaps::new(3.0, 10.0, 3.0, 10.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(bg)
            .border(border)
            .scale(fb.scale())
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| on_press.call(e))
            .child(
                label()
                    .text(self.label.clone())
                    .font_size(12.0)
                    .font_weight(if self.primary {
                        FontWeight::SEMI_BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .color(fg)
                    .max_lines(1),
            );

        if let Some(tip) = &self.tooltip {
            KitTooltip::new(tip.clone()).child(btn).into_element()
        } else {
            btn.into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

// ============================================================================
// Composer Bar Controls (`AP-12`, `DC-01`)
// ============================================================================

#[derive(Clone, PartialEq)]
struct ComposerAttachButton {
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for ComposerAttachButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ComposerAttachButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let bg = fb.background(
            Color::TRANSPARENT,
            theme.rgba(ThemeRole::Text).with_alpha(0.18).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.24).to_color(),
        );
        let icon_col = mix_color(
            theme.color(ThemeRole::TextMuted),
            theme.color(ThemeRole::Text),
            fb.hover_progress(),
        );
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let on_press = self.on_press.clone();

        KitTooltip::new("Attach file").child(
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Attach file")
                .width(Size::px(32.0))
                .height(Size::px(32.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(bg)
                .border(border)
                .scale(fb.scale())
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| on_press.call(e))
                .child(paperclip_icon(18.0, icon_col)),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct ComposerModelChip {
    agent_id: String,
    model_name: String,
    effort_label: Option<String>,
    text_only: bool,
    expanded: bool,
    on_sized: EventHandler<(f32, f32, f32, f32)>,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for ComposerModelChip {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ComposerModelChip {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let expanded_bg = theme.rgba(ThemeRole::Text).with_alpha(0.11).to_color();
        let rest_bg = if self.expanded { expanded_bg } else { Color::TRANSPARENT };
        let bg = fb.background(
            rest_bg,
            expanded_bg,
            theme.rgba(ThemeRole::Text).with_alpha(0.18).to_color(),
        );
        let text_col = if self.expanded || fb.is_hovered() {
            theme.color(ThemeRole::Text)
        } else {
            theme.rgba(ThemeRole::Text).with_alpha(0.90).to_color()
        };
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let tip = if self.text_only {
            "This model reads text only, not pictures (Pick a model)"
        } else {
            "Pick a model"
        };
        let on_sized = self.on_sized.clone();
        let on_press = self.on_press.clone();

        KitTooltip::new(tip).child(
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Select model and effort")
                .height(Size::px(32.0))
                .max_width(Size::px(220.0))
                .padding(Gaps::new(0.0, 10.0, 0.0, 7.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(bg)
                .border(border)
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(6.0)
                .cursor(CursorIcon::Pointer)
                .on_sized(move |e: Event<SizedEventData>| {
                    on_sized.call((e.area.min_x(), e.area.min_y(), e.area.width(), e.area.height()));
                })
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| on_press.call(e))
                .child(agent_badge_emblem(&self.agent_id, 16.0, 10.5, 999.0, false))
                .child(
                    RollingText::new(self.model_name.clone())
                        .font_size(12.0)
                        .font_weight(FontWeight::MEDIUM)
                        .color(text_col),
                )
                .maybe_child(self.effort_label.as_ref().map(|eff| {
                    RollingText::new(eff.clone())
                        .font_size(12.0)
                        .font_weight(FontWeight::MEDIUM)
                        .color(theme.color(ThemeRole::TextMuted))
                }))
                .maybe_child(self.text_only.then(|| {
                    rect()
                        .width(Size::px(6.0))
                        .height(Size::px(6.0))
                        .corner_radius(CornerRadius::new_all(999.0))
                        .background(theme.color(ThemeRole::Warning))
                })),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct ComposerInstructionsButton {
    active: bool,
    active_count: usize,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for ComposerInstructionsButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ComposerInstructionsButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let accent = theme.color(ThemeRole::Accent);
        let on_accent = match theme.scheme {
            ResolvedScheme::Light => Color::WHITE,
            ResolvedScheme::Dark => theme.color(ThemeRole::Ink),
        };

        // AP-12: Active ONLY when enabled_instruction_count > 0; solid accent + ink/white when active, 24%->36% at rest.
        let (bg, icon_role) = if self.active {
            let hover = mix_color(accent, theme.color(ThemeRole::Text), 0.15);
            let press = mix_color(accent, Color::BLACK, 0.12);
            (fb.background(accent, hover, press), ThemeRole::TextOnAccent)
        } else {
            let rest = theme.rgba(ThemeRole::Accent).with_alpha(0.24).to_color();
            let hover = theme.rgba(ThemeRole::Accent).with_alpha(0.36).to_color();
            let press = theme.rgba(ThemeRole::Accent).with_alpha(0.46).to_color();
            (fb.background(rest, hover, press), ThemeRole::Accent)
        };
        let _ = on_accent;
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let tip = if self.active_count > 0 {
            format!("Agent instructions \u{00b7} {} on", self.active_count)
        } else {
            "Agent instructions".to_string()
        };
        let on_press = self.on_press.clone();

        KitTooltip::new(tip.clone()).child(
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(tip)
                .width(Size::px(32.0))
                .height(Size::px(32.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(bg)
                .border(border)
                .scale(fb.scale())
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| on_press.call(e))
                .child(Icon::new("sparkles").size(16.0).role(icon_role)),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct ComposerMicButton {
    phase: AgentDictationPhase,
    static_snapshot: bool,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for ComposerMicButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ComposerMicButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let is_connecting = self.phase == AgentDictationPhase::Connecting && !self.static_snapshot;
        let is_listening = self.phase == AgentDictationPhase::Listening && !self.static_snapshot;

        // DC-01: 1.2s breathing opacity while connecting (`zendia-mic-wait`).
        let wait_anim = use_animation_with_dependencies(&is_connecting, |conf, active| {
            if *active {
                conf.on_creation(OnCreation::Run);
                conf.on_change(OnChange::Rerun);
                conf.on_finish(OnFinish::reverse());
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(1.0, 0.55).duration(motion::MIC_WAIT).ease(Ease::InOut)
        });

        // DC-01: 1.6s expanding accent pulse ring while recording (`zendia-mic-pulse`).
        let pulse_anim = use_animation_with_dependencies(&is_listening, |conf, active| {
            if *active {
                conf.on_creation(OnCreation::Run);
                conf.on_change(OnChange::Rerun);
                conf.on_finish(OnFinish::restart());
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(0.0, 1.0).duration(motion::MIC_PULSE).ease(Ease::Out)
        });

        let accent = theme.color(ThemeRole::Accent);
        let on_accent = match theme.scheme {
            ResolvedScheme::Light => Color::WHITE,
            ResolvedScheme::Dark => theme.color(ThemeRole::Ink),
        };

        let (bg, icon_col, tip) = match self.phase {
            AgentDictationPhase::Idle => {
                let hover_bg = theme.rgba(ThemeRole::Text).with_alpha(0.18).to_color();
                let press_bg = theme.rgba(ThemeRole::Text).with_alpha(0.24).to_color();
                let fg = mix_color(theme.color(ThemeRole::TextMuted), accent, fb.hover_progress());
                (
                    fb.background(Color::TRANSPARENT, hover_bg, press_bg),
                    fg,
                    "Dictate (hold Alt+Space)",
                )
            }
            AgentDictationPhase::Connecting => {
                let bg = theme.rgba(ThemeRole::Accent).with_alpha(0.24).to_color();
                (bg, accent, "Cancel dictation")
            }
            AgentDictationPhase::Listening => {
                let hover = mix_color(accent, theme.color(ThemeRole::Text), 0.15);
                let press = mix_color(accent, Color::BLACK, 0.12);
                (fb.background(accent, hover, press), on_accent, "Finish dictation")
            }
            AgentDictationPhase::Finishing => {
                let bg = theme.rgba(ThemeRole::Accent).with_alpha(0.36).to_color();
                (bg, accent, "Writing it down…")
            }
        };

        let opacity = if is_connecting {
            wait_anim.get().value().clamp(0.55, 1.0)
        } else {
            1.0
        };
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let on_press = self.on_press.clone();

        // Paint the 0..8px expanding pulse ring around the 32x32 mic button while listening.
        let pulse_ring = is_listening.then(|| {
            let p = pulse_anim.get().value().clamp(0.0, 1.0);
            let spread = 8.0 * p;
            let alpha = ((0.45 * (1.0 - p)) * 255.0).round() as u8;
            let sk_col = SkColor::from_argb(alpha, accent.r(), accent.g(), accent.b());
            let ring_cb = RenderCallback::new(move |ctx| {
                if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                    let cx = ctx.size.width * 0.5;
                    let cy = ctx.size.height * 0.5;
                    let mut paint = Paint::default();
                    paint.set_anti_alias(true);
                    paint.set_style(PaintStyle::Fill);
                    paint.set_color(sk_col);
                    canvas.draw_circle((cx, cy), 16.0 + spread, &paint);
                }
            });
            rect()
                .position(Position::new_absolute().top(-8.0).left(-8.0))
                .width(Size::px(48.0))
                .height(Size::px(48.0))
                .background(Fill::Callback(ring_cb))
        });

        const MIC_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="2" width="6" height="12" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v4"/></svg>"#;

        KitTooltip::new(tip).child(
            rect()
                .width(Size::px(32.0))
                .height(Size::px(32.0))
                .maybe_child(pulse_ring)
                .child(
                    rect()
                        .a11y_id(a11y_id)
                        .a11y_focusable(true)
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt(tip)
                        .width(Size::px(32.0))
                        .height(Size::px(32.0))
                        .corner_radius(CornerRadius::new_all(999.0))
                        .background(bg)
                        .border(border)
                        .opacity(opacity)
                        .scale(fb.scale())
                        .center()
                        .cursor(CursorIcon::Pointer)
                        .on_pointer_enter(fb.on_pointer_enter())
                        .on_pointer_leave(fb.on_pointer_leave())
                        .on_pointer_down(fb.on_pointer_down())
                        .on_global_pointer_up(fb.on_pointer_up())
                        .on_press(move |e: Event<PressEventData>| on_press.call(e))
                        .child(
                            SvgViewer::new(("zendia-agent-mic", Bytes::from_static(MIC_SVG)))
                                .width(Size::px(18.0))
                                .height(Size::px(18.0))
                                .color(icon_col),
                        ),
                ),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct ComposerSendButton {
    is_stop: bool,
    is_steering: bool,
    is_dictating: bool,
    send_after_dictation: bool,
    enabled: bool,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for ComposerSendButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ComposerSendButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(self.enabled);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let accent = theme.color(ThemeRole::Accent);
        let on_accent = match theme.scheme {
            ResolvedScheme::Light => Color::WHITE,
            ResolvedScheme::Dark => theme.color(ThemeRole::Ink),
        };

        let (bg, opacity) = if self.enabled {
            let hover = mix_color(accent, theme.color(ThemeRole::Text), 0.15);
            let press = mix_color(accent, Color::BLACK, 0.12);
            (fb.background(accent, hover, press), 1.0)
        } else {
            (theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color(), 0.70)
        };
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let (tip, shortcut) = if self.is_stop {
            ("Stop", Some("Ctrl+C"))
        } else if self.is_steering {
            ("Steer: tell it now, while it works", Some("Enter"))
        } else if self.is_dictating {
            if self.send_after_dictation {
                ("Sends once the dictation is written down", None)
            } else {
                ("Finish dictation and send", Some("Enter"))
            }
        } else {
            ("Send", Some("Enter"))
        };

        let on_press = self.on_press.clone();
        let enabled = self.enabled;
        let inner: Element = if self.is_stop {
            rect()
                .width(Size::px(11.0))
                .height(Size::px(11.0))
                .corner_radius(CornerRadius::new_all(2.0))
                .background(on_accent)
                .into_element()
        } else {
            Icon::new("arrow-up")
                .size(16.0)
                .role(if self.enabled {
                    ThemeRole::TextOnAccent
                } else {
                    ThemeRole::TextMuted
                })
                .into_element()
        };

        KitTooltip::new(tip).maybe_shortcut(shortcut.map(str::to_string)).child(
            rect()
                .a11y_id(a11y_id)
                .a11y_enabled(enabled)
                .a11y_focusable(enabled)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(tip)
                .width(Size::px(32.0))
                .height(Size::px(32.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(bg)
                .border(border)
                .opacity(opacity)
                .scale(fb.scale())
                .center()
                .cursor(if enabled {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                })
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| {
                    if enabled {
                        on_press.call(e);
                    }
                })
                .child(inner),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
