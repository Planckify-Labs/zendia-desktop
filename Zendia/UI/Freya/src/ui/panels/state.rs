/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Per-window reactive state for Zendia's side panel dock host (`DockHostState`).
//!
//! Tracks which side panels are open in this window, keyboard focus, hover/slide auto-hide state, live resize drag
//! preview, and view-local overlays for the agent panel (model card, instructions page, dictation state).

use freya::prelude::*;
use zendia_browser_core::agent::{AGENT_PANEL_ID, AGENTS_PANEL_ID, InstructionAttachment};
use zendia_browser_core::contributions::SidePanelId;
use zendia_browser_core::events::Event;
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
    /// True while the pointer is hovering over an open slide-mode panel.
    pub slide_pointer_inside: State<bool>,
    /// Sequence counter for cancelling stale slide-hide or slide-handle-open timers.
    pub slide_timer_seq: State<u64>,
    /// True while a text input or sub-overlay inside the panel requests slide auto-hide to stay paused.
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
    /// Returns whether `panel_id` is currently open in this window.
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

    /// Returns whether `panel_id` currently holds keyboard focus.
    pub fn is_focused(&self, panel_id: &SidePanelId) -> bool {
        self.focused_panel.read().as_ref() == Some(panel_id)
    }

    /// Opens `panel_id` (moving it to the top of open panels) and gives it focus.
    pub fn open_panel(&mut self, panel_id: SidePanelId) {
        let mut open = self.open_panels.peek().clone();
        open.retain(|id| id != &panel_id);
        open.push(panel_id.clone());
        self.open_panels.set(open);
        self.focused_panel.set_if_modified(Some(panel_id));
    }

    /// Closes `panel_id` (and clears sub-overlays when closing the agent or web chat panel).
    pub fn close_panel(&mut self, panel_id: &SidePanelId) {
        let mut open = self.open_panels.peek().clone();
        let before = open.len();
        open.retain(|id| id != panel_id);
        if open.len() != before {
            self.open_panels.set(open);
        }
        if self.focused_panel.peek().as_ref() == Some(panel_id) {
            self.focused_panel.set(None);
        }
        if panel_id.0 == AGENT_PANEL_ID {
            self.model_card_open.set_if_modified(false);
            self.model_card_list_view.set_if_modified(false);
            self.instructions_open.set_if_modified(false);
            self.slide_keep_open.set_if_modified(false);
        } else if panel_id.0 == WEBCHAT_PANEL_ID {
            self.webchat_accounts_open.set_if_modified(false);
            self.webchat_add_site_open.set_if_modified(false);
        }
    }

    /// Toggles `panel_id` open or closed, returning `true` if it is now open.
    pub fn toggle_panel(&mut self, panel_id: SidePanelId) -> bool {
        if self.open_panels.peek().last() == Some(&panel_id) {
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

    /// Applies view-level `BrowserCore` events that drive side panel visibility and agent sub-views.
    pub fn apply_event(&mut self, event: &Event) {
        match event {
            Event::PanelOpenRequested { panel_id, focus } => {
                self.open_panel(panel_id.clone());
                if !*focus {
                    self.focused_panel.set_if_modified(None);
                }
            }
            Event::PanelCloseRequested { panel_id } => {
                self.close_panel(panel_id);
            }
            Event::PanelToggled { panel_id } => {
                self.toggle_panel(panel_id.clone());
            }
            Event::AgentModelCardRequested => {
                let agent_id = SidePanelId::new(AGENT_PANEL_ID);
                if !self.is_open(&agent_id) {
                    self.open_panel(agent_id);
                }
                let next = !*self.model_card_open.peek();
                self.model_card_open.set(next);
                if next {
                    self.model_card_list_view.set_if_modified(false);
                }
            }
            Event::AgentInstructionsRequested => {
                let agent_id = SidePanelId::new(AGENT_PANEL_ID);
                if !self.is_open(&agent_id) {
                    self.open_panel(agent_id);
                }
                self.instructions_open.set_if_modified(true);
                self.model_card_open.set_if_modified(false);
            }
            _ => {}
        }
    }
}

/// Installs [`DockHostState`] into Freya context for the current window or gallery fixture.
pub fn use_provide_dock_state() -> DockHostState {
    let state = DockHostState {
        open_panels: use_state(Vec::new),
        focused_panel: use_state(|| None),
        model_card_open: use_state(|| false),
        model_card_list_view: use_state(|| false),
        instructions_open: use_state(|| false),
        webchat_accounts_open: use_state(|| false),
        webchat_add_site_open: use_state(|| false),
        dictation: use_state(|| DictationState::Idle),
        resize_preview: use_state(|| None),
        slide_pointer_inside: use_state(|| false),
        slide_timer_seq: use_state(|| 0u64),
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
