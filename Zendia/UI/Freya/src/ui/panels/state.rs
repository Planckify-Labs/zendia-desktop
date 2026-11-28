/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Per-window reactive state for Zendia's side panel dock host (`DockHostState`).
//!
//! Tracks which side panels are open (or animating closed) in this window, keyboard focus, per-panel hover/slide
//! auto-hide state, live resize drag preview, and view-local overlays for the agent and web chat panels.

use crate::ui::theme::motion;
use freya::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use zendia_browser_core::agent::{AGENT_PANEL_ID, AGENTS_PANEL_ID, InstructionAttachment};
use zendia_browser_core::contributions::SidePanelId;
use zendia_browser_core::events::ViewRequest;
use zendia_browser_core::webchat::WEBCHAT_PANEL_ID;

/// Microphone dictation state inside the agent composer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DictationState {
    #[default]
    Idle,
    Listening,
    Processing,
}

/// Window-local reactive state shared between [`WindowShell`], [`DockFrame`], [`AgentPanel`], and toolbar chrome.
#[derive(Clone, Copy, PartialEq)]
pub struct DockHostState {
    /// Side panels currently open in this window (in open order; last is most recently focused).
    pub open_panels: State<Vec<SidePanelId>>,
    /// Side panels currently playing their `180ms` close animation before unmounting (`DF-01`).
    pub closing_panels: State<BTreeMap<SidePanelId, u64>>,
    /// Sequence counter for cancelling stale close-unmount timers when a panel re-opens mid-close.
    pub closing_seq: State<u64>,
    /// Side panel that currently holds keyboard focus (shows the 2px inner accent ring).
    pub focused_panel: State<Option<SidePanelId>>,
    /// Whether the floating Model & Effort card popover is open inside the agent panel.
    pub model_card_open: State<bool>,
    /// Whether the Model card is showing the searchable model list sub-view instead of Effort + Context.
    pub model_card_list_view: State<bool>,
    /// Whether the full-panel Instructions overlay page is open inside the agent panel.
    pub instructions_open: State<bool>,
    /// Whether the Accounts dropdown menu is open inside the web chat panel.
    pub webchat_accounts_open: State<bool>,
    /// Whether the inline Add custom site bar is open inside the web chat panel.
    pub webchat_add_site_open: State<bool>,
    /// Live dictation button state in the composer.
    pub dictation: State<DictationState>,
    /// Live width override while the user is dragging a panel's edge resize grip.
    pub resize_preview: State<Option<(SidePanelId, f32)>>,
    /// Set of slide-mode panels whose pointer is currently inside their frame (`DF-13`).
    pub slide_pointer_inside: State<BTreeSet<SidePanelId>>,
    /// Per-panel sequence counters for cancelling stale slide-hide or slide-handle-open timers (`DF-13`).
    pub slide_timer_seq: State<BTreeMap<SidePanelId, u64>>,
    /// Set of panels currently being edge-resized so Slide auto-hide stays paused (`DF-08`, `DF-13`).
    pub slide_resizing: State<BTreeSet<SidePanelId>>,
    /// True while a text input or sub-overlay inside the agent panel requests slide auto-hide to stay paused.
    pub slide_keep_open: State<bool>,
    /// Measured width of the window's page stage (body excluding sidebar), used for Split fallback checks.
    pub stage_width: State<f32>,
    /// Measured global `(x, y, width, height)` of the model chip trigger for anchoring the model card.
    pub model_chip_anchor: State<Option<(f32, f32, f32, f32)>>,
    /// Optional pinned phase `[0, 1)` for `WorkingRing` in deterministic gallery snapshots (`None` = live 30fps).
    pub pinned_ring_phase: State<Option<f32>>,
    /// Optional pre-populated composer text override (used by gallery fixtures and live editing).
    pub composer_text: State<String>,
    /// Optional sample attachments currently staged in the composer.
    pub staged_attachments: State<Vec<InstructionAttachment>>,
}

impl DockHostState {
    /// Returns whether `panel_id` is currently open in this window (excludes panels animating closed).
    pub fn is_open(&self, panel_id: &SidePanelId) -> bool {
        self.open_panels.read().iter().any(|id| id == panel_id)
    }

    /// Returns whether the built-in agent panel (`zendia.agent`) is currently open.
    pub fn is_agent_open(&self) -> bool {
        self.open_panels.read().iter().any(|id| id.0 == AGENT_PANEL_ID)
    }

    /// Returns whether the built-in cross-window agents panel (`zendia.agents`) is currently open.
    pub fn is_agents_open(&self) -> bool {
        self.open_panels.read().iter().any(|id| id.0 == AGENTS_PANEL_ID)
    }

    /// Returns whether the built-in web chat panel (`zendia.webchat`) is currently open.
    pub fn is_webchat_open(&self) -> bool {
        self.open_panels.read().iter().any(|id| id.0 == WEBCHAT_PANEL_ID)
    }

    /// Returns every mounted panel in render order as `(panel_id, is_closing)`.
    pub fn visible_panels(&self) -> Vec<(SidePanelId, bool)> {
        let open = self.open_panels.read();
        let closing = self.closing_panels.read();
        let mut out: Vec<(SidePanelId, bool)> = open.iter().cloned().map(|id| (id, false)).collect();
        for id in closing.keys() {
            if !open.contains(id) {
                out.push((id.clone(), true));
            }
        }
        out
    }

    /// Returns whether `panel_id` currently holds keyboard focus.
    pub fn is_focused(&self, panel_id: &SidePanelId) -> bool {
        self.focused_panel.read().as_ref() == Some(panel_id)
    }

    /// Opens `panel_id` (moving it to the top of open panels) and optionally gives it focus (`XPN-05`).
    ///
    /// When `focus` is `false` (for example, opening a Slide panel via edge hover), any existing focus on another
    /// panel is preserved rather than cleared.
    pub fn open_panel_with_focus(&mut self, panel_id: SidePanelId, focus: bool) {
        if self.closing_panels.peek().contains_key(&panel_id) {
            let mut closing = self.closing_panels.peek().clone();
            closing.remove(&panel_id);
            self.closing_panels.set(closing);
        }
        let mut open = self.open_panels.peek().clone();
        open.retain(|id| id != &panel_id);
        open.push(panel_id.clone());
        self.open_panels.set(open);
        if focus {
            self.focused_panel.set_if_modified(Some(panel_id));
        }
    }

    /// Opens `panel_id` (moving it to the top of open panels) and gives it focus.
    pub fn open_panel(&mut self, panel_id: SidePanelId) {
        self.open_panel_with_focus(panel_id, true);
    }

    /// Closes `panel_id` (keeping it mounted for its `180ms` exit animation) and returns `true` if it held focus.
    pub fn close_panel(&mut self, panel_id: &SidePanelId) -> bool {
        let mut open = self.open_panels.peek().clone();
        let before = open.len();
        open.retain(|id| id != panel_id);
        let was_open = open.len() != before;
        if was_open {
            self.open_panels.set(open);
            // Keep mounted briefly for the 180ms ease-in exit animation (DF-01), except in static gallery snapshots.
            if self.pinned_ring_phase.peek().is_none() {
                let seq = self.closing_seq.peek().wrapping_add(1);
                self.closing_seq.set(seq);
                let mut closing = self.closing_panels.peek().clone();
                closing.insert(panel_id.clone(), seq);
                self.closing_panels.set(closing);
                let mut closing_state = self.closing_panels;
                let pid = panel_id.clone();
                spawn(async move {
                    timer(motion::DOCK_CLOSE).await;
                    if closing_state.peek().get(&pid) == Some(&seq) {
                        let mut next = closing_state.peek().clone();
                        next.remove(&pid);
                        closing_state.set(next);
                    }
                });
            }
        }

        let was_focused = self.focused_panel.peek().as_ref() == Some(panel_id);
        if was_focused {
            self.focused_panel.set(None);
        }

        self.set_slide_pointer_inside(panel_id, false);
        self.set_slide_resizing(panel_id, false);

        if panel_id.0 == AGENT_PANEL_ID {
            self.model_card_open.set_if_modified(false);
            self.model_card_list_view.set_if_modified(false);
            self.instructions_open.set_if_modified(false);
            self.slide_keep_open.set_if_modified(false);
        } else if panel_id.0 == WEBCHAT_PANEL_ID {
            self.webchat_accounts_open.set_if_modified(false);
            self.webchat_add_site_open.set_if_modified(false);
        }
        was_focused
    }

    /// Toggles `panel_id` open or closed, returning `true` if it is now open. Other open panels stay open beside it.
    pub fn toggle_panel(&mut self, panel_id: SidePanelId) -> bool {
        if self.open_panels.peek().contains(&panel_id) {
            self.close_panel(&panel_id);
            false
        } else {
            self.open_panel(panel_id);
            true
        }
    }

    /// Focuses `panel_id` (or clears focus when `None`).
    pub fn set_focus(&mut self, panel_id: Option<SidePanelId>) {
        self.focused_panel.set_if_modified(panel_id);
    }

    /// Records whether the pointer is inside slide-mode `panel_id`.
    pub fn set_slide_pointer_inside(&mut self, panel_id: &SidePanelId, inside: bool) {
        let already = self.slide_pointer_inside.peek().contains(panel_id);
        if already != inside {
            let mut next = self.slide_pointer_inside.peek().clone();
            if inside {
                next.insert(panel_id.clone());
            } else {
                next.remove(panel_id);
            }
            self.slide_pointer_inside.set(next);
        }
    }

    /// Returns whether the pointer is currently inside slide-mode `panel_id`.
    pub fn is_slide_pointer_inside(&self, panel_id: &SidePanelId) -> bool {
        self.slide_pointer_inside.peek().contains(panel_id)
    }

    /// Increments and returns the per-panel slide timer sequence number for `panel_id`.
    pub fn bump_slide_timer_seq(&mut self, panel_id: &SidePanelId) -> u64 {
        let mut map = self.slide_timer_seq.peek().clone();
        let next = map.get(panel_id).copied().unwrap_or(0).wrapping_add(1);
        map.insert(panel_id.clone(), next);
        self.slide_timer_seq.set(map);
        next
    }

    /// Returns the current per-panel slide timer sequence number for `panel_id`.
    pub fn slide_timer_seq_for(&self, panel_id: &SidePanelId) -> u64 {
        self.slide_timer_seq.peek().get(panel_id).copied().unwrap_or(0)
    }

    /// Records whether `panel_id` is actively being resized.
    pub fn set_slide_resizing(&mut self, panel_id: &SidePanelId, active: bool) {
        let already = self.slide_resizing.peek().contains(panel_id);
        if already != active {
            let mut next = self.slide_resizing.peek().clone();
            if active {
                next.insert(panel_id.clone());
            } else {
                next.remove(panel_id);
            }
            self.slide_resizing.set(next);
        }
    }

    /// Returns whether `panel_id` is currently being resized.
    pub fn is_slide_resizing(&self, panel_id: &SidePanelId) -> bool {
        self.slide_resizing.peek().contains(panel_id)
    }

    /// Returns whether any side panel is currently being dragged/resized.
    pub fn is_any_panel_resizing(&self) -> bool {
        !self.slide_resizing.peek().is_empty()
    }

    /// Returns `true` when any side panel surface is actively capturing text input (composer, model search,
    /// instructions editor, or web chat add-site field). Used by the window key handler so vim/global bindings do not
    /// intercept typing.
    pub fn captures_text_input(&self) -> bool {
        if *self.webchat_add_site_open.peek() {
            return true;
        }
        if self.open_panels.peek().is_empty() {
            return false;
        }
        self.focused_panel.peek().is_some()
            || *self.model_card_open.peek()
            || *self.instructions_open.peek()
            || *self.slide_keep_open.peek()
    }

    /// Returns the effective display width for `panel_id`, using any live resize-drag preview first.
    pub fn preview_width_for(&self, panel_id: &SidePanelId) -> Option<f32> {
        let preview = self.resize_preview.read();
        preview
            .as_ref()
            .and_then(|(id, width)| (id == panel_id).then_some(*width))
    }

    /// Applies a view request that opens or closes side panels or the agent panel's model card and instructions page.
    ///
    /// Returns `true` when a previously focused panel was closed so the caller can return keyboard focus to the
    /// active page surface (`XPN-01`).
    pub fn apply_request(&mut self, request: &ViewRequest) -> bool {
        match request {
            ViewRequest::PanelOpen { panel_id, focus } => {
                self.open_panel_with_focus(panel_id.clone(), *focus);
                false
            }
            ViewRequest::PanelClose { panel_id } => self.close_panel(panel_id),
            ViewRequest::PanelToggle { panel_id } => {
                if self.open_panels.peek().contains(panel_id) {
                    self.close_panel(panel_id)
                } else {
                    self.open_panel(panel_id.clone());
                    false
                }
            }
            ViewRequest::AgentModelCard => {
                let agent_id = SidePanelId::new(AGENT_PANEL_ID);
                if !self.is_open(&agent_id) {
                    self.open_panel(agent_id);
                }
                let next = !*self.model_card_open.peek();
                self.model_card_open.set(next);
                if next {
                    self.model_card_list_view.set_if_modified(false);
                }
                false
            }
            ViewRequest::AgentInstructions => {
                let agent_id = SidePanelId::new(AGENT_PANEL_ID);
                if !self.is_open(&agent_id) {
                    self.open_panel(agent_id);
                }
                self.instructions_open.set_if_modified(true);
                self.model_card_open.set_if_modified(false);
                false
            }
            ViewRequest::WebChatAccountsToggle => {
                let wc_id = SidePanelId::new(WEBCHAT_PANEL_ID);
                if !self.is_open(&wc_id) {
                    self.open_panel(wc_id);
                }
                let next = !*self.webchat_accounts_open.peek();
                self.webchat_accounts_open.set(next);
                if next {
                    self.webchat_add_site_open.set_if_modified(false);
                }
                false
            }
            _ => false,
        }
    }
}

/// Installs [`DockHostState`] into Freya context for the current window or gallery fixture.
pub fn use_provide_dock_state() -> DockHostState {
    let state = DockHostState {
        open_panels: use_state(Vec::new),
        closing_panels: use_state(BTreeMap::new),
        closing_seq: use_state(|| 0u64),
        focused_panel: use_state(|| None),
        model_card_open: use_state(|| false),
        model_card_list_view: use_state(|| false),
        instructions_open: use_state(|| false),
        webchat_accounts_open: use_state(|| false),
        webchat_add_site_open: use_state(|| false),
        dictation: use_state(|| DictationState::Idle),
        resize_preview: use_state(|| None),
        slide_pointer_inside: use_state(BTreeSet::new),
        slide_timer_seq: use_state(BTreeMap::new),
        slide_resizing: use_state(BTreeSet::new),
        slide_keep_open: use_state(|| false),
        stage_width: use_state(|| 1120.0f32),
        model_chip_anchor: use_state(|| None),
        pinned_ring_phase: use_state(|| None),
        composer_text: use_state(String::new),
        staged_attachments: use_state(Vec::new),
    };
    use_provide_context(move || state);
    state
}

/// Looks up [`DockHostState`] if a parent window or gallery fixture provided one.
pub fn try_use_dock_state() -> Option<DockHostState> {
    try_consume_context::<DockHostState>()
}
