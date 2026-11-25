/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 7 gallery section: cross-window Agents roster panel (`zendia.agents`) and Agents button badge.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::core_view::use_core;
use crate::ui::kit::{KitChip, Pill};
use crate::ui::page::{PageArea, PaneMode};
use crate::ui::panels::try_use_dock_state;
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::BrowserCore;
use zendia_browser_core::agent::{AGENTS_PANEL_DEFAULT_WIDTH, AGENTS_PANEL_ID, SidePanelPrefs};
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{SidePanelId, SidePanelLayout, SidePanelSide};
use zendia_browser_core::model::{
    Agent, AgentId, BrowserModel, Conversation, ConversationId, ConversationStatus, ConversationStep, ModelId,
    ServiceId, ServiceState, ServiceStatus, StepId, StepKind, StepStatus, Tab, TabId, TabKind,
};
use zendia_browser_core::stub::sample_model;
use zendia_module_api::ui::UiTone;

/// Gallery section showcasing all Phase 7 Agents panel states.
#[derive(Clone)]
pub struct AgentsPanelSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
    key: DiffKey,
}

impl PartialEq for AgentsPanelSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

impl KeyExt for AgentsPanelSection {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl AgentsPanelSection {
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
    } else {
        model.services.push(ServiceState {
            service: ServiceId::AgentServer,
            status,
        });
    }
}

fn build_fixture_model(state_id: &str) -> BrowserModel {
    let mut model = sample_model(true);
    model.conversations.clear();
    model.active_conversations.clear();
    model.default_agent_id = Some(AgentId::new("claude-code"));

    if let Some(claude) = model.agents.iter_mut().find(|a| a.id.0 == "claude-code") {
        claude.name = "Claude Code".to_string();
    }
    if !model.agents.iter().any(|a| a.id.0 == "antigravity") {
        model.agents.push(Agent {
            id: AgentId::new("antigravity"),
            name: "Antigravity".to_string(),
            description: "Antigravity coding agent".to_string(),
            icon: "antigravity".to_string(),
            accent_rgb: (66, 133, 244),
            available: true,
        });
    }

    // Seed clean reference-matching tab titles for the 4 roster items.
    let space_id = model.active_space;
    model.tabs = vec![
        Tab {
            id: TabId(10),
            view_id: None,
            space_id,
            folder_id: None,
            split_group_id: None,
            kind: TabKind::Normal,
            title: "Releases — Zendia".to_string(),
            custom_title: None,
            url: "https://zendia.local/releases".to_string(),
            favicon: None,
            is_loading: false,
            can_navigate_back: false,
            can_navigate_forward: false,
            is_audible: false,
            is_muted: false,
            closing: false,
        },
        Tab {
            id: TabId(11),
            view_id: None,
            space_id,
            folder_id: None,
            split_group_id: None,
            kind: TabKind::Normal,
            title: "Design Tokens — Zendia".to_string(),
            custom_title: None,
            url: "https://zendia.local/tokens".to_string(),
            favicon: None,
            is_loading: false,
            can_navigate_back: false,
            can_navigate_forward: false,
            is_audible: false,
            is_muted: false,
            closing: false,
        },
        Tab {
            id: TabId(12),
            view_id: None,
            space_id,
            folder_id: None,
            split_group_id: None,
            kind: TabKind::Normal,
            title: "Architecture — Zendia".to_string(),
            custom_title: None,
            url: "https://zendia.local/architecture".to_string(),
            favicon: None,
            is_loading: false,
            can_navigate_back: false,
            can_navigate_forward: false,
            is_audible: false,
            is_muted: false,
            closing: false,
        },
        Tab {
            id: TabId(13),
            view_id: None,
            space_id,
            folder_id: None,
            split_group_id: None,
            kind: TabKind::Normal,
            title: "Zendia Documentation".to_string(),
            custom_title: None,
            url: "https://zendia.local/docs".to_string(),
            favicon: None,
            is_loading: false,
            can_navigate_back: false,
            can_navigate_forward: false,
            is_audible: false,
            is_muted: false,
            closing: false,
        },
    ];
    model.active_tab = Some(TabId(13));

    let prefs = SidePanelPrefs {
        width: AGENTS_PANEL_DEFAULT_WIDTH,
        side: SidePanelSide::Left,
        layout: if state_id == "split-layout" {
            SidePanelLayout::Split
        } else {
            SidePanelLayout::Float
        },
        ..Default::default()
    };
    model.side_panel_prefs.insert(SidePanelId::new(AGENTS_PANEL_ID), prefs);

    if state_id == "service-down" {
        set_agent_server_status(
            &mut model,
            ServiceStatus::Stopped {
                reason: Some("AgentServer exited unexpectedly (code 1). Roster preserved.".to_string()),
            },
        );
    } else {
        set_agent_server_status(&mut model, ServiceStatus::Running);
    }

    if state_id != "empty" {
        model.conversations = vec![
            Conversation {
                id: ConversationId(101),
                agent_id: AgentId::new("claude-code"),
                tab_id: Some(TabId(13)),
                title: "Summarize the architecture principles".to_string(),
                model_id: ModelId::new("claude-sonnet-4"),
                status: ConversationStatus::WaitingForUser,
                steps: vec![
                    ConversationStep::simple(
                        StepId(1),
                        StepKind::UserPrompt,
                        "Summarize the architecture principles",
                        StepStatus::Completed,
                    ),
                    ConversationStep::simple(
                        StepId(2),
                        StepKind::Confirmation {
                            prompt: "Allow Claude Code to open `https://ladybird.org` in a new tab?".to_string(),
                            resolved: None,
                        },
                        "Allow Claude Code to open `https://ladybird.org` in a new tab?",
                        StepStatus::WaitingForUser,
                    ),
                ],
                unseen: true,
                elapsed_secs: 12,
            },
            Conversation {
                id: ConversationId(102),
                agent_id: AgentId::new("codex"),
                tab_id: Some(TabId(12)),
                title: "Audit all IPC endpoints in Services/Hub".to_string(),
                model_id: ModelId::new("gpt-5"),
                status: ConversationStatus::Working,
                steps: vec![
                    ConversationStep::simple(
                        StepId(3),
                        StepKind::UserPrompt,
                        "Audit all IPC endpoints in Services/Hub",
                        StepStatus::Completed,
                    ),
                    ConversationStep::simple(
                        StepId(4),
                        StepKind::ToolCall {
                            tool: "workspace.search".to_string(),
                        },
                        "Scanning HubClient.ipc definitions",
                        StepStatus::Running,
                    ),
                ],
                unseen: false,
                elapsed_secs: 49,
            },
            Conversation {
                id: ConversationId(103),
                agent_id: AgentId::new("antigravity"),
                tab_id: Some(TabId(11)),
                title: "Verify Tokyonight light glass contrast".to_string(),
                model_id: ModelId::new("claude-sonnet-4"),
                status: ConversationStatus::Finished,
                steps: vec![
                    ConversationStep::simple(
                        StepId(5),
                        StepKind::UserPrompt,
                        "Verify Tokyonight light glass contrast",
                        StepStatus::Completed,
                    ),
                    ConversationStep::simple(
                        StepId(6),
                        StepKind::Response,
                        "All 14 roles exceed WCAG AA 4.5:1 contrast ratio.",
                        StepStatus::Completed,
                    ),
                ],
                unseen: true,
                elapsed_secs: 7,
            },
            Conversation {
                id: ConversationId(104),
                agent_id: AgentId::new("claude-code"),
                tab_id: Some(TabId(10)),
                title: "Draft release notes for Phase 0".to_string(),
                model_id: ModelId::new("claude-sonnet-4"),
                status: ConversationStatus::Finished,
                steps: vec![
                    ConversationStep::simple(
                        StepId(7),
                        StepKind::UserPrompt,
                        "Draft release notes for Phase 0",
                        StepStatus::Completed,
                    ),
                    ConversationStep::simple(
                        StepId(8),
                        StepKind::Response,
                        "Drafted 11 bullet points covering BrowserCore, Keymap, and Gallery.",
                        StepStatus::Completed,
                    ),
                ],
                unseen: false,
                elapsed_secs: 7,
            },
        ];
    }

    model
}

impl Component for AgentsPanelSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let location = use_state(|| "https://zendia.local/docs".to_string());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut applied_state = use_state(String::new);

        if *applied_state.peek() != self.state_id {
            applied_state.set(self.state_id.clone());
            let mut fixture = build_fixture_model(&self.state_id);
            fixture.theme = self.gallery_core.model().theme;
            self.gallery_core.stub().replace_model(fixture);
            if let Some(mut ds) = dock_state {
                let agents_id = SidePanelId::new(AGENTS_PANEL_ID);
                ds.open_panels.set(vec![agents_id.clone()]);
                ds.focused_panel.set(Some(agents_id));
                ds.model_card_open.set(false);
                ds.instructions_open.set(false);
                ds.webchat_accounts_open.set(false);
                ds.webchat_add_site_open.set(false);
                ds.pinned_ring_phase.set(Some(0.32));
            }
        }

        let core_toggle = core.clone();
        let core_service = core.clone();
        let is_open = dock_state.is_some_and(|ds| ds.is_agents_open());
        let status_label = match self.state_id.as_str() {
            "split-layout" => "Split layout · left roster beside page",
            "empty" => "Empty roster · no agents at work",
            "service-down" => "AgentServer down · roster preserved",
            _ => "Float layout · 4 groups + inline permission + badge",
        };

        let controls = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .child(Pill::new(status_label).tone(UiTone::Accent))
            .child(
                KitChip::new(if is_open { "Close Agents" } else { "Open Agents" })
                    .icon("agents")
                    .selected(is_open)
                    .on_press(move |_| {
                        core_toggle.invoke_or_log(Command::AgentList);
                    }),
            )
            .child(KitChip::new("Toggle AgentServer").icon("warning").on_press(move |_| {
                core_service.invoke_or_log(Command::DebugToggleAgentService);
            }));

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
