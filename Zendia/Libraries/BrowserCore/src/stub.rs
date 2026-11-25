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
use crate::address_bar::{SearchEngine, builtin_palette_entries, resolve_navigation_url};
use crate::commands::{Command, CommandDescriptor, CommandError, CommandResult, builtin_command_descriptors};
use crate::contributions::{
    ContributionId, ContributionSource, Contributions, MenuEntryKind, SidePanelEntry, SidePanelId, SidePanelLayout,
    SidePanelSide, ToolbarArea, ToolbarItem, ToolbarItemKind, ToolbarPlacement, ToolbarPlacementRules,
};
use crate::events::{Event, EventBus, Subscription};
use crate::input_state::StatuslineSettings;
use crate::menus::{APP_MENU_VIM_MODE_ID, builtin_menu_entries};
use crate::model::{
    Agent, AgentId, Bookmark, BookmarkId, BrowserModel, ChatSite, ChatSiteId, Conversation, ConversationId,
    ConversationStatus, ConversationStep, Download, DownloadId, DownloadStatus, Essential, Folder, FolderId,
    HistoryEntry, HistoryEntryId, InputMode, LayoutSettings, MediaSession, ModelEffort, ModelId, ModelInfo,
    NotificationId, PinnedTab, ServiceId, ServiceState, ServiceStatus, SidebarNotification, SidebarNotificationLink,
    SidebarSide, Space, SpaceGradient, SpaceId, SplitDirection, SplitGroup, SplitGroupId, StepId, StepKind, StepStatus,
    Tab, TabId, TabKind, TabMoveTarget, ThemeOverrides, ThemeScheme, ThemeSettings, ToolbarLayout, WhichKeyState,
    Window, WindowId,
};
use crate::picker::{PickerRequest, PickerRequestId};
use crate::region::RegionCapture;
use crate::site_info::{
    PermissionState, SiteDataSummary, SiteExtension, SiteInfo, SitePermission, SitePermissionKind, origin_for_url,
};
use crate::tab_source::{TabSource, TabUpdate};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

mod focus;
mod input_state;
mod page_layouts;
mod pickers;

const MAX_ESSENTIALS: usize = 12;

struct StubState {
    model: BrowserModel,
    next_tab_id: u64,
    next_folder_id: u64,
    next_split_id: u64,
    next_conversation_id: u64,
    next_step_id: u64,
    debug_input_step: usize,
    /// The last tab opened, so a command that opens a tab through the engine can find it.
    last_opened_tab: Option<TabId>,
    last_toast_id: u64,
    registered_pickers: BTreeMap<PickerRequestId, PickerRequest>,
    active_picker: Option<PickerRequest>,
    last_region_capture: Option<RegionCapture>,
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
        let next_folder_id = model.folders.iter().map(|folder| folder.id.0).max().unwrap_or(0) + 1;
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
                next_folder_id,
                next_split_id,
                next_conversation_id,
                next_step_id,
                debug_input_step: 0,
                last_opened_tab: None,
                last_toast_id: 0,
                registered_pickers: BTreeMap::new(),
                active_picker: None,
                last_region_capture: None,
            }),
            tab_source: Mutex::new(None),
            bus: EventBus::new(),
        }
    }

    /// Replaces the stub model snapshot wholesale (preserving current theme scheme) and publishes
    /// [`Event::ModelReplaced`]. Used by gallery states to reset deterministic fixtures.
    pub fn replace_model(&self, mut model: BrowserModel) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            model.theme = state.model.theme.clone();
            state.next_tab_id = model.tabs.iter().map(|tab| tab.id.0).max().unwrap_or(0) + 1;
            state.next_folder_id = model.folders.iter().map(|folder| folder.id.0).max().unwrap_or(0) + 1;
            state.next_split_id = model.split_groups.iter().map(|group| group.id.0).max().unwrap_or(0) + 1;
            state.next_conversation_id = model.conversations.iter().map(|conv| conv.id.0).max().unwrap_or(0) + 1;
            state.next_step_id = model
                .conversations
                .iter()
                .flat_map(|conv| conv.steps.iter().map(|step| step.id.0))
                .max()
                .unwrap_or(0)
                + 1;
            state.model = model;
        }
        self.bus.publish(Event::ModelReplaced);
    }

    /// Resets the stub backend to its default populated sample model (preserving current theme).
    pub fn reset_to_sample(&self) {
        self.replace_model(sample_model(true));
    }

    /// Publishes a sidebar notification card into the model.
    pub fn show_notification(&self, notification: SidebarNotification) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(existing) = state
                .model
                .sidebar_notifications
                .iter_mut()
                .find(|item| item.id == notification.id)
            {
                *existing = notification.clone();
            } else {
                state.model.sidebar_notifications.push(notification.clone());
            }
        }
        self.bus.publish(Event::SidebarNotificationAdded { notification });
    }

    /// Updates the default search engine owned by the core (called by adapters reading profile settings).
    pub fn set_search_engine(&self, engine: SearchEngine) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.search_engine == engine {
                return;
            }
            state.model.search_engine = engine.clone();
        }
        self.bus.publish(Event::SearchEngineChanged { engine });
    }

    /// Shows or hides the empty-essentials promo card in the sidebar.
    pub fn set_essentials_promo(&self, visible: bool) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.show_essentials_promo = visible;
        }
        self.bus.publish(Event::EssentialsPromoChanged { visible });
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
            state.last_opened_tab = Some(tab.id);
            let should_activate = activate || state.model.active_tab.is_none();
            let activated = if should_activate {
                state.model.active_tab = Some(tab.id);
                if let Some(window) = state.model.windows.first_mut() {
                    window.active_tab = Some(tab.id);
                }
                if let Some(space_id) = tab.space_id
                    && let Some(space) = state.model.spaces.iter_mut().find(|s| s.id == space_id)
                {
                    space.last_active_tab = Some(tab.id);
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
            if let Some(id) = tab_id
                && let Some(space_id) = state.model.tab(id).and_then(|tab| tab.space_id)
                && let Some(space) = state.model.spaces.iter_mut().find(|s| s.id == space_id)
            {
                space.last_active_tab = Some(id);
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
            let closed_space = state.model.tabs[index].space_id;
            state.model.tabs.remove(index);
            state.model.essentials.retain(|item| item.tab_id != tab_id);
            normalize_essential_orders(&mut state.model.essentials);
            state.model.pinned_tabs.retain(|item| item.tab_id != tab_id);
            normalize_pinned_orders(&mut state.model.pinned_tabs);
            for folder in &mut state.model.folders {
                folder.tab_ids.retain(|id| *id != tab_id);
            }
            for space in &mut state.model.spaces {
                if space.last_active_tab == Some(tab_id) {
                    space.last_active_tab = None;
                }
            }
            for group in &mut state.model.split_groups {
                if let Some(position) = group.tab_ids.iter().position(|id| *id == tab_id) {
                    group.tab_ids.remove(position);
                    group.ratios = crate::split::default_ratios(group.direction, group.tab_ids.len());
                }
            }
            state.model.split_groups.retain(|group| group.tab_ids.len() > 1);
            if state.model.is_glance_tab(tab_id) {
                state.model.glance = None;
            }
            state.model.media_sessions.retain(|session| session.tab_id != tab_id);
            let was_active = state.model.active_tab == Some(tab_id);
            let next_tab = if was_active {
                state.model.active_tab = None;
                if let Some(window) = state.model.windows.first_mut() {
                    window.active_tab = None;
                }
                // Prefer another tab in the same space, then any visible tab in the active space, then any tab.
                let same_space_tab = closed_space.and_then(|space_id| {
                    state
                        .model
                        .tabs
                        .iter()
                        .find(|tab| tab.space_id == Some(space_id))
                        .map(|tab| tab.id)
                });
                same_space_tab.or_else(|| {
                    let active_order = state.model.tabs_in_active_space_order();
                    active_order.first().copied().or_else(|| {
                        state
                            .model
                            .tabs
                            .get(index.min(state.model.tabs.len().saturating_sub(1)))
                            .map(|tab| tab.id)
                    })
                })
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
        self.cycle_sample_input_state_impl();
    }

    fn adjacent_tab_id(&self, delta: isize) -> Option<TabId> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let ordered = state.model.tabs_in_active_space_order();
        let len = ordered.len();
        if len < 2 {
            return None;
        }
        let current = state
            .model
            .active_tab
            .and_then(|active_id| ordered.iter().position(|id| *id == active_id))
            .unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(len as isize) as usize;
        Some(ordered[next])
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
                custom_title: None,
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
            state.last_opened_tab = Some(id);
            let activated = if activate || state.model.active_tab.is_none() {
                state.model.active_tab = Some(id);
                if let Some(window) = state.model.windows.first_mut() {
                    window.active_tab = Some(id);
                }
                if let Some(space_id) = tab.space_id
                    && let Some(space) = state.model.spaces.iter_mut().find(|s| s.id == space_id)
                {
                    space.last_active_tab = Some(id);
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
        let engine = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.search_engine.clone()
        };
        let Some(url) = resolve_navigation_url(trimmed, &engine) else {
            return Err(CommandError::InvalidArgument {
                command: "navigation.load_url".to_string(),
                message: "location input is empty".to_string(),
            });
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

    fn resolve_site_origin(&self, origin: Option<String>, command: &str) -> Result<(String, String), CommandError> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(raw) = origin {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(CommandError::InvalidArgument {
                    command: command.to_string(),
                    message: "origin must not be empty".to_string(),
                });
            }
            return Ok((origin_for_url(trimmed), trimmed.to_string()));
        }
        let url = state
            .model
            .active_tab()
            .map(|tab| tab.url.clone())
            .unwrap_or_else(|| "about:blank".to_string());
        Ok((origin_for_url(&url), url))
    }

    fn handle_address_bar_submit(
        &self,
        input: String,
        new_tab: bool,
        tab_source: Option<Arc<dyn TabSource>>,
    ) -> CommandResult {
        let (resolved, active_tab) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(url) = resolve_navigation_url(&input, &state.model.search_engine) else {
                return Err(CommandError::InvalidArgument {
                    command: "address_bar.submit".to_string(),
                    message: "address bar input is empty".to_string(),
                });
            };
            (url, state.model.active_tab)
        };
        if new_tab {
            if let Some(source) = tab_source {
                source.new_tab(Some(&resolved), active_tab, true)
            } else {
                self.handle_stub_tab_new(Some(resolved), active_tab, true)
            }
        } else {
            let target = self.resolve_target_tab(None, "address_bar.submit")?;
            if let Some(source) = tab_source {
                source.load_url(target, &resolved)
            } else {
                self.handle_stub_load_url(target, resolved)
            }
        }
    }

    fn handle_address_bar_switch_to_tab(&self, tab_id: TabId, tab_source: Option<Arc<dyn TabSource>>) -> CommandResult {
        let space_switch = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(tab) = state.model.tab(tab_id).cloned() else {
                return Err(CommandError::TargetNotFound {
                    command: "address_bar.switch_to_tab".to_string(),
                    target: format!("tab {}", tab_id.0),
                });
            };
            if let Some(space_id) = tab.space_id
                && state.model.active_space != Some(space_id)
            {
                state.model.active_space = Some(space_id);
                if let Some(window) = state.model.windows.first_mut() {
                    window.active_space = Some(space_id);
                }
                Some(space_id)
            } else {
                None
            }
        };
        if let Some(space_id) = space_switch {
            self.bus.publish(Event::SpaceSwitched { space_id });
        }
        if let Some(source) = tab_source {
            source.activate_tab(tab_id)
        } else {
            self.feed_tab_activated(Some(tab_id));
            Ok(())
        }
    }

    fn handle_site_permission_set(
        &self,
        origin: Option<String>,
        permission: SitePermissionKind,
        perm_state: PermissionState,
    ) -> CommandResult {
        let (origin_key, url) = self.resolve_site_origin(origin, "site.permission.set")?;
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let bookmarked = state.model.is_url_bookmarked(&url);
            let entry = state
                .model
                .site_info
                .entry(origin_key)
                .or_insert_with(|| SiteInfo::for_url(&url));
            entry.set_permission(permission, perm_state);
            entry.bookmarked = bookmarked;
            entry.clone()
        };
        self.bus.publish(Event::SiteInfoUpdated { info: updated });
        Ok(())
    }

    fn handle_site_tracking_protection_toggle(&self, origin: Option<String>) -> CommandResult {
        let (origin_key, url) = self.resolve_site_origin(origin, "site.tracking_protection.toggle")?;
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let bookmarked = state.model.is_url_bookmarked(&url);
            let entry = state
                .model
                .site_info
                .entry(origin_key)
                .or_insert_with(|| SiteInfo::for_url(&url));
            entry.tracking_protection_enabled = !entry.tracking_protection_enabled;
            entry.bookmarked = bookmarked;
            entry.clone()
        };
        self.bus.publish(Event::SiteInfoUpdated { info: updated });
        Ok(())
    }

    fn handle_site_clear_data(&self, origin: Option<String>) -> CommandResult {
        let (origin_key, url) = self.resolve_site_origin(origin, "site.clear_data")?;
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let bookmarked = state.model.is_url_bookmarked(&url);
            let entry = state
                .model
                .site_info
                .entry(origin_key.clone())
                .or_insert_with(|| SiteInfo::for_url(&url));
            entry.clear_data();
            entry.bookmarked = bookmarked;
            entry.clone()
        };
        self.bus.publish(Event::SiteDataCleared { origin: origin_key });
        self.bus.publish(Event::SiteInfoUpdated { info: updated });
        Ok(())
    }

    fn handle_site_bookmark_toggle(&self, tab_id: Option<TabId>) -> CommandResult {
        let target = self.resolve_target_tab(tab_id, "site.bookmark.toggle")?;
        let (bookmarks, updated_site) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(tab) = state.model.tab(target).cloned() else {
                return Err(CommandError::TargetNotFound {
                    command: "site.bookmark.toggle".to_string(),
                    target: format!("tab {}", target.0),
                });
            };
            let normalized = tab.url.trim().trim_end_matches('/').to_string();
            if let Some(pos) = state
                .model
                .bookmarks
                .iter()
                .position(|b| b.url.trim().trim_end_matches('/').eq_ignore_ascii_case(&normalized))
            {
                state.model.bookmarks.remove(pos);
            } else {
                let next_id = state.model.bookmarks.iter().map(|b| b.id.0).max().unwrap_or(0) + 1;
                state.model.bookmarks.push(Bookmark {
                    id: BookmarkId(next_id),
                    url: tab.url.clone(),
                    title: tab.display_title(),
                    favicon: tab.favicon.clone(),
                    folder: None,
                    created_at_unix: 1_764_000_000,
                });
            }
            let bookmarked = state.model.is_url_bookmarked(&tab.url);
            let origin_key = origin_for_url(&tab.url);
            let updated_site = {
                let entry = state
                    .model
                    .site_info
                    .entry(origin_key)
                    .or_insert_with(|| SiteInfo::for_url(&tab.url));
                entry.bookmarked = bookmarked;
                entry.clone()
            };
            (state.model.bookmarks.clone(), updated_site)
        };
        self.bus.publish(Event::BookmarksChanged { bookmarks });
        self.bus.publish(Event::SiteInfoUpdated { info: updated_site });
        Ok(())
    }

    fn handle_folders_set_all_collapsed(&self, collapsed: bool) -> CommandResult {
        let changed: Vec<Folder> = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let active_space = state.model.active_space;
            let mut updated = Vec::new();
            for folder in &mut state.model.folders {
                if (active_space.is_none() || Some(folder.space_id) == active_space) && folder.collapsed != collapsed {
                    folder.collapsed = collapsed;
                    updated.push(folder.clone());
                }
            }
            updated
        };
        for folder in changed {
            self.bus.publish(Event::FolderUpdated { folder });
        }
        Ok(())
    }

    fn handle_space_switch(&self, space_id: SpaceId) -> CommandResult {
        let (spaces_snapshot, target_tab, should_forward_to_source) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if !state.model.spaces.iter().any(|space| space.id == space_id) {
                return Err(CommandError::TargetNotFound {
                    command: "space.switch".to_string(),
                    target: format!("space {}", space_id.0),
                });
            }

            // Record the leaving space's last active tab when it belongs to that space.
            if let (Some(prev_space_id), Some(prev_tab_id)) = (state.model.active_space, state.model.active_tab)
                && state
                    .model
                    .tab(prev_tab_id)
                    .is_some_and(|tab| tab.space_id == Some(prev_space_id))
                && let Some(prev_space) = state.model.spaces.iter_mut().find(|s| s.id == prev_space_id)
            {
                prev_space.last_active_tab = Some(prev_tab_id);
            }

            state.model.active_space = Some(space_id);
            if let Some(window) = state.model.windows.first_mut() {
                window.active_space = Some(space_id);
            }

            // Restore the destination space's last active tab (if still present in that space), or its first tab.
            let remembered = state
                .model
                .space(space_id)
                .and_then(|space| space.last_active_tab)
                .filter(|tab_id| {
                    state
                        .model
                        .tab(*tab_id)
                        .is_some_and(|tab| tab.space_id == Some(space_id))
                });
            let next_tab = remembered.or_else(|| {
                state
                    .model
                    .pinned_tabs_in_space(space_id)
                    .first()
                    .map(|tab| tab.id)
                    .or_else(|| {
                        state
                            .model
                            .folders_in_space(space_id)
                            .into_iter()
                            .flat_map(|folder| state.model.folder_tabs(folder.id))
                            .next()
                            .map(|tab| tab.id)
                    })
                    .or_else(|| state.model.normal_tabs_in_space(space_id).first().map(|tab| tab.id))
            });

            let has_view = next_tab
                .and_then(|id| state.model.tab(id))
                .is_some_and(|tab| tab.view_id.is_some());
            state.model.active_tab = next_tab;
            if let Some(window) = state.model.windows.first_mut() {
                window.active_tab = next_tab;
            }
            if let Some(id) = next_tab
                && let Some(space) = state.model.spaces.iter_mut().find(|s| s.id == space_id)
            {
                space.last_active_tab = Some(id);
            }
            (state.model.spaces.clone(), next_tab, has_view)
        };

        self.bus.publish(Event::SpacesUpdated {
            spaces: spaces_snapshot,
        });
        self.bus.publish(Event::SpaceSwitched { space_id });
        if should_forward_to_source && let (Some(source), Some(tab_id)) = (self.current_tab_source(), target_tab) {
            source.activate_tab(tab_id)?;
        } else {
            self.bus.publish(Event::TabActivated { tab_id: target_tab });
        }
        Ok(())
    }

    fn handle_space_step(&self, delta: isize) -> CommandResult {
        let next_space = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let len = state.model.spaces.len();
            if len < 2 {
                return Ok(());
            }
            let current = state
                .model
                .active_space
                .and_then(|id| state.model.spaces.iter().position(|space| space.id == id))
                .unwrap_or(0);
            let next = (current as isize + delta).rem_euclid(len as isize) as usize;
            state.model.spaces[next].id
        };
        self.handle_space_switch(next_space)
    }

    fn handle_space_clear_tabs(&self, space_id: Option<SpaceId>) -> CommandResult {
        let normal_ids: Vec<TabId> = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let target_space = space_id
                .or(state.model.active_space)
                .ok_or_else(|| CommandError::TargetNotFound {
                    command: "space.clear_tabs".to_string(),
                    target: "active space".to_string(),
                })?;
            if state.model.space(target_space).is_none() {
                return Err(CommandError::TargetNotFound {
                    command: "space.clear_tabs".to_string(),
                    target: format!("space {}", target_space.0),
                });
            }
            state
                .model
                .normal_tabs_in_space(target_space)
                .into_iter()
                .map(|tab| tab.id)
                .collect()
        };

        let tab_source = self.current_tab_source();
        for tab_id in normal_ids {
            if let Some(source) = &tab_source {
                source.close_tab(tab_id, false)?;
            } else {
                self.handle_stub_tab_close(tab_id)?;
            }
        }
        Ok(())
    }

    fn handle_tab_pin(&self, tab_id: TabId) -> CommandResult {
        let (updated_tab, pinned_tabs, essentials_changed, updated_folder) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let active_space = state.model.active_space.unwrap_or(SpaceId(1));
            let Some(tab) = state.model.tabs.iter_mut().find(|t| t.id == tab_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "tab.pin".to_string(),
                    target: format!("tab {}", tab_id.0),
                });
            };
            let target_space = tab.space_id.unwrap_or(active_space);
            let old_folder = tab.folder_id.take();
            let was_essential = tab.kind == TabKind::Essential;
            tab.kind = TabKind::Pinned;
            tab.space_id = Some(target_space);
            let updated_tab = tab.clone();

            let mut updated_folder = None;
            if let Some(folder_id) = old_folder
                && let Some(folder) = state.model.folders.iter_mut().find(|f| f.id == folder_id)
            {
                folder.tab_ids.retain(|id| *id != tab_id);
                updated_folder = Some(folder.clone());
            }

            let essentials_changed = if was_essential {
                state.model.essentials.retain(|item| item.tab_id != tab_id);
                normalize_essential_orders(&mut state.model.essentials);
                Some(state.model.essentials.clone())
            } else {
                None
            };

            if !state
                .model
                .pinned_tabs
                .iter()
                .any(|item| item.tab_id == tab_id && item.space_id == target_space)
            {
                state.model.pinned_tabs.retain(|item| item.tab_id != tab_id);
                let order = state
                    .model
                    .pinned_tabs
                    .iter()
                    .filter(|item| item.space_id == target_space)
                    .count();
                state.model.pinned_tabs.push(PinnedTab {
                    tab_id,
                    space_id: target_space,
                    order,
                });
                normalize_pinned_orders(&mut state.model.pinned_tabs);
            }

            (
                updated_tab,
                state.model.pinned_tabs.clone(),
                essentials_changed,
                updated_folder,
            )
        };

        self.bus.publish(Event::TabUpdated { tab: updated_tab });
        self.bus.publish(Event::PinnedTabsChanged { pinned_tabs });
        if let Some(essentials) = essentials_changed {
            self.bus.publish(Event::EssentialsChanged { essentials });
        }
        if let Some(folder) = updated_folder {
            self.bus.publish(Event::FolderUpdated { folder });
        }
        Ok(())
    }

    fn handle_tab_unpin(&self, tab_id: TabId) -> CommandResult {
        let (updated_tab, pinned_tabs) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let active_space = state.model.active_space.unwrap_or(SpaceId(1));
            let Some(tab) = state.model.tabs.iter_mut().find(|t| t.id == tab_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "tab.unpin".to_string(),
                    target: format!("tab {}", tab_id.0),
                });
            };
            if tab.kind == TabKind::Pinned {
                tab.kind = TabKind::Normal;
                if tab.space_id.is_none() {
                    tab.space_id = Some(active_space);
                }
            }
            let updated_tab = tab.clone();
            state.model.pinned_tabs.retain(|item| item.tab_id != tab_id);
            normalize_pinned_orders(&mut state.model.pinned_tabs);
            (updated_tab, state.model.pinned_tabs.clone())
        };

        self.bus.publish(Event::TabUpdated { tab: updated_tab });
        self.bus.publish(Event::PinnedTabsChanged { pinned_tabs });
        Ok(())
    }

    fn handle_tab_toggle_pin(&self, tab_id: TabId) -> CommandResult {
        let is_pinned = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state
                .model
                .tab(tab_id)
                .ok_or_else(|| CommandError::TargetNotFound {
                    command: "tab.toggle_pin".to_string(),
                    target: format!("tab {}", tab_id.0),
                })?
                .kind
                == TabKind::Pinned
        };
        if is_pinned {
            self.handle_tab_unpin(tab_id)
        } else {
            self.handle_tab_pin(tab_id)
        }
    }

    fn handle_tab_add_to_essentials(&self, tab_id: TabId) -> CommandResult {
        let (updated_tab, essentials, pinned_changed, updated_folder, promo_hidden) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let already_essential = state
                .model
                .tab(tab_id)
                .ok_or_else(|| CommandError::TargetNotFound {
                    command: "tab.add_to_essentials".to_string(),
                    target: format!("tab {}", tab_id.0),
                })?
                .kind
                == TabKind::Essential;
            if !already_essential && state.model.essentials.len() >= MAX_ESSENTIALS {
                return Err(CommandError::Failed(format!(
                    "essentials limit ({MAX_ESSENTIALS}) reached"
                )));
            }
            let tab = state.model.tabs.iter_mut().find(|t| t.id == tab_id).unwrap();
            let old_folder = tab.folder_id.take();
            let was_pinned = tab.kind == TabKind::Pinned;
            tab.kind = TabKind::Essential;
            tab.space_id = None;
            let updated_tab = tab.clone();

            let mut updated_folder = None;
            if let Some(folder_id) = old_folder
                && let Some(folder) = state.model.folders.iter_mut().find(|f| f.id == folder_id)
            {
                folder.tab_ids.retain(|id| *id != tab_id);
                updated_folder = Some(folder.clone());
            }

            let pinned_changed = if was_pinned {
                state.model.pinned_tabs.retain(|item| item.tab_id != tab_id);
                normalize_pinned_orders(&mut state.model.pinned_tabs);
                Some(state.model.pinned_tabs.clone())
            } else {
                None
            };

            if !state.model.essentials.iter().any(|item| item.tab_id == tab_id) {
                let order = state.model.essentials.len();
                state.model.essentials.push(Essential { tab_id, order });
                normalize_essential_orders(&mut state.model.essentials);
            }

            let promo_hidden = if state.model.show_essentials_promo {
                state.model.show_essentials_promo = false;
                true
            } else {
                false
            };

            (
                updated_tab,
                state.model.essentials.clone(),
                pinned_changed,
                updated_folder,
                promo_hidden,
            )
        };

        self.bus.publish(Event::TabUpdated { tab: updated_tab });
        self.bus.publish(Event::EssentialsChanged { essentials });
        if let Some(pinned_tabs) = pinned_changed {
            self.bus.publish(Event::PinnedTabsChanged { pinned_tabs });
        }
        if let Some(folder) = updated_folder {
            self.bus.publish(Event::FolderUpdated { folder });
        }
        if promo_hidden {
            self.bus.publish(Event::EssentialsPromoChanged { visible: false });
        }
        Ok(())
    }

    fn handle_tab_remove_from_essentials(&self, tab_id: TabId) -> CommandResult {
        let (updated_tab, essentials) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let active_space = state.model.active_space.unwrap_or(SpaceId(1));
            let Some(tab) = state.model.tabs.iter_mut().find(|t| t.id == tab_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "tab.remove_from_essentials".to_string(),
                    target: format!("tab {}", tab_id.0),
                });
            };
            if tab.kind == TabKind::Essential {
                tab.kind = TabKind::Normal;
                tab.space_id = Some(active_space);
            }
            let updated_tab = tab.clone();
            state.model.essentials.retain(|item| item.tab_id != tab_id);
            normalize_essential_orders(&mut state.model.essentials);
            (updated_tab, state.model.essentials.clone())
        };

        self.bus.publish(Event::TabUpdated { tab: updated_tab });
        self.bus.publish(Event::EssentialsChanged { essentials });
        Ok(())
    }

    fn handle_tab_toggle_essential(&self, tab_id: TabId) -> CommandResult {
        let is_essential = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state
                .model
                .tab(tab_id)
                .ok_or_else(|| CommandError::TargetNotFound {
                    command: "tab.toggle_essential".to_string(),
                    target: format!("tab {}", tab_id.0),
                })?
                .kind
                == TabKind::Essential
        };
        if is_essential {
            self.handle_tab_remove_from_essentials(tab_id)
        } else {
            self.handle_tab_add_to_essentials(tab_id)
        }
    }

    fn handle_tab_toggle_mute(&self, tab_id: TabId) -> CommandResult {
        let updated_tab = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(tab) = state.model.tabs.iter_mut().find(|t| t.id == tab_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "tab.toggle_mute".to_string(),
                    target: format!("tab {}", tab_id.0),
                });
            };
            tab.is_muted = !tab.is_muted;
            tab.clone()
        };
        self.bus.publish(Event::TabUpdated { tab: updated_tab });
        Ok(())
    }

    fn handle_tab_duplicate(&self, tab_id: TabId) -> CommandResult {
        let url = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state
                .model
                .tab(tab_id)
                .ok_or_else(|| CommandError::TargetNotFound {
                    command: "tab.duplicate".to_string(),
                    target: format!("tab {}", tab_id.0),
                })?
                .url
                .clone()
        };
        if let Some(source) = self.current_tab_source() {
            source.new_tab(Some(&url), Some(tab_id), true)
        } else {
            self.handle_stub_tab_new(Some(url), Some(tab_id), true)
        }
    }

    fn handle_tab_rename(&self, tab_id: TabId, title: String) -> CommandResult {
        let trimmed = title.split_whitespace().collect::<Vec<_>>().join(" ");
        let updated_tab = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(tab) = state.model.tabs.iter_mut().find(|t| t.id == tab_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "tab.rename".to_string(),
                    target: format!("tab {}", tab_id.0),
                });
            };
            tab.custom_title = if trimmed.is_empty() { None } else { Some(trimmed) };
            tab.clone()
        };
        self.bus.publish(Event::TabUpdated { tab: updated_tab });
        Ok(())
    }

    fn handle_tab_start_rename(&self, tab_id: TabId) -> CommandResult {
        self.bus.publish(Event::TabRenameRequested { tab_id });
        Ok(())
    }

    fn handle_tab_move(&self, tab_id: TabId, target: TabMoveTarget) -> CommandResult {
        let (tabs, essentials, pinned_tabs, folders) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(tab_pos) = state.model.tabs.iter().position(|t| t.id == tab_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "tab.move".to_string(),
                    target: format!("tab {}", tab_id.0),
                });
            };

            let active_space = state.model.active_space.unwrap_or(SpaceId(1));

            // Validate destination space/folder before mutating state.
            match target {
                TabMoveTarget::Normal { space_id, .. } | TabMoveTarget::Pinned { space_id, .. } => {
                    let resolved = space_id.unwrap_or(active_space);
                    if state.model.space(resolved).is_none() {
                        return Err(CommandError::TargetNotFound {
                            command: "tab.move".to_string(),
                            target: format!("space {}", resolved.0),
                        });
                    }
                }
                TabMoveTarget::Folder { folder_id, .. } => {
                    if state.model.folder(folder_id).is_none() {
                        return Err(CommandError::TargetNotFound {
                            command: "tab.move".to_string(),
                            target: format!("folder {}", folder_id.0),
                        });
                    }
                }
                TabMoveTarget::Essential { .. } => {
                    let already_essential = state.model.essentials.iter().any(|e| e.tab_id == tab_id);
                    if !already_essential && state.model.essentials.len() >= MAX_ESSENTIALS {
                        return Err(CommandError::Failed(format!(
                            "essentials limit ({MAX_ESSENTIALS}) reached"
                        )));
                    }
                }
            }

            // Strip old section memberships.
            state.model.essentials.retain(|item| item.tab_id != tab_id);
            state.model.pinned_tabs.retain(|item| item.tab_id != tab_id);
            for folder in &mut state.model.folders {
                folder.tab_ids.retain(|id| *id != tab_id);
            }

            let mut tab = state.model.tabs.remove(tab_pos);

            match target {
                TabMoveTarget::Normal { space_id, index } => {
                    let dest_space = space_id.unwrap_or(active_space);
                    tab.kind = TabKind::Normal;
                    tab.space_id = Some(dest_space);
                    tab.folder_id = None;

                    let normal_positions: Vec<usize> = state
                        .model
                        .tabs
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| {
                            t.kind == TabKind::Normal && t.space_id == Some(dest_space) && t.folder_id.is_none()
                        })
                        .map(|(idx, _)| idx)
                        .collect();
                    let insert_pos = if normal_positions.is_empty() {
                        state.model.tabs.len()
                    } else if index >= normal_positions.len() {
                        normal_positions[normal_positions.len() - 1] + 1
                    } else {
                        normal_positions[index]
                    };
                    state.model.tabs.insert(insert_pos, tab);
                }
                TabMoveTarget::Pinned { space_id, index } => {
                    let dest_space = space_id.unwrap_or(active_space);
                    tab.kind = TabKind::Pinned;
                    tab.space_id = Some(dest_space);
                    tab.folder_id = None;
                    let insert_idx = tab_pos.min(state.model.tabs.len());
                    state.model.tabs.insert(insert_idx, tab);

                    let mut space_pinned: Vec<TabId> = state
                        .model
                        .pinned_tabs
                        .iter()
                        .filter(|item| item.space_id == dest_space)
                        .map(|item| item.tab_id)
                        .collect();
                    let clamped = index.min(space_pinned.len());
                    space_pinned.insert(clamped, tab_id);
                    state.model.pinned_tabs.retain(|item| item.space_id != dest_space);
                    for (order, id) in space_pinned.into_iter().enumerate() {
                        state.model.pinned_tabs.push(PinnedTab {
                            tab_id: id,
                            space_id: dest_space,
                            order,
                        });
                    }
                }
                TabMoveTarget::Folder { folder_id, index } => {
                    let folder_space = state.model.folder(folder_id).unwrap().space_id;
                    tab.kind = TabKind::Normal;
                    tab.space_id = Some(folder_space);
                    tab.folder_id = Some(folder_id);
                    let insert_idx = tab_pos.min(state.model.tabs.len());
                    state.model.tabs.insert(insert_idx, tab);
                    if let Some(folder) = state.model.folders.iter_mut().find(|f| f.id == folder_id) {
                        let clamped = index.min(folder.tab_ids.len());
                        folder.tab_ids.insert(clamped, tab_id);
                    }
                }
                TabMoveTarget::Essential { index } => {
                    tab.kind = TabKind::Essential;
                    tab.space_id = None;
                    tab.folder_id = None;
                    let insert_idx = tab_pos.min(state.model.tabs.len());
                    state.model.tabs.insert(insert_idx, tab);

                    let mut ordered: Vec<TabId> = state.model.essentials.iter().map(|item| item.tab_id).collect();
                    let clamped = index.min(ordered.len());
                    ordered.insert(clamped, tab_id);
                    state.model.essentials = ordered
                        .into_iter()
                        .enumerate()
                        .map(|(order, id)| Essential { tab_id: id, order })
                        .collect();
                }
            }

            normalize_essential_orders(&mut state.model.essentials);
            normalize_pinned_orders(&mut state.model.pinned_tabs);

            (
                state.model.tabs.clone(),
                state.model.essentials.clone(),
                state.model.pinned_tabs.clone(),
                state.model.folders.clone(),
            )
        };

        self.bus.publish(Event::TabsReordered {
            tabs,
            essentials,
            pinned_tabs,
            folders,
        });
        Ok(())
    }

    fn handle_tab_move_to_folder(&self, tab_id: TabId, folder_id: Option<FolderId>) -> CommandResult {
        let target = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(tab) = state.model.tab(tab_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "tab.move_to_folder".to_string(),
                    target: format!("tab {}", tab_id.0),
                });
            };
            match folder_id {
                Some(folder_id) => {
                    let Some(folder) = state.model.folder(folder_id) else {
                        return Err(CommandError::TargetNotFound {
                            command: "tab.move_to_folder".to_string(),
                            target: format!("folder {}", folder_id.0),
                        });
                    };
                    TabMoveTarget::Folder {
                        folder_id,
                        index: folder.tab_ids.len(),
                    }
                }
                None => {
                    let space_id = tab.space_id.or(state.model.active_space);
                    let count = space_id
                        .map(|sid| state.model.normal_tabs_in_space(sid).len())
                        .unwrap_or(0);
                    TabMoveTarget::Normal { space_id, index: count }
                }
            }
        };
        self.handle_tab_move(tab_id, target)
    }

    fn handle_tab_move_to_space(&self, tab_id: TabId, space_id: SpaceId) -> CommandResult {
        let (target, was_active_in_old_space) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(tab) = state.model.tab(tab_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "tab.move_to_space".to_string(),
                    target: format!("tab {}", tab_id.0),
                });
            };
            if state.model.space(space_id).is_none() {
                return Err(CommandError::TargetNotFound {
                    command: "tab.move_to_space".to_string(),
                    target: format!("space {}", space_id.0),
                });
            }
            let target = match tab.kind {
                TabKind::Pinned => TabMoveTarget::Pinned {
                    space_id: Some(space_id),
                    index: state.model.pinned_tabs_in_space(space_id).len(),
                },
                TabKind::Normal | TabKind::Essential => TabMoveTarget::Normal {
                    space_id: Some(space_id),
                    index: state.model.normal_tabs_in_space(space_id).len(),
                },
            };
            let was_active = state.model.active_tab == Some(tab_id) && state.model.active_space != Some(space_id);
            (target, was_active)
        };

        self.handle_tab_move(tab_id, target)?;

        if was_active_in_old_space {
            let replacement = {
                let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                state.model.tabs_in_active_space_order().first().copied()
            };
            if let (Some(source), Some(next_id)) = (self.current_tab_source(), replacement) {
                let _ = source.activate_tab(next_id);
            } else {
                self.feed_tab_activated(replacement);
            }
        }
        Ok(())
    }

    fn handle_folder_create(
        &self,
        name: Option<String>,
        space_id: Option<SpaceId>,
        tab_id: Option<TabId>,
    ) -> CommandResult {
        let (folder, moved_tab_id) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let target_space = space_id
                .or_else(|| tab_id.and_then(|id| state.model.tab(id)).and_then(|t| t.space_id))
                .or(state.model.active_space)
                .ok_or_else(|| CommandError::TargetNotFound {
                    command: "folder.create".to_string(),
                    target: "active space".to_string(),
                })?;
            if state.model.space(target_space).is_none() {
                return Err(CommandError::TargetNotFound {
                    command: "folder.create".to_string(),
                    target: format!("space {}", target_space.0),
                });
            }
            if let Some(id) = tab_id
                && state.model.tab(id).is_none()
            {
                return Err(CommandError::TargetNotFound {
                    command: "folder.create".to_string(),
                    target: format!("tab {}", id.0),
                });
            }

            let folder_id = FolderId(state.next_folder_id);
            state.next_folder_id += 1;
            let folder_name = name
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "New Folder".to_string());
            let folder = Folder {
                id: folder_id,
                space_id: target_space,
                name: folder_name,
                icon: "folder".to_string(),
                collapsed: false,
                tab_ids: Vec::new(),
            };
            state.model.folders.push(folder.clone());
            (folder, tab_id)
        };

        let folder_id = folder.id;
        self.bus.publish(Event::FolderCreated { folder });
        if let Some(tab_id) = moved_tab_id {
            self.handle_tab_move(tab_id, TabMoveTarget::Folder { folder_id, index: 0 })?;
        }
        Ok(())
    }

    fn handle_folder_rename(&self, folder_id: FolderId, name: String) -> CommandResult {
        let trimmed = name.split_whitespace().collect::<Vec<_>>().join(" ");
        if trimmed.is_empty() {
            return Err(CommandError::InvalidArgument {
                command: "folder.rename".to_string(),
                message: "folder name must not be empty".to_string(),
            });
        }
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(folder) = state.model.folders.iter_mut().find(|f| f.id == folder_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "folder.rename".to_string(),
                    target: format!("folder {}", folder_id.0),
                });
            };
            folder.name = trimmed;
            folder.clone()
        };
        self.bus.publish(Event::FolderUpdated { folder: updated });
        Ok(())
    }

    fn handle_folder_start_rename(&self, folder_id: FolderId) -> CommandResult {
        let exists = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.folder(folder_id).is_some()
        };
        if !exists {
            return Err(CommandError::TargetNotFound {
                command: "folder.start_rename".to_string(),
                target: format!("folder {}", folder_id.0),
            });
        }
        self.bus.publish(Event::FolderRenameRequested { folder_id });
        Ok(())
    }

    fn handle_folder_toggle_collapsed(&self, folder_id: FolderId) -> CommandResult {
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(folder) = state.model.folders.iter_mut().find(|f| f.id == folder_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "folder.toggle_collapsed".to_string(),
                    target: format!("folder {}", folder_id.0),
                });
            };
            folder.collapsed = !folder.collapsed;
            folder.clone()
        };
        self.bus.publish(Event::FolderUpdated { folder: updated });
        Ok(())
    }

    fn handle_folder_unpack(&self, folder_id: FolderId) -> CommandResult {
        let updated_tabs = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(pos) = state.model.folders.iter().position(|f| f.id == folder_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "folder.unpack".to_string(),
                    target: format!("folder {}", folder_id.0),
                });
            };
            let folder = state.model.folders.remove(pos);
            let mut changed = Vec::new();
            for tab in &mut state.model.tabs {
                if tab.folder_id == Some(folder_id) || folder.tab_ids.contains(&tab.id) {
                    tab.folder_id = None;
                    tab.space_id = Some(folder.space_id);
                    changed.push(tab.clone());
                }
            }
            changed
        };
        for tab in updated_tabs {
            self.bus.publish(Event::TabUpdated { tab });
        }
        self.bus.publish(Event::FolderDeleted { folder_id });
        Ok(())
    }

    fn handle_folder_delete(&self, folder_id: FolderId) -> CommandResult {
        let child_tabs = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(pos) = state.model.folders.iter().position(|f| f.id == folder_id) else {
                return Err(CommandError::TargetNotFound {
                    command: "folder.delete".to_string(),
                    target: format!("folder {}", folder_id.0),
                });
            };
            let folder = state.model.folders.remove(pos);
            let mut ids: Vec<TabId> = folder.tab_ids;
            for tab in &state.model.tabs {
                if tab.folder_id == Some(folder_id) && !ids.contains(&tab.id) {
                    ids.push(tab.id);
                }
            }
            ids
        };

        self.bus.publish(Event::FolderDeleted { folder_id });
        let tab_source = self.current_tab_source();
        for tab_id in child_tabs {
            if let Some(source) = &tab_source {
                source.close_tab(tab_id, false)?;
            } else {
                self.handle_stub_tab_close(tab_id)?;
            }
        }
        Ok(())
    }

    fn handle_notification_dismiss(&self, notification_id: NotificationId) -> CommandResult {
        let removed = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let before = state.model.sidebar_notifications.len();
            state
                .model
                .sidebar_notifications
                .retain(|item| item.id != notification_id);
            state.model.sidebar_notifications.len() != before
        };
        if !removed {
            return Err(CommandError::TargetNotFound {
                command: "notification.dismiss".to_string(),
                target: format!("notification {}", notification_id.0),
            });
        }
        self.bus
            .publish(Event::SidebarNotificationDismissed { notification_id });
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

    fn handle_layout_set_toolbar_layout(&self, layout: ToolbarLayout) -> CommandResult {
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.layout.toolbar_layout = layout;
            state.model.layout.sidebar_expanded = !matches!(layout, ToolbarLayout::Collapsed);
            state.model.layout.clone()
        };
        self.bus.publish(Event::LayoutSettingsChanged { layout: updated });
        Ok(())
    }

    fn handle_layout_set_compact_mode(&self, enabled: bool) -> CommandResult {
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.layout.compact_mode = enabled;
            state.model.layout.clone()
        };
        self.bus.publish(Event::LayoutSettingsChanged { layout: updated });
        Ok(())
    }

    fn handle_sidebar_toggle_expanded(&self) -> CommandResult {
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let next_expanded = !state.model.layout.sidebar_expanded;
            state.model.layout.sidebar_expanded = next_expanded;
            state.model.layout.toolbar_layout = if next_expanded {
                ToolbarLayout::Multiple
            } else {
                ToolbarLayout::Collapsed
            };
            state.model.layout.clone()
        };
        self.bus.publish(Event::LayoutSettingsChanged { layout: updated });
        Ok(())
    }

    fn handle_sidebar_set_expanded(&self, expanded: bool) -> CommandResult {
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.layout.sidebar_expanded = expanded;
            if !expanded {
                state.model.layout.toolbar_layout = ToolbarLayout::Collapsed;
            } else if state.model.layout.toolbar_layout == ToolbarLayout::Collapsed {
                state.model.layout.toolbar_layout = ToolbarLayout::Multiple;
            }
            state.model.layout.clone()
        };
        self.bus.publish(Event::LayoutSettingsChanged { layout: updated });
        Ok(())
    }

    fn handle_sidebar_set_side(&self, side: SidebarSide) -> CommandResult {
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.layout.sidebar_side = side;
            state.model.layout.clone()
        };
        self.bus.publish(Event::LayoutSettingsChanged { layout: updated });
        Ok(())
    }

    fn handle_sidebar_set_width(&self, width: f32) -> CommandResult {
        if !width.is_finite() {
            return Err(CommandError::InvalidArgument {
                command: "sidebar.set_width".to_string(),
                message: "sidebar width must be a finite number".to_string(),
            });
        }
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.layout.sidebar_width = LayoutSettings::clamp_sidebar_width(width);
            state.model.layout.clone()
        };
        self.bus.publish(Event::LayoutSettingsChanged { layout: updated });
        Ok(())
    }

    fn handle_vim_set_enabled(&self, enabled: bool) -> CommandResult {
        let updated_contributions = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.vim_mode_enabled = enabled;
            for entry in &mut state.model.contributions.menu_entries {
                if entry.id.as_str() == APP_MENU_VIM_MODE_ID {
                    entry.kind = MenuEntryKind::Toggle { checked: enabled };
                    entry.command_args = vec![("enabled".to_string(), (!enabled).to_string())];
                }
            }
            state.model.contributions.clone()
        };
        self.bus.publish(Event::VimModeChanged { enabled });
        self.bus.publish(Event::ContributionsChanged {
            contributions: updated_contributions,
        });
        if !enabled {
            self.clear_focus_on_vim_disabled();
            self.on_vim_mode_disabled();
        }
        Ok(())
    }

    fn handle_stub_zoom(&self, next_zoom: impl FnOnce(f32) -> f32) -> CommandResult {
        let zoom_level = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let updated = next_zoom(state.model.zoom_level).clamp(0.3, 3.0);
            state.model.zoom_level = (updated * 100.0).round() / 100.0;
            state.model.zoom_level
        };
        self.bus.publish(Event::ZoomChanged { zoom_level });
        Ok(())
    }

    /// Updates the reported zoom level from an external [`TabSource`] (such as Ladybird's Hub).
    pub fn feed_zoom_level(&self, zoom_level: f32) {
        let rounded = (zoom_level.clamp(0.3, 3.0) * 100.0).round() / 100.0;
        let changed = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if (state.model.zoom_level - rounded).abs() < 0.001 {
                false
            } else {
                state.model.zoom_level = rounded;
                true
            }
        };
        if changed {
            self.bus.publish(Event::ZoomChanged { zoom_level: rounded });
        }
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
            Command::AddressBarOpen { new_tab, initial_query } => {
                self.bus
                    .publish(Event::AddressBarOpenRequested { new_tab, initial_query });
                Ok(())
            }
            Command::AddressBarSubmit { input, new_tab } => self.handle_address_bar_submit(input, new_tab, tab_source),
            Command::AddressBarSwitchToTab { tab_id } => self.handle_address_bar_switch_to_tab(tab_id, tab_source),
            Command::AddressBarCopyCurrentUrl => {
                let url = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state.model.active_tab().map(|tab| tab.url.clone()).unwrap_or_default()
                };
                if !url.is_empty() {
                    self.bus.publish(Event::ClipboardCopyRequested { text: url });
                    self.show_toast(crate::toasts::messages::COPIED_CURRENT_URL.to_string(), None, None);
                }
                Ok(())
            }
            Command::PaletteOpen { initial_query } => {
                self.bus.publish(Event::PaletteOpenRequested { initial_query });
                Ok(())
            }
            Command::SitePanelToggle => {
                self.bus.publish(Event::SitePanelToggleRequested);
                Ok(())
            }
            Command::SitePermissionSet {
                origin,
                permission,
                state,
            } => self.handle_site_permission_set(origin, permission, state),
            Command::SiteTrackingProtectionToggle { origin } => self.handle_site_tracking_protection_toggle(origin),
            Command::SiteClearData { origin } => self.handle_site_clear_data(origin),
            Command::SiteBookmarkToggle { tab_id } => self.handle_site_bookmark_toggle(tab_id),
            Command::SiteScreenshot { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                eprintln!(
                    "zendia: stub site.screenshot for tab {} — capture service not connected yet",
                    target.0
                );
                Ok(())
            }
            Command::SiteShare { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                let url = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state.model.tab(target).map(|tab| tab.url.clone()).unwrap_or_default()
                };
                if !url.is_empty() {
                    self.bus.publish(Event::ClipboardCopyRequested { text: url });
                }
                Ok(())
            }
            Command::SiteEditStyle { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                eprintln!(
                    "zendia: stub site.edit_style for tab {} — appearance editor not connected yet",
                    target.0
                );
                Ok(())
            }
            Command::SiteReaderModeToggle { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                eprintln!(
                    "zendia: stub site.reader_mode.toggle for tab {} — reader mode not connected yet",
                    target.0
                );
                Ok(())
            }
            Command::SettingsSetSearchEngine {
                name: engine_name,
                search_url,
            } => {
                if engine_name.trim().is_empty() || search_url.trim().is_empty() {
                    return Err(CommandError::InvalidArgument {
                        command: name,
                        message: "search engine name and search_url must not be empty".to_string(),
                    });
                }
                self.set_search_engine(SearchEngine::new(engine_name.trim(), search_url.trim()));
                Ok(())
            }
            Command::LayoutToggleCompactMode => {
                let current = self.model().layout.compact_mode;
                self.handle_layout_set_compact_mode(!current)
            }
            Command::SidebarToggleSide => {
                let next = match self.model().layout.sidebar_side {
                    SidebarSide::Left => SidebarSide::Right,
                    SidebarSide::Right => SidebarSide::Left,
                };
                self.handle_sidebar_set_side(next)
            }
            Command::FolderCollapseAll => self.handle_folders_set_all_collapsed(true),
            Command::FolderExpandAll => self.handle_folders_set_all_collapsed(false),
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
            Command::SpaceNext => self.handle_space_step(1),
            Command::SpacePrevious => self.handle_space_step(-1),
            Command::SpaceClearTabs { space_id } => self.handle_space_clear_tabs(space_id),
            Command::TabPin { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                self.handle_tab_pin(target)
            }
            Command::TabUnpin { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                self.handle_tab_unpin(target)
            }
            Command::TabTogglePin { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                self.handle_tab_toggle_pin(target)
            }
            Command::TabAddToEssentials { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                self.handle_tab_add_to_essentials(target)
            }
            Command::TabRemoveFromEssentials { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                self.handle_tab_remove_from_essentials(target)
            }
            Command::TabToggleEssential { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                self.handle_tab_toggle_essential(target)
            }
            Command::TabToggleMute { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                self.handle_tab_toggle_mute(target)
            }
            Command::TabDuplicate { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                self.handle_tab_duplicate(target)
            }
            Command::TabRename { tab_id, title } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                self.handle_tab_rename(target, title)
            }
            Command::TabStartRename { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                self.handle_tab_start_rename(target)
            }
            Command::TabMove { tab_id, target } => self.handle_tab_move(tab_id, target),
            Command::TabMoveToFolder { tab_id, folder_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                self.handle_tab_move_to_folder(target, folder_id)
            }
            Command::TabMoveToSpace { tab_id, space_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                self.handle_tab_move_to_space(target, space_id)
            }
            Command::FolderCreate { name, space_id, tab_id } => self.handle_folder_create(name, space_id, tab_id),
            Command::FolderRename { folder_id, name } => self.handle_folder_rename(folder_id, name),
            Command::FolderStartRename { folder_id } => self.handle_folder_start_rename(folder_id),
            Command::FolderDelete { folder_id } => self.handle_folder_delete(folder_id),
            Command::FolderUnpack { folder_id } => self.handle_folder_unpack(folder_id),
            Command::FolderToggleCollapsed { folder_id } => self.handle_folder_toggle_collapsed(folder_id),
            Command::NotificationDismiss { notification_id } => self.handle_notification_dismiss(notification_id),
            Command::SplitAdd { tab_id, direction } => self.handle_split_add(tab_id, direction),
            Command::PanelToggle { panel_id } => self.handle_panel_toggle(panel_id),
            Command::AgentSend {
                conversation_id,
                agent_id,
                prompt,
            } => self.handle_agent_send(conversation_id, agent_id, prompt),
            Command::WebChatAsk { site_id, prompt } => self.handle_webchat_ask(site_id, prompt),
            Command::ThemeSet { scheme, overrides } => self.handle_theme_set(scheme, overrides),
            Command::LayoutSetToolbarLayout { layout } => self.handle_layout_set_toolbar_layout(layout),
            Command::LayoutSetCompactMode { enabled } => self.handle_layout_set_compact_mode(enabled),
            Command::SidebarToggleExpanded => self.handle_sidebar_toggle_expanded(),
            Command::SidebarSetExpanded { expanded } => self.handle_sidebar_set_expanded(expanded),
            Command::SidebarSetSide { side } => self.handle_sidebar_set_side(side),
            Command::SidebarSetWidth { width } => self.handle_sidebar_set_width(width),
            Command::DebugCycleInputState => {
                self.cycle_sample_input_state();
                Ok(())
            }
            command @ (Command::StatuslineSetShow { .. }
            | Command::StatuslineAnnounce { .. }
            | Command::StatuslineClearAnnouncement
            | Command::WhichKeyShowAll
            | Command::WhichKeyHide) => self.handle_input_state_command(command),
            Command::VimSetEnabled { enabled } => self.handle_vim_set_enabled(enabled),
            Command::ZoomIn { tab_id } => {
                if let Some(source) = tab_source {
                    let target = self.resolve_target_tab(tab_id, &name)?;
                    source.zoom_in(target)
                } else {
                    self.handle_stub_zoom(|z| z + 0.1)
                }
            }
            Command::ZoomOut { tab_id } => {
                if let Some(source) = tab_source {
                    let target = self.resolve_target_tab(tab_id, &name)?;
                    source.zoom_out(target)
                } else {
                    self.handle_stub_zoom(|z| z - 0.1)
                }
            }
            Command::ZoomReset { tab_id } => {
                if let Some(source) = tab_source {
                    let target = self.resolve_target_tab(tab_id, &name)?;
                    source.zoom_reset(target)
                } else {
                    self.handle_stub_zoom(|_| 1.0)
                }
            }
            Command::MenuAppToggle => {
                self.bus.publish(Event::AppMenuToggleRequested);
                Ok(())
            }
            Command::AppAbout => {
                self.bus.publish(Event::AboutDialogRequested);
                Ok(())
            }
            Command::AppQuit => {
                self.bus.publish(Event::QuitRequested);
                Ok(())
            }
            Command::PageInspectElement { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.inspect_element(target)
                } else {
                    Ok(())
                }
            }
            Command::PageCut { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.cut_selection(target)
                } else {
                    Ok(())
                }
            }
            Command::PageCopy { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.copy_selection(target)
                } else {
                    Ok(())
                }
            }
            Command::PagePaste { tab_id, match_style } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.paste(target, match_style)
                } else {
                    Ok(())
                }
            }
            Command::PageSelectAll { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.select_all(target)
                } else {
                    Ok(())
                }
            }
            Command::PageCopyLink { url } => {
                self.bus.publish(Event::ClipboardCopyRequested { text: url });
                Ok(())
            }
            Command::PageOpenLinkInNewTab { tab_id, url, activate } => {
                if let Some(source) = tab_source {
                    let target = self.resolve_target_tab(tab_id, &name)?;
                    source.open_url_in_new_tab(target, &url, activate)
                } else {
                    self.handle_stub_tab_new(Some(url), tab_id, activate)
                }
            }
            Command::PageViewSource { tab_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.view_source(target)
                } else {
                    Ok(())
                }
            }
            Command::PageReplaceMisspelling { tab_id, replacement } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.replace_misspelling(target, &replacement)
                } else {
                    Ok(())
                }
            }
            Command::PageIgnoreMisspelling { tab_id, word } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.ignore_misspelling(target, &word)
                } else {
                    Ok(())
                }
            }
            Command::PageLearnMisspelling { tab_id, word } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.learn_misspelling(target, &word)
                } else {
                    Ok(())
                }
            }
            Command::MediaContextAction { tab_id, action } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.media_action(target, action)
                } else {
                    Ok(())
                }
            }
            Command::PageSelectDropdownChoose { tab_id, item_id } => {
                let target = self.resolve_target_tab(tab_id, &name)?;
                if let Some(source) = tab_source {
                    source.select_dropdown_choose(target, item_id)
                } else {
                    Ok(())
                }
            }
            command @ (Command::SplitTabs { .. }
            | Command::SplitNewEmpty
            | Command::SplitOpenUrl { .. }
            | Command::SplitDrop { .. }
            | Command::SplitRemoveTab { .. }
            | Command::SplitUnsplit { .. }
            | Command::SplitSetLayout { .. }
            | Command::SplitResize { .. }
            | Command::GlanceOpen { .. }
            | Command::GlanceClose
            | Command::GlanceExpand
            | Command::GlanceSplit
            | Command::ToastShow { .. }
            | Command::ToastDismiss { .. }
            | Command::MediaSessionControl { .. }
            | Command::LayoutSetCompactHide { .. }
            | Command::LayoutCompactRevealSidebar
            | Command::LayoutSetTabSwitchMotion { .. }
            | Command::DebugSampleDownload
            | Command::DebugSampleMedia) => self.handle_page_layout_command(command, tab_source),
            command @ (Command::ExplorerOpen
            | Command::ExplorerClose
            | Command::ExplorerMove { .. }
            | Command::ExplorerSelect { .. }
            | Command::ExplorerActivate
            | Command::ExplorerPick
            | Command::FocusHintInput { .. }
            | Command::FocusHintCancel
            | Command::ToolbarFocusOpen
            | Command::ToolbarFocusClose
            | Command::ToolbarFocusMove { .. }
            | Command::ToolbarFocusActivate
            | Command::WindowModeOpen
            | Command::WindowModeClose
            | Command::WindowModeFocusPane { .. }
            | Command::WindowModeMovePane { .. }
            | Command::WindowModeCarryToggle
            | Command::WindowModeDrop) => self.handle_focus_command(command, tab_source),
            command @ (Command::PickerOpen { .. }
            | Command::PickerChoose { .. }
            | Command::PickerClose { .. }
            | Command::RegionPick { .. }
            | Command::RegionCapture { .. }
            | Command::RegionCancel) => self.handle_picker_or_region_command(command),
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

fn normalize_essential_orders(essentials: &mut [Essential]) {
    essentials.sort_by_key(|item| item.order);
    for (idx, item) in essentials.iter_mut().enumerate() {
        item.order = idx;
    }
}

fn normalize_pinned_orders(pinned_tabs: &mut [PinnedTab]) {
    pinned_tabs.sort_by(|a, b| a.space_id.cmp(&b.space_id).then_with(|| a.order.cmp(&b.order)));
    let mut current_space: Option<SpaceId> = None;
    let mut next_order = 0usize;
    for item in pinned_tabs.iter_mut() {
        if current_space != Some(item.space_id) {
            current_space = Some(item.space_id);
            next_order = 0;
        }
        item.order = next_order;
        next_order += 1;
    }
}

/// Sample sidebar update notification matching Zen's "Update Complete!" card.
pub fn sample_update_notification() -> SidebarNotification {
    SidebarNotification {
        id: NotificationId(1),
        heading: "Update Complete!".to_string(),
        links: vec![
            SidebarNotificationLink {
                label: "What's new in Zendia".to_string(),
                icon: "sparkles".to_string(),
                url: Some("https://github.com/Planckify-Labs/zendia/releases".to_string()),
                command: None,
                special: true,
            },
            SidebarNotificationLink {
                label: "Support Zendia".to_string(),
                icon: "heart-circle-fill".to_string(),
                url: Some("https://github.com/sponsors/Planckify-Labs".to_string()),
                command: None,
                special: false,
            },
            SidebarNotificationLink {
                label: "Something broke?".to_string(),
                icon: "warning".to_string(),
                url: Some("https://github.com/Planckify-Labs/zendia/issues".to_string()),
                command: None,
                special: false,
            },
        ],
        auto_hide_ms: None,
    }
}

fn sample_contributions() -> Contributions {
    Contributions {
        toolbar_items: vec![
            ToolbarItem {
                id: ContributionId::new("builtin.library"),
                title: "Library".to_string(),
                tooltip: Some("Open Library".to_string()),
                icon: "library".to_string(),
                command: "library.toggle".to_string(),
                command_args: Vec::new(),
                kind: ToolbarItemKind::Button,
                placement: ToolbarPlacement::SidebarBottom,
                placement_rules: ToolbarPlacementRules {
                    single_toolbar: Some(ToolbarArea::SidebarBottom),
                    multiple_toolbars: Some(ToolbarArea::SidebarBottom),
                    collapsed_toolbar: Some(ToolbarArea::SidebarBottom),
                },
                order: -50,
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
                command: "zendia.agents.toggle".to_string(),
                command_args: Vec::new(),
                kind: ToolbarItemKind::Button,
                placement: ToolbarPlacement::NavigationEnd,
                placement_rules: ToolbarPlacementRules {
                    single_toolbar: Some(ToolbarArea::SidebarBottom),
                    multiple_toolbars: Some(ToolbarArea::TopBarEnd),
                    collapsed_toolbar: Some(ToolbarArea::TopBarEnd),
                },
                order: -30,
                badge: None,
                visible: true,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
            ToolbarItem {
                id: ContributionId::new("zendia.chat"),
                title: "Chat".to_string(),
                tooltip: Some("Open Agent Chat".to_string()),
                icon: "chat".to_string(),
                command: "zendia.chat.toggle".to_string(),
                command_args: Vec::new(),
                kind: ToolbarItemKind::PillButton,
                placement: ToolbarPlacement::NavigationEnd,
                placement_rules: ToolbarPlacementRules {
                    single_toolbar: Some(ToolbarArea::SidebarBottom),
                    multiple_toolbars: Some(ToolbarArea::TopBarEnd),
                    collapsed_toolbar: Some(ToolbarArea::TopBarEnd),
                },
                order: -20,
                badge: None,
                visible: true,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
            ToolbarItem {
                id: ContributionId::new("zendia.webchat"),
                title: "Web chat".to_string(),
                tooltip: Some("Open Web Chat Panel".to_string()),
                icon: "webchat".to_string(),
                command: "zendia.webchat.toggle".to_string(),
                command_args: Vec::new(),
                kind: ToolbarItemKind::Button,
                placement: ToolbarPlacement::NavigationEnd,
                placement_rules: ToolbarPlacementRules {
                    single_toolbar: None,
                    multiple_toolbars: Some(ToolbarArea::TopBarEnd),
                    collapsed_toolbar: Some(ToolbarArea::TopBarEnd),
                },
                order: -10,
                badge: None,
                visible: true,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
            ToolbarItem {
                id: ContributionId::new("builtin.app_menu"),
                title: "Application Menu".to_string(),
                tooltip: Some("Open Application Menu".to_string()),
                icon: "menu".to_string(),
                command: "menu.app.toggle".to_string(),
                command_args: Vec::new(),
                kind: ToolbarItemKind::AppMenu,
                placement: ToolbarPlacement::SidebarTop,
                placement_rules: ToolbarPlacementRules {
                    single_toolbar: Some(ToolbarArea::SidebarTopStart),
                    multiple_toolbars: Some(ToolbarArea::TopBarEnd),
                    collapsed_toolbar: Some(ToolbarArea::TopBarEnd),
                },
                order: 5,
                badge: None,
                visible: true,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
            ToolbarItem {
                id: ContributionId::new("builtin.sidebar_toggle"),
                title: "Toggle Sidebar".to_string(),
                tooltip: Some("Expand or Collapse Sidebar".to_string()),
                icon: "sidebar".to_string(),
                command: "sidebar.toggle_expanded".to_string(),
                command_args: Vec::new(),
                kind: ToolbarItemKind::Button,
                placement: ToolbarPlacement::SidebarTop,
                placement_rules: ToolbarPlacementRules {
                    single_toolbar: Some(ToolbarArea::SidebarTopStart),
                    multiple_toolbars: Some(ToolbarArea::TopBarStart),
                    collapsed_toolbar: Some(ToolbarArea::TopBarStart),
                },
                order: 10,
                badge: None,
                visible: true,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
            ToolbarItem {
                id: ContributionId::new("builtin.back"),
                title: "Back".to_string(),
                tooltip: Some("Go Back".to_string()),
                icon: "back".to_string(),
                command: "navigation.back".to_string(),
                command_args: Vec::new(),
                kind: ToolbarItemKind::Button,
                placement: ToolbarPlacement::NavigationStart,
                placement_rules: ToolbarPlacementRules {
                    single_toolbar: Some(ToolbarArea::SidebarTopEnd),
                    multiple_toolbars: Some(ToolbarArea::TopBarStart),
                    collapsed_toolbar: Some(ToolbarArea::TopBarStart),
                },
                order: 20,
                badge: None,
                visible: true,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
            ToolbarItem {
                id: ContributionId::new("builtin.forward"),
                title: "Forward".to_string(),
                tooltip: Some("Go Forward".to_string()),
                icon: "forward".to_string(),
                command: "navigation.forward".to_string(),
                command_args: Vec::new(),
                kind: ToolbarItemKind::Button,
                placement: ToolbarPlacement::NavigationStart,
                placement_rules: ToolbarPlacementRules {
                    single_toolbar: Some(ToolbarArea::SidebarTopEnd),
                    multiple_toolbars: Some(ToolbarArea::TopBarStart),
                    collapsed_toolbar: Some(ToolbarArea::TopBarStart),
                },
                order: 30,
                badge: None,
                visible: true,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
            ToolbarItem {
                id: ContributionId::new("builtin.reload"),
                title: "Reload".to_string(),
                tooltip: Some("Reload Page".to_string()),
                icon: "reload".to_string(),
                command: "navigation.reload".to_string(),
                command_args: Vec::new(),
                kind: ToolbarItemKind::Button,
                placement: ToolbarPlacement::NavigationStart,
                placement_rules: ToolbarPlacementRules {
                    single_toolbar: Some(ToolbarArea::SidebarTopEnd),
                    multiple_toolbars: Some(ToolbarArea::TopBarStart),
                    collapsed_toolbar: Some(ToolbarArea::TopBarStart),
                },
                order: 40,
                badge: None,
                visible: true,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
            ToolbarItem {
                id: ContributionId::new("builtin.url_bar"),
                title: "Address Bar".to_string(),
                tooltip: Some("Search or Enter Address".to_string()),
                icon: "search-glass".to_string(),
                command: "location.focus".to_string(),
                command_args: Vec::new(),
                kind: ToolbarItemKind::UrlBar,
                placement: ToolbarPlacement::AddressBarEnd,
                placement_rules: ToolbarPlacementRules {
                    single_toolbar: Some(ToolbarArea::SidebarUrlBar),
                    multiple_toolbars: Some(ToolbarArea::TopBarCenter),
                    collapsed_toolbar: Some(ToolbarArea::TopBarCenter),
                },
                order: 50,
                badge: None,
                visible: true,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
            ToolbarItem {
                id: ContributionId::new("builtin.new_tab"),
                title: "New Tab".to_string(),
                tooltip: Some("Open a New Tab".to_string()),
                icon: "plus".to_string(),
                command: "tab.new".to_string(),
                command_args: Vec::new(),
                kind: ToolbarItemKind::Button,
                placement: ToolbarPlacement::SidebarBottom,
                placement_rules: ToolbarPlacementRules {
                    single_toolbar: Some(ToolbarArea::SidebarBottom),
                    multiple_toolbars: Some(ToolbarArea::SidebarBottom),
                    collapsed_toolbar: Some(ToolbarArea::SidebarBottom),
                },
                order: 90,
                badge: None,
                visible: true,
                enabled: true,
                source: ContributionSource::BuiltIn,
            },
        ],
        menu_entries: builtin_menu_entries(false, "ChatGPT", 0),
        palette_entries: builtin_palette_entries(),
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

pub fn sample_model(include_sample_tabs: bool) -> BrowserModel {
    let default_space = SpaceId(1);
    let work_space = SpaceId(2);

    let (
        tabs,
        active_tab,
        default_last_tab,
        work_last_tab,
        essentials,
        pinned_tabs,
        folders,
        split_groups,
        media_sessions,
    ) = if include_sample_tabs {
        let ess_mail = TabId(1);
        let ess_arch = TabId(2);
        let ess_tokens = TabId(3);
        let pinned_arch = TabId(4);
        let pinned_releases = TabId(5);
        let folder_docs = TabId(6);
        let folder_tokens = TabId(7);
        let folder_engine_hub = TabId(8);
        let normal_releases = TabId(9);
        let normal_tokens = TabId(10);
        let normal_arch = TabId(11);
        let normal_docs = TabId(12);
        let work_arch = TabId(13);
        let work_spec = TabId(14);
        let ui_spec_folder = FolderId(1);
        let engine_folder = FolderId(2);

        let tabs = vec![
            Tab {
                id: ess_mail,
                view_id: None,
                space_id: None,
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Essential,
                title: "Mail".to_string(),
                custom_title: None,
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
                id: ess_arch,
                view_id: None,
                space_id: None,
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Essential,
                title: "Architecture".to_string(),
                custom_title: None,
                url: "https://zendia.local/architecture".to_string(),
                favicon: Some("code".to_string()),
                is_loading: false,
                can_navigate_back: false,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: ess_tokens,
                view_id: None,
                space_id: None,
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Essential,
                title: "Design System".to_string(),
                custom_title: None,
                url: "https://zendia.local/design-system".to_string(),
                favicon: Some("sparkles".to_string()),
                is_loading: false,
                can_navigate_back: false,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: pinned_arch,
                view_id: None,
                space_id: Some(default_space),
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Pinned,
                title: "Architecture — Zendia".to_string(),
                custom_title: None,
                url: "https://zendia.local/architecture".to_string(),
                favicon: Some("code".to_string()),
                is_loading: false,
                can_navigate_back: true,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: pinned_releases,
                view_id: None,
                space_id: Some(default_space),
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Pinned,
                title: "Releases — Zendia".to_string(),
                custom_title: None,
                url: "https://zendia.local/releases".to_string(),
                favicon: Some("flame".to_string()),
                is_loading: false,
                can_navigate_back: true,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: folder_docs,
                view_id: None,
                space_id: Some(default_space),
                folder_id: Some(ui_spec_folder),
                split_group_id: None,
                kind: TabKind::Normal,
                title: "Zendia Documentation".to_string(),
                custom_title: None,
                url: "https://zendia.local/docs".to_string(),
                favicon: Some("book".to_string()),
                is_loading: false,
                can_navigate_back: true,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: folder_tokens,
                view_id: None,
                space_id: Some(default_space),
                folder_id: Some(ui_spec_folder),
                split_group_id: None,
                kind: TabKind::Normal,
                title: "Design Tokens — Zendia".to_string(),
                custom_title: None,
                url: "https://zendia.local/tokens".to_string(),
                favicon: Some("palette".to_string()),
                is_loading: false,
                can_navigate_back: false,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: folder_engine_hub,
                view_id: None,
                space_id: Some(default_space),
                folder_id: Some(engine_folder),
                split_group_id: None,
                kind: TabKind::Normal,
                title: "Hub Protocol Spec".to_string(),
                custom_title: None,
                url: "https://zendia.local/engine/hub".to_string(),
                favicon: Some("terminal".to_string()),
                is_loading: false,
                can_navigate_back: false,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: normal_releases,
                view_id: None,
                space_id: Some(default_space),
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Normal,
                title: "Releases — Zendia".to_string(),
                custom_title: None,
                url: "https://zendia.local/releases".to_string(),
                favicon: Some("flame".to_string()),
                is_loading: false,
                can_navigate_back: true,
                can_navigate_forward: false,
                is_audible: true,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: normal_tokens,
                view_id: None,
                space_id: Some(default_space),
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Normal,
                title: "Design Tokens — Zendia".to_string(),
                custom_title: None,
                url: "https://zendia.local/tokens".to_string(),
                favicon: Some("palette".to_string()),
                is_loading: false,
                can_navigate_back: false,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: normal_arch,
                view_id: None,
                space_id: Some(default_space),
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Normal,
                title: "Architecture — Zendia".to_string(),
                custom_title: None,
                url: "https://zendia.local/architecture".to_string(),
                favicon: Some("code".to_string()),
                is_loading: false,
                can_navigate_back: true,
                can_navigate_forward: true,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: normal_docs,
                view_id: None,
                space_id: Some(default_space),
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Normal,
                title: "Zendia Documentation".to_string(),
                custom_title: None,
                url: "https://zendia.local/docs".to_string(),
                favicon: Some("book".to_string()),
                is_loading: false,
                can_navigate_back: false,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: work_arch,
                view_id: None,
                space_id: Some(work_space),
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Normal,
                title: "Architecture — Zendia".to_string(),
                custom_title: None,
                url: "https://zendia.local/architecture".to_string(),
                favicon: Some("code".to_string()),
                is_loading: false,
                can_navigate_back: true,
                can_navigate_forward: false,
                is_audible: false,
                is_muted: false,
                closing: false,
            },
            Tab {
                id: work_spec,
                view_id: None,
                space_id: Some(work_space),
                folder_id: None,
                split_group_id: None,
                kind: TabKind::Normal,
                title: "HTML Living Standard".to_string(),
                custom_title: None,
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

        let essentials = vec![
            Essential {
                tab_id: ess_mail,
                order: 0,
            },
            Essential {
                tab_id: ess_arch,
                order: 1,
            },
            Essential {
                tab_id: ess_tokens,
                order: 2,
            },
        ];
        let pinned_tabs = vec![
            PinnedTab {
                tab_id: pinned_arch,
                space_id: default_space,
                order: 0,
            },
            PinnedTab {
                tab_id: pinned_releases,
                space_id: default_space,
                order: 1,
            },
        ];
        let folders = vec![
            Folder {
                id: ui_spec_folder,
                space_id: default_space,
                name: "UI Spec".to_string(),
                icon: "folder".to_string(),
                collapsed: false,
                tab_ids: vec![folder_docs, folder_tokens],
            },
            Folder {
                id: engine_folder,
                space_id: default_space,
                name: "Engine".to_string(),
                icon: "folder".to_string(),
                collapsed: true,
                tab_ids: vec![folder_engine_hub],
            },
        ];
        // Split groups and media start empty so every earlier gallery state keeps its look; the phase 4 gallery
        // states add their own.
        let split_groups: Vec<SplitGroup> = Vec::new();
        let media_sessions: Vec<MediaSession> = Vec::new();

        (
            tabs,
            Some(folder_docs),
            Some(folder_docs),
            Some(work_arch),
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
            None,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    };

    let spaces = vec![
        Space {
            id: default_space,
            name: "Space".to_string(),
            icon: String::new(),
            gradient: SpaceGradient {
                stops: vec![(62, 54, 102), (96, 64, 118)],
                angle_deg: 135.0,
                opacity: 0.9,
            },
            last_active_tab: default_last_tab,
        },
        Space {
            id: work_space,
            name: "Work".to_string(),
            icon: "briefcase".to_string(),
            gradient: SpaceGradient {
                stops: vec![(20, 74, 92), (32, 118, 138)],
                angle_deg: 150.0,
                opacity: 0.9,
            },
            last_active_tab: work_last_tab,
        },
    ];

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
        HistoryEntry {
            id: HistoryEntryId(3),
            url: "https://doc.rust-lang.org/std/".to_string(),
            title: "Rust Standard Library".to_string(),
            visited_at_unix: 1_763_982_000,
            visit_count: 9,
        },
        HistoryEntry {
            id: HistoryEntryId(4),
            url: "https://zendia.local/changelog".to_string(),
            title: "Zendia Changelog".to_string(),
            visited_at_unix: 1_763_978_400,
            visit_count: 7,
        },
    ];

    let bookmarks = vec![
        Bookmark {
            id: BookmarkId(1),
            url: "https://wikipedia.org/".to_string(),
            title: "Wikipedia".to_string(),
            favicon: Some("book".to_string()),
            folder: Some("Top Sites".to_string()),
            created_at_unix: 1_763_900_000,
        },
        Bookmark {
            id: BookmarkId(2),
            url: "https://youtube.com/".to_string(),
            title: "YouTube".to_string(),
            favicon: Some("media-play".to_string()),
            folder: Some("Top Sites".to_string()),
            created_at_unix: 1_763_900_100,
        },
        Bookmark {
            id: BookmarkId(3),
            url: "https://reddit.com/".to_string(),
            title: "Reddit".to_string(),
            favicon: Some("flame".to_string()),
            folder: Some("Top Sites".to_string()),
            created_at_unix: 1_763_900_200,
        },
        Bookmark {
            id: BookmarkId(4),
            url: "https://addons.zendia.org/".to_string(),
            title: "Add-ons for Zendia".to_string(),
            favicon: Some("extension".to_string()),
            folder: Some("Top Sites".to_string()),
            created_at_unix: 1_763_900_300,
        },
        Bookmark {
            id: BookmarkId(5),
            url: "https://zendia.local/docs".to_string(),
            title: "Zendia Documentation".to_string(),
            favicon: Some("book".to_string()),
            folder: Some("Engineering".to_string()),
            created_at_unix: 1_763_910_000,
        },
    ];

    let mut site_info = BTreeMap::new();
    let mut zendia_site = SiteInfo::for_url("https://zendia.local/docs");
    zendia_site.bookmarked = true;
    zendia_site.data = SiteDataSummary {
        cookie_count: 4,
        storage_bytes: 128 * 1024,
    };
    zendia_site.permissions = vec![
        SitePermission {
            kind: SitePermissionKind::Camera,
            state: PermissionState::Allow,
        },
        SitePermission {
            kind: SitePermissionKind::Location,
            state: PermissionState::Block,
        },
        SitePermission {
            kind: SitePermissionKind::Notifications,
            state: PermissionState::Allow,
        },
    ];
    site_info.insert(zendia_site.origin.clone(), zendia_site);

    let site_extensions = vec![SiteExtension {
        id: ContributionId::new("ext.ublock_origin"),
        name: "uBlock Origin".to_string(),
        icon: "tracking-protection-fill".to_string(),
        enabled: true,
        has_site_access: true,
        command: "extension.ublock.toggle".to_string(),
        source: ContributionSource::Extension {
            extension_id: "ublock-origin@zendia".to_string(),
        },
    }];

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
        active_space: Some(default_space),
        active_tab,
    }];

    BrowserModel {
        windows,
        spaces,
        active_space: Some(default_space),
        tabs,
        active_tab,
        essentials,
        pinned_tabs,
        folders,
        split_groups,
        downloads,
        history,
        bookmarks,
        search_engine: SearchEngine::default(),
        site_info,
        site_extensions,
        media_sessions,
        sidebar_notifications: Vec::new(),
        show_essentials_promo: false,
        glance: None,
        toasts: Vec::new(),
        contributions: sample_contributions(),
        agents,
        conversations,
        models,
        chat_sites,
        services,
        input_mode: InputMode::Normal,
        pending_keys: Vec::new(),
        pending_count: None,
        which_key: None,
        statusline: StatuslineSettings::default(),
        statusline_announcement: None,
        focus: crate::focus::FocusState::default(),
        theme: ThemeSettings::default(),
        layout: LayoutSettings::default(),
        vim_mode_enabled: false,
        zoom_level: 1.0,
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
                custom_title: None,
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
        assert!(!model.downloads.is_empty());
        assert!(!model.history.is_empty());
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
        assert_eq!(core.model().active_tab().unwrap().url, "https://ladybird.org");
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
        assert_eq!(view_model.active_tab().unwrap().space_id, Some(SpaceId(2)));

        // Switching back to Space 1 restores the tab that was last active there.
        core.invoke(Command::SpaceSwitch { space_id: SpaceId(1) }).unwrap();
        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }
        assert_eq!(view_model, core.model());
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
        let initial_tabs = core.model().tabs_in_active_space_order();
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
        assert_eq!(
            Command::from_name_and_args("vim.set_enabled", &[("enabled".to_string(), "true".to_string())]).unwrap(),
            Command::VimSetEnabled { enabled: true }
        );
        assert_eq!(
            Command::from_name_and_args("menu.app.toggle", &[]).unwrap(),
            Command::MenuAppToggle
        );
        assert_eq!(
            Command::from_name_and_args("app.about", &[]).unwrap(),
            Command::AppAbout
        );
        assert_eq!(
            Command::from_name_and_args("page.paste_and_match_style", &[]).unwrap(),
            Command::PagePaste {
                tab_id: None,
                match_style: true,
            }
        );
        assert_eq!(
            Command::from_name_and_args("media.toggle_loop", &[("tab_id".to_string(), "7".to_string())]).unwrap(),
            Command::MediaContextAction {
                tab_id: Some(TabId(7)),
                action: crate::commands::MediaAction::ToggleLoop,
            }
        );
        assert_eq!(
            Command::from_name_and_args(
                "page.select_dropdown_choose",
                &[
                    ("tab_id".to_string(), "3".to_string()),
                    ("item_id".to_string(), "2".to_string())
                ]
            )
            .unwrap(),
            Command::PageSelectDropdownChoose {
                tab_id: Some(TabId(3)),
                item_id: Some(2),
            }
        );
    }

    #[test]
    fn vim_mode_and_zoom_commands_update_model_and_publish_events() {
        let core = StubBackend::new();
        let mut view_model = core.model();
        let sub = core.subscribe();

        assert!(!view_model.vim_mode_enabled);
        assert!((view_model.zoom_level - 1.0).abs() < 1e-5);

        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        core.invoke(Command::ZoomIn { tab_id: None }).unwrap();
        core.invoke(Command::ZoomReset { tab_id: None }).unwrap();

        let mut saw_vim = false;
        let mut saw_zoom = false;
        while let Ok(event) = sub.try_recv() {
            match event {
                Event::VimModeChanged { enabled: true } => saw_vim = true,
                Event::ZoomChanged { .. } => saw_zoom = true,
                _ => {}
            }
            view_model.apply_event(&event);
        }
        assert!(saw_vim);
        assert!(saw_zoom);
        assert!(view_model.vim_mode_enabled);
        assert!((view_model.zoom_level - 1.0).abs() < 1e-5);
        assert_eq!(view_model, core.model());
    }

    #[test]
    fn layout_settings_defaults_and_commands_publish_events() {
        use crate::model::{SIDEBAR_COLLAPSED_WIDTH, SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH};

        let core = StubBackend::new();
        let mut view_model = core.model();
        let sub = core.subscribe();

        assert_eq!(view_model.layout.toolbar_layout, ToolbarLayout::Single);
        assert_eq!(view_model.layout.sidebar_side, SidebarSide::Left);
        assert!(view_model.layout.sidebar_expanded);
        assert_eq!(view_model.layout.sidebar_width, SIDEBAR_DEFAULT_WIDTH);
        assert!(!view_model.layout.compact_mode);
        assert!(!view_model.layout.has_top_toolbar());
        assert_eq!(view_model.layout.effective_sidebar_width(), SIDEBAR_DEFAULT_WIDTH);

        core.invoke(Command::LayoutSetToolbarLayout {
            layout: ToolbarLayout::Multiple,
        })
        .unwrap();
        core.invoke(Command::SidebarSetSide {
            side: SidebarSide::Right,
        })
        .unwrap();
        core.invoke(Command::SidebarSetWidth { width: 260.0 }).unwrap();
        core.invoke(Command::LayoutSetCompactMode { enabled: true }).unwrap();

        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }
        assert_eq!(view_model.layout, core.model().layout);
        assert_eq!(view_model.layout.toolbar_layout, ToolbarLayout::Multiple);
        assert_eq!(view_model.layout.sidebar_side, SidebarSide::Right);
        assert!(view_model.layout.sidebar_expanded);
        assert!(view_model.layout.has_top_toolbar());
        assert_eq!(view_model.layout.sidebar_width, 260.0);
        assert!(view_model.layout.compact_mode);

        // Collapsing sidebar switches to Collapsed layout and reports collapsed width.
        core.invoke(Command::SidebarToggleExpanded).unwrap();
        let collapsed = core.model().layout;
        assert!(!collapsed.sidebar_expanded);
        assert_eq!(collapsed.toolbar_layout, ToolbarLayout::Collapsed);
        assert_eq!(collapsed.effective_sidebar_width(), SIDEBAR_COLLAPSED_WIDTH);
        assert_eq!(collapsed.sidebar_width, 260.0);

        // Expanding again switches to Multiple toolbars and restores expanded width.
        core.invoke(Command::SidebarToggleExpanded).unwrap();
        let expanded = core.model().layout;
        assert!(expanded.sidebar_expanded);
        assert_eq!(expanded.toolbar_layout, ToolbarLayout::Multiple);
        assert_eq!(expanded.effective_sidebar_width(), 260.0);

        // Width clamps to Zen's min/max bounds and rejects non-finite values.
        core.invoke(Command::SidebarSetWidth { width: 20.0 }).unwrap();
        assert_eq!(core.model().layout.sidebar_width, SIDEBAR_MIN_WIDTH);
        core.invoke(Command::SidebarSetWidth { width: 900.0 }).unwrap();
        assert_eq!(core.model().layout.sidebar_width, SIDEBAR_MAX_WIDTH);
        assert!(matches!(
            core.invoke(Command::SidebarSetWidth { width: f32::NAN }),
            Err(CommandError::InvalidArgument { .. })
        ));
    }

    #[test]
    fn layout_commands_parse_from_name_and_args() {
        assert_eq!(
            Command::from_name_and_args(
                "layout.set_toolbar_layout",
                &[("layout".to_string(), "multiple".to_string())]
            )
            .unwrap(),
            Command::LayoutSetToolbarLayout {
                layout: ToolbarLayout::Multiple,
            }
        );
        assert_eq!(
            Command::from_name_and_args("sidebar.set_side", &[("side".to_string(), "right".to_string())]).unwrap(),
            Command::SidebarSetSide {
                side: SidebarSide::Right,
            }
        );
        assert_eq!(
            Command::from_name_and_args("sidebar.set_width", &[("width".to_string(), "220".to_string())]).unwrap(),
            Command::SidebarSetWidth { width: 220.0 }
        );
        assert_eq!(
            Command::from_name_and_args("sidebar.toggle_expanded", &[]).unwrap(),
            Command::SidebarToggleExpanded
        );
        assert!(matches!(
            Command::from_name_and_args(
                "layout.set_toolbar_layout",
                &[("layout".to_string(), "bogus".to_string())]
            ),
            Err(CommandError::InvalidArgument { .. })
        ));
    }

    #[test]
    fn builtin_toolbar_contributions_follow_layout_placement_rules() {
        let core = StubBackend::new();
        let model = core.model();
        let items = &model.contributions.toolbar_items;

        for expected in [
            "builtin.back",
            "builtin.forward",
            "builtin.reload",
            "builtin.url_bar",
            "builtin.sidebar_toggle",
            "builtin.new_tab",
            "builtin.app_menu",
            "builtin.library",
            "zendia.chat",
            "zendia.agents",
            "zendia.webchat",
        ] {
            assert!(
                items.iter().any(|item| item.id.as_str() == expected),
                "missing built-in toolbar contribution `{expected}`"
            );
        }

        let item = |id: &str| items.iter().find(|i| i.id.as_str() == id).unwrap();
        let single = LayoutSettings {
            toolbar_layout: ToolbarLayout::Single,
            ..LayoutSettings::default()
        };
        let multiple = LayoutSettings {
            toolbar_layout: ToolbarLayout::Multiple,
            ..LayoutSettings::default()
        };
        let collapsed = LayoutSettings {
            toolbar_layout: ToolbarLayout::Collapsed,
            sidebar_expanded: false,
            ..LayoutSettings::default()
        };

        assert_eq!(
            single.resolve_toolbar_area(item("builtin.url_bar").placement_rules),
            Some(ToolbarArea::SidebarUrlBar)
        );
        assert_eq!(
            multiple.resolve_toolbar_area(item("builtin.url_bar").placement_rules),
            Some(ToolbarArea::TopBarCenter)
        );
        assert_eq!(
            single.resolve_toolbar_area(item("zendia.chat").placement_rules),
            Some(ToolbarArea::SidebarBottom)
        );
        assert_eq!(
            multiple.resolve_toolbar_area(item("zendia.chat").placement_rules),
            Some(ToolbarArea::TopBarEnd)
        );
        assert_eq!(
            single.resolve_toolbar_area(item("zendia.webchat").placement_rules),
            None
        );
        assert_eq!(
            collapsed.resolve_toolbar_area(item("zendia.webchat").placement_rules),
            Some(ToolbarArea::TopBarEnd)
        );
        assert_eq!(
            single.resolve_toolbar_area(item("builtin.app_menu").placement_rules),
            Some(ToolbarArea::SidebarTopStart)
        );
        assert_eq!(
            multiple.resolve_toolbar_area(item("builtin.app_menu").placement_rules),
            Some(ToolbarArea::TopBarEnd)
        );
    }

    #[test]
    fn space_switching_filters_tabs_and_restores_last_active_tab() {
        let core = StubBackend::new();
        let mut view_model = core.model();
        let sub = core.subscribe();

        assert_eq!(view_model.active_space, Some(SpaceId(1)));
        assert_eq!(view_model.spaces.len(), 2);
        assert_eq!(view_model.spaces[0].name, "Space");
        assert_eq!(view_model.spaces[1].name, "Work");

        // Activate a specific normal tab in Space 1.
        let space1_normal = view_model.normal_tabs_in_space(SpaceId(1))[0].id;
        core.invoke(Command::TabActivate { tab_id: space1_normal }).unwrap();

        // Switch to next space ("Work"): activates Work's last active tab.
        core.invoke(Command::SpaceNext).unwrap();
        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }
        assert_eq!(view_model, core.model());
        assert_eq!(view_model.active_space, Some(SpaceId(2)));
        assert_eq!(view_model.active_tab().unwrap().space_id, Some(SpaceId(2)));

        // New tabs belong to the active space ("Work").
        core.invoke(Command::TabNew {
            url: Some("https://work.example/".to_string()),
            after: None,
            activate: true,
        })
        .unwrap();
        let new_work_tab = core.model().active_tab.unwrap();
        assert_eq!(core.model().tab(new_work_tab).unwrap().space_id, Some(SpaceId(2)));

        // Essentials are shared across spaces; Space 1's folders/pinned tabs do not show in Space 2.
        assert_eq!(core.model().essential_tabs().len(), 3);
        assert!(core.model().folders_in_space(SpaceId(2)).is_empty());
        assert!(core.model().pinned_tabs_in_space(SpaceId(2)).is_empty());

        // Switching back to Space 1 via SpacePrevious restores Space 1's last active tab.
        core.invoke(Command::SpacePrevious).unwrap();
        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }
        assert_eq!(view_model, core.model());
        assert_eq!(view_model.active_space, Some(SpaceId(1)));
        assert_eq!(view_model.active_tab, Some(space1_normal));

        // Switching back to Work restores the newly opened Work tab.
        core.invoke(Command::SpaceSwitch { space_id: SpaceId(2) }).unwrap();
        assert_eq!(core.model().active_tab, Some(new_work_tab));
    }

    #[test]
    fn essentials_pin_unpin_and_clear_tabs_enforce_section_rules() {
        let core = StubBackend::new();
        let mut view_model = core.model();
        let sub = core.subscribe();

        let normal_tab = view_model.normal_tabs_in_space(SpaceId(1))[0].id;
        let pinned_before = view_model.pinned_tabs_in_space(SpaceId(1)).len();
        let essentials_before = view_model.essential_tabs().len();

        // Pin normal tab.
        core.invoke(Command::TabPin {
            tab_id: Some(normal_tab),
        })
        .unwrap();
        assert_eq!(core.model().tab(normal_tab).unwrap().kind, TabKind::Pinned);
        assert_eq!(core.model().pinned_tabs_in_space(SpaceId(1)).len(), pinned_before + 1);

        // Unpin back to normal.
        core.invoke(Command::TabUnpin {
            tab_id: Some(normal_tab),
        })
        .unwrap();
        assert_eq!(core.model().tab(normal_tab).unwrap().kind, TabKind::Normal);
        assert_eq!(core.model().pinned_tabs_in_space(SpaceId(1)).len(), pinned_before);

        // Promote to essentials (shared across spaces: space_id becomes None).
        core.invoke(Command::TabAddToEssentials {
            tab_id: Some(normal_tab),
        })
        .unwrap();
        assert_eq!(core.model().tab(normal_tab).unwrap().kind, TabKind::Essential);
        assert_eq!(core.model().tab(normal_tab).unwrap().space_id, None);
        assert_eq!(core.model().essential_tabs().len(), essentials_before + 1);

        // Remove from essentials while in Space 2 -> lands in Space 2.
        core.invoke(Command::SpaceSwitch { space_id: SpaceId(2) }).unwrap();
        core.invoke(Command::TabRemoveFromEssentials {
            tab_id: Some(normal_tab),
        })
        .unwrap();
        assert_eq!(core.model().tab(normal_tab).unwrap().kind, TabKind::Normal);
        assert_eq!(core.model().tab(normal_tab).unwrap().space_id, Some(SpaceId(2)));
        assert_eq!(core.model().essential_tabs().len(), essentials_before);

        // Switch back to Space 1 and clear normal tabs: pinned, folders, and essentials remain intact.
        core.invoke(Command::SpaceSwitch { space_id: SpaceId(1) }).unwrap();
        let pinned_count = core.model().pinned_tabs_in_space(SpaceId(1)).len();
        let folder_count = core.model().folders_in_space(SpaceId(1)).len();
        core.invoke(Command::SpaceClearTabs { space_id: None }).unwrap();

        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }
        assert_eq!(view_model, core.model());
        assert!(view_model.normal_tabs_in_space(SpaceId(1)).is_empty());
        assert_eq!(view_model.pinned_tabs_in_space(SpaceId(1)).len(), pinned_count);
        assert_eq!(view_model.folders_in_space(SpaceId(1)).len(), folder_count);
        assert_eq!(view_model.essential_tabs().len(), essentials_before);
    }

    #[test]
    fn folders_collapse_with_active_child_and_support_crud_commands() {
        let core = StubBackend::new();
        let mut view_model = core.model();
        let sub = core.subscribe();

        // Collapsed folder "Engine" (FolderId(2)) hides its inactive child tab.
        let engine_folder = FolderId(2);
        assert!(view_model.folder(engine_folder).unwrap().collapsed);
        assert!(view_model.visible_folder_tabs(engine_folder).is_empty());

        // Activating the child inside the collapsed folder keeps that active child visible.
        let engine_child = view_model.folder_tabs(engine_folder)[0].id;
        core.invoke(Command::TabActivate { tab_id: engine_child }).unwrap();
        assert_eq!(
            core.model()
                .visible_folder_tabs(engine_folder)
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>(),
            vec![engine_child]
        );

        // Toggle collapsed expands it, then rename and create/unpack/delete folders.
        core.invoke(Command::FolderToggleCollapsed {
            folder_id: engine_folder,
        })
        .unwrap();
        assert!(!core.model().folder(engine_folder).unwrap().collapsed);

        core.invoke(Command::FolderRename {
            folder_id: engine_folder,
            name: "Core Engine".to_string(),
        })
        .unwrap();
        assert_eq!(core.model().folder(engine_folder).unwrap().name, "Core Engine");

        let normal_tab = core.model().normal_tabs_in_space(SpaceId(1))[0].id;
        core.invoke(Command::FolderCreate {
            name: Some("Reading".to_string()),
            space_id: Some(SpaceId(1)),
            tab_id: Some(normal_tab),
        })
        .unwrap();
        let created_id = core.model().folders.iter().find(|f| f.name == "Reading").unwrap().id;
        assert_eq!(core.model().tab(normal_tab).unwrap().folder_id, Some(created_id));

        // Unpack keeps the tab and deletes the folder.
        core.invoke(Command::FolderUnpack { folder_id: created_id }).unwrap();
        assert!(core.model().folder(created_id).is_none());
        assert!(core.model().tab(normal_tab).is_some());
        assert_eq!(core.model().tab(normal_tab).unwrap().folder_id, None);

        // Delete removes the folder and closes its tabs.
        core.invoke(Command::FolderDelete {
            folder_id: engine_folder,
        })
        .unwrap();
        assert!(core.model().folder(engine_folder).is_none());
        assert!(core.model().tab(engine_child).is_none());

        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }
        assert_eq!(view_model, core.model());
    }

    #[test]
    fn tab_move_reorders_within_and_across_sections() {
        let core = StubBackend::new();
        let mut view_model = core.model();
        let sub = core.subscribe();

        let normals: Vec<TabId> = view_model
            .normal_tabs_in_space(SpaceId(1))
            .iter()
            .map(|t| t.id)
            .collect();
        assert!(normals.len() >= 3);
        let moved = normals[2];

        // Move third normal tab to index 0.
        core.invoke(Command::TabMove {
            tab_id: moved,
            target: TabMoveTarget::Normal {
                space_id: Some(SpaceId(1)),
                index: 0,
            },
        })
        .unwrap();
        assert_eq!(core.model().normal_tabs_in_space(SpaceId(1))[0].id, moved);

        // Move into folder UI Spec at index 0.
        core.invoke(Command::TabMove {
            tab_id: moved,
            target: TabMoveTarget::Folder {
                folder_id: FolderId(1),
                index: 0,
            },
        })
        .unwrap();
        assert_eq!(core.model().folder_tabs(FolderId(1))[0].id, moved);

        // Move into pinned tabs at index 0.
        core.invoke(Command::TabMove {
            tab_id: moved,
            target: TabMoveTarget::Pinned {
                space_id: Some(SpaceId(1)),
                index: 0,
            },
        })
        .unwrap();
        assert_eq!(core.model().pinned_tabs_in_space(SpaceId(1))[0].id, moved);

        // Move to Space 2 via tab.move_to_space.
        core.invoke(Command::TabMoveToSpace {
            tab_id: Some(moved),
            space_id: SpaceId(2),
        })
        .unwrap();
        assert_eq!(core.model().tab(moved).unwrap().space_id, Some(SpaceId(2)));

        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }
        assert_eq!(view_model, core.model());
    }

    #[test]
    fn tab_rename_and_sidebar_notification_commands_publish_events() {
        let core = StubBackend::new();
        let mut view_model = core.model();
        let sub = core.subscribe();

        let target = view_model.normal_tabs_in_space(SpaceId(1))[0].id;
        core.invoke(Command::TabRename {
            tab_id: Some(target),
            title: "  Sprint   Notes  ".to_string(),
        })
        .unwrap();
        assert_eq!(core.model().tab(target).unwrap().display_title(), "Sprint Notes");

        // Page title updates preserve custom_title.
        core.feed_tab_updated(
            target,
            TabUpdate {
                title: Some("Original Page Title".to_string()),
                ..TabUpdate::default()
            },
        );
        assert_eq!(core.model().tab(target).unwrap().display_title(), "Sprint Notes");

        // Empty rename clears the custom title back to the page title.
        core.invoke(Command::TabRename {
            tab_id: Some(target),
            title: "   ".to_string(),
        })
        .unwrap();
        assert_eq!(core.model().tab(target).unwrap().display_title(), "Original Page Title");

        // Sidebar notification add and dismiss.
        let notif = sample_update_notification();
        let notif_id = notif.id;
        core.show_notification(notif);
        assert_eq!(core.model().sidebar_notifications.len(), 1);
        core.invoke(Command::NotificationDismiss {
            notification_id: notif_id,
        })
        .unwrap();
        assert!(core.model().sidebar_notifications.is_empty());

        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }
        assert_eq!(view_model, core.model());
    }

    #[test]
    fn phase2_commands_parse_and_have_descriptors() {
        let core = StubBackend::new();
        let names: Vec<String> = core.commands().into_iter().map(|d| d.name).collect();
        for required in [
            "space.switch",
            "space.next",
            "space.previous",
            "space.clear_tabs",
            "tab.pin",
            "tab.unpin",
            "tab.toggle_pin",
            "tab.add_to_essentials",
            "tab.remove_from_essentials",
            "tab.toggle_essential",
            "tab.toggle_mute",
            "tab.duplicate",
            "tab.rename",
            "tab.start_rename",
            "tab.move",
            "tab.move_to_folder",
            "tab.move_to_space",
            "folder.create",
            "folder.rename",
            "folder.start_rename",
            "folder.delete",
            "folder.unpack",
            "folder.toggle_collapsed",
            "notification.dismiss",
        ] {
            assert!(names.iter().any(|n| n == required), "missing descriptor `{required}`");
        }

        assert_eq!(
            Command::from_name_and_args("space.next", &[]).unwrap(),
            Command::SpaceNext
        );
        assert_eq!(
            Command::from_name_and_args("space.previous", &[]).unwrap(),
            Command::SpacePrevious
        );
        assert_eq!(
            Command::from_name_and_args(
                "tab.move",
                &[
                    ("tab_id".to_string(), "9".to_string()),
                    ("section".to_string(), "folder".to_string()),
                    ("folder_id".to_string(), "1".to_string()),
                    ("index".to_string(), "2".to_string()),
                ]
            )
            .unwrap(),
            Command::TabMove {
                tab_id: TabId(9),
                target: TabMoveTarget::Folder {
                    folder_id: FolderId(1),
                    index: 2,
                },
            }
        );
        assert_eq!(
            Command::from_name_and_args(
                "notification.dismiss",
                &[("notification_id".to_string(), "1".to_string())]
            )
            .unwrap(),
            Command::NotificationDismiss {
                notification_id: NotificationId(1),
            }
        );
    }

    #[test]
    fn address_bar_palette_and_site_commands_handle_stub_state_and_events() {
        let core = StubBackend::new();
        let sub = core.subscribe();

        // View-level open requests emit their events.
        core.invoke(Command::AddressBarOpen {
            new_tab: true,
            initial_query: None,
        })
        .unwrap();
        assert_eq!(
            sub.try_recv().unwrap(),
            Event::AddressBarOpenRequested {
                new_tab: true,
                initial_query: None
            }
        );

        core.invoke(Command::PaletteOpen {
            initial_query: Some("theme".to_string()),
        })
        .unwrap();
        assert_eq!(
            sub.try_recv().unwrap(),
            Event::PaletteOpenRequested {
                initial_query: Some("theme".to_string())
            }
        );

        core.invoke(Command::SitePanelToggle).unwrap();
        assert_eq!(sub.try_recv().unwrap(), Event::SitePanelToggleRequested);

        // Search engine setting updates core state and governs address_bar.submit.
        core.invoke(Command::SettingsSetSearchEngine {
            name: "DuckDuckGo".to_string(),
            search_url: "https://duckduckgo.com/?q=%s".to_string(),
        })
        .unwrap();
        assert_eq!(core.model().search_engine.name, "DuckDuckGo");
        assert!(matches!(sub.try_recv().unwrap(), Event::SearchEngineChanged { .. }));

        let before_tabs = core.model().tabs.len();
        core.invoke(Command::AddressBarSubmit {
            input: "rust language".to_string(),
            new_tab: true,
        })
        .unwrap();
        assert_eq!(core.model().tabs.len(), before_tabs + 1);
        assert_eq!(
            core.model().active_tab().unwrap().url,
            "https://duckduckgo.com/?q=rust+language"
        );
        // Drain tab opened & activated events.
        while sub.try_recv().is_ok() {}

        // Switch to tab across spaces switches space and activates target tab.
        let work_tab_id = TabId(14);
        core.invoke(Command::AddressBarSwitchToTab { tab_id: work_tab_id })
            .unwrap();
        assert_eq!(core.model().active_space, Some(SpaceId(2)));
        assert_eq!(core.model().active_tab, Some(work_tab_id));
        while sub.try_recv().is_ok() {}

        // Site permissions, tracking protection, clear data, and bookmarks.
        core.invoke(Command::SitePermissionSet {
            origin: None,
            permission: SitePermissionKind::Microphone,
            state: PermissionState::Allow,
        })
        .unwrap();
        assert_eq!(
            core.model()
                .active_site_info()
                .permission_state(SitePermissionKind::Microphone),
            PermissionState::Allow
        );
        assert!(matches!(sub.try_recv().unwrap(), Event::SiteInfoUpdated { .. }));

        let tp_before = core.model().active_site_info().tracking_protection_enabled;
        core.invoke(Command::SiteTrackingProtectionToggle { origin: None })
            .unwrap();
        assert_eq!(core.model().active_site_info().tracking_protection_enabled, !tp_before);
        assert!(matches!(sub.try_recv().unwrap(), Event::SiteInfoUpdated { .. }));

        core.invoke(Command::SiteClearData { origin: None }).unwrap();
        assert!(core.model().active_site_info().data.is_empty());
        assert!(matches!(sub.try_recv().unwrap(), Event::SiteDataCleared { .. }));
        assert!(matches!(sub.try_recv().unwrap(), Event::SiteInfoUpdated { .. }));

        let was_bookmarked = core.model().active_site_info().bookmarked;
        core.invoke(Command::SiteBookmarkToggle { tab_id: None }).unwrap();
        assert_eq!(core.model().active_site_info().bookmarked, !was_bookmarked);
        while sub.try_recv().is_ok() {}

        // Action row commands and global action helpers.
        core.invoke(Command::SiteShare { tab_id: None }).unwrap();
        assert!(matches!(sub.try_recv().unwrap(), Event::ClipboardCopyRequested { .. }));
        core.invoke(Command::SiteScreenshot { tab_id: None }).unwrap();
        core.invoke(Command::SiteEditStyle { tab_id: None }).unwrap();
        core.invoke(Command::SiteReaderModeToggle { tab_id: None }).unwrap();

        let compact_before = core.model().layout.compact_mode;
        core.invoke(Command::LayoutToggleCompactMode).unwrap();
        assert_eq!(core.model().layout.compact_mode, !compact_before);

        let side_before = core.model().layout.sidebar_side;
        core.invoke(Command::SidebarToggleSide).unwrap();
        assert_ne!(core.model().layout.sidebar_side, side_before);

        // Command descriptors and parsing round-trip.
        let names: Vec<String> = core.commands().into_iter().map(|d| d.name).collect();
        for required in [
            "address_bar.open",
            "address_bar.submit",
            "address_bar.switch_to_tab",
            "address_bar.copy_current_url",
            "palette.open",
            "site.panel.toggle",
            "site.permission.set",
            "site.tracking_protection.toggle",
            "site.clear_data",
            "site.bookmark.toggle",
            "site.screenshot",
            "site.share",
            "site.edit_style",
            "site.reader_mode.toggle",
            "settings.set_search_engine",
            "layout.toggle_compact_mode",
            "sidebar.toggle_side",
            "folder.collapse_all",
            "folder.expand_all",
        ] {
            assert!(names.iter().any(|n| n == required), "missing descriptor `{required}`");
        }
        assert_eq!(
            Command::from_name_and_args(
                "site.permission.set",
                &[
                    ("permission".to_string(), "camera".to_string()),
                    ("state".to_string(), "block".to_string()),
                ]
            )
            .unwrap(),
            Command::SitePermissionSet {
                origin: None,
                permission: SitePermissionKind::Camera,
                state: PermissionState::Block,
            }
        );
    }
}
