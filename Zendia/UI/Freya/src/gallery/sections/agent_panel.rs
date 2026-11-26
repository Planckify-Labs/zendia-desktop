/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 6 gallery section for Side panels and Agent: Float/Slide/Split dock host, empty state, rich Markdown +
//! interactive table + confirmation conversation, working motion ring & rolling text, closed-panel pill, Slide edge
//! handle, Model & Effort card (slider + searchable list), Instructions overlay (empty + populated), declared UI card
//! + media-compatibility warning + CAPTCHA button, and AgentServer-down state.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::core_view::use_core;
use crate::ui::kit::{KitChip, Pill};
use crate::ui::page::{PageArea, PaneMode};
use crate::ui::panels::try_use_dock_state;
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::agent::{
    AGENT_PANEL_ID, AgentInstruction, AgentSelection, AttachmentKind, InstructionAttachment, SidePanelPrefs,
    SteerStatus,
};
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{SidePanelId, SidePanelLayout, SidePanelSide};
use zendia_browser_core::model::{
    AgentId, BrowserModel, Conversation, ConversationId, ConversationStatus, ConversationStep, ModelEffort, ModelId,
    SIDEBAR_DEFAULT_WIDTH, ServiceId, ServiceStatus, SidebarSide, SpaceId, StepId, StepKind, StepStatus, TabId,
    ToolbarLayout,
};
use zendia_browser_core::stub::{build_rich_sample_conversation, sample_declared_agent_page, sample_model};
use zendia_module_api::ui::UiTone;

#[derive(Clone)]
pub struct AgentPanelSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
    key: DiffKey,
}

impl KeyExt for AgentPanelSection {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl PartialEq for AgentPanelSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

impl AgentPanelSection {
    pub fn new(state_id: impl Into<String>, gallery_core: Arc<GalleryCore>) -> Self {
        Self {
            state_id: state_id.into(),
            gallery_core,
            key: DiffKey::None,
        }
    }
}

fn set_agent_server_status(model: &mut BrowserModel, status: ServiceStatus) {
    if let Some(svc) = model.services.iter_mut().find(|s| s.service == ServiceId::AgentServer) {
        svc.status = status;
    }
}

fn build_fixture_model(state_id: &str) -> BrowserModel {
    let mut model = sample_model(true);
    model.layout.toolbar_layout = ToolbarLayout::Single;
    model.layout.sidebar_side = SidebarSide::Left;
    model.layout.sidebar_expanded = true;
    model.layout.sidebar_width = SIDEBAR_DEFAULT_WIDTH;
    model.layout.compact_mode = false;
    model.active_space = Some(SpaceId(1));
    model.active_tab = Some(TabId(6));
    model.split_groups.clear();
    model.glance = None;
    model.conversations.clear();
    model.active_conversations.clear();
    model.agent_instructions.clear();
    model.tab_captcha_findings.clear();
    model.default_agent_id = Some(AgentId::new("claude-code"));
    if let Some(claude) = model.agents.iter_mut().find(|a| a.id.0 == "claude-code") {
        claude.name = "Claude Code".to_string();
    }

    let panel_id = SidePanelId::new(AGENT_PANEL_ID);
    let prefs = SidePanelPrefs {
        width: 420.0,
        side: SidePanelSide::Right,
        layout: match state_id {
            "split-layout" => SidePanelLayout::Split,
            "slide-handle" => SidePanelLayout::Slide,
            _ => SidePanelLayout::Float,
        },
        ..Default::default()
    };
    model.side_panel_prefs.insert(panel_id, prefs);

    // Default to healthy AgentServer except in `service-down`.
    if state_id == "service-down" {
        set_agent_server_status(
            &mut model,
            ServiceStatus::Stopped {
                reason: Some("AgentServer exited unexpectedly (code 1). Previous trace preserved.".to_string()),
            },
        );
    } else {
        set_agent_server_status(&mut model, ServiceStatus::Running);
    }

    match state_id {
        "empty-float" => {
            // Empty idle conversation so the empty state renders cleanly.
        }
        "working-state" => {
            model.agent_selections.insert(
                AgentId::new("codex"),
                AgentSelection {
                    model_id: Some(ModelId::new("gpt-5")),
                    effort: Some(ModelEffort::High),
                    context_id: None,
                },
            );
            let conv = Conversation {
                id: ConversationId(10),
                agent_id: AgentId::new("codex"),
                tab_id: Some(TabId(6)),
                title: "Audit all IPC endpoints".to_string(),
                model_id: ModelId::new("gpt-5"),
                status: ConversationStatus::Working,
                steps: vec![
                    ConversationStep::simple(
                        StepId(101),
                        StepKind::UserPrompt,
                        "Audit all IPC endpoints in Services/Hub",
                        StepStatus::Completed,
                    ),
                    ConversationStep {
                        id: StepId(102),
                        kind: StepKind::ToolCall {
                            tool: "workspace.search".to_string(),
                        },
                        summary: "Scanning HubClient.ipc definitions".to_string(),
                        detail: None,
                        status: StepStatus::Running,
                        files: Vec::new(),
                        steer: None,
                        streaming: false,
                    },
                ],
                unseen: false,
                elapsed_secs: 1,
            };
            model.conversations.push(conv);
            model.active_conversations.insert(TabId(6), ConversationId(10));
        }
        "closed-pill" | "slide-handle" => {
            let mut conv = Conversation {
                id: ConversationId(10),
                agent_id: AgentId::new("claude-code"),
                tab_id: Some(TabId(6)),
                title: "Analyze architecture docs".to_string(),
                model_id: ModelId::new("claude-sonnet-4"),
                status: ConversationStatus::Working,
                steps: vec![
                    ConversationStep::simple(
                        StepId(101),
                        StepKind::UserPrompt,
                        "Audit the side panel dock layout and check split-mode fallback rules.",
                        StepStatus::Completed,
                    ),
                    ConversationStep {
                        id: StepId(102),
                        kind: StepKind::UserPrompt,
                        summary: "Also include Slide handle hover timing.".to_string(),
                        detail: None,
                        status: StepStatus::Pending,
                        files: Vec::new(),
                        steer: Some(SteerStatus::Queued),
                        streaming: false,
                    },
                ],
                unseen: state_id == "slide-handle",
                elapsed_secs: 12,
            };
            if state_id == "slide-handle" {
                conv.status = ConversationStatus::Finished;
                conv.unseen = true;
            }
            model.conversations.push(conv);
            model.active_conversations.insert(TabId(6), ConversationId(10));
        }
        "model-card" | "model-card-list" => {
            let conv = Conversation {
                id: ConversationId(15),
                agent_id: AgentId::new("claude-code"),
                tab_id: Some(TabId(6)),
                title: "Command registry routing".to_string(),
                model_id: ModelId::new("claude-sonnet-4"),
                status: ConversationStatus::Finished,
                steps: vec![
                    ConversationStep::simple(
                        StepId(151),
                        StepKind::UserPrompt,
                        "Explain how the command registry routes shortcuts and plugins.",
                        StepStatus::Completed,
                    ),
                    ConversationStep::simple(
                        StepId(152),
                        StepKind::Response,
                        "Every caller — keyboard, toolbar, palette, plugin, or agent — invokes a named command through `BrowserCore`.",
                        StepStatus::Completed,
                    ),
                ],
                unseen: false,
                elapsed_secs: 2,
            };
            model.conversations.push(conv);
            model.active_conversations.insert(TabId(6), ConversationId(15));
        }
        "instructions-populated" => {
            let conv = build_rich_sample_conversation(ConversationId(20), 200, Some(TabId(6)));
            model.conversations.push(conv);
            model.active_conversations.insert(TabId(6), ConversationId(20));
            model.agent_instructions = vec![
                AgentInstruction {
                    id: zendia_browser_core::agent::InstructionId(1),
                    enabled: true,
                    title: "Concise technical answers".to_string(),
                    text: "Prefer bullet points and concrete file paths. Avoid filler introductions.".to_string(),
                    file: None,
                },
                AgentInstruction {
                    id: zendia_browser_core::agent::InstructionId(2),
                    enabled: false,
                    title: "Ask before submitting forms".to_string(),
                    text: "Always request confirmation before clicking any submit, purchase, or delete button."
                        .to_string(),
                    file: None,
                },
            ];
        }
        "declared-page-and-media" => {
            // Select Haiku (no image/PDF support) so the staged PNG triggers the media compatibility warning.
            model.agent_selections.insert(
                AgentId::new("claude-code"),
                AgentSelection {
                    model_id: Some(ModelId::new("claude-haiku-4-5")),
                    effort: Some(ModelEffort::Medium),
                    context_id: None,
                },
            );
            model
                .tab_captcha_findings
                .insert(TabId(6), "Turnstile challenge detected on login form".to_string());
            let conv = Conversation {
                id: ConversationId(30),
                agent_id: AgentId::new("claude-code"),
                tab_id: Some(TabId(6)),
                title: "Declared UI & page summary".to_string(),
                model_id: ModelId::new("claude-haiku-4-5"),
                status: ConversationStatus::Finished,
                steps: vec![
                    ConversationStep {
                        id: StepId(301),
                        kind: StepKind::UserPrompt,
                        summary: "Build a quick action card for this page and check the screenshot.".to_string(),
                        detail: None,
                        status: StepStatus::Completed,
                        files: vec!["viewport.png".to_string()],
                        steer: None,
                        streaming: false,
                    },
                    ConversationStep::simple(
                        StepId(302),
                        StepKind::DeclaredPage(sample_declared_agent_page()),
                        "Phase 6 Verification Card",
                        StepStatus::Completed,
                    ),
                ],
                unseen: false,
                elapsed_secs: 3,
            };
            model.conversations.push(conv);
            model.active_conversations.insert(TabId(6), ConversationId(30));
        }
        _ => {
            // conversation-float, split-layout, model-card, model-card-list, instructions-empty, service-down
            let conv = build_rich_sample_conversation(ConversationId(1), 10, Some(TabId(6)));
            // Also include a second background chat on the same tab so the header's other-chats stack is visible.
            let second = Conversation {
                id: ConversationId(2),
                agent_id: AgentId::new("codex"),
                tab_id: Some(TabId(6)),
                title: "Token audit".to_string(),
                model_id: ModelId::new("gpt-5"),
                status: ConversationStatus::Finished,
                steps: vec![ConversationStep::simple(
                    StepId(99),
                    StepKind::UserPrompt,
                    "Verify Tokyonight contrast tokens",
                    StepStatus::Completed,
                )],
                unseen: true,
                elapsed_secs: 4,
            };
            model.conversations.push(conv);
            model.conversations.push(second);
            model.active_conversations.insert(TabId(6), ConversationId(1));
        }
    }

    model
}

impl Component for AgentPanelSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let location = use_state(|| "https://zendia.local/docs".to_string());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut applied_state = use_state(String::new);

        if *applied_state.peek() != self.state_id {
            applied_state.set(self.state_id.clone());
            let fixture = build_fixture_model(&self.state_id);
            self.gallery_core.stub().replace_model(fixture);
            if let Some(mut ds) = dock_state {
                let agent_id = SidePanelId::new(AGENT_PANEL_ID);
                let should_open = !matches!(self.state_id.as_str(), "closed-pill" | "slide-handle");
                if should_open {
                    ds.open_panels.set(vec![agent_id.clone()]);
                    ds.focused_panel.set(Some(agent_id));
                } else {
                    ds.open_panels.set(Vec::new());
                    ds.focused_panel.set(None);
                }
                ds.model_card_open
                    .set(matches!(self.state_id.as_str(), "model-card" | "model-card-list"));
                ds.model_card_list_view.set(self.state_id == "model-card-list");
                ds.instructions_open.set(matches!(
                    self.state_id.as_str(),
                    "instructions-empty" | "instructions-populated"
                ));
                ds.pinned_ring_phase.set(Some(0.32));
                ds.composer_text.set(String::new());
                if self.state_id == "declared-page-and-media" {
                    ds.staged_attachments.set(vec![InstructionAttachment {
                        name: "capture-1.png".to_string(),
                        path: "/tmp/capture-1.png".to_string(),
                        size_bytes: 184_320,
                        kind: AttachmentKind::Image,
                    }]);
                } else {
                    ds.staged_attachments.set(Vec::new());
                }
            }
        }

        let core_toggle = core.clone();
        let core_sample = core.clone();
        let core_service = core.clone();
        let is_open = dock_state.is_some_and(|ds| ds.is_agent_open());
        let status_label = match self.state_id.as_str() {
            "split-layout" => "Split layout · beside page card",
            "slide-handle" => "Slide layout · closed edge handle",
            "closed-pill" => "Closed · working pill visible",
            "working-state" => "Live working ring · streaming reply",
            "model-card" => "Model card · effort slider + context",
            "model-card-list" => "Model card · searchable model list",
            "instructions-empty" => "Instructions overlay · empty",
            "instructions-populated" => "Instructions overlay · cards + file",
            "declared-page-and-media" => "Declared UI + media warning + CAPTCHA",
            "service-down" => "AgentServer stopped · trace preserved",
            "empty-float" => "Float layout · empty state",
            _ => "Float layout · Markdown + table + confirm",
        };

        let controls = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .main_align(Alignment::SpaceBetween)
            .spacing(8.0)
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .child(Pill::new(status_label).tone(UiTone::Accent))
                    .child(
                        KitChip::new(if is_open { "Close panel" } else { "Open panel" })
                            .icon("chat")
                            .selected(is_open)
                            .on_press(move |_| {
                                core_toggle.invoke_or_log(Command::AgentToggle);
                            }),
                    )
                    .child(KitChip::new("Load sample chat").icon("sparkles").on_press(move |_| {
                        core_sample.invoke_or_log(Command::DebugSampleAgent);
                    }))
                    .child(KitChip::new("Toggle AgentServer").icon("warning").on_press(move |_| {
                        core_service.invoke_or_log(Command::DebugToggleAgentService);
                    })),
            );

        let shell = rect()
            .width(Size::fill())
            .height(Size::px(672.0))
            .corner_radius(CornerRadius::new_all(theme.radii.card))
            .overflow(Overflow::Clip)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .child(
                WindowShell::new(location, location_a11y_id, PageArea::new(PaneMode::Placeholder))
                    .interactive_window_controls(false),
            );

        rect().width(Size::fill()).spacing(12.0).child(controls).child(shell)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
