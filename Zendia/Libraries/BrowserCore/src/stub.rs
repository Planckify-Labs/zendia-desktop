/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! In-memory stub backend implementing [`BrowserCore`] with sample data and an optional [`TabSource`] hook.
//!
//! When attached to Ladybird's Hub, real tabs and navigation commands pass through the [`TabSource`] adapter while
//! spaces, library entries, agents, web chat sites, service states, and built-in contributions come from sample data.
//! Without a [`TabSource`] (such as in the UI gallery or headless tests), tabs also come from sample data.

use crate::BrowserCore;
use crate::commands::{Command, CommandDescriptor, CommandError, CommandResult, builtin_command_descriptors};
use crate::contributions::{
    BadgeTone, ContributionId, ContributionSource, Contributions, MenuEntry, MenuLocation, PaletteEntry,
    SidePanelEntry, SidePanelId, SidePanelLayout, SidePanelSide, ToolbarBadge, ToolbarItem, ToolbarPlacement,
};
use crate::events::{Event, EventBus, Subscription};
use crate::model::{
    Agent, AgentId, BrowserModel, ChatSite, ChatSiteId, Conversation, ConversationId, ConversationStatus,
    ConversationStep, Download, DownloadId, DownloadStatus, Essential, Folder, FolderId, HistoryEntry, HistoryEntryId,
    InputMode, MediaSession, MediaSessionId, ModelEffort, ModelId, ModelInfo, PinnedTab, ServiceId, ServiceState,
    ServiceStatus, Space, SpaceGradient, SpaceId, SplitDirection, SplitGroup, SplitGroupId, StepId, StepKind,
    StepStatus, Tab, TabId, TabKind, ThemeOverrides, ThemeScheme, ThemeSettings, WhichKeyEntry, WhichKeyState, Window,
    WindowId,
};
use crate::tab_source::{TabSource, TabUpdate};
use std::sync::{Arc, Mutex};

struct StubState {
    model: BrowserModel,
    next_tab_id: u64,
    next_split_id: u64,
    next_conversation_id: u64,
    next_step_id: u64,
    debug_input_step: usize,
}

/// Stub [`BrowserCore`] backend used during the Freya UI port until the real command registry lands.
pub struct StubBackend {
    state: Mutex<StubState>,
    tab_source: Mutex<Option<Arc<dyn TabSource>>>,
    bus: EventBus,
}

impl Default for StubBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl StubBackend {
    /// Creates a standalone stub pre-populated with sample tabs and sample data for every surface.
    pub fn new() -> Self {
        Self::from_model(sample_model(true))
    }

    /// Creates a stub whose tabs will be fed in by an external [`TabSource`] (such as Ladybird's Hub).
    ///
    /// Non-tab data (spaces, downloads, history, agents, conversations, models, chat sites, services, and built-in
    /// contributions) is still populated from sample data.
    pub fn new_for_external_tabs() -> Self {
        Self::from_model(sample_model(false))
    }

    fn from_model(model: BrowserModel) -> Self {
        let next_tab_id = model.tabs.iter().map(|tab| tab.id.0).max().unwrap_or(0) + 1;
        let next_split_id = model.split_groups.iter().map(|group| group.id.0).max().unwrap_or(0) + 1;
        let next_conversation_id = model.conversations.iter().map(|conv| conv.id.0).max().unwrap_or(0) + 1;
        let next_step_id = model
            .conversations
            .iter()
            .flat_map(|conv| conv.steps.iter().map(|step| step.id.0))
            .max()
            .unwrap_or(0)
            + 1;

        Self {
            state: Mutex::new(StubState {
                model,
                next_tab_id,
                next_split_id,
                next_conversation_id,
                next_step_id,
                debug_input_step: 0,
            }),
            tab_source: Mutex::new(None),
            bus: EventBus::new(),
        }
    }

    /// Attaches a real tab source so tab and navigation commands forward to it.
    pub fn attach_tab_source(&self, source: Arc<dyn TabSource>) {
        *self.tab_source.lock().unwrap_or_else(|error| error.into_inner()) = Some(source);
    }

    fn current_tab_source(&self) -> Option<Arc<dyn TabSource>> {
        self.tab_source
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// Records a tab opened by an attached [`TabSource`] and publishes the corresponding events.
    pub fn feed_tab_opened(&self, mut tab: Tab, after: Option<TabId>, activate: bool) {
        let (opened_event, activated_event) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.tabs.iter().any(|existing| existing.id == tab.id) {
                return;
            }
            if tab.space_id.is_none() && tab.kind != TabKind::Essential {
                tab.space_id = state.model.active_space;
            }
            let index = after
                .and_then(|after_id| state.model.tabs.iter().position(|existing| existing.id == after_id))
                .map(|position| position + 1)
                .unwrap_or(state.model.tabs.len());
            state.model.tabs.insert(index, tab.clone());
            let should_activate = activate || state.model.active_tab.is_none();
            let activated = if should_activate {
                state.model.active_tab = Some(tab.id);
                if let Some(window) = state.model.windows.first_mut() {
                    window.active_tab = Some(tab.id);
                }
                Some(Event::TabActivated { tab_id: Some(tab.id) })
            } else {
                None
            };
            (Event::TabOpened { tab, index }, activated)
        };

        self.bus.publish(opened_event);
        if let Some(activated_event) = activated_event {
            self.bus.publish(activated_event);
        }
    }

    /// Applies incremental field updates from an attached [`TabSource`] and publishes [`Event::TabUpdated`].
    pub fn feed_tab_updated(&self, tab_id: TabId, update: TabUpdate) {
        let updated_tab = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(tab) = state.model.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
                return;
            };
            if let Some(title) = update.title {
                tab.title = title;
            }
            if let Some(url) = update.url {
                tab.url = url;
            }
            if let Some(is_loading) = update.is_loading {
                tab.is_loading = is_loading;
            }
            if let Some(can_navigate_back) = update.can_navigate_back {
                tab.can_navigate_back = can_navigate_back;
            }
            if let Some(can_navigate_forward) = update.can_navigate_forward {
                tab.can_navigate_forward = can_navigate_forward;
            }
            if let Some(is_audible) = update.is_audible {
                tab.is_audible = is_audible;
            }
            if let Some(is_muted) = update.is_muted {
                tab.is_muted = is_muted;
            }
            if let Some(closing) = update.closing {
                tab.closing = closing;
            }
            tab.clone()
        };

        self.bus.publish(Event::TabUpdated { tab: updated_tab });
    }

    /// Records the active tab reported by an attached [`TabSource`] and publishes [`Event::TabActivated`].
    pub fn feed_tab_activated(&self, tab_id: Option<TabId>) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.active_tab = tab_id;
            if let Some(window) = state.model.windows.first_mut() {
                window.active_tab = tab_id;
            }
        }
        self.bus.publish(Event::TabActivated { tab_id });
    }

    /// Records a tab closed by an attached [`TabSource`] and returns the suggested next tab to activate if the closed
    /// tab was active.
    pub fn feed_tab_closed(&self, tab_id: TabId) -> Option<TabId> {
        let (removed, was_active, next_tab) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let index = state.model.tabs.iter().position(|tab| tab.id == tab_id)?;
            state.model.tabs.remove(index);
            state.model.essentials.retain(|item| item.tab_id != tab_id);
            state.model.pinned_tabs.retain(|item| item.tab_id != tab_id);
            for folder in &mut state.model.folders {
                folder.tab_ids.retain(|id| *id != tab_id);
            }
            for group in &mut state.model.split_groups {
                group.tab_ids.retain(|id| *id != tab_id);
            }
            state.model.split_groups.retain(|group| group.tab_ids.len() > 1);
            let was_active = state.model.active_tab == Some(tab_id);
            let next_tab = if was_active {
                state.model.active_tab = None;
                if let Some(window) = state.model.windows.first_mut() {
                    window.active_tab = None;
                }
                state
                    .model
                    .tabs
                    .get(index.min(state.model.tabs.len().saturating_sub(1)))
                    .map(|tab| tab.id)
            } else {
                None
            };
            (true, was_active, next_tab)
        };

        if removed {
            self.bus.publish(Event::TabClosed { tab_id });
            if was_active && next_tab.is_none() {
                self.bus.publish(Event::TabActivated { tab_id: None });
            }
        }
        next_tab
    }

    /// Updates the published input mode (used by debug shortcuts until InputServer arrives).
    pub fn set_input_mode(&self, mode: InputMode) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.input_mode = mode;
        }
        self.bus.publish(Event::ModeChanged { mode });
    }

    /// Updates the pending key chord published by InputServer.
    pub fn set_pending_keys(&self, keys: Vec<String>) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.pending_keys = keys.clone();
        }
        self.bus.publish(Event::PendingKeysChanged { keys });
    }

    /// Updates the active which-key popup state published by InputServer.
    pub fn set_which_key(&self, which_key: Option<WhichKeyState>) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.which_key = which_key.clone();
        }
        self.bus.publish(Event::WhichKeyChanged { state: which_key });
    }

    /// Cycles through sample InputServer states (modes, pending keys, and which-key candidates) for UI development
    /// and testing before InputServer is wired in.
    pub fn cycle_sample_input_state(&self) {
        let step = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.debug_input_step = (state.debug_input_step + 1) % 8;
            state.debug_input_step
        };

        let (mode, pending, which_key): (InputMode, Vec<String>, Option<WhichKeyState>) = match step {
            1 => (
                InputMode::Normal,
                vec!["<Space>".to_string()],
                Some(WhichKeyState {
                    title: "Leader".to_string(),
                    entries: vec![
                        WhichKeyEntry {
                            key: "f".to_string(),
                            description: "+find".to_string(),
                            is_group: true,
                        },
                        WhichKeyEntry {
                            key: "w".to_string(),
                            description: "+window".to_string(),
                            is_group: true,
                        },
                        WhichKeyEntry {
                            key: "a".to_string(),
                            description: "Open agent chat".to_string(),
                            is_group: false,
                        },
                    ],
                }),
            ),
            2 => (
                InputMode::Normal,
                vec!["<Space>".to_string(), "f".to_string()],
                Some(WhichKeyState {
                    title: "Find".to_string(),
                    entries: vec![
                        WhichKeyEntry {
                            key: "f".to_string(),
                            description: "Command palette".to_string(),
                            is_group: false,
                        },
                        WhichKeyEntry {
                            key: "t".to_string(),
                            description: "Switch tab".to_string(),
                            is_group: false,
                        },
                        WhichKeyEntry {
                            key: "h".to_string(),
                            description: "Browse history".to_string(),
                            is_group: false,
                        },
                    ],
                }),
            ),
            3 => (InputMode::Insert, Vec::new(), None),
            4 => (InputMode::Visual, Vec::new(), None),
            5 => (InputMode::Explorer, vec!["g".to_string()], None),
            6 => (InputMode::Toolbar, Vec::new(), None),
            7 => (InputMode::Window, Vec::new(), None),
            _ => (InputMode::Normal, Vec::new(), None),
        };

        self.set_input_mode(mode);
        self.set_pending_keys(pending);
        self.set_which_key(which_key);
    }

    fn adjacent_tab_id(&self, delta: isize) -> Option<TabId> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let len = state.model.tabs.len();
        if len < 2 {
            return None;
        }
        let current = state
            .model
            .active_tab
            .and_then(|active_id| state.model.tabs.iter().position(|tab| tab.id == active_id))
            .unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(len as isize) as usize;
        Some(state.model.tabs[next].id)
    }

    /// Updates a child service's reported health state.
    pub fn set_service_state(&self, service_state: ServiceState) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(existing) = state
                .model
                .services
                .iter_mut()
                .find(|entry| entry.service == service_state.service)
            {
                *existing = service_state.clone();
            } else {
                state.model.services.push(service_state.clone());
            }
        }
        self.bus.publish(Event::ServiceStateChanged { state: service_state });
    }

    fn resolve_target_tab(&self, tab_id: Option<TabId>, command_name: &str) -> Result<TabId, CommandError> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let id = tab_id
            .or(state.model.active_tab)
            .ok_or_else(|| CommandError::TargetNotFound {
                command: command_name.to_string(),
                target: "active tab".to_string(),
            })?;
        if state.model.tab(id).is_none() {
            return Err(CommandError::TargetNotFound {
                command: command_name.to_string(),
                target: format!("tab {}", id.0),
            });
        }
        Ok(id)
    }

    fn handle_stub_tab_new(&self, url: Option<String>, after: Option<TabId>, activate: bool) -> CommandResult {
        let (opened_event, activated_event) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let id = TabId(state.next_tab_id);
            state.next_tab_id += 1;
            let resolved_url = url.unwrap_or_else(|| "about:newtab".to_string());
            let tab = Tab {
                id,
                view_id: None,
                space_id: state.model.active_space,
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Normal,
                title: String::new(),
                url: resolved_url,
                favicon: None,
                is_loading: false,
                can_navigate_back: false,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            };
            let index = after
                .and_then(|after_id| state.model.tabs.iter().position(|existing| existing.id == after_id))
                .map(|position| position + 1)
                .unwrap_or(state.model.tabs.len());
            state.model.tabs.insert(index, tab.clone());
            let activated = if activate || state.model.active_tab.is_none() {
                state.model.active_tab = Some(id);
                if let Some(window) = state.model.windows.first_mut() {
                    window.active_tab = Some(id);
                }
                Some(Event::TabActivated { tab_id: Some(id) })
            } else {
                None
            };
            (Event::TabOpened { tab, index }, activated)
        };
        self.bus.publish(opened_event);
        if let Some(event) = activated_event {
            self.bus.publish(event);
        }
        Ok(())
    }

    fn handle_stub_tab_close(&self, tab_id: TabId) -> CommandResult {
        let next = self.feed_tab_closed(tab_id);
        if let Some(next_id) = next {
            self.feed_tab_activated(Some(next_id));
        }
        Ok(())
    }

    fn handle_stub_load_url(&self, tab_id: TabId, input: String) -> CommandResult {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(CommandError::InvalidArgument {
                command: "navigation.load_url".to_string(),
                message: "input must not be empty".to_string(),
            });
        }
        let url = if trimmed.contains("://") || trimmed.starts_with("about:") {
            trimmed.to_string()
        } else {
            format!("https://{trimmed}")
        };
        self.feed_tab_updated(
            tab_id,
            TabUpdate {
                title: Some(trimmed.to_string()),
                url: Some(url),
                is_loading: Some(false),
                can_navigate_back: Some(true),
                ..TabUpdate::default()
            },
        );
        Ok(())
    }

    fn handle_space_switch(&self, space_id: SpaceId) -> CommandResult {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if !state.model.spaces.iter().any(|space| space.id == space_id) {
                return Err(CommandError::TargetNotFound {
                    command: "space.switch".to_string(),
                    target: format!("space {}", space_id.0),
                });
            }
            state.model.active_space = Some(space_id);
            if let Some(window) = state.model.windows.first_mut() {
                window.active_space = Some(space_id);
            }
        }
        self.bus.publish(Event::SpaceSwitched { space_id });
        Ok(())
    }

    fn handle_split_add(&self, tab_id: TabId, direction: SplitDirection) -> CommandResult {
        let group = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(active_tab_id) = state.model.active_tab else {
                return Err(CommandError::TargetNotFound {
                    command: "split.add".to_string(),
                    target: "active tab".to_string(),
                });
            };
            if state.model.tab(tab_id).is_none() {
                return Err(CommandError::TargetNotFound {
                    command: "split.add".to_string(),
                    target: format!("tab {}", tab_id.0),
                });
            }
            if active_tab_id == tab_id {
                return Err(CommandError::InvalidArgument {
                    command: "split.add".to_string(),
                    message: "cannot split a tab with itself".to_string(),
                });
            }
            let space_id = state.model.active_space.unwrap_or(SpaceId(1));
            if let Some(existing) = state
                .model
                .split_groups
                .iter_mut()
                .find(|group| group.tab_ids.contains(&active_tab_id))
            {
                if !existing.tab_ids.contains(&tab_id) {
                    existing.tab_ids.push(tab_id);
                    let share = 1.0 / existing.tab_ids.len() as f32;
                    existing.ratios = vec![share; existing.tab_ids.len()];
                }
                existing.direction = direction;
                let group_id = existing.id;
                let updated = existing.clone();
                if let Some(tab) = state.model.tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    tab.split_group_id = Some(group_id);
                }
                updated
            } else {
                let id = SplitGroupId(state.next_split_id);
                state.next_split_id += 1;
                let group = SplitGroup {
                    id,
                    space_id,
                    direction,
                    tab_ids: vec![active_tab_id, tab_id],
                    ratios: vec![0.5, 0.5],
                };
                for member_id in [active_tab_id, tab_id] {
                    if let Some(tab) = state.model.tabs.iter_mut().find(|tab| tab.id == member_id) {
                        tab.split_group_id = Some(id);
                    }
                }
                state.model.split_groups.push(group.clone());
                group
            }
        };

        self.bus.publish(Event::SplitGroupUpdated { group });
        Ok(())
    }

    fn handle_panel_toggle(&self, panel_id: SidePanelId) -> CommandResult {
        let known = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state
                .model
                .contributions
                .side_panels
                .iter()
                .any(|panel| panel.id == panel_id)
        };
        if !known {
            return Err(CommandError::TargetNotFound {
                command: "panel.toggle".to_string(),
                target: format!("panel {}", panel_id.as_str()),
            });
        }
        self.bus.publish(Event::PanelToggled { panel_id });
        Ok(())
    }

    fn handle_agent_send(
        &self,
        conversation_id: Option<ConversationId>,
        agent_id: Option<AgentId>,
        prompt: String,
    ) -> CommandResult {
        if prompt.trim().is_empty() {
            return Err(CommandError::InvalidArgument {
                command: "agent.send".to_string(),
                message: "prompt must not be empty".to_string(),
            });
        }

        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let user_step_id = StepId(state.next_step_id);
            state.next_step_id += 1;
            let reply_step_id = StepId(state.next_step_id);
            state.next_step_id += 1;

            let user_step = ConversationStep {
                id: user_step_id,
                kind: StepKind::UserPrompt,
                summary: prompt.clone(),
                detail: None,
                status: StepStatus::Completed,
            };
            let reply_step = ConversationStep {
                id: reply_step_id,
                kind: StepKind::Response,
                summary: format!("Stub response for: {prompt}"),
                detail: Some("AgentServer is not running yet; this response came from the stub backend.".to_string()),
                status: StepStatus::Completed,
            };

            if let Some(conv_id) = conversation_id {
                let Some(conversation) = state.model.conversations.iter_mut().find(|conv| conv.id == conv_id) else {
                    return Err(CommandError::TargetNotFound {
                        command: "agent.send".to_string(),
                        target: format!("conversation {}", conv_id.0),
                    });
                };
                conversation.steps.push(user_step);
                conversation.steps.push(reply_step);
                conversation.status = ConversationStatus::Finished;
                conversation.clone()
            } else {
                let chosen_agent = agent_id
                    .or_else(|| state.model.agents.first().map(|agent| agent.id.clone()))
                    .unwrap_or_else(|| AgentId::new("zendia.assistant"));
                let chosen_model = state
                    .model
                    .models
                    .first()
                    .map(|model| model.id.clone())
                    .unwrap_or_else(|| ModelId::new("stub-model"));
                let id = ConversationId(state.next_conversation_id);
                state.next_conversation_id += 1;
                let conversation = Conversation {
                    id,
                    agent_id: chosen_agent,
                    tab_id: state.model.active_tab,
                    title: prompt,
                    model_id: chosen_model,
                    status: ConversationStatus::Finished,
                    steps: vec![user_step, reply_step],
                };
                state.model.conversations.push(conversation.clone());
                conversation
            }
        };

        self.bus.publish(Event::ConversationUpdated { conversation: updated });
        Ok(())
    }

    fn handle_webchat_ask(&self, site_id: ChatSiteId, prompt: String) -> CommandResult {
        let known = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.chat_sites.iter().any(|site| site.id == site_id)
        };
        if !known {
            return Err(CommandError::TargetNotFound {
                command: "webchat.ask".to_string(),
                target: format!("chat site {}", site_id.0),
            });
        }
        eprintln!(
            "zendia: stub webchat.ask for `{}` ({} chars) — Automation World not connected yet",
            site_id.0,
            prompt.len()
        );
        self.bus.publish(Event::PanelToggled {
            panel_id: SidePanelId::new("zendia.webchat"),
        });
        Ok(())
    }

    fn handle_theme_set(&self, scheme: Option<ThemeScheme>, overrides: Option<ThemeOverrides>) -> CommandResult {
        if let Some(overrides) = &overrides {
            if let Some(opacity) = overrides.glass.opacity
                && (!(0.0..=1.0).contains(&opacity) || opacity.is_nan())
            {
                return Err(CommandError::InvalidArgument {
                    command: "theme.set".to_string(),
                    message: "glass opacity must be between 0.0 and 1.0".to_string(),
                });
            }
            if let Some(blur) = overrides.glass.blur
                && (blur < 0.0 || blur.is_nan())
            {
                return Err(CommandError::InvalidArgument {
                    command: "theme.set".to_string(),
                    message: "glass blur radius must be non-negative".to_string(),
                });
            }
        }

        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(scheme) = scheme {
                state.model.theme.scheme = scheme;
            }
            if let Some(overrides) = overrides {
                state.model.theme.overrides = overrides;
            }
            state.model.theme.clone()
        };

        self.bus.publish(Event::SettingsChanged { theme: updated });
        Ok(())
    }
}

impl BrowserCore for StubBackend {
    fn model(&self) -> BrowserModel {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .model
            .clone()
    }

    fn subscribe(&self) -> Subscription {
        self.bus.subscribe()
    }

    fn invoke(&self, command: Command) -> CommandResult {
        let name = command.name().to_string();
        let tab_source = self.current_tab_source();

        match command {
            Command::TabNew { url, after, activate } => {
                if let Some(source) = tab_source {
                    source.new_tab(url.as_deref(), after, activate)
                } else {
                    self.handle_stub_tab_new(url, after, activate)
                }
            }
            Command::TabClose { tab_id, force } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.close_tab(target, force)
                } else {
                    self.handle_stub_tab_close(target)
                }
            }
            Command::TabActivate { tab_id } => {
                let target = self.resolve_target_tab(Some(tab_id), &name)?;
                if let Some(source) = tab_source {
                    source.activate_tab(target)
                } else {
                    self.feed_tab_activated(Some(target));
                    Ok(())
                }
            }
            Command::TabNext | Command::TabPrevious => {
                let delta = if matches!(command, Command::TabNext) { 1 } else { -1 };
                let Some(next_id) = self.adjacent_tab_id(delta) else {
                    return Ok(());
                };
                if let Some(source) = tab_source {
                    source.activate_tab(next_id)
                } else {
                    self.feed_tab_activated(Some(next_id));
                    Ok(())
                }
            }
            Command::LocationFocus => {
                self.bus.publish(Event::LocationFocusRequested);
                Ok(())
            }
            Command::NavigationBack { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.navigate_back(target)
                } else {
                    self.feed_tab_updated(
                        target,
                        TabUpdate {
                            can_navigate_forward: Some(true),
                            ..TabUpdate::default()
                        },
                    );
                    Ok(())
                }
            }
            Command::NavigationForward { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.navigate_forward(target)
                } else {
                    self.feed_tab_updated(
                        target,
                        TabUpdate {
                            can_navigate_back: Some(true),
                            ..TabUpdate::default()
                        },
                    );
                    Ok(())
                }
            }
            Command::NavigationReload { tab_id, bypass_cache } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.reload(target, bypass_cache)
                } else {
                    self.feed_tab_updated(
                        target,
                        TabUpdate {
                            is_loading: Some(false),
                            ..TabUpdate::default()
                        },
                    );
                    Ok(())
                }
            }
            Command::NavigationStop { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.stop_loading(target)
                } else {
                    self.feed_tab_updated(
                        target,
                        TabUpdate {
                            is_loading: Some(false),
                            ..TabUpdate::default()
                        },
                    );
                    Ok(())
                }
            }
            Command::NavigationLoadUrl { tab_id, input } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.load_url(target, &input)
                } else {
                    self.handle_stub_load_url(target, input)
                }
            }
            Command::SpaceSwitch { space_id } => self.handle_space_switch(space_id),
            Command::SplitAdd { tab_id, direction } => self.handle_split_add(tab_id, direction),
            Command::PanelToggle { panel_id } => self.handle_panel_toggle(panel_id),
            Command::AgentSend {
                conversation_id,
                agent_id,
                prompt,
            } => self.handle_agent_send(conversation_id, agent_id, prompt),
            Command::WebChatAsk { site_id, prompt } => self.handle_webchat_ask(site_id, prompt),
            Command::ThemeSet { scheme, overrides } => self.handle_theme_set(scheme, overrides),
            Command::DebugCycleInputState => {
                self.cycle_sample_input_state();
                Ok(())
            }
            Command::Custom { name, args } => {
                eprintln!("zendia: stub backend does not handle command `{name}` (args: {args:?})");
                Err(CommandError::UnknownCommand(name))
            }
        }
    }

    fn commands(&self) -> Vec<CommandDescriptor> {
        builtin_command_descriptors()
    }
}

fn sample_contributions() -> Contributions {
    Contributions {
        toolbar_items: vec![
            ToolbarItem {
                id: ContributionId::new("zendia.chat"),
                title: "Chat".to_string(),
                tooltip: Some("Open Agent Chat".to_string()),
                icon: "chat".to_string(),
                command: "panel.toggle".to_string(),
                command_args: vec![("panel_id".to_string(), "zendia.agent".to_string())],
                placement: ToolbarPlacement::NavigationEnd,
                order: 10,
                badge: None,
                visible: true,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
            ToolbarItem {
                id: ContributionId::new("zendia.agents"),
                title: "Agents".to_string(),
                tooltip: Some("Show All Agent Runs".to_string()),
                icon: "agents".to_string(),
                command: "panel.toggle".to_string(),
                command_args: vec![("panel_id".to_string(), "zendia.agents".to_string())],
                placement: ToolbarPlacement::NavigationEnd,
                order: 20,
                badge: Some(ToolbarBadge {
                    text: "1".to_string(),
                    tone: BadgeTone::Warning,
                }),
                visible: true,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
            ToolbarItem {
                id: ContributionId::new("zendia.webchat"),
                title: "Web chat".to_string(),
                tooltip: Some("Open Web Chat Panel".to_string()),
                icon: "webchat".to_string(),
                command: "panel.toggle".to_string(),
                command_args: vec![("panel_id".to_string(), "zendia.webchat".to_string())],
                placement: ToolbarPlacement::NavigationEnd,
                order: 30,
                badge: None,
                visible: true,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
        ],
        menu_entries: vec![
            MenuEntry {
                id: ContributionId::new("zendia.menu.new_tab"),
                location: MenuLocation::AppMenu,
                label: "New Tab".to_string(),
                icon: Some("plus".to_string()),
                command: "tab.new".to_string(),
                command_args: Vec::new(),
                shortcut_hint: Some("Ctrl+T".to_string()),
                group: "tabs".to_string(),
                order: 10,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
            MenuEntry {
                id: ContributionId::new("zendia.menu.ask_chatgpt"),
                location: MenuLocation::PageContext,
                label: "Ask ChatGPT".to_string(),
                icon: Some("webchat".to_string()),
                command: "webchat.ask".to_string(),
                command_args: vec![("site_id".to_string(), "chatgpt".to_string())],
                shortcut_hint: None,
                group: "zendia".to_string(),
                order: 20,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
        ],
        palette_entries: vec![
            PaletteEntry {
                id: ContributionId::new("zendia.palette.new_tab"),
                title: "New Tab".to_string(),
                detail: Some("Open a new tab in the current space".to_string()),
                category: "Tabs".to_string(),
                icon: Some("plus".to_string()),
                command: "tab.new".to_string(),
                command_args: Vec::new(),
                shortcut_hint: Some("Ctrl+T".to_string()),
                source: ContributionSource::BuiltIn,
            },
            PaletteEntry {
                id: ContributionId::new("zendia.palette.toggle_chat"),
                title: "Toggle Agent Panel".to_string(),
                detail: Some("Open or close the active agent conversation".to_string()),
                category: "Agents".to_string(),
                icon: Some("chat".to_string()),
                command: "panel.toggle".to_string(),
                command_args: vec![("panel_id".to_string(), "zendia.agent".to_string())],
                shortcut_hint: None,
                source: ContributionSource::BuiltIn,
            },
            PaletteEntry {
                id: ContributionId::new("zendia.palette.toggle_webchat"),
                title: "Toggle Web Chat Panel".to_string(),
                detail: Some("Open ChatGPT, Gemini, or another chat site".to_string()),
                category: "Web chat".to_string(),
                icon: Some("webchat".to_string()),
                command: "panel.toggle".to_string(),
                command_args: vec![("panel_id".to_string(), "zendia.webchat".to_string())],
                shortcut_hint: None,
                source: ContributionSource::BuiltIn,
            },
        ],
        side_panels: vec![
            SidePanelEntry {
                id: SidePanelId::new("zendia.agent"),
                title: "Agent".to_string(),
                icon: "chat".to_string(),
                default_layout: SidePanelLayout::Float,
                default_side: SidePanelSide::Right,
                default_width: 420.0,
                source: ContributionSource::BuiltIn,
            },
            SidePanelEntry {
                id: SidePanelId::new("zendia.agents"),
                title: "Agents".to_string(),
                icon: "agents".to_string(),
                default_layout: SidePanelLayout::Float,
                default_side: SidePanelSide::Right,
                default_width: 400.0,
                source: ContributionSource::BuiltIn,
            },
            SidePanelEntry {
                id: SidePanelId::new("zendia.webchat"),
                title: "Web chat".to_string(),
                icon: "webchat".to_string(),
                default_layout: SidePanelLayout::Split,
                default_side: SidePanelSide::Right,
                default_width: 460.0,
                source: ContributionSource::BuiltIn,
            },
        ],
    }
}

fn sample_model(include_sample_tabs: bool) -> BrowserModel {
    let work_space = SpaceId(1);
    let research_space = SpaceId(2);

    let spaces = vec![
        Space {
            id: work_space,
            name: "Work".to_string(),
            icon: "briefcase".to_string(),
            gradient: SpaceGradient {
                stops: vec![(34, 36, 54), (47, 51, 77)],
                angle_deg: 135.0,
                opacity: 0.9,
            },
        },
        Space {
            id: research_space,
            name: "Research".to_string(),
            icon: "book".to_string(),
            gradient: SpaceGradient {
                stops: vec![(30, 42, 58), (45, 63, 118)],
                angle_deg: 150.0,
                opacity: 0.9,
            },
        },
    ];

    let (tabs, active_tab, essentials, pinned_tabs, folders, split_groups, media_sessions) = if include_sample_tabs {
        let essential_tab = TabId(1);
        let pinned_tab = TabId(2);
        let folder_tab = TabId(3);
        let split_left = TabId(4);
        let split_right = TabId(5);
        let research_tab = TabId(6);
        let folder_id = FolderId(1);
        let split_id = SplitGroupId(1);

        let tabs = vec![
            Tab {
                id: essential_tab,
                view_id: None,
                space_id: None,
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Essential,
                title: "Mail".to_string(),
                url: "https://mail.example.org/".to_string(),
                favicon: Some("mail".to_string()),
                is_loading: false,
                can_navigate_back: false,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: pinned_tab,
                view_id: None,
                space_id: Some(work_space),
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Pinned,
                title: "Ladybird".to_string(),
                url: "https://ladybird.org/".to_string(),
                favicon: Some("globe".to_string()),
                is_loading: false,
                can_navigate_back: true,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: folder_tab,
                view_id: None,
                space_id: Some(work_space),
                folder_id: Some(folder_id),
                split_group_id: None,
                kind: TabKind::Normal,
                title: "Freya Docs".to_string(),
                url: "https://book.freyaui.dev/".to_string(),
                favicon: Some("book".to_string()),
                is_loading: false,
                can_navigate_back: false,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: split_left,
                view_id: None,
                space_id: Some(work_space),
                folder_id: None,
                split_group_id: Some(split_id),
                kind: TabKind::Normal,
                title: "Rust Reference".to_string(),
                url: "https://doc.rust-lang.org/reference/".to_string(),
                favicon: Some("code".to_string()),
                is_loading: false,
                can_navigate_back: true,
                can_navigate_forward: true,
                is_audible: true,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: split_right,
                view_id: None,
                space_id: Some(work_space),
                folder_id: None,
                split_group_id: Some(split_id),
                kind: TabKind::Normal,
                title: "Zendia Architecture".to_string(),
                url: "https://zendia.local/architecture".to_string(),
                favicon: Some("compass".to_string()),
                is_loading: false,
                can_navigate_back: false,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: research_tab,
                view_id: None,
                space_id: Some(research_space),
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Normal,
                title: "HTML Living Standard".to_string(),
                url: "https://html.spec.whatwg.org/".to_string(),
                favicon: Some("book".to_string()),
                is_loading: false,
                can_navigate_back: false,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
        ];

        let essentials = vec![Essential {
            tab_id: essential_tab,
            order: 0,
        }];
        let pinned_tabs = vec![PinnedTab {
            tab_id: pinned_tab,
            space_id: work_space,
            order: 0,
        }];
        let folders = vec![Folder {
            id: folder_id,
            space_id: work_space,
            name: "Docs".to_string(),
            icon: "folder".to_string(),
            collapsed: false,
            tab_ids: vec![folder_tab],
        }];
        let split_groups = vec![SplitGroup {
            id: split_id,
            space_id: work_space,
            direction: SplitDirection::Horizontal,
            tab_ids: vec![split_left, split_right],
            ratios: vec![0.5, 0.5],
        }];
        let media_sessions = vec![MediaSession {
            id: MediaSessionId(1),
            tab_id: split_left,
            title: "Focus Radio — Night Coding".to_string(),
            artist: "Zendia Sample Stream".to_string(),
            is_playing: true,
            is_muted: false,
            position_secs: 142.0,
            duration_secs: Some(360.0),
        }];

        (
            tabs,
            Some(split_left),
            essentials,
            pinned_tabs,
            folders,
            split_groups,
            media_sessions,
        )
    } else {
        (
            Vec::new(),
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    };

    let downloads = vec![
        Download {
            id: DownloadId(1),
            filename: "ladybird-architecture.pdf".to_string(),
            url: "https://ladybird.org/docs/architecture.pdf".to_string(),
            bytes_received: 2_450_000,
            total_bytes: Some(2_450_000),
            status: DownloadStatus::Completed,
        },
        Download {
            id: DownloadId(2),
            filename: "freya-0.5.4.tar.gz".to_string(),
            url: "https://crates.io/api/v1/crates/freya/0.5.4/download".to_string(),
            bytes_received: 640_000,
            total_bytes: Some(1_280_000),
            status: DownloadStatus::InProgress,
        },
    ];

    let history = vec![
        HistoryEntry {
            id: HistoryEntryId(1),
            url: "https://ladybird.org/".to_string(),
            title: "Ladybird Browser".to_string(),
            visited_at_unix: 1_763_990_400,
            visit_count: 12,
        },
        HistoryEntry {
            id: HistoryEntryId(2),
            url: "https://book.freyaui.dev/".to_string(),
            title: "Freya GUI Book".to_string(),
            visited_at_unix: 1_763_986_800,
            visit_count: 5,
        },
    ];

    let claude_agent = AgentId::new("claude-code");
    let codex_agent = AgentId::new("codex");
    let agents = vec![
        Agent {
            id: claude_agent.clone(),
            name: "Claude".to_string(),
            description: "General browsing and coding agent".to_string(),
            icon: "agent-claude".to_string(),
            accent_rgb: (217, 119, 87),
            available: true,
        },
        Agent {
            id: codex_agent,
            name: "Codex".to_string(),
            description: "Fast page inspection and automation".to_string(),
            icon: "agent-codex".to_string(),
            accent_rgb: (130, 170, 255),
            available: true,
        },
    ];

    let primary_model = ModelId::new("claude-sonnet-4");
    let models = vec![
        ModelInfo {
            id: primary_model.clone(),
            name: "Claude Sonnet 4".to_string(),
            provider: "Anthropic".to_string(),
            context_window: 200_000,
            effort: ModelEffort::Medium,
        },
        ModelInfo {
            id: ModelId::new("gpt-5"),
            name: "GPT-5".to_string(),
            provider: "OpenAI".to_string(),
            context_window: 256_000,
            effort: ModelEffort::High,
        },
        ModelInfo {
            id: ModelId::new("gemini-2.5-pro"),
            name: "Gemini 2.5 Pro".to_string(),
            provider: "Google".to_string(),
            context_window: 1_000_000,
            effort: ModelEffort::Medium,
        },
    ];

    let conversations = vec![Conversation {
        id: ConversationId(1),
        agent_id: claude_agent,
        tab_id: active_tab,
        title: "Summarize release notes".to_string(),
        model_id: primary_model,
        status: ConversationStatus::WaitingForUser,
        steps: vec![
            ConversationStep {
                id: StepId(1),
                kind: StepKind::UserPrompt,
                summary: "Summarize the main changes on this page".to_string(),
                detail: None,
                status: StepStatus::Completed,
            },
            ConversationStep {
                id: StepId(2),
                kind: StepKind::ToolCall {
                    tool: "page.snapshot".to_string(),
                },
                summary: "Read accessibility tree".to_string(),
                detail: Some("Captured 42 landmarks and headings".to_string()),
                status: StepStatus::Completed,
            },
            ConversationStep {
                id: StepId(3),
                kind: StepKind::Confirmation {
                    prompt: "Open three linked RFC tabs in the background?".to_string(),
                    resolved: None,
                },
                summary: "Waiting for permission to open tabs".to_string(),
                detail: None,
                status: StepStatus::WaitingForUser,
            },
        ],
    }];

    let chat_sites = vec![
        ChatSite {
            id: ChatSiteId::new("chatgpt"),
            name: "ChatGPT".to_string(),
            url: "https://chatgpt.com/".to_string(),
            icon: "chatgpt".to_string(),
        },
        ChatSite {
            id: ChatSiteId::new("gemini"),
            name: "Gemini".to_string(),
            url: "https://gemini.google.com/".to_string(),
            icon: "gemini".to_string(),
        },
        ChatSite {
            id: ChatSiteId::new("claude"),
            name: "Claude".to_string(),
            url: "https://claude.ai/".to_string(),
            icon: "claude".to_string(),
        },
    ];

    let services = vec![
        ServiceState {
            service: ServiceId::Hub,
            status: ServiceStatus::Running,
        },
        ServiceState {
            service: ServiceId::InputServer,
            status: ServiceStatus::Stopped {
                reason: Some("stub mode".to_string()),
            },
        },
        ServiceState {
            service: ServiceId::AgentServer,
            status: ServiceStatus::Stopped {
                reason: Some("stub mode".to_string()),
            },
        },
        ServiceState {
            service: ServiceId::PluginHost,
            status: ServiceStatus::Stopped {
                reason: Some("stub mode".to_string()),
            },
        },
        ServiceState {
            service: ServiceId::ExtensionServer,
            status: ServiceStatus::Stopped {
                reason: Some("stub mode".to_string()),
            },
        },
    ];

    let windows = vec![Window {
        id: WindowId(1),
        title: "Zendia".to_string(),
        active_space: Some(work_space),
        active_tab,
    }];

    BrowserModel {
        windows,
        spaces,
        active_space: Some(work_space),
        tabs,
        active_tab,
        essentials,
        pinned_tabs,
        folders,
        split_groups,
        downloads,
        history,
        media_sessions,
        contributions: sample_contributions(),
        agents,
        conversations,
        models,
        chat_sites,
        services,
        input_mode: InputMode::Normal,
        pending_keys: Vec::new(),
        which_key: None,
        theme: ThemeSettings::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::TryRecvError;

    struct RecordingTabSource {
        core: Arc<StubBackend>,
        calls: Mutex<Vec<String>>,
        next_view_id: Mutex<u64>,
    }

    impl RecordingTabSource {
        fn new(core: Arc<StubBackend>) -> Self {
            Self {
                core,
                calls: Mutex::new(Vec::new()),
                next_view_id: Mutex::new(100),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl TabSource for RecordingTabSource {
        fn new_tab(&self, url: Option<&str>, after: Option<TabId>, activate: bool) -> CommandResult {
            self.calls
                .lock()
                .unwrap()
                .push(format!("new_tab:{}:{activate}", url.unwrap_or("")));
            let mut next = self.next_view_id.lock().unwrap();
            let view_id = *next;
            *next += 1;
            let tab = Tab {
                id: TabId(view_id),
                view_id: Some(view_id),
                space_id: None,
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Normal,
                title: "Hub Page".to_string(),
                url: url.unwrap_or("about:newtab").to_string(),
                favicon: None,
                is_loading: false,
                can_navigate_back: false,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            };
            self.core.feed_tab_opened(tab, after, activate);
            Ok(())
        }

        fn close_tab(&self, tab_id: TabId, force: bool) -> CommandResult {
            self.calls
                .lock()
                .unwrap()
                .push(format!("close_tab:{}:{force}", tab_id.0));
            let next = self.core.feed_tab_closed(tab_id);
            if let Some(next_id) = next {
                self.core.feed_tab_activated(Some(next_id));
            }
            Ok(())
        }

        fn activate_tab(&self, tab_id: TabId) -> CommandResult {
            self.calls.lock().unwrap().push(format!("activate_tab:{}", tab_id.0));
            self.core.feed_tab_activated(Some(tab_id));
            Ok(())
        }

        fn navigate_back(&self, tab_id: TabId) -> CommandResult {
            self.calls.lock().unwrap().push(format!("back:{}", tab_id.0));
            Ok(())
        }

        fn navigate_forward(&self, tab_id: TabId) -> CommandResult {
            self.calls.lock().unwrap().push(format!("forward:{}", tab_id.0));
            Ok(())
        }

        fn reload(&self, tab_id: TabId, bypass_cache: bool) -> CommandResult {
            self.calls
                .lock()
                .unwrap()
                .push(format!("reload:{}:{bypass_cache}", tab_id.0));
            Ok(())
        }

        fn stop_loading(&self, tab_id: TabId) -> CommandResult {
            self.calls.lock().unwrap().push(format!("stop:{}", tab_id.0));
            Ok(())
        }

        fn load_url(&self, tab_id: TabId, input: &str) -> CommandResult {
            self.calls
                .lock()
                .unwrap()
                .push(format!("load_url:{}:{input}", tab_id.0));
            self.core.feed_tab_updated(
                tab_id,
                TabUpdate {
                    url: Some(input.to_string()),
                    ..TabUpdate::default()
                },
            );
            Ok(())
        }
    }

    #[test]
    fn sample_model_contains_all_expected_sections() {
        let core = StubBackend::new();
        let model = core.model();

        assert!(!model.windows.is_empty());
        assert!(model.spaces.len() >= 2);
        assert!(!model.tabs.is_empty());
        assert!(!model.essentials.is_empty());
        assert!(!model.pinned_tabs.is_empty());
        assert!(!model.folders.is_empty());
        assert!(!model.split_groups.is_empty());
        assert!(!model.downloads.is_empty());
        assert!(!model.history.is_empty());
        assert!(!model.media_sessions.is_empty());
        assert!(!model.agents.is_empty());
        assert!(!model.conversations.is_empty());
        assert!(!model.conversations[0].steps.is_empty());
        assert!(!model.models.is_empty());
        assert!(model.chat_sites.iter().any(|site| site.url.contains("chatgpt.com")));
        assert!(
            model
                .chat_sites
                .iter()
                .any(|site| site.url.contains("gemini.google.com"))
        );
        assert!(!model.services.is_empty());

        let toolbar_ids: Vec<&str> = model
            .contributions
            .toolbar_items
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        assert!(toolbar_ids.contains(&"zendia.chat"));
        assert!(toolbar_ids.contains(&"zendia.agents"));
        assert!(toolbar_ids.contains(&"zendia.webchat"));
        assert!(!model.contributions.menu_entries.is_empty());
        assert!(!model.contributions.palette_entries.is_empty());
        assert!(!model.contributions.side_panels.is_empty());
    }

    #[test]
    fn command_descriptors_enumerate_core_commands() {
        let core = StubBackend::new();
        let names: Vec<String> = core.commands().into_iter().map(|desc| desc.name).collect();
        for required in [
            "tab.new",
            "tab.close",
            "tab.activate",
            "navigation.back",
            "navigation.forward",
            "navigation.reload",
            "navigation.stop",
            "navigation.load_url",
            "space.switch",
            "split.add",
            "panel.toggle",
            "agent.send",
            "webchat.ask",
            "theme.set",
        ] {
            assert!(
                names.iter().any(|name| name == required),
                "missing descriptor {required}"
            );
        }
    }

    #[test]
    fn standalone_commands_publish_events_that_keep_snapshot_in_sync() {
        let core = StubBackend::new();
        let mut view_model = core.model();
        let sub = core.subscribe();

        core.invoke(Command::TabNew {
            url: Some("https://example.org/".to_string()),
            after: None,
            activate: true,
        })
        .unwrap();
        core.invoke(Command::NavigationLoadUrl {
            tab_id: None,
            input: "ladybird.org".to_string(),
        })
        .unwrap();
        core.invoke(Command::SpaceSwitch { space_id: SpaceId(2) }).unwrap();
        core.invoke(Command::AgentSend {
            conversation_id: Some(ConversationId(1)),
            agent_id: None,
            prompt: "Continue".to_string(),
        })
        .unwrap();
        core.invoke(Command::PanelToggle {
            panel_id: SidePanelId::new("zendia.agent"),
        })
        .unwrap();

        let mut saw_panel_toggle = false;
        while let Ok(event) = sub.try_recv() {
            if matches!(event, Event::PanelToggled { .. }) {
                saw_panel_toggle = true;
            }
            view_model.apply_event(&event);
        }

        assert!(saw_panel_toggle);
        assert_eq!(view_model, core.model());
        assert_eq!(view_model.active_space, Some(SpaceId(2)));
        assert_eq!(view_model.active_tab().unwrap().url, "https://ladybird.org");
    }

    #[test]
    fn tab_close_activates_adjacent_tab_and_cleans_references() {
        let core = StubBackend::new();
        let sub = core.subscribe();
        let active_before = core.model().active_tab.unwrap();
        let count_before = core.model().tabs.len();

        core.invoke(Command::TabClose {
            tab_id: Some(active_before),
            force: false,
        })
        .unwrap();

        let after = core.model();
        assert_eq!(after.tabs.len(), count_before - 1);
        assert!(after.tab(active_before).is_none());
        assert!(after.active_tab.is_some());
        assert_ne!(after.active_tab, Some(active_before));

        let mut saw_closed = false;
        let mut saw_activated = false;
        while let Ok(event) = sub.try_recv() {
            match event {
                Event::TabClosed { tab_id } if tab_id == active_before => saw_closed = true,
                Event::TabActivated { tab_id: Some(_) } => saw_activated = true,
                _ => {}
            }
        }
        assert!(saw_closed);
        assert!(saw_activated);
    }

    #[test]
    fn unhandled_custom_command_returns_unknown_error() {
        let core = StubBackend::new();
        let result = core.invoke(Command::Custom {
            name: "future.feature".to_string(),
            args: vec![("k".to_string(), "v".to_string())],
        });
        assert_eq!(result, Err(CommandError::UnknownCommand("future.feature".to_string())));
    }

    #[test]
    fn external_tab_source_receives_commands_and_feeds_tabs_without_deadlock() {
        let core = Arc::new(StubBackend::new_for_external_tabs());
        assert!(core.model().tabs.is_empty());

        let source = Arc::new(RecordingTabSource::new(Arc::clone(&core)));
        core.attach_tab_source(Arc::clone(&source) as Arc<dyn TabSource>);

        let sub = core.subscribe();
        core.invoke(Command::TabNew {
            url: Some("https://ladybird.org/".to_string()),
            after: None,
            activate: true,
        })
        .unwrap();
        core.invoke(Command::NavigationLoadUrl {
            tab_id: None,
            input: "https://zendia.dev/".to_string(),
        })
        .unwrap();
        core.invoke(Command::NavigationReload {
            tab_id: None,
            bypass_cache: true,
        })
        .unwrap();
        core.invoke(Command::TabClose {
            tab_id: None,
            force: true,
        })
        .unwrap();

        assert_eq!(
            source.calls(),
            vec![
                "new_tab:https://ladybird.org/:true",
                "load_url:100:https://zendia.dev/",
                "reload:100:true",
                "close_tab:100:true",
            ]
        );
        assert!(core.model().tabs.is_empty());
        assert_eq!(core.model().active_tab, None);

        let mut events = Vec::new();
        while let Ok(event) = sub.try_recv() {
            events.push(event);
        }
        assert!(events.iter().any(|e| matches!(e, Event::TabOpened { .. })));
        assert!(events.iter().any(|e| matches!(e, Event::TabUpdated { .. })));
        assert!(events.iter().any(|e| matches!(e, Event::TabClosed { .. })));
        assert_eq!(sub.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn event_bus_fans_out_to_multiple_subscribers() {
        let core = StubBackend::new();
        let sub_a = core.subscribe();
        let sub_b = core.subscribe();

        core.set_input_mode(InputMode::Explorer);

        assert_eq!(
            sub_a.try_recv(),
            Ok(Event::ModeChanged {
                mode: InputMode::Explorer
            })
        );
        assert_eq!(
            sub_b.try_recv(),
            Ok(Event::ModeChanged {
                mode: InputMode::Explorer
            })
        );
    }

    #[test]
    fn theme_set_updates_model_and_publishes_settings_changed() {
        use crate::model::{ThemeColorOverride, ThemeGlassOverrides, ThemeRoleOverride};

        let core = StubBackend::new();
        let mut view_model = core.model();
        let sub = core.subscribe();

        assert_eq!(view_model.theme.scheme, ThemeScheme::System);

        let mut overrides = ThemeOverrides {
            glass: ThemeGlassOverrides {
                opacity: Some(0.65),
                blur: Some(32.0),
            },
            ..ThemeOverrides::default()
        };
        overrides
            .palette
            .insert("pink".to_string(), ThemeColorOverride::Single("#ff79c6".to_string()));
        overrides
            .roles
            .insert("accent".to_string(), ThemeRoleOverride::Single("teal".to_string()));
        overrides.agents.insert("claude".to_string(), "#e07a5f".to_string());

        core.invoke(Command::ThemeSet {
            scheme: Some(ThemeScheme::Light),
            overrides: Some(overrides.clone()),
        })
        .unwrap();

        let event = sub.try_recv().unwrap();
        assert_eq!(
            event,
            Event::SettingsChanged {
                theme: ThemeSettings {
                    scheme: ThemeScheme::Light,
                    overrides: overrides.clone(),
                }
            }
        );
        assert!(view_model.apply_event(&event));
        assert_eq!(view_model.theme, core.model().theme);

        // Partial update changes only scheme while preserving existing overrides.
        core.invoke(Command::ThemeSet {
            scheme: Some(ThemeScheme::Dark),
            overrides: None,
        })
        .unwrap();
        assert_eq!(core.model().theme.scheme, ThemeScheme::Dark);
        assert_eq!(core.model().theme.overrides, overrides);
    }

    #[test]
    fn theme_set_rejects_out_of_range_glass_values() {
        use crate::model::ThemeGlassOverrides;

        let core = StubBackend::new();
        let bad_opacity = core.invoke(Command::ThemeSet {
            scheme: None,
            overrides: Some(ThemeOverrides {
                glass: ThemeGlassOverrides {
                    opacity: Some(1.5),
                    blur: None,
                },
                ..ThemeOverrides::default()
            }),
        });
        assert!(matches!(bad_opacity, Err(CommandError::InvalidArgument { .. })));

        let bad_blur = core.invoke(Command::ThemeSet {
            scheme: None,
            overrides: Some(ThemeOverrides {
                glass: ThemeGlassOverrides {
                    opacity: None,
                    blur: Some(-4.0),
                },
                ..ThemeOverrides::default()
            }),
        });
        assert!(matches!(bad_blur, Err(CommandError::InvalidArgument { .. })));
    }

    #[test]
    fn tab_next_and_previous_wrap_around() {
        let core = StubBackend::new();
        let initial_tabs: Vec<TabId> = core.model().tabs.iter().map(|tab| tab.id).collect();
        assert!(initial_tabs.len() >= 2);
        let first = initial_tabs[0];
        let last = *initial_tabs.last().unwrap();

        core.invoke(Command::TabActivate { tab_id: first }).unwrap();
        assert_eq!(core.model().active_tab, Some(first));

        core.invoke(Command::TabPrevious).unwrap();
        assert_eq!(core.model().active_tab, Some(last));

        core.invoke(Command::TabNext).unwrap();
        assert_eq!(core.model().active_tab, Some(first));
    }

    #[test]
    fn location_focus_and_debug_cycle_publish_input_events() {
        let core = StubBackend::new();
        let mut view_model = core.model();
        let sub = core.subscribe();

        core.invoke(Command::LocationFocus).unwrap();
        assert_eq!(sub.try_recv(), Ok(Event::LocationFocusRequested));

        core.invoke(Command::DebugCycleInputState).unwrap();
        let mut events = Vec::new();
        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
            events.push(event);
        }
        assert!(events.iter().any(|e| matches!(e, Event::ModeChanged { .. })));
        assert!(events.iter().any(|e| matches!(e, Event::PendingKeysChanged { .. })));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::WhichKeyChanged { state: Some(_) }))
        );
        assert_eq!(view_model.pending_keys, vec!["<Space>".to_string()]);
        assert!(view_model.which_key.is_some());
        assert_eq!(view_model, core.model());

        // Cycle until Insert mode appears.
        core.invoke(Command::DebugCycleInputState).unwrap();
        core.invoke(Command::DebugCycleInputState).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Insert);
        assert!(core.model().pending_keys.is_empty());
        assert!(core.model().which_key.is_none());
    }

    #[test]
    fn command_from_name_and_args_parses_typed_commands() {
        assert_eq!(
            Command::from_name_and_args("tab.new", &[("activate".to_string(), "false".to_string())]).unwrap(),
            Command::TabNew {
                url: None,
                after: None,
                activate: false,
            }
        );
        assert_eq!(
            Command::from_name_and_args("navigation.reload", &[("bypass_cache".to_string(), "true".to_string())])
                .unwrap(),
            Command::NavigationReload {
                tab_id: None,
                bypass_cache: true,
            }
        );
        assert_eq!(Command::from_name_and_args("tab.next", &[]).unwrap(), Command::TabNext);
        assert_eq!(
            Command::from_name_and_args("tab.previous", &[]).unwrap(),
            Command::TabPrevious
        );
        assert_eq!(
            Command::from_name_and_args("location.focus", &[]).unwrap(),
            Command::LocationFocus
        );
        assert_eq!(
            Command::from_name_and_args("debug.cycle_input_state", &[]).unwrap(),
            Command::DebugCycleInputState
        );
        assert!(matches!(
            Command::from_name_and_args(
                "navigation.reload",
                &[("bypass_cache".to_string(), "maybe".to_string())]
            ),
            Err(CommandError::InvalidArgument { .. })
        ));
    }
}
