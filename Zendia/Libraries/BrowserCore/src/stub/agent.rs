/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub backend handling for side panel dock preferences, agent sessions, conversations, confirmations, models, and
//! instructions.

use super::StubBackend;
use crate::agent::{
    AgentDeclaredPage, AgentInstruction, AgentTable, AgentTableRow, InstructionAttachment, InstructionId,
    SidePanelPrefs, SteerStatus,
};
use crate::commands::{Command, CommandError, CommandResult};
use crate::contributions::{SidePanelId, SidePanelLayout, SidePanelSide};
use crate::events::{Event, ViewRequest};
use crate::model::{
    AgentId, Conversation, ConversationId, ConversationStatus, ConversationStep, ModelEffort, ModelId, ServiceId,
    ServiceState, ServiceStatus, StepId, StepKind, StepStatus, TabId,
};
use zendia_module_api::ui::{UiAction, UiNode, UiRole, UiTone};

/// Built-in panel identifier for the single-tab AI agent side panel.
pub const AGENT_PANEL_ID: &str = "zendia.agent";
/// Built-in panel identifier for the cross-window agents roster panel.
pub const AGENTS_PANEL_ID: &str = "zendia.agents";

impl StubBackend {
    pub(super) fn handle_panel_or_agent_command(&self, command: Command) -> CommandResult {
        match command {
            Command::PanelOpen { panel_id, focus } => self.handle_panel_open(panel_id, focus),
            Command::PanelClose { panel_id } => self.handle_panel_close(panel_id),
            Command::PanelSetLayout { panel_id, layout } => self.handle_panel_set_layout(panel_id, layout),
            Command::PanelSetSide { panel_id, side } => self.handle_panel_set_side(panel_id, side),
            Command::PanelSetWidth { panel_id, width } => self.handle_panel_set_width(panel_id, width),
            Command::PanelResetWidth { panel_id } => self.handle_panel_reset_width(panel_id),
            Command::AgentToggle => self.handle_panel_toggle(SidePanelId::new(AGENT_PANEL_ID)),
            Command::AgentAsk => self.handle_panel_open(SidePanelId::new(AGENT_PANEL_ID), true),
            Command::AgentSendWithFiles {
                conversation_id,
                agent_id,
                prompt,
                files,
            } => self.handle_agent_send_with_files(conversation_id, agent_id, prompt, files),
            Command::AgentStop { conversation_id } => self.handle_agent_stop(conversation_id),
            Command::AgentNew { tab_id } => self.handle_agent_new(tab_id),
            Command::AgentSelect { agent_id } => self.handle_agent_select(agent_id),
            Command::AgentCycle { direction } => self.handle_agent_cycle(direction),
            Command::AgentModel { agent_id, model_id } => self.handle_agent_model(agent_id, model_id),
            Command::AgentSetEffort { agent_id, effort } => self.handle_agent_set_effort(agent_id, effort),
            Command::AgentSetContext { agent_id, context_id } => self.handle_agent_set_context(agent_id, context_id),
            Command::AgentConfirm {
                conversation_id,
                step_id,
                allow,
            } => self.handle_agent_confirm(conversation_id, step_id, allow),
            Command::AgentSwitchConversation { conversation_id } => {
                self.handle_agent_switch_conversation(conversation_id)
            }
            Command::AgentMarkSeen { conversation_id } => self.handle_agent_mark_seen(conversation_id),
            Command::AgentOpenInstructions => {
                self.request_view(ViewRequest::PanelOpen {
                    panel_id: SidePanelId::new(AGENT_PANEL_ID),
                    focus: true,
                });
                self.request_view(ViewRequest::AgentInstructions);
                Ok(())
            }
            Command::AgentInstructionAdd => self.handle_instruction_add(),
            Command::AgentInstructionUpdate {
                id,
                enabled,
                title,
                text,
            } => self.handle_instruction_update(id, enabled, title, text),
            Command::AgentInstructionRemove { id } => self.handle_instruction_remove(id),
            Command::AgentInstructionSetFile { id, file } => self.handle_instruction_set_file(id, file),
            Command::AgentSummarizeCaptcha { tab_id } => self.handle_summarize_captcha(tab_id),
            Command::AgentLog { tab_id } => self.handle_agent_log(tab_id),
            Command::AgentList => self.handle_panel_toggle(SidePanelId::new(AGENTS_PANEL_ID)),
            Command::AgentDismiss { conversation_id } => self.handle_agent_dismiss(conversation_id),
            Command::DebugSampleAgent => self.handle_debug_sample_agent(),
            Command::DebugToggleAgentService => self.handle_debug_toggle_agent_service(),
            other => Err(CommandError::UnknownCommand(other.name().to_string())),
        }
    }

    fn ensure_known_panel(&self, panel_id: &SidePanelId, command: &str) -> CommandResult {
        let known = {
            let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state
                .model
                .contributions
                .side_panels
                .iter()
                .any(|panel| &panel.id == panel_id)
        };
        if !known {
            return Err(CommandError::TargetNotFound {
                command: command.to_string(),
                target: format!("panel {}", panel_id.as_str()),
            });
        }
        Ok(())
    }

    fn handle_panel_open(&self, panel_id: SidePanelId, focus: bool) -> CommandResult {
        self.ensure_known_panel(&panel_id, "panel.open")?;
        if panel_id.as_str() == AGENT_PANEL_ID {
            let updated_conv = {
                let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                let active_conv_id = state
                    .model
                    .active_tab
                    .and_then(|tab_id| state.model.active_conversations.get(&tab_id).copied());
                active_conv_id.and_then(|conv_id| {
                    let conv = state.model.conversations.iter_mut().find(|c| c.id == conv_id)?;
                    if conv.unseen {
                        conv.unseen = false;
                        Some(conv.clone())
                    } else {
                        None
                    }
                })
            };
            if let Some(conversation) = updated_conv {
                self.bus.publish(Event::ConversationUpdated { conversation });
            }
        }
        self.request_view(ViewRequest::PanelOpen { panel_id, focus });
        Ok(())
    }

    fn handle_panel_close(&self, panel_id: SidePanelId) -> CommandResult {
        self.ensure_known_panel(&panel_id, "panel.close")?;
        self.request_view(ViewRequest::PanelClose { panel_id });
        Ok(())
    }

    fn update_panel_prefs(
        &self,
        panel_id: SidePanelId,
        command: &str,
        mutate: impl FnOnce(&mut SidePanelPrefs),
    ) -> CommandResult {
        self.ensure_known_panel(&panel_id, command)?;
        let prefs = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let mut prefs = state.model.panel_prefs(&panel_id);
            mutate(&mut prefs);
            prefs.width = SidePanelPrefs::clamp_width_for(panel_id.as_str(), prefs.width);
            state.model.side_panel_prefs.insert(panel_id.clone(), prefs);
            prefs
        };
        self.bus.publish(Event::PanelPrefsChanged { panel_id, prefs });
        Ok(())
    }

    fn handle_panel_set_layout(&self, panel_id: SidePanelId, layout: SidePanelLayout) -> CommandResult {
        self.update_panel_prefs(panel_id, "panel.set_layout", |prefs| prefs.layout = layout)
    }

    fn handle_panel_set_side(&self, panel_id: SidePanelId, side: SidePanelSide) -> CommandResult {
        self.update_panel_prefs(panel_id, "panel.set_side", |prefs| prefs.side = side)
    }

    fn handle_panel_set_width(&self, panel_id: SidePanelId, width: f32) -> CommandResult {
        self.update_panel_prefs(panel_id, "panel.set_width", |prefs| prefs.width = width)
    }

    fn handle_panel_reset_width(&self, panel_id: SidePanelId) -> CommandResult {
        let default_width = SidePanelPrefs::default_width_for(panel_id.as_str());
        self.update_panel_prefs(panel_id, "panel.reset_width", |prefs| {
            prefs.width = default_width;
        })
    }

    pub(super) fn handle_agent_send_with_files(
        &self,
        conversation_id: Option<ConversationId>,
        agent_id: Option<AgentId>,
        prompt: String,
        files: Vec<String>,
    ) -> CommandResult {
        let trimmed = prompt.trim();
        if trimmed.is_empty() && files.is_empty() {
            return Err(CommandError::InvalidArgument {
                command: "agent.send".to_string(),
                message: "prompt must not be empty".to_string(),
            });
        }

        let (updated, active_changed) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let is_service_down = state
                .model
                .service_state(ServiceId::AgentServer)
                .is_some_and(|s| !matches!(s.status, ServiceStatus::Running));

            // Check if appending to an existing running conversation (steer).
            let target_conv_id = conversation_id.or_else(|| {
                state
                    .model
                    .active_tab
                    .and_then(|tab_id| state.model.active_conversation_for_tab(tab_id).map(|c| c.id))
            });

            if let Some(conv_id) = target_conv_id
                && let Some(pos) = state.model.conversations.iter().position(|c| c.id == conv_id)
            {
                let was_working = state.model.conversations[pos].status == ConversationStatus::Working;
                let user_step_id = StepId(state.next_step_id);
                state.next_step_id += 1;
                let mut user_step = ConversationStep::simple(
                    user_step_id,
                    StepKind::UserPrompt,
                    if trimmed.is_empty() {
                        "Inspect attached files".to_string()
                    } else {
                        trimmed.to_string()
                    },
                    StepStatus::Completed,
                );
                user_step.files = files;
                if was_working {
                    user_step.steer = Some(SteerStatus::Queued);
                    state.model.conversations[pos].steps.push(user_step);
                    (state.model.conversations[pos].clone(), None)
                } else if is_service_down {
                    state.model.conversations[pos].steps.push(user_step);
                    let err_id = StepId(state.next_step_id);
                    state.next_step_id += 1;
                    state.model.conversations[pos].steps.push(ConversationStep::simple(
                        err_id,
                        StepKind::Error,
                        "AgentServer is stopped. Restart the service to continue this run.",
                        StepStatus::Failed,
                    ));
                    state.model.conversations[pos].status = ConversationStatus::Stopped;
                    (state.model.conversations[pos].clone(), None)
                } else {
                    let tool_step_id = StepId(state.next_step_id);
                    state.next_step_id += 1;
                    let reply_step_id = StepId(state.next_step_id);
                    state.next_step_id += 1;

                    let tool_step = ConversationStep::simple(
                        tool_step_id,
                        StepKind::ToolCall {
                            tool: "page.snapshot".to_string(),
                        },
                        "Read active tab accessibility tree",
                        StepStatus::Completed,
                    );
                    let reply_step = ConversationStep::simple(
                        reply_step_id,
                        StepKind::Response,
                        format!(
                            "Stub response for **{prompt_text}**.\n\n- Captured active tab context via `BrowserCore`\n- AgentServer is running in stub mode until step 9 lands.",
                            prompt_text = if trimmed.is_empty() {
                                "your attachments"
                            } else {
                                trimmed
                            }
                        ),
                        StepStatus::Completed,
                    );
                    let conv = &mut state.model.conversations[pos];
                    conv.steps.push(user_step);
                    conv.steps.push(tool_step);
                    conv.steps.push(reply_step);
                    conv.status = ConversationStatus::Finished;
                    conv.unseen = false;
                    (conv.clone(), None)
                }
            } else {
                if let Some(cid) = conversation_id {
                    return Err(CommandError::TargetNotFound {
                        command: "agent.send".to_string(),
                        target: format!("conversation {}", cid.0),
                    });
                }
                let chosen_agent = agent_id
                    .or_else(|| state.model.default_agent().map(|a| a.id.clone()))
                    .or_else(|| state.model.agents.first().map(|a| a.id.clone()))
                    .unwrap_or_else(|| AgentId::new("claude-code"));
                let chosen_model = state
                    .model
                    .effective_model_for_agent(&chosen_agent)
                    .map(|m| m.id.clone())
                    .unwrap_or_else(|| ModelId::new("claude-sonnet-4"));
                let conv_id = ConversationId(state.next_conversation_id);
                state.next_conversation_id += 1;

                let user_step_id = StepId(state.next_step_id);
                state.next_step_id += 1;
                let mut user_step = ConversationStep::simple(
                    user_step_id,
                    StepKind::UserPrompt,
                    if trimmed.is_empty() {
                        "Inspect attached files".to_string()
                    } else {
                        trimmed.to_string()
                    },
                    StepStatus::Completed,
                );
                user_step.files = files;

                let reply_step_id = StepId(state.next_step_id);
                state.next_step_id += 1;
                let (reply_step, status) = if is_service_down {
                    (
                        ConversationStep::simple(
                            reply_step_id,
                            StepKind::Error,
                            "AgentServer is stopped. Restart the service to run this request.",
                            StepStatus::Failed,
                        ),
                        ConversationStatus::Stopped,
                    )
                } else {
                    (
                        ConversationStep::simple(
                            reply_step_id,
                            StepKind::Response,
                            format!(
                                "Stub response for **{prompt_text}**.\n\nAgentServer is running in stub mode until step 9 lands, so this answer came from the stub backend.",
                                prompt_text = if trimmed.is_empty() {
                                    "your attachments"
                                } else {
                                    trimmed
                                }
                            ),
                            StepStatus::Completed,
                        ),
                        ConversationStatus::Finished,
                    )
                };

                let tab_id = state.model.active_tab;
                let conversation = Conversation {
                    id: conv_id,
                    agent_id: chosen_agent,
                    tab_id,
                    title: if trimmed.is_empty() {
                        "Attached files".to_string()
                    } else {
                        trimmed.to_string()
                    },
                    model_id: chosen_model,
                    status,
                    steps: vec![user_step, reply_step],
                    unseen: false,
                    elapsed_secs: 1,
                };
                state.model.conversations.push(conversation.clone());
                let active_changed = tab_id.map(|tid| {
                    state.model.active_conversations.insert(tid, conv_id);
                    (tid, Some(conv_id))
                });
                (conversation, active_changed)
            }
        };

        self.bus.publish(Event::ConversationUpdated { conversation: updated });
        if let Some((tab_id, conversation_id)) = active_changed {
            self.bus.publish(Event::ActiveConversationChanged {
                tab_id,
                conversation_id,
            });
        }
        Ok(())
    }

    fn handle_agent_stop(&self, conversation_id: Option<ConversationId>) -> CommandResult {
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let target_id = conversation_id.or_else(|| {
                state
                    .model
                    .active_tab
                    .and_then(|tab_id| state.model.active_conversation_for_tab(tab_id).map(|c| c.id))
            });
            let Some(conv_id) = target_id else {
                return Ok(());
            };
            let Some(conv) = state.model.conversations.iter_mut().find(|c| c.id == conv_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "agent.stop".to_string(),
                    target: format!("conversation {}", conv_id.0),
                });
            };
            for step in &mut conv.steps {
                if step.status == StepStatus::Running {
                    step.status = StepStatus::Completed;
                }
                step.streaming = false;
            }
            conv.status = ConversationStatus::Stopped;
            conv.clone()
        };
        self.bus.publish(Event::ConversationUpdated { conversation: updated });
        Ok(())
    }

    fn handle_agent_new(&self, tab_id: Option<TabId>) -> CommandResult {
        let (conv, active_tab) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let target_tab = tab_id.or(state.model.active_tab);
            let agent_id = state
                .model
                .default_agent()
                .map(|a| a.id.clone())
                .or_else(|| state.model.agents.first().map(|a| a.id.clone()))
                .unwrap_or_else(|| AgentId::new("claude-code"));
            let model_id = state
                .model
                .effective_model_for_agent(&agent_id)
                .map(|m| m.id.clone())
                .unwrap_or_else(|| ModelId::new("claude-sonnet-4"));
            let conv_id = ConversationId(state.next_conversation_id);
            state.next_conversation_id += 1;
            let conv = Conversation {
                id: conv_id,
                agent_id,
                tab_id: target_tab,
                title: String::new(),
                model_id,
                status: ConversationStatus::Idle,
                steps: Vec::new(),
                unseen: false,
                elapsed_secs: 0,
            };
            state.model.conversations.push(conv.clone());
            if let Some(tid) = target_tab {
                state.model.active_conversations.insert(tid, conv_id);
            }
            (conv, target_tab)
        };
        let conv_id = conv.id;
        self.bus.publish(Event::ConversationUpdated { conversation: conv });
        if let Some(tid) = active_tab {
            self.bus.publish(Event::ActiveConversationChanged {
                tab_id: tid,
                conversation_id: Some(conv_id),
            });
        }
        Ok(())
    }

    fn handle_agent_select(&self, agent_id: Option<AgentId>) -> CommandResult {
        let Some(agent_id) = agent_id else {
            return self.handle_picker_or_region_command(Command::PickerOpen {
                name: Some("agents".to_string()),
                request_id: None,
            });
        };
        let (default_agent_id, selections, updated_conv) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let Some(agent) = state.model.agent(&agent_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "agent.select".to_string(),
                    target: format!("agent {}", agent_id.0),
                });
            };
            if !agent.available {
                return Err(CommandError::InvalidArgument {
                    command: "agent.select".to_string(),
                    message: format!("agent `{}` is not available", agent.name),
                });
            }
            state.model.default_agent_id = Some(agent_id.clone());
            let updated_conv = if let Some(tab_id) = state.model.active_tab
                && let Some(conv_id) = state.model.active_conversation_for_tab(tab_id).map(|c| c.id)
                && let Some(conv) = state.model.conversations.iter_mut().find(|c| c.id == conv_id)
                && conv.status != ConversationStatus::Working
            {
                conv.agent_id = agent_id.clone();
                Some(conv.clone())
            } else {
                None
            };
            (
                state.model.default_agent_id.clone(),
                state.model.agent_selections.clone(),
                updated_conv,
            )
        };
        self.bus.publish(Event::AgentSettingsChanged {
            default_agent_id,
            selections,
        });
        if let Some(conversation) = updated_conv {
            self.bus.publish(Event::ConversationUpdated { conversation });
        }
        Ok(())
    }

    fn handle_agent_cycle(&self, direction: i8) -> CommandResult {
        let next_id = {
            let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let available: Vec<&AgentId> = state
                .model
                .agents
                .iter()
                .filter(|a| a.available)
                .map(|a| &a.id)
                .collect();
            if available.is_empty() {
                return Ok(());
            }
            let current = state
                .model
                .active_tab
                .and_then(|tid| state.model.active_conversation_for_tab(tid))
                .map(|c| &c.agent_id)
                .or(state.model.default_agent_id.as_ref());
            let idx = current
                .and_then(|cid| available.iter().position(|id| *id == cid))
                .unwrap_or(0);
            let step = if direction < 0 { -1isize } else { 1isize };
            let next_idx = (idx as isize + step).rem_euclid(available.len() as isize) as usize;
            available[next_idx].clone()
        };
        self.handle_agent_select(Some(next_id))
    }

    fn resolve_target_agent_id(&self, agent_id: Option<AgentId>) -> AgentId {
        if let Some(id) = agent_id {
            return id;
        }
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state
            .model
            .active_tab
            .and_then(|tid| state.model.active_conversation_for_tab(tid))
            .map(|c| c.agent_id.clone())
            .or_else(|| state.model.default_agent().map(|a| a.id.clone()))
            .unwrap_or_else(|| AgentId::new("claude-code"))
    }

    fn handle_agent_model(&self, agent_id: Option<AgentId>, model_id: Option<ModelId>) -> CommandResult {
        if agent_id.is_none() && model_id.is_none() {
            self.request_view(ViewRequest::PanelOpen {
                panel_id: SidePanelId::new(AGENT_PANEL_ID),
                focus: false,
            });
            self.request_view(ViewRequest::AgentModelCard);
            return Ok(());
        }
        let target_agent = self.resolve_target_agent_id(agent_id);
        let (default_agent_id, selections, updated_conv) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(mid) = &model_id
                && !state.model.models.iter().any(|m| &m.id == mid)
            {
                return Err(CommandError::TargetNotFound {
                    command: "agent.model".to_string(),
                    target: format!("model {}", mid.0),
                });
            }
            let entry = state.model.agent_selections.entry(target_agent.clone()).or_default();
            entry.model_id = model_id.clone();

            let effective_model = model_id
                .or_else(|| state.model.models.first().map(|m| m.id.clone()))
                .unwrap_or_else(|| ModelId::new("claude-sonnet-4"));
            let updated_conv = if let Some(tab_id) = state.model.active_tab
                && let Some(conv_id) = state.model.active_conversation_for_tab(tab_id).map(|c| c.id)
                && let Some(conv) = state.model.conversations.iter_mut().find(|c| c.id == conv_id)
                && conv.agent_id == target_agent
            {
                conv.model_id = effective_model;
                Some(conv.clone())
            } else {
                None
            };
            (
                state.model.default_agent_id.clone(),
                state.model.agent_selections.clone(),
                updated_conv,
            )
        };
        self.bus.publish(Event::AgentSettingsChanged {
            default_agent_id,
            selections,
        });
        if let Some(conversation) = updated_conv {
            self.bus.publish(Event::ConversationUpdated { conversation });
        }
        Ok(())
    }

    fn handle_agent_set_effort(&self, agent_id: Option<AgentId>, effort: Option<ModelEffort>) -> CommandResult {
        let target_agent = self.resolve_target_agent_id(agent_id);
        let (default_agent_id, selections) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let entry = state.model.agent_selections.entry(target_agent).or_default();
            entry.effort = effort;
            (
                state.model.default_agent_id.clone(),
                state.model.agent_selections.clone(),
            )
        };
        self.bus.publish(Event::AgentSettingsChanged {
            default_agent_id,
            selections,
        });
        Ok(())
    }

    fn handle_agent_set_context(&self, agent_id: Option<AgentId>, context_id: String) -> CommandResult {
        let target_agent = self.resolve_target_agent_id(agent_id);
        let (default_agent_id, selections) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let entry = state.model.agent_selections.entry(target_agent).or_default();
            entry.context_id = Some(context_id);
            (
                state.model.default_agent_id.clone(),
                state.model.agent_selections.clone(),
            )
        };
        self.bus.publish(Event::AgentSettingsChanged {
            default_agent_id,
            selections,
        });
        Ok(())
    }

    fn handle_agent_confirm(&self, conversation_id: ConversationId, step_id: StepId, allow: bool) -> CommandResult {
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let Some(conv) = state.model.conversations.iter_mut().find(|c| c.id == conversation_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "agent.confirm".to_string(),
                    target: format!("conversation {}", conversation_id.0),
                });
            };
            let Some(step) = conv.steps.iter_mut().find(|s| s.id == step_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "agent.confirm".to_string(),
                    target: format!("step {}", step_id.0),
                });
            };
            if let StepKind::Confirmation { resolved, .. } = &mut step.kind {
                *resolved = Some(allow);
                step.status = if allow {
                    StepStatus::Completed
                } else {
                    StepStatus::Failed
                };
            } else {
                return Err(CommandError::InvalidArgument {
                    command: "agent.confirm".to_string(),
                    message: format!("step {} is not a confirmation step", step_id.0),
                });
            }
            let still_waiting = conv
                .steps
                .iter()
                .any(|s| matches!(s.kind, StepKind::Confirmation { resolved: None, .. }));
            if !still_waiting && conv.status == ConversationStatus::WaitingForUser {
                conv.status = ConversationStatus::Finished;
            }
            conv.clone()
        };
        self.bus.publish(Event::ConversationUpdated { conversation: updated });
        Ok(())
    }

    fn handle_agent_switch_conversation(&self, conversation_id: ConversationId) -> CommandResult {
        let (tab_id, updated) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let fallback_tab = state.model.active_tab;
            let Some(conv) = state.model.conversations.iter_mut().find(|c| c.id == conversation_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "agent.switch_conversation".to_string(),
                    target: format!("conversation {}", conversation_id.0),
                });
            };
            conv.unseen = false;
            let tab_id = conv.tab_id.or(fallback_tab);
            let conv_clone = conv.clone();
            if let Some(tid) = tab_id {
                state.model.active_conversations.insert(tid, conversation_id);
            }
            (tab_id, conv_clone)
        };
        self.bus.publish(Event::ConversationUpdated { conversation: updated });
        if let Some(tid) = tab_id {
            self.bus.publish(Event::ActiveConversationChanged {
                tab_id: tid,
                conversation_id: Some(conversation_id),
            });
        }
        Ok(())
    }

    fn handle_agent_mark_seen(&self, conversation_id: ConversationId) -> CommandResult {
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let Some(conv) = state.model.conversations.iter_mut().find(|c| c.id == conversation_id) else {
                return Ok(());
            };
            if !conv.unseen {
                return Ok(());
            }
            conv.unseen = false;
            conv.clone()
        };
        self.bus.publish(Event::ConversationUpdated { conversation: updated });
        Ok(())
    }

    fn handle_instruction_add(&self) -> CommandResult {
        let instructions = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let next_id = state
                .model
                .agent_instructions
                .iter()
                .map(|item| item.id.0)
                .max()
                .unwrap_or(0)
                + 1;
            state.model.agent_instructions.push(AgentInstruction {
                id: InstructionId(next_id),
                enabled: true,
                title: String::new(),
                text: String::new(),
                file: None,
            });
            state.model.agent_instructions.clone()
        };
        self.bus.publish(Event::AgentInstructionsChanged { instructions });
        Ok(())
    }

    fn handle_instruction_update(
        &self,
        id: InstructionId,
        enabled: Option<bool>,
        title: Option<String>,
        text: Option<String>,
    ) -> CommandResult {
        let instructions = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let Some(item) = state.model.agent_instructions.iter_mut().find(|i| i.id == id) else {
                return Err(CommandError::TargetNotFound {
                    command: "agent.instructions.update".to_string(),
                    target: format!("instruction {}", id.0),
                });
            };
            if let Some(v) = enabled {
                item.enabled = v;
            }
            if let Some(v) = title {
                item.title = v;
            }
            if let Some(v) = text {
                item.text = v;
            }
            state.model.agent_instructions.clone()
        };
        self.bus.publish(Event::AgentInstructionsChanged { instructions });
        Ok(())
    }

    fn handle_instruction_remove(&self, id: InstructionId) -> CommandResult {
        let instructions = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let before = state.model.agent_instructions.len();
            state.model.agent_instructions.retain(|item| item.id != id);
            if state.model.agent_instructions.len() == before {
                return Err(CommandError::TargetNotFound {
                    command: "agent.instructions.remove".to_string(),
                    target: format!("instruction {}", id.0),
                });
            }
            state.model.agent_instructions.clone()
        };
        self.bus.publish(Event::AgentInstructionsChanged { instructions });
        Ok(())
    }

    fn handle_instruction_set_file(&self, id: InstructionId, file: Option<InstructionAttachment>) -> CommandResult {
        let instructions = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let Some(item) = state.model.agent_instructions.iter_mut().find(|i| i.id == id) else {
                return Err(CommandError::TargetNotFound {
                    command: "agent.instructions.set_file".to_string(),
                    target: format!("instruction {}", id.0),
                });
            };
            item.file = file;
            state.model.agent_instructions.clone()
        };
        self.bus.publish(Event::AgentInstructionsChanged { instructions });
        Ok(())
    }

    fn handle_summarize_captcha(&self, tab_id: Option<TabId>) -> CommandResult {
        let target_tab = tab_id.or_else(|| self.state.lock().unwrap_or_else(|e| e.into_inner()).model.active_tab);
        let summary = {
            let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            target_tab
                .and_then(|tid| state.model.tab_captcha_findings.get(&tid).cloned())
                .unwrap_or_else(|| "No CAPTCHA detected on this tab.".to_string())
        };
        self.handle_agent_send_with_files(None, None, format!("Summarize CAPTCHA: {summary}"), Vec::new())
    }

    fn handle_agent_log(&self, _tab_id: Option<TabId>) -> CommandResult {
        let toast = crate::toasts::Toast {
            id: crate::toasts::ToastId(9001),
            message: "Agent trace log is available once AgentServer is running.".to_string(),
            description: None,
            timeout_ms: 3000,
            action: None,
            bounce_seq: 0,
        };
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            crate::toasts::push_toast(&mut state.model.toasts, toast.clone());
        }
        self.bus.publish(Event::ToastShown { toast });
        Ok(())
    }

    fn handle_agent_dismiss(&self, conversation_id: ConversationId) -> CommandResult {
        let removed = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let before = state.model.conversations.len();
            state.model.conversations.retain(|conv| conv.id != conversation_id);
            state.model.active_conversations.retain(|_, id| *id != conversation_id);
            state.model.conversations.len() != before
        };
        if !removed {
            return Err(CommandError::TargetNotFound {
                command: "agent.dismiss".to_string(),
                target: format!("conversation {}", conversation_id.0),
            });
        }
        self.bus.publish(Event::ConversationRemoved { conversation_id });
        self.bus.publish(Event::AgentRosterChanged);
        Ok(())
    }

    fn handle_debug_sample_agent(&self) -> CommandResult {
        let (conv, tab_id) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let tab_id = state.model.active_tab;
            let conv_id = ConversationId(state.next_conversation_id);
            state.next_conversation_id += 1;
            let base_step = state.next_step_id;
            state.next_step_id += 10;
            let conv = build_rich_sample_conversation(conv_id, base_step, tab_id);
            state.model.conversations.push(conv.clone());
            if let Some(tid) = tab_id {
                state.model.active_conversations.insert(tid, conv_id);
            }
            // Ensure AgentServer is running so the sample conversation is interactive.
            if let Some(svc) = state
                .model
                .services
                .iter_mut()
                .find(|s| s.service == ServiceId::AgentServer)
            {
                svc.status = ServiceStatus::Running;
            }
            (conv, tab_id)
        };
        let conv_id = conv.id;
        self.bus.publish(Event::ServiceStateChanged {
            state: ServiceState {
                service: ServiceId::AgentServer,
                status: ServiceStatus::Running,
            },
        });
        self.bus.publish(Event::ConversationUpdated { conversation: conv });
        if let Some(tid) = tab_id {
            self.bus.publish(Event::ActiveConversationChanged {
                tab_id: tid,
                conversation_id: Some(conv_id),
            });
        }
        self.request_view(ViewRequest::PanelOpen {
            panel_id: SidePanelId::new(AGENT_PANEL_ID),
            focus: true,
        });
        Ok(())
    }

    fn handle_debug_toggle_agent_service(&self) -> CommandResult {
        let next = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let current_running = state
                .model
                .service_state(ServiceId::AgentServer)
                .is_some_and(|s| matches!(s.status, ServiceStatus::Running));
            let next_status = if current_running {
                ServiceStatus::Stopped {
                    reason: Some("AgentServer exited unexpectedly (code 1)".to_string()),
                }
            } else {
                ServiceStatus::Running
            };
            let svc_state = ServiceState {
                service: ServiceId::AgentServer,
                status: next_status,
            };
            if let Some(existing) = state
                .model
                .services
                .iter_mut()
                .find(|s| s.service == ServiceId::AgentServer)
            {
                *existing = svc_state.clone();
            } else {
                state.model.services.push(svc_state.clone());
            }
            svc_state
        };
        self.bus.publish(Event::ServiceStateChanged { state: next });
        Ok(())
    }
}

/// Builds a rich sample conversation showcasing folded steps, GFM Markdown, an interactive native table, and a pending
/// confirmation card (matching the Phase 6 reference screenshots).
pub fn build_rich_sample_conversation(id: ConversationId, first_step_id: u64, tab_id: Option<TabId>) -> Conversation {
    let md_answer = "\
# Architecture Summary

## Key Principles
- **Command registry**: every action flows through `invoke(name, args)`
- **Process isolation**: untrusted code and secrets live in sandboxed services

```rust
let res = core.invoke(Command::TabClose { tab_id: None, force: false });
```

| Crate | Responsibility | Depends on Freya |
| --- | --- | --- |
| `BrowserCore` | Model, commands, events | No |
| `Keymap` | Compiled shortcut tables | No |
| `UI/Freya` | Native window & gallery | Yes |";

    let mut prompt_step = ConversationStep::simple(
        StepId(first_step_id),
        StepKind::UserPrompt,
        "Summarize Zendia's architecture and crate boundaries from this tab.",
        StepStatus::Completed,
    );
    prompt_step.files = vec!["notes.md".to_string()];

    Conversation {
        id,
        agent_id: AgentId::new("claude-code"),
        tab_id,
        title: "Summarize Zendia's architecture".to_string(),
        model_id: ModelId::new("claude-sonnet-4"),
        status: ConversationStatus::WaitingForUser,
        steps: vec![
            prompt_step,
            ConversationStep::simple(
                StepId(first_step_id + 1),
                StepKind::ToolCall {
                    tool: "page.snapshot".to_string(),
                },
                "Read accessibility tree",
                StepStatus::Completed,
            ),
            ConversationStep::simple(
                StepId(first_step_id + 2),
                StepKind::Thinking,
                "Mapping crate dependencies and boundaries",
                StepStatus::Completed,
            ),
            ConversationStep::simple(
                StepId(first_step_id + 3),
                StepKind::ToolCall {
                    tool: "page.extract_links".to_string(),
                },
                "Collected 3 architecture crate references",
                StepStatus::Completed,
            ),
            ConversationStep::simple(
                StepId(first_step_id + 4),
                StepKind::Response,
                md_answer,
                StepStatus::Completed,
            ),
            ConversationStep::simple(
                StepId(first_step_id + 5),
                StepKind::AgentTable(AgentTable {
                    title: "Crate Dependencies".to_string(),
                    columns: vec!["Crate".to_string(), "Layer".to_string(), "Status".to_string()],
                    rows: vec![
                        AgentTableRow {
                            cells: vec!["BrowserCore".to_string(), "Core".to_string(), "Ready".to_string()],
                            url: Some("https://ladybird.org".to_string()),
                            tab_id: None,
                        },
                        AgentTableRow {
                            cells: vec!["ModuleApi".to_string(), "Contract".to_string(), "Ready".to_string()],
                            url: None,
                            tab_id: None,
                        },
                        AgentTableRow {
                            cells: vec!["UI/Freya".to_string(), "View".to_string(), "Phase 6".to_string()],
                            url: None,
                            tab_id: None,
                        },
                    ],
                    note: None,
                }),
                "Crate Dependencies",
                StepStatus::Completed,
            ),
            ConversationStep::simple(
                StepId(first_step_id + 6),
                StepKind::Confirmation {
                    prompt: "open `https://ladybird.org` in a new tab".to_string(),
                    resolved: None,
                },
                "Permission needed",
                StepStatus::WaitingForUser,
            ),
        ],
        unseen: false,
        elapsed_secs: 4,
    }
}

/// Builds a sample declared UI page emitted natively by an agent using `zendia-module-api` nodes.
pub fn sample_declared_agent_page() -> AgentDeclaredPage {
    AgentDeclaredPage {
        title: "Tab Audit Report".to_string(),
        draft: false,
        root: UiNode::Column {
            spacing: 8.0,
            padding: 4.0,
            children: vec![
                UiNode::Row {
                    spacing: 8.0,
                    padding: 0.0,
                    children: vec![
                        UiNode::Pill {
                            label: "3 checks passed".to_string(),
                            icon: Some("check".to_string()),
                            tone: UiTone::Success,
                            action: None,
                        },
                        UiNode::Pill {
                            label: "1 permission pending".to_string(),
                            icon: Some("warning".to_string()),
                            tone: UiTone::Warning,
                            action: None,
                        },
                    ],
                },
                UiNode::Text {
                    text: "All rendered natively via ModuleApi UiNode without HTML or WebContent.".to_string(),
                    role: UiRole::TextMuted,
                    size: Some(12.0),
                    bold: false,
                },
                UiNode::Row {
                    spacing: 6.0,
                    padding: 0.0,
                    children: vec![
                        UiNode::Chip {
                            label: "Open Architecture Tab".to_string(),
                            icon: Some("code".to_string()),
                            selected: false,
                            disabled: false,
                            tone: UiTone::Accent,
                            action: Some(UiAction::new("tab.new").with_arg("url", "https://ladybird.org")),
                        },
                        UiNode::Chip {
                            label: "Copy Summary".to_string(),
                            icon: Some("copy".to_string()),
                            selected: false,
                            disabled: false,
                            tone: UiTone::Neutral,
                            action: Some(UiAction::new("address_bar.copy_current_url")),
                        },
                    ],
                },
            ],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrowserCore;

    #[test]
    fn panel_prefs_agent_settings_confirmations_and_instructions_round_trip() {
        let core = StubBackend::new();
        let mut view = core.model();
        let sub = core.subscribe();
        let panel = SidePanelId::new(AGENT_PANEL_ID);

        core.invoke(Command::PanelSetLayout {
            panel_id: panel.clone(),
            layout: SidePanelLayout::Split,
        })
        .unwrap();
        core.invoke(Command::PanelSetSide {
            panel_id: panel.clone(),
            side: SidePanelSide::Left,
        })
        .unwrap();
        core.invoke(Command::PanelSetWidth {
            panel_id: panel.clone(),
            width: 520.0,
        })
        .unwrap();
        core.invoke(Command::AgentSetEffort {
            agent_id: Some(AgentId::new("claude-code")),
            effort: Some(ModelEffort::High),
        })
        .unwrap();
        core.invoke(Command::AgentInstructionAdd).unwrap();
        core.invoke(Command::AgentInstructionUpdate {
            id: InstructionId(1),
            enabled: Some(true),
            title: Some("Concise answers".to_string()),
            text: Some("Use bullet points.".to_string()),
        })
        .unwrap();
        core.invoke(Command::AgentConfirm {
            conversation_id: ConversationId(1),
            step_id: StepId(3),
            allow: true,
        })
        .unwrap();

        while let Ok(event) = sub.try_recv() {
            view.apply_event(&event);
        }

        let prefs = view.panel_prefs(&panel);
        assert_eq!(prefs.layout, SidePanelLayout::Split);
        assert_eq!(prefs.side, SidePanelSide::Left);
        assert_eq!(prefs.width, 520.0);
        assert_eq!(view.agent_instructions.len(), 1);
        assert_eq!(view.agent_instructions[0].title, "Concise answers");
        assert_eq!(
            view.agent_selections.get(&AgentId::new("claude-code")).unwrap().effort,
            Some(ModelEffort::High)
        );
        let conv = view.conversations.iter().find(|c| c.id == ConversationId(1)).unwrap();
        assert_eq!(conv.status, ConversationStatus::Finished);
        assert_eq!(view, core.model());
    }
}
