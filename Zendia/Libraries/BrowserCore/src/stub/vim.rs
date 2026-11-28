/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for the vim key engine's commands: per-page modes, pending keys, scrolling, and field edits.

use super::StubBackend;
use crate::BrowserCore;
use crate::commands::CommandResult;
use crate::events::Event;
use crate::input_state::which_key_for_prefix;
use crate::model::{InputMode, TabId};
use crate::page_actions::PageHintSession;
use crate::vim_keys::VimCommand;
use std::collections::{BTreeMap, BTreeSet};
use zendia_automation::{AutomationRequest, ViewportRect};

fn url_without_fragment(url: &str) -> &str {
    url.split('#').next().unwrap_or(url)
}

/// Per-page vim state (`MD-01`, `MD-02`, `MD-03`, `RT-02`, `RT-04`, `SL-03`).
#[derive(Clone, Debug)]
pub(super) struct PageVimState {
    pub(super) mode: InputMode,
    /// Insert mode was asked for with `I`, so it lasts until `<Esc>` whatever has the focus.
    pub(super) explicit_insert: bool,
    /// Whether an editable field of this page has focus (`INS-01`).
    pub(super) editing: bool,
    pub(super) pending_keys: Vec<String>,
    pub(super) pending_count: Option<u32>,
    pub(super) page_hints: Option<PageHintSession>,
    pub(super) visual_caret: Option<ViewportRect>,
    /// True while an async mode-changing request (`i`, `gi`, `v`, label pick) is in flight (`RT-04`).
    pub(super) mode_transition_pending: bool,
    pub(super) last_url: String,
}

impl Default for PageVimState {
    fn default() -> Self {
        Self {
            mode: InputMode::Normal,
            explicit_insert: false,
            editing: false,
            pending_keys: Vec::new(),
            pending_count: None,
            page_hints: None,
            visual_caret: None,
            mode_transition_pending: false,
            last_url: String::new(),
        }
    }
}

/// Destination in the `[left panels, page, right panels]` focus row (`ZendiaPanes.sys.mjs`, `WIN-06`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PanePlace {
    Page,
    Panel(crate::contributions::SidePanelId),
}

/// Blank placeholder pane opened while the floating URL bar waits for a split destination (`WIN-20`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EmptySplitPane {
    pub(super) empty_tab: TabId,
    pub(super) anchor_tab: TabId,
    pub(super) side: crate::split::DropZone,
}

/// Window- and per-page state remembered for the vim key engine.
#[derive(Debug, Default)]
pub(super) struct VimState {
    pub(super) pages: BTreeMap<TabId, PageVimState>,
    /// Auxiliary page surface (extension popup or web chat panel) currently holding keyboard focus.
    pub(super) keyed_page: Option<TabId>,
    /// Auxiliary Hub view IDs registered by the window shell.
    pub(super) auxiliary_pages: BTreeSet<TabId>,
    /// Active window-level chrome mode (`Explorer`, `Toolbar`, `Library`, `Window`), if any.
    pub(super) chrome_mode: Option<InputMode>,
    pub(super) chrome_pending_keys: Vec<String>,
    pub(super) chrome_pending_count: Option<u32>,
    /// Most-recently-activated tabs (oldest first, newest last) for `tab.alternate` and `split.last`.
    pub(super) tab_recency: Vec<TabId>,
    /// Open side panels in open order, synced with `DockHostState` (`WIN-06`).
    pub(super) open_panels: Vec<crate::contributions::SidePanelId>,
    /// Side panel currently holding keyboard focus (`None` when the page does) (`WIN-06`).
    pub(super) focused_panel: Option<crate::contributions::SidePanelId>,
    /// Most-recently-focused places (latest first) between `Page` and open side panels for `<C-\>` (`WIN-06`).
    pub(super) pane_focus_recency: Vec<PanePlace>,
    /// Active blank placeholder pane waiting for the floating URL bar (`WIN-20`).
    pub(super) empty_split_pane: Option<EmptySplitPane>,
    /// Pending `(anchor_tab, side)` when `"Open URL…"` opened a new-tab URL bar from an existing split (`WIN-20`).
    pub(super) pending_split_url_bar: Option<(TabId, crate::split::DropZone)>,
    /// Monotonic counter for `1.2 s` pane-chip flashes (`WIN-11`).
    pub(super) chip_flash_seq: u64,
    /// Deduplicated ring of recently yanked texts.
    pub(super) yank_ring: crate::yank_webchat_actions::YankRing,
}

impl StubBackend {
    pub(super) fn handle_vim_command(&self, command: VimCommand) -> CommandResult {
        let name = command.name();
        match command {
            VimCommand::ModeNormal => {
                // <Esc> in a field leaves it, so the keys that follow are normal mode's and not typed into it.
                // When a staying CAPTCHA jump yielded to the answer field (`CAP-08`), `<Esc>` blurs the field and
                // returns the keys directly to the staying labels!
                let target = self.effective_keyed_tab();
                if let Some(tab_id) = target {
                    let mut release_steps = Vec::new();
                    {
                        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                        let page = state.vim.pages.entry(tab_id).or_default();
                        let yields_back_to_staying = (page.editing || page.mode == InputMode::Insert)
                            && page.page_hints.as_ref().is_some_and(|h| h.staying);
                        page.explicit_insert = false;
                        page.editing = false;
                        page.mode = InputMode::Normal;
                        if !yields_back_to_staying
                            && let Some(mut old_hints) = page.page_hints.take()
                            && let Some(mut pad) = old_hints.cursor_pad.take()
                        {
                            release_steps = pad.release_if_held();
                        }
                        page.visual_caret = None;
                        page.mode_transition_pending = false;
                    }
                    if !release_steps.is_empty()
                        && let Some(src) = self.current_tab_source()
                    {
                        let _ = src.dispatch_pointer_steps(tab_id, &release_steps);
                    }
                    self.sync_published_vim_state();
                    self.ask_page_for_tab(tab_id, AutomationRequest::BlurField)
                } else {
                    self.set_input_mode(InputMode::Normal);
                    Ok(())
                }
            }
            VimCommand::ModeInsert => {
                if let Some(tab_id) = self.effective_keyed_tab() {
                    {
                        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                        let page = state.vim.pages.entry(tab_id).or_default();
                        page.explicit_insert = true;
                        page.mode = InputMode::Insert;
                        page.page_hints = None;
                        page.visual_caret = None;
                        page.mode_transition_pending = false;
                    }
                    self.sync_published_vim_state();
                } else {
                    self.set_input_mode(InputMode::Insert);
                }
                Ok(())
            }
            VimCommand::ReportPending { keys, count } => {
                self.report_pending_keys(keys, count);
                Ok(())
            }
            VimCommand::EditableFocusChanged { tab_id, editing } => {
                let tab_id = self.resolve_target_tab(tab_id, name)?;
                self.apply_editable_focus(tab_id, editing);
                Ok(())
            }
            VimCommand::KeyedPageChanged { tab_id } => {
                let prev_effective = self.effective_keyed_tab();
                {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let valid =
                        tab_id.filter(|id| state.vim.auxiliary_pages.contains(id) || state.model.tab(*id).is_some());
                    state.vim.keyed_page = valid;
                    state.model.keyed_page = valid;
                }
                self.on_effective_page_changed(prev_effective);
                Ok(())
            }
            VimCommand::ReportExplorerViewport {
                visible_rows,
                first_visible,
                end_visible,
            } => {
                let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                state.model.focus.explorer_visible_rows = Some(visible_rows.max(1));
                state.model.focus.explorer_visible_range =
                    (end_visible > first_visible).then_some((first_visible, end_visible));
                Ok(())
            }
            VimCommand::PanelFocusChanged { panel_id } => {
                self.record_panel_focus(panel_id);
                Ok(())
            }
            VimCommand::AddressBarCancelled => self.handle_address_bar_cancelled(),
            VimCommand::RegisterAuxiliaryPage { tab_id, registered } => {
                let prev_effective = self.effective_keyed_tab();
                {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    if registered {
                        state.vim.auxiliary_pages.insert(tab_id);
                    } else {
                        state.vim.auxiliary_pages.remove(&tab_id);
                        state.vim.pages.remove(&tab_id);
                        state.model.tab_zoom.remove(&tab_id);
                        if state.vim.keyed_page == Some(tab_id) {
                            state.vim.keyed_page = None;
                            state.model.keyed_page = None;
                        }
                    }
                }
                self.on_effective_page_changed(prev_effective);
                Ok(())
            }
            VimCommand::SyncPageMode => {
                // Preserves per-page Visual and explicit `I` Insert modes while aligning Normal/Insert with `editing`.
                {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    if let Some(tab_id) = state.model.effective_keyed_tab() {
                        let page = state.vim.pages.entry(tab_id).or_default();
                        if page.mode == InputMode::Normal && page.editing {
                            page.mode = InputMode::Insert;
                        } else if page.mode == InputMode::Insert && !page.editing && !page.explicit_insert {
                            page.mode = InputMode::Normal;
                        }
                    }
                }
                self.sync_published_vim_state();
                Ok(())
            }
            VimCommand::LeaveField => {
                // A vim key moved to another tab: if that destination page kept a field focused (without explicit `I`),
                // blur the field and enter normal mode there (`gi` returns to it), while preserving Visual and `I` (`MD-01`).
                let blur_tab = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    if let Some(tab_id) = state.model.effective_keyed_tab() {
                        let page = state.vim.pages.entry(tab_id).or_default();
                        if page.editing && !page.explicit_insert {
                            page.editing = false;
                            page.mode = InputMode::Normal;
                            Some(tab_id)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                self.sync_published_vim_state();
                if let Some(tab_id) = blur_tab {
                    self.ask_page_for_tab(tab_id, AutomationRequest::BlurField)?;
                }
                Ok(())
            }
            VimCommand::ScrollPage { tab_id, motion, count } => {
                let tab_id = self.resolve_target_tab(tab_id, name)?;
                let count = if motion.is_counted() {
                    count.unwrap_or(1).clamp(1, 100_000)
                } else {
                    1
                };
                self.ask_page_for_tab(
                    tab_id,
                    AutomationRequest::Scroll {
                        motion: motion.automation(),
                        count,
                    },
                )
            }
            VimCommand::EditField { tab_id, edit } => {
                let tab_id = self.resolve_target_tab(tab_id, name)?;
                match self.current_tab_source() {
                    Some(source) => source.edit_field(tab_id, edit),
                    None => Ok(()),
                }
            }
        }
    }

    pub(super) fn effective_keyed_tab(&self) -> Option<TabId> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .model
            .effective_keyed_tab()
    }

    /// Applies an editable-field focus transition on `tab_id` (`INS-01`, `MD-01`).
    pub(super) fn apply_editable_focus(&self, tab_id: TabId, editing: bool) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let vim_enabled = state.model.vim_mode_enabled;
            let page = state.vim.pages.entry(tab_id).or_default();
            page.editing = editing;
            page.mode_transition_pending = false;
            if vim_enabled {
                match (editing, page.mode) {
                    (true, InputMode::Normal) => {
                        page.mode = InputMode::Insert;
                        if !page.page_hints.as_ref().is_some_and(|h| h.staying) {
                            page.page_hints = None;
                        }
                    }
                    // Clicking into a field ends visual mode.
                    (true, InputMode::Visual) => {
                        page.visual_caret = None;
                        page.page_hints = None;
                        page.mode = InputMode::Insert;
                    }
                    (false, InputMode::Insert) if !page.explicit_insert => {
                        page.mode = InputMode::Normal;
                    }
                    _ => {}
                }
            }
        }
        self.sync_published_vim_state();
    }

    /// Whether a field of `tab_id`'s page has the focus, as last reported.
    pub fn page_is_editing(&self, tab_id: TabId) -> bool {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.vim.pages.get(&tab_id).is_some_and(|p| p.editing)
    }

    /// Enters a window-level chrome mode (`Explorer`, `Toolbar`, `Library`, `Window`) while keeping every page's mode.
    /// Also closes any floating or sliding agent/webchat panels (`AGK-06`, `Zendia.sys.mjs:795-840`).
    pub(super) fn set_chrome_mode(&self, mode: InputMode) {
        let panels_to_close = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.vim.chrome_mode = Some(mode);
            state.vim.chrome_pending_keys.clear();
            state.vim.chrome_pending_count = None;
            if let Some(eff) = state.model.effective_keyed_tab()
                && let Some(page) = state.vim.pages.get_mut(&eff)
            {
                page.page_hints = None;
            }
            let candidates = [
                crate::agent::AGENT_PANEL_ID,
                crate::agent::AGENTS_PANEL_ID,
                crate::webchat::WEBCHAT_PANEL_ID,
            ];
            state
                .vim
                .open_panels
                .iter()
                .filter(|pid| {
                    candidates.contains(&pid.as_str())
                        && state.model.panel_prefs(pid).layout != crate::contributions::SidePanelLayout::Split
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        for panel_id in panels_to_close {
            self.request_view(crate::events::ViewRequest::PanelClose { panel_id });
        }
        self.clear_pending_and_which_key();
        self.sync_published_vim_state();
    }

    /// Enforces the fork's panel mutual-exclusion rules and clears pending keys (`AGK-06`, `Zendia.sys.mjs:795-840`).
    pub(super) fn enforce_panel_open_rules(&self, opening_panel: &crate::contributions::SidePanelId) {
        use crate::agent::{AGENT_PANEL_ID, AGENTS_PANEL_ID};
        use crate::contributions::{SidePanelId, SidePanelLayout};
        use crate::webchat::WEBCHAT_PANEL_ID;

        let name = opening_panel.as_str();
        if name != AGENT_PANEL_ID && name != AGENTS_PANEL_ID && name != WEBCHAT_PANEL_ID {
            return;
        }
        let to_close = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let opening_prefs = state.model.panel_prefs(opening_panel);
            let mut close_ids: Vec<SidePanelId> = Vec::new();
            let same_floating_side = |other_id: &SidePanelId| {
                let other_prefs = state.model.panel_prefs(other_id);
                opening_prefs.layout != SidePanelLayout::Split
                    && other_prefs.layout != SidePanelLayout::Split
                    && opening_prefs.side == other_prefs.side
            };
            if name == AGENT_PANEL_ID {
                let wc = SidePanelId::new(WEBCHAT_PANEL_ID);
                if state.vim.open_panels.contains(&wc) {
                    close_ids.push(wc);
                }
                let ags = SidePanelId::new(AGENTS_PANEL_ID);
                if state.vim.open_panels.contains(&ags) && same_floating_side(&ags) {
                    close_ids.push(ags);
                }
            } else if name == WEBCHAT_PANEL_ID {
                let ag = SidePanelId::new(AGENT_PANEL_ID);
                if state.vim.open_panels.contains(&ag) {
                    close_ids.push(ag);
                }
                let ags = SidePanelId::new(AGENTS_PANEL_ID);
                if state.vim.open_panels.contains(&ags) && same_floating_side(&ags) {
                    close_ids.push(ags);
                }
            } else if name == AGENTS_PANEL_ID {
                for other in [AGENT_PANEL_ID, WEBCHAT_PANEL_ID] {
                    let oid = SidePanelId::new(other);
                    if state.vim.open_panels.contains(&oid) && same_floating_side(&oid) {
                        close_ids.push(oid);
                    }
                }
            }
            close_ids
        };
        for panel_id in to_close {
            self.request_view(crate::events::ViewRequest::PanelClose { panel_id });
        }
        self.clear_pending_and_which_key();
    }

    pub(super) fn clear_pending_and_which_key(&self) {
        self.clear_pending_in_vim_state();
        let (had_count, had_keys, had_wk) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            (
                state.model.pending_count.is_some(),
                !state.model.pending_keys.is_empty(),
                state.model.which_key.is_some(),
            )
        };
        if had_count {
            self.set_pending_count(None);
        }
        if had_keys {
            self.set_pending_keys(Vec::new());
        }
        if had_wk {
            self.set_which_key(None);
        }
    }

    /// Leaves any active chrome mode, restoring the effective page's mode (`MD-01`).
    pub(super) fn leave_chrome_mode(&self) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.vim.chrome_mode = None;
            state.vim.chrome_pending_keys.clear();
            state.vim.chrome_pending_count = None;
        }
        self.sync_published_vim_state();
    }

    pub(super) fn set_page_mode_for_tab(&self, tab_id: TabId, mode: InputMode) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let page = state.vim.pages.entry(tab_id).or_default();
            page.mode = mode;
            if mode != InputMode::Visual {
                page.visual_caret = None;
            }
            if mode != InputMode::Insert {
                page.explicit_insert = false;
            }
        }
        self.sync_published_vim_state();
    }

    pub(super) fn set_mode_transition_pending(&self, tab_id: TabId, pending: bool) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.vim.pages.entry(tab_id).or_default().mode_transition_pending = pending;
        }
        self.sync_published_vim_state();
    }

    pub(super) fn clear_pending_in_vim_state(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.vim.chrome_pending_keys.clear();
        state.vim.chrome_pending_count = None;
        if let Some(eff) = state.model.effective_keyed_tab()
            && let Some(page) = state.vim.pages.get_mut(&eff)
        {
            page.pending_keys.clear();
            page.pending_count = None;
        }
    }

    /// Resets document-scoped vim state (`Visual`, `I`, hints, caret, pending keys) on navigation or unload (`MD-02`, `MD-03`).
    pub fn on_tab_navigated(&self, tab_id: TabId, new_url: Option<&str>) {
        let changed = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let page = state.vim.pages.entry(tab_id).or_default();
            if let Some(url) = new_url {
                let same_doc =
                    !page.last_url.is_empty() && url_without_fragment(&page.last_url) == url_without_fragment(url);
                page.last_url = url.to_string();
                if same_doc {
                    return;
                }
            }
            page.mode = InputMode::Normal;
            page.explicit_insert = false;
            page.editing = false;
            page.pending_keys.clear();
            page.pending_count = None;
            page.page_hints = None;
            page.visual_caret = None;
            page.mode_transition_pending = false;
            true
        };
        if changed {
            self.sync_published_vim_state();
        }
    }

    /// Cancels jump labels on the page left behind and syncs the model to the newly effective page (`MD-01`, `MD-03`, `SL-03`).
    pub(super) fn on_effective_page_changed(&self, previous_effective: Option<TabId>) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let next_effective = state.model.effective_keyed_tab();
            if let Some(prev) = previous_effective
                && Some(prev) != next_effective
                && let Some(prev_page) = state.vim.pages.get_mut(&prev)
            {
                prev_page.page_hints = None;
                prev_page.mode_transition_pending = false;
            }
        }
        self.sync_published_vim_state();
    }

    pub(super) fn on_tab_closed_vim(&self, tab_id: TabId) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.vim.pages.remove(&tab_id);
            state.vim.auxiliary_pages.remove(&tab_id);
            state.vim.tab_recency.retain(|id| *id != tab_id);
            if state.vim.keyed_page == Some(tab_id) {
                state.vim.keyed_page = None;
                state.model.keyed_page = None;
            }
        }
        self.sync_published_vim_state();
    }

    /// Leaves all pages' vim modes when vim mode is turned off, so turning it back on starts cleanly in normal mode.
    pub(super) fn reset_vim_page_mode(&self) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.vim.chrome_mode = None;
            state.vim.chrome_pending_keys.clear();
            state.vim.chrome_pending_count = None;
            for page in state.vim.pages.values_mut() {
                page.explicit_insert = false;
                page.mode = InputMode::Normal;
                page.pending_keys.clear();
                page.pending_count = None;
                page.page_hints = None;
                page.visual_caret = None;
                page.mode_transition_pending = false;
            }
        }
        self.sync_published_vim_state();
        self.set_input_mode(InputMode::Normal);
        self.set_pending_count(None);
        self.set_pending_keys(Vec::new());
        self.set_which_key(None);
    }

    fn report_pending_keys(&self, keys: Vec<String>, count: Option<u32>) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.vim.chrome_mode.is_some() {
                state.vim.chrome_pending_keys = keys;
                state.vim.chrome_pending_count = count;
            } else if let Some(eff) = state.model.effective_keyed_tab() {
                let page = state.vim.pages.entry(eff).or_default();
                page.pending_keys = keys;
                page.pending_count = count;
            } else {
                drop(state);
                let mode = self.model().input_mode;
                let wk = which_key_for_prefix(mode, &keys).map(|view| view.to_which_key_state());
                self.set_pending_count(count);
                self.set_pending_keys(keys);
                self.set_which_key(wk);
                return;
            }
        }
        self.sync_published_vim_state();
    }

    /// Synchronizes `state.model`'s published vim fields with the active chrome mode or effective keyed page, emitting
    /// events only for fields that changed.
    pub(super) fn sync_published_vim_state(&self) {
        let events = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let mut events = Vec::new();
            let eff = state.model.effective_keyed_tab();

            // Sync zoom level to the effective tab even when vim is off (`SL-03`).
            if let Some(tab_id) = eff
                && let Some(&tab_zoom) = state.model.tab_zoom.get(&tab_id)
                && (state.model.zoom_level - tab_zoom).abs() >= 0.001
            {
                state.model.zoom_level = tab_zoom;
                events.push(Event::ZoomChanged { zoom_level: tab_zoom });
            }

            let page_snapshot = eff.and_then(|id| state.vim.pages.get(&id).cloned()).unwrap_or_default();
            let desired_keyed = state.vim.keyed_page;
            let desired_editing = page_snapshot.editing;
            let desired_explicit = page_snapshot.explicit_insert;
            let desired_transition = state.model.vim_mode_enabled && page_snapshot.mode_transition_pending;

            if state.model.keyed_page != desired_keyed
                || state.model.page_editing != desired_editing
                || state.model.explicit_insert != desired_explicit
                || state.model.mode_transition_pending != desired_transition
            {
                state.model.keyed_page = desired_keyed;
                state.model.page_editing = desired_editing;
                state.model.explicit_insert = desired_explicit;
                state.model.mode_transition_pending = desired_transition;
                events.push(Event::VimPageStateChanged {
                    keyed_page: desired_keyed,
                    page_editing: desired_editing,
                    explicit_insert: desired_explicit,
                    mode_transition_pending: desired_transition,
                });
            }

            if !state.model.vim_mode_enabled {
                if state.model.page_hints.is_some() {
                    state.model.page_hints = None;
                    events.push(Event::PageHintsChanged { hints: None });
                }
                if state.model.visual_caret.is_some() {
                    state.model.visual_caret = None;
                    events.push(Event::VisualCaretChanged { caret: None });
                }
                return self.publish_vim_events(events);
            }

            let (desired_mode, desired_keys, desired_count, desired_hints, desired_caret) =
                if let Some(chrome_mode) = state.vim.chrome_mode {
                    (
                        chrome_mode,
                        state.vim.chrome_pending_keys.clone(),
                        state.vim.chrome_pending_count,
                        None,
                        None,
                    )
                } else {
                    let caret = if page_snapshot.mode == InputMode::Visual {
                        page_snapshot.visual_caret
                    } else {
                        None
                    };
                    (
                        page_snapshot.mode,
                        page_snapshot.pending_keys,
                        page_snapshot.pending_count,
                        page_snapshot.page_hints,
                        caret,
                    )
                };

            let desired_wk = which_key_for_prefix(desired_mode, &desired_keys).map(|view| view.to_which_key_state());

            if state.model.input_mode != desired_mode {
                state.model.input_mode = desired_mode;
                events.push(Event::ModeChanged { mode: desired_mode });
            }
            if state.model.pending_count != desired_count {
                state.model.pending_count = desired_count;
                events.push(Event::PendingCountChanged { count: desired_count });
            }
            if state.model.pending_keys != desired_keys {
                state.model.pending_keys = desired_keys.clone();
                events.push(Event::PendingKeysChanged { keys: desired_keys });
            }
            if state.model.which_key != desired_wk {
                state.model.which_key = desired_wk.clone();
                events.push(Event::WhichKeyChanged { state: desired_wk });
            }
            if state.model.page_hints != desired_hints {
                state.model.page_hints = desired_hints.clone();
                events.push(Event::PageHintsChanged { hints: desired_hints });
            }
            if state.model.visual_caret != desired_caret {
                state.model.visual_caret = desired_caret;
                events.push(Event::VisualCaretChanged { caret: desired_caret });
            }

            events
        };
        self.publish_vim_events(events);
    }

    fn publish_vim_events(&self, events: Vec<Event>) {
        for event in events {
            self.bus.publish(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrowserCore;
    use crate::commands::Command;
    use zendia_automation::{AutomationResponse, JumpKind, PageTarget};

    fn vim_core() -> StubBackend {
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        core
    }

    fn focus(core: &StubBackend, editing: bool) {
        core.invoke(Command::Vim(VimCommand::EditableFocusChanged { tab_id: None, editing }))
            .unwrap();
    }

    #[test]
    fn field_focus_switches_between_normal_and_insert() {
        let core = vim_core();
        focus(&core, true);
        assert_eq!(core.model().input_mode, InputMode::Insert);
        assert!(core.model().page_editing);
        focus(&core, false);
        assert_eq!(core.model().input_mode, InputMode::Normal);
        assert!(!core.model().page_editing);
    }

    #[test]
    fn explicit_insert_outlasts_the_field() {
        let core = vim_core();
        core.invoke(Command::Vim(VimCommand::ModeInsert)).unwrap();
        focus(&core, true);
        focus(&core, false);
        assert_eq!(core.model().input_mode, InputMode::Insert, "I lasts until <Esc>");
        assert!(core.model().explicit_insert);
        core.invoke(Command::Vim(VimCommand::ModeNormal)).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
        assert!(!core.model().explicit_insert);
    }

    #[test]
    fn field_focus_does_nothing_with_vim_off() {
        let core = StubBackend::new();
        focus(&core, true);
        assert_eq!(core.model().input_mode, InputMode::Normal);
    }

    #[test]
    fn pending_keys_reach_the_model_with_which_key() {
        let core = vim_core();
        core.invoke(Command::Vim(VimCommand::ReportPending {
            keys: vec!["<Space>".to_string()],
            count: Some(3),
        }))
        .unwrap();
        let model = core.model();
        assert_eq!(model.pending_keys, vec!["<Space>"]);
        assert_eq!(model.pending_count, Some(3));
        assert_eq!(model.which_key.as_ref().map(|w| w.title.as_str()), Some("SPC"));

        core.invoke(Command::Vim(VimCommand::ReportPending {
            keys: Vec::new(),
            count: None,
        }))
        .unwrap();
        let model = core.model();
        assert!(model.pending_keys.is_empty());
        assert!(model.which_key.is_none());
    }

    #[test]
    fn turning_vim_off_leaves_insert_mode() {
        let core = vim_core();
        focus(&core, true);
        core.invoke(Command::VimSetEnabled { enabled: false }).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
    }

    #[test]
    fn per_page_modes_pending_keys_and_jump_cancellation_across_tabs_and_navigations() {
        let core = vim_core();
        let t1 = core.model().active_tab.unwrap();

        // Enter Visual mode and stage pending keys on t1.
        core.feed_automation_response(
            t1,
            AutomationResponse::VisualState {
                active: true,
                yanked: None,
                copied_by_page: false,
                caret: None,
                land: false,
                flash: Vec::new(),
            },
        );
        core.invoke(Command::Vim(VimCommand::ReportPending {
            keys: vec!["i".to_string()],
            count: Some(2),
        }))
        .unwrap();
        assert_eq!(core.model().input_mode, InputMode::Visual);

        // Open t2: t2 starts in Normal with empty pending keys (`MD-01`).
        core.invoke(Command::TabNew {
            url: Some("https://two.example".to_string()),
            after: Some(t1),
            activate: true,
        })
        .unwrap();
        let t2 = core.model().active_tab.unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
        assert!(core.model().pending_keys.is_empty());
        assert_eq!(core.model().pending_count, None);

        // Open jump hints on t2, then switch back to t1: t2's jump ends (`MD-03`) and t1's Visual + pending keys return (`MD-01`).
        core.invoke(Command::from_name_and_args("jump.hints", &[]).unwrap())
            .unwrap();
        core.feed_automation_response(
            t2,
            AutomationResponse::Targets {
                jump: JumpKind::Hints,
                pattern: String::new(),
                targets: vec![PageTarget {
                    id: "b1".to_string(),
                    x: 10.0,
                    y: 10.0,
                    width: 30.0,
                    height: 20.0,
                    kind: "control".to_string(),
                    nexts: Vec::new(),
                    distance: 0.0,
                    href: None,
                    challenge: false,
                    drag: false,
                    slider: false,
                    track: None,
                    picture: false,
                    purpose: None,
                }],
                captcha_tokens: 0,
                captcha_areas: Vec::new(),
                viewport: None,
            },
        );
        assert!(core.model().page_hints.is_some());

        core.invoke(Command::TabActivate { tab_id: t1 }).unwrap();
        core.invoke(Command::Vim(VimCommand::LeaveField)).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Visual);
        assert_eq!(core.model().pending_keys, vec!["i".to_string()]);
        assert_eq!(core.model().pending_count, Some(2));
        assert!(core.model().page_hints.is_none());

        // Switching back to t2 confirms t2's jump was cancelled when leaving it (`MD-03`).
        core.invoke(Command::TabActivate { tab_id: t2 }).unwrap();
        assert!(core.model().page_hints.is_none());

        // Navigating t1 resets its Visual mode to Normal (`MD-02`).
        core.invoke(Command::TabActivate { tab_id: t1 }).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Visual);
        core.invoke(Command::NavigationLoadUrl {
            tab_id: Some(t1),
            input: "https://three.example/page".to_string(),
        })
        .unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
        assert!(core.model().pending_keys.is_empty());
    }

    #[test]
    fn auxiliary_pages_and_glance_have_independent_modes() {
        let core = vim_core();
        let tab = core.model().active_tab.unwrap();
        let popup = TabId(9001);
        core.invoke(Command::Vim(VimCommand::RegisterAuxiliaryPage {
            tab_id: popup,
            registered: true,
        }))
        .unwrap();
        core.invoke(Command::Vim(VimCommand::KeyedPageChanged { tab_id: Some(popup) }))
            .unwrap();
        assert_eq!(core.model().effective_keyed_tab(), Some(popup));

        // Focusing a field inside the popup puts the popup in Insert mode without touching the underlying tab (`RT-02`, `EXT-02`, `SL-03`).
        core.feed_automation_response(popup, AutomationResponse::EditableFocus { editing: true });
        assert_eq!(core.model().input_mode, InputMode::Insert);
        assert!(core.page_is_editing(popup));
        assert!(!core.page_is_editing(tab));

        // Leaving the popup returns to the tab's Normal mode.
        core.invoke(Command::Vim(VimCommand::KeyedPageChanged { tab_id: None }))
            .unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
    }
}
