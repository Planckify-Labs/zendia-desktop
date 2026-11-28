/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Cross-window Agents roster side panel (`zendia.agents`).
//!
//! Groups every agent conversation across tabs by urgency (`NEEDS YOU`, `WORKING`, `FINISHED`, `IDLE`), with
//! 12-state status derivation, rotating agent-colored working avatar rings, pulsing attention dots, hover/cursor
//! action reveal, disabled dismiss on working runs, inline permission approval (`Allow [y]` / `Deny [n]`), inline
//! stop, `3 px` segmented work-list progress bars, `1 px` hairline step peeks, `"New agent"` picker, auto-focus on
//! open/click, scroll-into-view, and standard + existing keyboard navigation.

use crate::ui::agent::{agent_logo_element, chevron_down_icon};
use crate::ui::core_view::use_core;
use crate::ui::kit::{
    Icon, KitTooltip, ListNavAction, ListNavConfig, PopupPresence, classify_list_key, focus_ring_border, mix_color,
    scroll_item_into_view, use_interactive_feedback,
};
use crate::ui::panels::{DockLayoutControls, ResolvedDockPanel, try_use_dock_state};
use crate::ui::theme::{ResolvedScheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::collections::HashSet;
use zendia_browser_core::agent::{
    AGENT_PANEL_ID, AGENTS_PANEL_ID, RosterActivityItem, RosterActivityKind, RosterEntryState, RosterGroup, RosterKind,
    WorkListProgress, derive_roster_state, format_duration_secs, format_relative_age, recent_roster_activity,
    work_list_progress,
};
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::SidePanelId;
use zendia_browser_core::model::{
    AgentId, Conversation, ConversationId, ConversationStatus, ServiceId, ServiceStatus, StepId, StepKind, StepStatus,
    TabId,
};

/// Estimated height of one roster row for scroll-into-view calculations (`AG-08`).
const ROSTER_ROW_EST_HEIGHT: f32 = 76.0;
const ROSTER_GROUP_HEAD_HEIGHT: f32 = 22.0;

/// Cross-window Agents roster panel rendered inside [`crate::ui::panels::DockFrame`].
#[derive(Clone, PartialEq)]
pub struct AgentsPanel {
    pub resolved: ResolvedDockPanel,
    key: DiffKey,
}

impl KeyExt for AgentsPanel {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl AgentsPanel {
    pub fn new(resolved: ResolvedDockPanel) -> Self {
        Self {
            resolved,
            key: DiffKey::None,
        }
    }
}

#[derive(Clone, PartialEq)]
struct RosterRowData {
    state: RosterEntryState,
    conv: Conversation,
    agent_name: String,
    agent_color: Color,
    tab_title: String,
    tab_favicon: Option<String>,
    tab_closed: bool,
    extra_tabs: usize,
    pending_confirm: Option<(StepId, String)>,
    progress: Option<WorkListProgress>,
    activity: Vec<RosterActivityItem>,
}

#[derive(Clone, PartialEq)]
struct PickerAgentItem {
    id: AgentId,
    name: String,
    accent: Color,
    available: bool,
    is_default: bool,
    install_cmd: String,
}

fn agent_install_hint(agent_id: &str) -> &'static str {
    match agent_id {
        "claude-code" | "claude" => "npm i -g @anthropic-ai/claude-code",
        "codex" => "npm i -g @openai/codex",
        "cursor" => "curl https://cursor.com/install -fsS | bash",
        "opencode" => "npm i -g opencode-ai",
        "kiro" => "curl -fsSL https://cli.kiro.dev/install | bash",
        _ => "Install from agent documentation",
    }
}

impl Component for AgentsPanel {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let mut cursor = use_state(|| 0usize);
        let mut peeked = use_state(HashSet::<u64>::new);
        let mut picker_open = use_state(|| false);
        let mut scroll_controller = use_scroll_controller(ScrollConfig::default);
        let mut last_scrolled_cursor = use_state(|| 0usize);
        let panel_a11y_id = use_a11y();
        let focus = use_focus(panel_a11y_id);

        let static_snapshot = dock_state.is_some_and(|ds| ds.pinned_ring_phase.read().is_some());
        let is_picker_open = *picker_open.read();

        // AG-11: Auto-focus the roster list whenever the panel opens or regains focus (unless the new-agent picker owns focus).
        let focus_dep = (self.resolved.focused, is_picker_open);
        use_side_effect_with_deps(&focus_dep, move |(focused, picking): &(bool, bool)| {
            if *focused && !*picking {
                panel_a11y_id.request_focus();
            }
        });

        let (rows, active_tab_id, active_tab_busy, service_status, all_agents) = {
            let model = core.model().read();
            let active_tab_id = model.active_tab;
            let active_tab_busy = active_tab_id.is_some_and(|tid| {
                model
                    .conversations
                    .iter()
                    .any(|c| c.tab_id == Some(tid) && c.status == ConversationStatus::Working)
            });
            let service_status = model
                .services
                .iter()
                .find(|s| s.service == ServiceId::AgentServer)
                .map(|s| s.status.clone())
                .unwrap_or(ServiceStatus::Running);

            let default_agent_id = model.default_agent().map(|a| a.id.clone());
            let all_agents: Vec<PickerAgentItem> = model
                .agents
                .iter()
                .map(|a| {
                    let (r, g, b) = a.accent_rgb;
                    PickerAgentItem {
                        id: a.id.clone(),
                        name: a.name.clone(),
                        accent: Color::from_rgb(r, g, b),
                        available: a.available,
                        is_default: default_agent_id.as_ref() == Some(&a.id),
                        install_cmd: agent_install_hint(&a.id.0).to_string(),
                    }
                })
                .collect();

            let mut grouped: Vec<RosterRowData> = Vec::new();
            for group in RosterGroup::ALL {
                for conv in &model.conversations {
                    let captcha_holds = conv
                        .tab_id
                        .is_some_and(|tid| model.tab_captcha_findings.contains_key(&tid));
                    let state = derive_roster_state(conv, captcha_holds);
                    if state.group != group {
                        continue;
                    }
                    let agent_rec = model.agent(&conv.agent_id);
                    let agent_name = agent_rec
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| conv.agent_id.0.clone());
                    let agent_color = agent_rec
                        .map(|a| Color::from_rgb(a.accent_rgb.0, a.accent_rgb.1, a.accent_rgb.2))
                        .unwrap_or_else(|| theme.color(ThemeRole::Accent));
                    let tab = conv.tab_id.and_then(|tid| model.tab(tid));
                    let (tab_title, tab_favicon, tab_closed) = match tab {
                        Some(t) => (t.display_title().to_string(), t.favicon.clone(), false),
                        None => ("Tab closed".to_string(), None, true),
                    };
                    let pending_confirm = conv.steps.iter().rev().find_map(|step| match &step.kind {
                        StepKind::Confirmation { resolved: None, prompt } => Some((step.id, prompt.clone())),
                        _ => None,
                    });
                    let progress = work_list_progress(&conv.steps);
                    let activity = recent_roster_activity(&conv.steps, 6);

                    grouped.push(RosterRowData {
                        state,
                        conv: conv.clone(),
                        agent_name,
                        agent_color,
                        tab_title,
                        tab_favicon,
                        tab_closed,
                        extra_tabs: 0,
                        pending_confirm,
                        progress,
                        activity,
                    });
                }
            }
            (grouped, active_tab_id, active_tab_busy, service_status, all_agents)
        };

        let row_count = rows.len();
        let selected_idx = if row_count == 0 {
            0
        } else {
            (*cursor.read()).min(row_count - 1)
        };

        // AG-08: Keep active cursor row scrolled into view when cursor moves.
        if row_count > 0 && *last_scrolled_cursor.peek() != selected_idx {
            last_scrolled_cursor.set(selected_idx);
            let bounds = *scroll_controller.bounds.peek();
            if let Some((content, viewport)) = bounds {
                let mut est_y = 10.0f32;
                let mut seen_group: Option<RosterGroup> = None;
                for (i, r) in rows.iter().enumerate() {
                    if seen_group != Some(r.state.group) {
                        if seen_group.is_some() {
                            est_y += 12.0;
                        }
                        est_y += ROSTER_GROUP_HEAD_HEIGHT;
                        seen_group = Some(r.state.group);
                    }
                    if i == selected_idx {
                        break;
                    }
                    est_y += ROSTER_ROW_EST_HEIGHT
                        + if r.pending_confirm.is_some() { 82.0 } else { 0.0 }
                        + if peeked.peek().contains(&r.conv.id.0) {
                            64.0
                        } else {
                            0.0
                        };
                }
                scroll_item_into_view(
                    &mut scroll_controller,
                    est_y,
                    ROSTER_ROW_EST_HEIGHT,
                    viewport.height,
                    content.height,
                );
            }
        }

        let theme_accent = theme.color(ThemeRole::Accent);
        let is_service_down = !matches!(service_status, ServiceStatus::Running);
        let service_reason = match &service_status {
            ServiceStatus::Stopped { reason } => reason
                .clone()
                .unwrap_or_else(|| "AgentServer is not running.".to_string()),
            ServiceStatus::Starting => "Starting AgentServer…".to_string(),
            ServiceStatus::Restarting => "Restarting AgentServer…".to_string(),
            ServiceStatus::Running => String::new(),
        };

        let core_close = core.clone();
        let header = rect()
            .width(Size::fill())
            .height(Size::px(44.0))
            .padding(Gaps::new(0.0, 8.0, 0.0, 12.0))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .main_align(Alignment::SpaceBetween)
            .spacing(6.0)
            .child(
                label()
                    .text("Agents")
                    .font_size(13.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(theme.color(ThemeRole::Text)),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(4.0)
                    .child(AgentsHeaderButton {
                        icon: "plus",
                        icon_size: 14.0,
                        tooltip: "New agent (a)",
                        active: is_picker_open,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            let next = !*picker_open.peek();
                            picker_open.set(next);
                        }),
                        key: DiffKey::from(&"agents-hdr-new"),
                    })
                    .child(DockLayoutControls {
                        panel_id: self.resolved.panel_id.clone(),
                        prefs: self.resolved.prefs,
                        split_fell_back: self.resolved.split_fell_back,
                        accent: theme_accent,
                    })
                    .child(AgentsHeaderButton {
                        icon: "close",
                        icon_size: 13.5,
                        tooltip: "Close (Esc)",
                        active: false,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_close.invoke_or_log(Command::PanelClose {
                                panel_id: SidePanelId::new(AGENTS_PANEL_ID),
                            });
                        }),
                        key: DiffKey::from(&"agents-hdr-close"),
                    }),
            );

        let service_banner = is_service_down.then(|| {
            let core_retry = core.clone();
            rect()
                .width(Size::fill())
                .margin(Gaps::new(8.0, 10.0, 0.0, 10.0))
                .padding(Gaps::new(8.0, 10.0, 8.0, 10.0))
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
                        .spacing(2.0)
                        .child(
                            label()
                                .text("AgentServer unavailable")
                                .font_size(11.5)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(theme.color(ThemeRole::Text)),
                        )
                        .child(
                            label()
                                .text(service_reason)
                                .font_size(10.5)
                                .color(theme.color(ThemeRole::TextMuted))
                                .max_lines(2),
                        ),
                )
                .child(RosterPillButton {
                    label: "Retry".to_string(),
                    keycap: None,
                    primary: false,
                    on_press: EventHandler::new(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        core_retry.invoke_or_log(Command::DebugToggleAgentService);
                    }),
                    key: DiffKey::from(&"agents-service-retry"),
                })
        });

        let list_focused = focus() != Focus::Not;
        let keyboard_focused = focus() == Focus::Keyboard;

        let body: Element = if rows.is_empty() {
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .padding(Gaps::new(24.0, 28.0, 24.0, 28.0))
                .center()
                .spacing(6.0)
                .child(
                    label()
                        .width(Size::fill())
                        .text("No agents at work")
                        .font_size(15.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .text_align(TextAlign::Center)
                        .color(theme.color(ThemeRole::Accent)),
                )
                .child(
                    label()
                        .width(Size::fill())
                        .text(
                            "Start one with + above, or on any tab with <leader>aa. Every agent you start shows up here, in any window.",
                        )
                        .font_size(13.0)
                        .line_height(1.45)
                        .text_align(TextAlign::Center)
                        .color(theme.color(ThemeRole::TextMuted)),
                )
                .into_element()
        } else {
            let mut list_col = rect()
                .width(Size::fill())
                .padding(Gaps::new(10.0, 10.0, 12.0, 10.0))
                .spacing(12.0);

            let mut flat_idx = 0usize;
            for group in RosterGroup::ALL {
                let group_indices: Vec<usize> = rows
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, r)| (r.state.group == group).then_some(idx))
                    .collect();
                if group_indices.is_empty() {
                    continue;
                }
                let heading_color = match group {
                    RosterGroup::NeedsYou => theme.color(ThemeRole::Warning),
                    _ => theme.color(ThemeRole::TextMuted),
                };
                let mut section = rect().width(Size::fill()).spacing(4.0).child(
                    rect().padding(Gaps::new(0.0, 4.0, 2.0, 4.0)).child(
                        label()
                            .text(format!("{} · {}", group.heading(), group_indices.len()))
                            .font_size(10.0)
                            .font_weight(FontWeight::BOLD)
                            .color(heading_color),
                    ),
                );

                for idx in group_indices {
                    let row = rows[idx].clone();
                    let conv_id = row.conv.id;
                    let is_selected = flat_idx == selected_idx;
                    let is_peeked = peeked.read().contains(&conv_id.0);
                    section = section.child(RosterRow {
                        row,
                        index: flat_idx,
                        is_selected,
                        keyboard_focused,
                        is_peeked,
                        static_snapshot,
                        panel_a11y_id,
                        cursor,
                        peeked,
                        key: DiffKey::from(&format!("roster-row-{}", conv_id.0)),
                    });
                    flat_idx += 1;
                }
                list_col = list_col.child(section);
            }

            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .child(
                    ScrollView::new_controlled(scroll_controller)
                        .width(Size::fill())
                        .height(Size::fill())
                        .child(list_col),
                )
                .into_element()
        };

        // --- Keyboard navigation (standard Arrow/Home/End/Enter/Space/Escape + existing j/k/x/d/a/y/n) ---
        let key_rows: Vec<(ConversationId, Option<TabId>, ConversationStatus, Option<StepId>)> = rows
            .iter()
            .map(|r| {
                (
                    r.conv.id,
                    r.conv.tab_id,
                    r.conv.status,
                    r.pending_confirm.as_ref().map(|(sid, _)| *sid),
                )
            })
            .collect();
        let core_keys = core.clone();
        let on_key_down = move |e: Event<KeyboardEventData>| {
            if *picker_open.peek() {
                if e.key == Key::Named(NamedKey::Escape) {
                    picker_open.set(false);
                    panel_a11y_id.request_focus();
                    e.stop_propagation();
                }
                return;
            }
            if e.modifiers.contains(Modifiers::CONTROL)
                || e.modifiers.contains(Modifiers::ALT)
                || e.modifiers.contains(Modifiers::META)
            {
                return;
            }
            let count = key_rows.len();
            match &e.key {
                Key::Character(ch) if ch == "j" => {
                    if count > 0 {
                        let next = (*cursor.peek() + 1).min(count - 1);
                        cursor.set(next);
                        e.stop_propagation();
                    }
                }
                Key::Named(NamedKey::ArrowDown) => {
                    if count > 0 {
                        let next = (*cursor.peek() + 1).min(count - 1);
                        cursor.set(next);
                        e.stop_propagation();
                    }
                }
                Key::Character(ch) if ch == "k" => {
                    let next = cursor.peek().saturating_sub(1);
                    cursor.set(next);
                    e.stop_propagation();
                }
                Key::Named(NamedKey::ArrowUp) => {
                    let next = cursor.peek().saturating_sub(1);
                    cursor.set(next);
                    e.stop_propagation();
                }
                Key::Named(NamedKey::Home) => {
                    if count > 0 {
                        cursor.set(0);
                    }
                    e.stop_propagation();
                }
                Key::Character(ch) if ch == "g" => {
                    if count > 0 {
                        cursor.set(0);
                    }
                    e.stop_propagation();
                }
                Key::Named(NamedKey::End) => {
                    if count > 0 {
                        cursor.set(count - 1);
                    }
                    e.stop_propagation();
                }
                Key::Character(ch) if ch == "G" => {
                    if count > 0 {
                        cursor.set(count - 1);
                    }
                    e.stop_propagation();
                }
                Key::Named(NamedKey::Enter) => {
                    if let Some((conv_id, tab_id, _, _)) = key_rows.get((*cursor.peek()).min(count.saturating_sub(1))) {
                        if let Some(tid) = tab_id {
                            core_keys.invoke_or_log(Command::TabActivate { tab_id: *tid });
                        }
                        core_keys.invoke_or_log(Command::AgentSwitchConversation {
                            conversation_id: *conv_id,
                        });
                        core_keys.invoke_or_log(Command::PanelOpen {
                            panel_id: SidePanelId::new(AGENT_PANEL_ID),
                            focus: true,
                        });
                    }
                    e.stop_propagation();
                }
                Key::Character(ch) if ch == "o" => {
                    if let Some((conv_id, tab_id, _, _)) = key_rows.get((*cursor.peek()).min(count.saturating_sub(1))) {
                        if let Some(tid) = tab_id {
                            core_keys.invoke_or_log(Command::TabActivate { tab_id: *tid });
                        }
                        core_keys.invoke_or_log(Command::AgentSwitchConversation {
                            conversation_id: *conv_id,
                        });
                        core_keys.invoke_or_log(Command::PanelOpen {
                            panel_id: SidePanelId::new(AGENT_PANEL_ID),
                            focus: true,
                        });
                    }
                    e.stop_propagation();
                }
                Key::Character(ch) if ch == " " => {
                    if let Some((conv_id, _, _, _)) = key_rows.get((*cursor.peek()).min(count.saturating_sub(1))) {
                        let mut set = peeked.peek().clone();
                        if !set.remove(&conv_id.0) {
                            set.insert(conv_id.0);
                        }
                        peeked.set(set);
                    }
                    e.stop_propagation();
                }
                Key::Named(NamedKey::ArrowRight) => {
                    if let Some((conv_id, _, _, _)) = key_rows.get((*cursor.peek()).min(count.saturating_sub(1))) {
                        let mut set = peeked.peek().clone();
                        if set.insert(conv_id.0) {
                            peeked.set(set);
                        }
                    }
                    e.stop_propagation();
                }
                Key::Character(ch) if ch == "l" => {
                    if let Some((conv_id, _, _, _)) = key_rows.get((*cursor.peek()).min(count.saturating_sub(1))) {
                        let mut set = peeked.peek().clone();
                        if set.insert(conv_id.0) {
                            peeked.set(set);
                        }
                    }
                    e.stop_propagation();
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    if let Some((conv_id, _, _, _)) = key_rows.get((*cursor.peek()).min(count.saturating_sub(1))) {
                        let mut set = peeked.peek().clone();
                        if set.remove(&conv_id.0) {
                            peeked.set(set);
                        }
                    }
                    e.stop_propagation();
                }
                Key::Character(ch) if ch == "h" => {
                    if let Some((conv_id, _, _, _)) = key_rows.get((*cursor.peek()).min(count.saturating_sub(1))) {
                        let mut set = peeked.peek().clone();
                        if set.remove(&conv_id.0) {
                            peeked.set(set);
                        }
                    }
                    e.stop_propagation();
                }
                Key::Character(ch) if ch == "x" => {
                    if let Some((conv_id, _, ConversationStatus::Working, _)) =
                        key_rows.get((*cursor.peek()).min(count.saturating_sub(1)))
                    {
                        core_keys.invoke_or_log(Command::AgentStop {
                            conversation_id: Some(*conv_id),
                        });
                        e.stop_propagation();
                    }
                }
                // AG-05: Refuse to dismiss a working agent until stopped first.
                Key::Character(ch) if ch == "d" => {
                    if let Some((conv_id, _, status, _)) = key_rows.get((*cursor.peek()).min(count.saturating_sub(1))) {
                        if *status != ConversationStatus::Working {
                            core_keys.invoke_or_log(Command::AgentDismiss {
                                conversation_id: *conv_id,
                            });
                        }
                        e.stop_propagation();
                    }
                }
                Key::Character(ch) if ch == "a" => {
                    picker_open.set(true);
                    e.stop_propagation();
                }
                Key::Character(ch) if ch == "y" || ch == "n" => {
                    if let Some((conv_id, _, _, Some(step_id))) =
                        key_rows.get((*cursor.peek()).min(count.saturating_sub(1)))
                    {
                        core_keys.invoke_or_log(Command::AgentConfirm {
                            conversation_id: *conv_id,
                            step_id: *step_id,
                            allow: ch == "y",
                        });
                        e.stop_propagation();
                    }
                }
                Key::Named(NamedKey::Escape) => {
                    core_keys.invoke_or_log(Command::PanelClose {
                        panel_id: SidePanelId::new(AGENTS_PANEL_ID),
                    });
                    e.stop_propagation();
                }
                Key::Character(ch) if ch == "q" => {
                    core_keys.invoke_or_log(Command::PanelClose {
                        panel_id: SidePanelId::new(AGENTS_PANEL_ID),
                    });
                    e.stop_propagation();
                }
                _ => {
                    e.stop_propagation();
                }
            }
        };

        // --- Footer key legend (dimmed to 0.6 when unfocused, `AG-08`) ---
        let footer = rect()
            .width(Size::fill())
            .padding(Gaps::new(8.0, 14.0, 10.0, 14.0))
            .opacity(if list_focused { 1.0 } else { 0.6 })
            .spacing(4.0)
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(10.0)
                    .child(legend_pair(&["j k"], "move", &theme))
                    .child(legend_pair(&["⏎"], "open", &theme))
                    .child(legend_pair(&["␣"], "peek", &theme))
                    .child(legend_pair(&["x"], "stop", &theme))
                    .child(legend_pair(&["d"], "dismiss", &theme)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(10.0)
                    .child(legend_pair(&["a"], "new", &theme)),
            );

        let core_pick = core.clone();
        let new_agent_popover = NewAgentPopover {
            open: is_picker_open,
            agents: all_agents,
            on_select: EventHandler::new(move |agent_id: AgentId| {
                picker_open.set(false);
                // AG-02: Only open a fresh tab if another agent is actively working the current tab.
                if active_tab_busy {
                    core_pick.invoke_or_log(Command::TabNew {
                        url: None,
                        after: active_tab_id,
                        activate: true,
                    });
                }
                core_pick.invoke_or_log(Command::AgentSelect {
                    agent_id: Some(agent_id),
                });
                core_pick.invoke_or_log(Command::AgentNew {
                    tab_id: if active_tab_busy { None } else { active_tab_id },
                });
                core_pick.invoke_or_log(Command::PanelOpen {
                    panel_id: SidePanelId::new(AGENT_PANEL_ID),
                    focus: true,
                });
            }),
            on_close: EventHandler::new(move |()| {
                picker_open.set(false);
                panel_a11y_id.request_focus();
                // Keep AgentsPanel open if DockFrame also processed Escape.
                core.invoke_or_log(Command::PanelOpen {
                    panel_id: SidePanelId::new(AGENTS_PANEL_ID),
                    focus: true,
                });
            }),
            key: DiffKey::from(&"agents-new-popover"),
        };

        rect()
            .a11y_id(panel_a11y_id)
            .a11y_focusable(true)
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .on_pointer_down(move |_| {
                // AG-11: Clicking anywhere inside the roster focuses it so keyboard shortcuts always work.
                if !*picker_open.peek() {
                    panel_a11y_id.request_focus();
                }
            })
            .on_key_down(on_key_down)
            .child(header)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(0.5))
                    .background(theme.rgba(ThemeRole::Border).with_alpha(0.70).to_color()),
            )
            .maybe_child(service_banner)
            .child(body)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(0.5))
                    .background(theme.rgba(ThemeRole::Border).with_alpha(0.70).to_color()),
            )
            .child(footer)
            .child(new_agent_popover)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Interactive `24×24` header icon button with `120 ms` hover wash, focus ring, and tooltip (`AG-02`).
#[derive(Clone, PartialEq)]
struct AgentsHeaderButton {
    icon: &'static str,
    icon_size: f32,
    tooltip: &'static str,
    active: bool,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for AgentsHeaderButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for AgentsHeaderButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let rest = if self.active {
            theme.rgba(ThemeRole::Accent).with_alpha(0.18).to_color()
        } else {
            Color::TRANSPARENT
        };
        let bg = fb.background(
            rest,
            theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.16).to_color(),
        );
        let icon_role = if self.active {
            ThemeRole::Accent
        } else if fb.is_hovered() {
            ThemeRole::Text
        } else {
            ThemeRole::TextMuted
        };
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let on_press = self.on_press.clone();

        KitTooltip::new(self.tooltip).child(
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(self.tooltip)
                .width(Size::px(24.0))
                .height(Size::px(24.0))
                .corner_radius(CornerRadius::new_all(6.0))
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
                .child(Icon::new(self.icon).size(self.icon_size).role(icon_role)),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// `"New agent"` picker popover anchored beneath the Agents header `+` button (`AG-02`).
#[derive(Clone, PartialEq)]
struct NewAgentPopover {
    open: bool,
    agents: Vec<PickerAgentItem>,
    on_select: EventHandler<AgentId>,
    on_close: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for NewAgentPopover {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for NewAgentPopover {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let a11y_id = use_a11y();
        let mut highlighted = use_state(|| 0usize);

        let open = self.open;
        use_side_effect_with_deps(&open, move |is_open: &bool| {
            if *is_open {
                highlighted.set(0);
                a11y_id.request_focus();
            }
        });

        let agents_keys = self.agents.clone();
        let on_select_keys = self.on_select.clone();
        let on_close_keys = self.on_close.clone();

        let count = self.agents.len();
        let cur_idx = (*highlighted.read()).min(count.saturating_sub(1));

        let mut list = rect().width(Size::fill()).spacing(2.0);
        for (idx, agent) in self.agents.iter().enumerate() {
            let aid = agent.id.clone();
            let on_pick = self.on_select.clone();
            list = list.child(NewAgentPopoverRow {
                agent: agent.clone(),
                selected: idx == cur_idx,
                on_hover: EventHandler::new(move |()| highlighted.set_if_modified(idx)),
                on_press: EventHandler::new(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    on_pick.call(aid.clone());
                }),
                key: DiffKey::from(&format!("new-agent-opt-{}", agent.id.0)),
            });
        }

        let card_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(248, 28, 32, 48),
            ResolvedScheme::Light => Color::from_argb(252, 250, 251, 255),
        };
        let content = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .width(Size::px(228.0))
            .padding(Gaps::new_all(6.0))
            .spacing(4.0)
            .corner_radius(CornerRadius::new_all(12.0))
            .background(card_bg)
            .border(
                Border::new()
                    .width(0.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.glass.outline.to_color()),
            )
            .shadow(theme.shadows.card.to_shadow())
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if let Some(action) = classify_list_key(&e, ListNavConfig::default()) {
                    match action {
                        ListNavAction::MoveNext => {
                            if !agents_keys.is_empty() {
                                let next = (*highlighted.peek() + 1) % agents_keys.len();
                                highlighted.set(next);
                            }
                            e.stop_propagation();
                        }
                        ListNavAction::MovePrev => {
                            if !agents_keys.is_empty() {
                                let cur = *highlighted.peek();
                                let next = if cur == 0 { agents_keys.len() - 1 } else { cur - 1 };
                                highlighted.set(next);
                            }
                            e.stop_propagation();
                        }
                        ListNavAction::First => {
                            highlighted.set(0);
                            e.stop_propagation();
                        }
                        ListNavAction::Last => {
                            if !agents_keys.is_empty() {
                                highlighted.set(agents_keys.len() - 1);
                            }
                            e.stop_propagation();
                        }
                        ListNavAction::Activate => {
                            if let Some(ag) = agents_keys.get(*highlighted.peek())
                                && ag.available
                            {
                                on_select_keys.call(ag.id.clone());
                            }
                            e.stop_propagation();
                        }
                        ListNavAction::Dismiss => {
                            on_close_keys.call(());
                            e.stop_propagation();
                        }
                        _ => {}
                    }
                }
            })
            .child(
                rect().padding(Gaps::new(2.0, 6.0, 2.0, 6.0)).child(
                    label()
                        .text("New agent")
                        .font_size(10.5)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(theme.color(ThemeRole::TextMuted)),
                ),
            )
            .child(list);

        rect()
            .layer(Layer::Relative(70))
            .position(Position::new_absolute().top(42.0).right(10.0))
            .child(PopupPresence::inline(self.open).child(content))
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct NewAgentPopoverRow {
    agent: PickerAgentItem,
    selected: bool,
    on_hover: EventHandler<()>,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for NewAgentPopoverRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for NewAgentPopoverRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(self.agent.available);
        let bg = if self.selected && self.agent.available {
            theme.rgba(ThemeRole::Accent).with_alpha(0.16).to_color()
        } else {
            fb.background(
                Color::TRANSPARENT,
                theme.rgba(ThemeRole::Text).with_alpha(0.08).to_color(),
                theme.rgba(ThemeRole::Text).with_alpha(0.14).to_color(),
            )
        };
        let on_hover = self.on_hover.clone();
        let on_press = self.on_press.clone();
        let available = self.agent.available;
        let sub = if !self.agent.available {
            Some("Not installed".to_string())
        } else if self.agent.is_default {
            Some("default".to_string())
        } else {
            None
        };
        let row = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.agent.name.clone())
            .width(Size::fill())
            .height(Size::px(30.0))
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .corner_radius(CornerRadius::new_all(7.0))
            .background(bg)
            .opacity(if available { 1.0 } else { 0.45 })
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .cursor(if available {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(move |e: Event<PointerEventData>| {
                fb.on_pointer_enter()(e);
                on_hover.call(());
            })
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                if available {
                    on_press.call(e);
                }
            })
            .child(agent_logo_element(&self.agent.id.0, 16.0, !available))
            .child(
                rect().width(Size::flex(1.0)).child(
                    label()
                        .text(self.agent.name.clone())
                        .font_size(12.0)
                        .font_weight(FontWeight::MEDIUM)
                        .color(theme.color(ThemeRole::Text))
                        .max_lines(1),
                ),
            )
            .maybe_child(sub.map(|s| label().text(s).font_size(10.0).color(theme.color(ThemeRole::TextMuted))));

        if available {
            row.into_element()
        } else {
            KitTooltip::new(self.agent.install_cmd.clone())
                .child(row)
                .into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// One row in the Agents roster with hover/cursor action reveal, animated avatar, work progress, inline confirm & peek.
#[derive(Clone, PartialEq)]
struct RosterRow {
    row: RosterRowData,
    index: usize,
    is_selected: bool,
    keyboard_focused: bool,
    is_peeked: bool,
    static_snapshot: bool,
    panel_a11y_id: AccessibilityId,
    cursor: State<usize>,
    peeked: State<HashSet<u64>>,
    key: DiffKey,
}

impl KeyExt for RosterRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for RosterRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let core = use_core();
        let fb = use_interactive_feedback(true);

        let conv_id = self.row.conv.id;
        let tab_id = self.row.conv.tab_id;
        let group = self.row.state.group;
        let kind = self.row.state.kind;
        let is_needs_you = group == RosterGroup::NeedsYou;
        let is_working = group == RosterGroup::Working;
        let is_running = self.row.conv.status == ConversationStatus::Working;
        let hovered = fb.is_hovered();
        let show_actions = hovered || self.is_selected;

        let status_color = match (group, kind) {
            (_, RosterKind::Error) => theme.color(ThemeRole::Danger),
            (RosterGroup::NeedsYou, _) => theme.color(ThemeRole::Warning),
            (RosterGroup::Working, _) => mix_color(self.row.agent_color, theme.color(ThemeRole::Text), 0.30),
            (RosterGroup::Finished, _) => theme.color(ThemeRole::Success),
            (RosterGroup::Idle, _) => theme.color(ThemeRole::TextMuted),
        };

        // AG-05: Hover wash (text 6%), cursor wash (text 9%), attention wash (warning 9% / cursor 15%).
        let card_bg = if is_needs_you {
            let alpha = if self.is_selected {
                0.15
            } else if hovered {
                0.12
            } else {
                0.09
            };
            theme.rgba(ThemeRole::Warning).with_alpha(alpha).to_color()
        } else if self.is_selected {
            theme.rgba(ThemeRole::Text).with_alpha(0.09).to_color()
        } else if hovered {
            theme.rgba(ThemeRole::Text).with_alpha(0.06).to_color()
        } else {
            Color::TRANSPARENT
        };

        let card_border = if self.is_selected && self.keyboard_focused {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.rgba(ThemeRole::Accent).with_alpha(0.70).to_color())
        } else {
            Border::new().width(0.0).fill(Color::TRANSPARENT)
        };

        let time_label = if is_working {
            format_duration_secs(self.row.conv.elapsed_secs)
        } else {
            format_relative_age(self.row.conv.elapsed_secs.max(7))
        };

        let core_open = core.clone();
        let core_stop = core.clone();
        let core_dismiss = core.clone();
        let mut cursor = self.cursor;
        let mut peeked = self.peeked;
        let index = self.index;
        let panel_a11y_id = self.panel_a11y_id;

        let avatar = RosterAvatar {
            agent_id: self.row.conv.agent_id.0.clone(),
            agent_color: self.row.agent_color,
            group,
            kind,
            static_snapshot: self.static_snapshot,
            key: DiffKey::from(&format!("avatar-{}", conv_id.0)),
        };

        let top_right: Element = if show_actions {
            rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(1.0)
                .child(RosterPeekButton {
                    expanded: self.is_peeked,
                    on_press: EventHandler::new(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        cursor.set(index);
                        let mut set = peeked.peek().clone();
                        if !set.remove(&conv_id.0) {
                            set.insert(conv_id.0);
                        }
                        peeked.set(set);
                        panel_a11y_id.request_focus();
                    }),
                    key: DiffKey::from(&format!("peek-btn-{}", conv_id.0)),
                })
                .child(RosterDismissButton {
                    disabled: is_running,
                    on_press: EventHandler::new(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        if !is_running {
                            core_dismiss.invoke_or_log(Command::AgentDismiss {
                                conversation_id: conv_id,
                            });
                        }
                        panel_a11y_id.request_focus();
                    }),
                    key: DiffKey::from(&format!("dismiss-btn-{}", conv_id.0)),
                })
                .into_element()
        } else {
            label()
                .text(time_label)
                .font_size(11.0)
                .color(theme.color(ThemeRole::TextMuted))
                .max_lines(1)
                .into_element()
        };

        let stop_btn = is_running.then(|| RosterStopButton {
            on_press: EventHandler::new(move |e: Event<PressEventData>| {
                e.stop_propagation();
                cursor.set(index);
                core_stop.invoke_or_log(Command::AgentStop {
                    conversation_id: Some(conv_id),
                });
                panel_a11y_id.request_focus();
            }),
            key: DiffKey::from(&format!("stop-btn-{}", conv_id.0)),
        });

        let tab_label = if self.row.extra_tabs > 0 && !self.row.tab_closed {
            format!("{} +{}", self.row.tab_title, self.row.extra_tabs)
        } else {
            self.row.tab_title.clone()
        };
        let favicon_name = if self.row.tab_closed {
            "globe".to_string()
        } else {
            self.row.tab_favicon.clone().unwrap_or_else(|| "globe".to_string())
        };

        let progress_bar = self.row.progress.map(|prog| WorkProgressBar {
            progress: prog,
            key: DiffKey::from(&format!("prog-{}", conv_id.0)),
        });

        // AG-09: Inline permission card styled per zendia.css:5010-5033 with hover/press feedback on Deny/Allow.
        let inline_confirm = self.row.pending_confirm.as_ref().map(|(step_id, prompt)| {
            let sid = *step_id;
            let core_deny = core.clone();
            let core_allow = core.clone();
            rect()
                .width(Size::fill())
                .margin(Gaps::new(8.0, 0.0, 0.0, 38.0))
                .padding(Gaps::new(8.0, 10.0, 8.0, 10.0))
                .corner_radius(CornerRadius::new_all(10.0))
                .background(theme.rgba(ThemeRole::AgentConfirm).with_alpha(0.12).to_color())
                .border(
                    Border::new()
                        .width(0.5)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.rgba(ThemeRole::AgentConfirm).with_alpha(0.45).to_color()),
                )
                .spacing(8.0)
                .child(
                    label()
                        .width(Size::fill())
                        .text(prompt.clone())
                        .font_size(12.0)
                        .line_height(1.4)
                        .color(theme.color(ThemeRole::Text))
                        .max_lines(4),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .main_align(Alignment::end())
                        .cross_align(Alignment::center())
                        .spacing(7.0)
                        .child(RosterPillButton {
                            label: "Deny".to_string(),
                            keycap: Some("n".to_string()),
                            primary: false,
                            on_press: EventHandler::new(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_deny.invoke_or_log(Command::AgentConfirm {
                                    conversation_id: conv_id,
                                    step_id: sid,
                                    allow: false,
                                });
                                panel_a11y_id.request_focus();
                            }),
                            key: DiffKey::from(&format!("deny-{}", conv_id.0)),
                        })
                        .child(RosterPillButton {
                            label: "Allow".to_string(),
                            keycap: Some("y".to_string()),
                            primary: true,
                            on_press: EventHandler::new(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_allow.invoke_or_log(Command::AgentConfirm {
                                    conversation_id: conv_id,
                                    step_id: sid,
                                    allow: true,
                                });
                                panel_a11y_id.request_focus();
                            }),
                            key: DiffKey::from(&format!("allow-{}", conv_id.0)),
                        }),
                )
        });

        // AG-07: Peek drawer with 1px agent-color 50% left hairline (margin-left: 38px, no box bg) & up to 6 entries.
        let peek_drawer = self.is_peeked.then(|| {
            let hairline = Color::from_argb(
                128,
                self.row.agent_color.r(),
                self.row.agent_color.g(),
                self.row.agent_color.b(),
            );
            let mut items_col = rect()
                .width(Size::flex(1.0))
                .padding(Gaps::new(0.0, 0.0, 0.0, 10.0))
                .spacing(3.0);
            if self.row.activity.is_empty() {
                items_col = items_col.child(
                    label()
                        .text(if is_running {
                            "Getting started…"
                        } else {
                            "Nothing yet."
                        })
                        .font_size(11.5)
                        .line_height(1.4)
                        .color(theme.color(ThemeRole::TextMuted)),
                );
            } else {
                for item in &self.row.activity {
                    let text_col = match item.kind {
                        RosterActivityKind::Agent => theme.color(ThemeRole::Text),
                        RosterActivityKind::Error => theme.color(ThemeRole::Danger),
                        RosterActivityKind::Step | RosterActivityKind::Note => theme.color(ThemeRole::TextMuted),
                    };
                    let step_glyph = (item.kind == RosterActivityKind::Step).then(|| {
                        let (ch, col) = match item.status {
                            StepStatus::Running => ("◌", self.row.agent_color),
                            StepStatus::Completed => ("✓", theme.color(ThemeRole::TextMuted)),
                            StepStatus::Failed => ("✕", theme.color(ThemeRole::Danger)),
                            StepStatus::WaitingForUser => ("!", theme.color(ThemeRole::Warning)),
                            StepStatus::Pending => ("·", theme.color(ThemeRole::TextMuted)),
                        };
                        label().text(ch).font_size(11.0).color(col)
                    });
                    items_col = items_col.child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .cross_align(Alignment::start())
                            .spacing(6.0)
                            .maybe_child(step_glyph)
                            .child(
                                rect().width(Size::flex(1.0)).child(
                                    label()
                                        .text(item.text.clone())
                                        .font_size(11.5)
                                        .line_height(1.4)
                                        .color(text_col)
                                        .max_lines(2)
                                        .text_overflow(TextOverflow::Ellipsis),
                                ),
                            ),
                    );
                }
            }
            rect()
                .width(Size::fill())
                .margin(Gaps::new(8.0, 0.0, 0.0, 38.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .child(rect().width(Size::px(1.0)).height(Size::fill()).background(hairline))
                .child(items_col)
        });

        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Open agent run: {}", self.row.conv.title))
            .width(Size::fill())
            .padding(Gaps::new_all(8.0))
            .corner_radius(CornerRadius::new_all(10.0))
            .background(card_bg)
            .border(card_border)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                cursor.set(index);
                if let Some(tid) = tab_id {
                    core_open.invoke_or_log(Command::TabActivate { tab_id: tid });
                }
                core_open.invoke_or_log(Command::AgentSwitchConversation {
                    conversation_id: conv_id,
                });
                core_open.invoke_or_log(Command::PanelOpen {
                    panel_id: SidePanelId::new(AGENT_PANEL_ID),
                    focus: true,
                });
            })
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::start())
                    .spacing(10.0)
                    .child(avatar)
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .spacing(2.0)
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .direction(Direction::Horizontal)
                                    .content(Content::Flex)
                                    .cross_align(Alignment::center())
                                    .main_align(Alignment::SpaceBetween)
                                    .spacing(8.0)
                                    .child(
                                        rect().width(Size::flex(1.0)).overflow(Overflow::Clip).child(
                                            label()
                                                .text(self.row.conv.title.clone())
                                                .font_size(13.0)
                                                .font_weight(FontWeight::SEMI_BOLD)
                                                .color(theme.color(ThemeRole::Text))
                                                .max_lines(1)
                                                .text_overflow(TextOverflow::Ellipsis),
                                        ),
                                    )
                                    .child(top_right),
                            )
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .direction(Direction::Horizontal)
                                    .content(Content::Flex)
                                    .cross_align(Alignment::center())
                                    .spacing(6.0)
                                    .child(
                                        label()
                                            .text(self.row.state.label.clone())
                                            .font_size(12.0)
                                            .font_weight(FontWeight::SEMI_BOLD)
                                            .color(status_color)
                                            .max_lines(1),
                                    )
                                    .maybe_child((!self.row.state.detail.is_empty()).then(|| {
                                        rect().width(Size::flex(1.0)).overflow(Overflow::Clip).child(
                                            label()
                                                .text(self.row.state.detail.clone())
                                                .font_size(12.0)
                                                .color(theme.color(ThemeRole::TextMuted))
                                                .max_lines(1)
                                                .text_overflow(TextOverflow::Ellipsis),
                                        )
                                    })),
                            )
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .direction(Direction::Horizontal)
                                    .content(Content::Flex)
                                    .cross_align(Alignment::center())
                                    .main_align(Alignment::SpaceBetween)
                                    .spacing(6.0)
                                    .child(
                                        rect()
                                            .width(Size::flex(1.0))
                                            .overflow(Overflow::Clip)
                                            .direction(Direction::Horizontal)
                                            .content(Content::Flex)
                                            .cross_align(Alignment::center())
                                            .spacing(5.0)
                                            .child(Icon::new(favicon_name).size(12.0).role(ThemeRole::TextMuted))
                                            .child(
                                                rect().width(Size::flex(1.0)).overflow(Overflow::Clip).child(
                                                    label()
                                                        .text(tab_label)
                                                        .font_size(11.0)
                                                        .color(theme.color(ThemeRole::TextMuted))
                                                        .max_lines(1)
                                                        .text_overflow(TextOverflow::Ellipsis),
                                                ),
                                            ),
                                    )
                                    .child(
                                        label()
                                            .text(self.row.agent_name.clone())
                                            .font_size(11.0)
                                            .color(theme.rgba(ThemeRole::TextMuted).with_alpha(0.80).to_color())
                                            .max_lines(1),
                                    ),
                            )
                            .maybe_child(progress_bar),
                    )
                    .maybe_child(stop_btn),
            )
            .maybe_child(inline_confirm)
            .maybe_child(peek_drawer)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// `28×28` avatar (`22 px` logo) with rotating `1.1 s` agent-colored working ring, pulsing `1.4 s` `9 px` attention
/// dot, solid red error dot, solid green done dot, or dimmed `0.7` idle logo with no dot (`AG-03`).
#[derive(Clone, PartialEq)]
struct RosterAvatar {
    agent_id: String,
    agent_color: Color,
    group: RosterGroup,
    kind: RosterKind,
    static_snapshot: bool,
    key: DiffKey,
}

impl KeyExt for RosterAvatar {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for RosterAvatar {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let is_working = self.group == RosterGroup::Working;
        let is_pulsing_attention =
            self.group == RosterGroup::NeedsYou && self.kind != RosterKind::Error && !self.static_snapshot;
        let spin_active = is_working && !self.static_snapshot;

        let spin = use_animation_with_dependencies(&spin_active, |conf, active| {
            if *active {
                conf.on_creation(OnCreation::Run);
                conf.on_change(OnChange::Rerun);
                conf.on_finish(OnFinish::restart());
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(0.0, 360.0)
                .duration(motion::AGENTS_RING_TURN)
                .ease(Ease::InOut)
                .function(Function::Linear)
        });

        let pulse = use_animation_with_dependencies(&is_pulsing_attention, |conf, active| {
            if *active {
                conf.on_creation(OnCreation::Run);
                conf.on_change(OnChange::Rerun);
                conf.on_finish(OnFinish::reverse());
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(0.42, 1.0)
                .duration(motion::AGENTS_ATTENTION_PULSE)
                .ease(Ease::InOut)
                .function(Function::Sine)
        });

        let ring_angle = if spin_active { spin.get().value() } else { 42.0 };
        let agent_color = self.agent_color;
        let working_ring = is_working.then(|| {
            rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .width(Size::px(28.0))
                .height(Size::px(28.0))
                .interactive(Interactive::No)
                .background(conic_working_ring_fill(agent_color, ring_angle))
        });

        let dot: Option<(Color, f32)> = match (self.group, self.kind) {
            (_, RosterKind::Error) => Some((theme.color(ThemeRole::Danger), 1.0)),
            (RosterGroup::NeedsYou, _) => Some((
                theme.color(ThemeRole::Warning),
                if is_pulsing_attention {
                    pulse.get().value().clamp(0.42, 1.0)
                } else {
                    1.0
                },
            )),
            (RosterGroup::Finished, _) => Some((theme.color(ThemeRole::Success), 1.0)),
            (RosterGroup::Working | RosterGroup::Idle, _) => None,
        };

        let is_idle = self.group == RosterGroup::Idle;
        rect()
            .width(Size::px(28.0))
            .height(Size::px(28.0))
            .margin(Gaps::new(1.0, 0.0, 0.0, 0.0))
            .center()
            .child(
                rect()
                    .width(Size::px(22.0))
                    .height(Size::px(22.0))
                    .opacity(if is_idle { 0.70 } else { 1.0 })
                    .center()
                    .child(agent_logo_element(&self.agent_id, 22.0, false)),
            )
            .maybe_child(working_ring)
            .maybe_child(dot.map(|(dot_col, alpha)| {
                rect()
                    .position(Position::new_absolute().right(-1.0).bottom(-1.0))
                    .width(Size::px(9.0))
                    .height(Size::px(9.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .opacity(alpha)
                    .background(dot_col)
                    .border(
                        Border::new()
                            .width(2.0)
                            .alignment(BorderAlignment::Outer)
                            .fill(theme.color(ThemeRole::Surface)),
                    )
            }))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Paints a smooth `2 px` rotating conic-gradient ring (`transparent 0..30% -> color 100%`) matching `zendia.css:4748`.
fn conic_working_ring_fill(color: Color, angle_deg: f32) -> Fill {
    use freya::engine::prelude::{Paint, PaintStyle, PathBuilder, Rect, SkColor};
    let (r, g, b) = (color.r(), color.g(), color.b());
    Fill::Callback(RenderCallback::new(move |ctx| {
        if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
            let size = ctx.size.width.min(ctx.size.height);
            if size <= 4.0 {
                return;
            }
            let inset = 1.0;
            let oval = Rect::from_xywh(inset, inset, size - 2.0 * inset, size - 2.0 * inset);
            let segments = 28usize;
            let sweep_total = 252.0f32; // 70% of 360 deg (30% transparent gap)
            let start_base = angle_deg - 90.0 + 108.0;
            let step = sweep_total / segments as f32;
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(2.0);
            for i in 0..segments {
                let frac = (i + 1) as f32 / segments as f32;
                let a = (frac * frac * 255.0).round().clamp(0.0, 255.0) as u8;
                paint.set_color(SkColor::from_argb(a, r, g, b));
                let mut pb = PathBuilder::new();
                pb.arc_to(oval, start_base + i as f32 * step, step + 1.5, true);
                let path = pb.detach();
                canvas.draw_path(&path, &paint);
            }
        }
    }))
}

/// `3 px` segmented work-list progress bar (`done` green + `other` muted, `300 ms` width transition, `AG-06`).
#[derive(Clone, PartialEq)]
struct WorkProgressBar {
    progress: WorkListProgress,
    key: DiffKey,
}

impl KeyExt for WorkProgressBar {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for WorkProgressBar {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let total = self.progress.total.max(1) as f32;
        let done_pct = (self.progress.done as f32 / total * 100.0).clamp(0.0, 100.0);
        let other_pct = (self.progress.other as f32 / total * 100.0).clamp(0.0, 100.0 - done_pct);

        let mut prev_done = use_state(|| done_pct);
        let mut prev_other = use_state(|| other_pct);
        let done_anim = use_animation_with_dependencies(&done_pct, move |conf, target| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let from = *prev_done.peek();
            prev_done.set(*target);
            AnimNum::new(from, *target)
                .duration(motion::PROGRESS_BAR_WIDTH)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let other_anim = use_animation_with_dependencies(&other_pct, move |conf, target| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let from = *prev_other.peek();
            prev_other.set(*target);
            AnimNum::new(from, *target)
                .duration(motion::PROGRESS_BAR_WIDTH)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let tip = format!("{} of {} steps done", self.progress.done, self.progress.total);
        KitTooltip::new(tip).child(
            rect()
                .width(Size::fill())
                .height(Size::px(3.0))
                .margin(Gaps::new(4.0, 0.0, 0.0, 0.0))
                .corner_radius(CornerRadius::new_all(2.0))
                .overflow(Overflow::Clip)
                .background(theme.rgba(ThemeRole::Border).with_alpha(0.80).to_color())
                .direction(Direction::Horizontal)
                .child(
                    rect()
                        .width(Size::percent(done_anim.get().value().clamp(0.0, 100.0)))
                        .height(Size::fill())
                        .background(theme.color(ThemeRole::Success)),
                )
                .child(
                    rect()
                        .width(Size::percent(other_anim.get().value().clamp(0.0, 100.0)))
                        .height(Size::fill())
                        .background(theme.rgba(ThemeRole::TextMuted).with_alpha(0.70).to_color()),
                ),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Peek toggle button (`22×22`) with `150 ms` rotating caret (`AG-07`, `AG-09`).
#[derive(Clone, PartialEq)]
struct RosterPeekButton {
    expanded: bool,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for RosterPeekButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for RosterPeekButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let rot = use_animation_with_dependencies(&self.expanded, |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *open { (0.0, 180.0) } else { (180.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::PEEK_CARET_TURN)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let bg = fb.background(
            Color::TRANSPARENT,
            theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.16).to_color(),
        );
        let icon_col = if fb.is_hovered() {
            theme.color(ThemeRole::Text)
        } else {
            theme.color(ThemeRole::TextMuted)
        };
        let tip = if self.expanded {
            "Hide steps (Space)"
        } else {
            "Peek at its steps (Space)"
        };
        let on_press = self.on_press.clone();

        KitTooltip::new(tip).child(
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(tip)
                .width(Size::px(22.0))
                .height(Size::px(22.0))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(bg)
                .scale(fb.scale())
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| on_press.call(e))
                .child(
                    rect()
                        .rotate(rot.get().value())
                        .center()
                        .child(chevron_down_icon(10.5, icon_col)),
                ),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Dismiss row button (`22×22`), disabled with `"Stop it first to dismiss it"` tooltip while working (`AG-05`, `AG-09`).
#[derive(Clone, PartialEq)]
struct RosterDismissButton {
    disabled: bool,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for RosterDismissButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for RosterDismissButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(!self.disabled);
        let bg = fb.background(
            Color::TRANSPARENT,
            theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.16).to_color(),
        );
        let tip = if self.disabled {
            "Stop it first to dismiss it"
        } else {
            "Dismiss (d)"
        };
        let on_press = self.on_press.clone();
        let disabled = self.disabled;

        KitTooltip::new(tip).child(
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(tip)
                .width(Size::px(22.0))
                .height(Size::px(22.0))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(bg)
                .opacity(if disabled { 0.38 } else { 1.0 })
                .scale(fb.scale())
                .center()
                .cursor(if disabled {
                    CursorIcon::Default
                } else {
                    CursorIcon::Pointer
                })
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| {
                    if !disabled {
                        on_press.call(e);
                    } else {
                        e.stop_propagation();
                    }
                })
                .child(Icon::new("trash").size(11.5).role(if fb.is_hovered() && !disabled {
                    ThemeRole::Text
                } else {
                    ThemeRole::TextMuted
                })),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Round red-square Stop button (`26×26`) with `12% -> 22%` hover fill and `scale(0.92)` press (`AG-09`).
#[derive(Clone, PartialEq)]
struct RosterStopButton {
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for RosterStopButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for RosterStopButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let bg = fb.background(
            theme.rgba(ThemeRole::Danger).with_alpha(0.12).to_color(),
            theme.rgba(ThemeRole::Danger).with_alpha(0.22).to_color(),
            theme.rgba(ThemeRole::Danger).with_alpha(0.28).to_color(),
        );
        let scale = if fb.is_pressed() { 0.92 } else { 1.0 };
        let on_press = self.on_press.clone();

        KitTooltip::new("Stop (x)").child(
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Stop agent")
                .width(Size::px(26.0))
                .height(Size::px(26.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(bg)
                .border(
                    Border::new()
                        .width(0.5)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.rgba(ThemeRole::Danger).with_alpha(0.35).to_color()),
                )
                .scale(scale)
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| on_press.call(e))
                .child(
                    rect()
                        .width(Size::px(8.0))
                        .height(Size::px(8.0))
                        .corner_radius(CornerRadius::new_all(2.0))
                        .background(theme.color(ThemeRole::Danger)),
                ),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Interactive pill action button (`Allow`/`Deny`/`Retry`/`New agent`) with hover fill & `scale(0.97)` press (`AG-09`).
#[derive(Clone, PartialEq)]
struct RosterPillButton {
    label: String,
    keycap: Option<String>,
    primary: bool,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for RosterPillButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for RosterPillButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let (rest_bg, hover_bg, fg, keycap_bg) = if self.primary {
            let fg = match theme.scheme {
                ResolvedScheme::Dark => Color::from_rgb(24, 24, 38),
                ResolvedScheme::Light => Color::WHITE,
            };
            let kc = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(36, 24, 24, 38),
                ResolvedScheme::Light => Color::from_argb(56, 255, 255, 255),
            };
            (
                theme.color(ThemeRole::Accent),
                mix_color(theme.color(ThemeRole::Accent), Color::WHITE, 0.10),
                fg,
                kc,
            )
        } else {
            let (r, h, kc) = match theme.scheme {
                ResolvedScheme::Dark => (
                    Color::from_argb(36, 255, 255, 255),
                    Color::from_argb(54, 255, 255, 255),
                    Color::from_argb(34, 255, 255, 255),
                ),
                ResolvedScheme::Light => (
                    Color::from_argb(24, 20, 24, 38),
                    Color::from_argb(38, 20, 24, 38),
                    Color::from_argb(28, 20, 24, 38),
                ),
            };
            (r, h, theme.color(ThemeRole::Text), kc)
        };
        let bg = fb.background(rest_bg, hover_bg, hover_bg);
        let scale = if fb.is_pressed() { 0.97 } else { 1.0 };
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let on_press = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label.clone())
            .height(Size::px(26.0))
            .padding(Gaps::new(0.0, 8.0, 0.0, 11.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(bg)
            .border(border)
            .scale(scale)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(6.0)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| on_press.call(e))
            .child(
                label()
                    .text(self.label.clone())
                    .font_size(11.5)
                    .font_weight(if self.primary {
                        FontWeight::SEMI_BOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .color(fg),
            )
            .maybe_child(self.keycap.as_ref().map(|k| {
                rect()
                    .min_width(Size::px(16.0))
                    .height(Size::px(16.0))
                    .padding(Gaps::new(0.0, 3.0, 0.0, 3.0))
                    .corner_radius(CornerRadius::new_all(4.0))
                    .background(keycap_bg)
                    .center()
                    .child(label().text(k.clone()).font_size(9.5).color(if self.primary {
                        fg
                    } else {
                        theme.color(ThemeRole::TextMuted)
                    }))
            }))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn legend_pair(keys: &[&str], desc: &str, theme: &crate::ui::theme::ResolvedTheme) -> Element {
    let cap_bg = match theme.scheme {
        ResolvedScheme::Dark => Color::from_argb(26, 255, 255, 255),
        ResolvedScheme::Light => Color::from_argb(18, 20, 24, 38),
    };
    let mut row = rect()
        .direction(Direction::Horizontal)
        .cross_align(Alignment::center())
        .spacing(4.0);
    for k in keys {
        row = row.child(
            rect()
                .min_width(Size::px(16.0))
                .height(Size::px(16.0))
                .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
                .corner_radius(CornerRadius::new_all(4.0))
                .background(cap_bg)
                .center()
                .child(
                    label()
                        .text((*k).to_string())
                        .font_size(9.5)
                        .font_weight(FontWeight::MEDIUM)
                        .color(theme.color(ThemeRole::TextMuted)),
                ),
        );
    }
    row.child(
        label()
            .text(desc.to_string())
            .font_size(10.5)
            .color(theme.color(ThemeRole::TextMuted)),
    )
    .into_element()
}
