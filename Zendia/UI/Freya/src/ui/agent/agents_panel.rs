/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Cross-window Agents roster side panel (`zendia.agents`).
//!
//! Groups every agent conversation across tabs by urgency (`NEEDS YOU`, `WORKING`, `FINISHED`, `IDLE`), with inline
//! permission approval (`Allow [y]` / `Deny [n]`), inline stop, expandable step peek, dismiss, and vim-style keyboard
//! navigation (`j`/`k`, `Enter`, `Space`, `x`, `d`, `a`).

use crate::ui::agent::motion::WorkingRing;
use crate::ui::agent::{agent_badge_emblem, chevron_down_icon, chevron_right_icon};
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::panels::{DockLayoutControls, ResolvedDockPanel};
use crate::ui::theme::{ResolvedScheme, ThemeRole, use_theme};
use freya::prelude::*;
use std::collections::HashSet;
use zendia_browser_core::agent::{
    AGENT_PANEL_ID, AGENTS_PANEL_ID, RosterGroup, classify_roster_group, format_duration_secs, format_relative_age,
};
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::SidePanelId;
use zendia_browser_core::model::{
    Conversation, ConversationId, ConversationStatus, ServiceId, ServiceStatus, StepId, StepKind, StepStatus,
};

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

struct RosterRowData {
    group: RosterGroup,
    conv: Conversation,
    agent_name: String,
    tab_title: String,
    tab_color: Color,
    pending_confirm: Option<(StepId, String)>,
    snippet: String,
}

impl Component for AgentsPanel {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut cursor = use_state(|| 0usize);
        let mut peeked = use_state(HashSet::<u64>::new);
        let panel_a11y_id = use_a11y();

        let (rows, active_tab_id, service_status) = {
            let model = core.model().read();
            let active_tab_id = model.active_tab;
            let service_status = model
                .services
                .iter()
                .find(|s| s.service == ServiceId::AgentServer)
                .map(|s| s.status.clone())
                .unwrap_or(ServiceStatus::Running);

            let mut grouped: Vec<RosterRowData> = Vec::new();
            for group in RosterGroup::ALL {
                for conv in &model.conversations {
                    if classify_roster_group(conv) != group {
                        continue;
                    }
                    let agent_name = model
                        .agent(&conv.agent_id)
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| conv.agent_id.0.clone());
                    let tab = conv.tab_id.and_then(|tid| model.tab(tid));
                    let tab_title = tab
                        .map(|t| t.display_title().to_string())
                        .unwrap_or_else(|| "Current tab".to_string());
                    let tab_color = tab_badge_color(conv.tab_id.map(|t| t.0).unwrap_or(0));
                    let pending_confirm = conv.steps.iter().rev().find_map(|step| match &step.kind {
                        StepKind::Confirmation { resolved: None, prompt } => Some((step.id, prompt.clone())),
                        _ => None,
                    });
                    let snippet = pending_confirm
                        .as_ref()
                        .map(|(_, s)| s.clone())
                        .or_else(|| {
                            conv.steps
                                .iter()
                                .rev()
                                .find(|s| !matches!(s.kind, StepKind::UserPrompt))
                                .map(|s| s.summary.clone())
                        })
                        .unwrap_or_else(|| conv.title.clone());

                    grouped.push(RosterRowData {
                        group,
                        conv: conv.clone(),
                        agent_name,
                        tab_title,
                        tab_color,
                        pending_confirm,
                        snippet,
                    });
                }
            }
            (grouped, active_tab_id, service_status)
        };

        let row_count = rows.len();
        let selected_idx = if row_count == 0 {
            0
        } else {
            (*cursor.read()).min(row_count - 1)
        };

        let border_col = theme.color(ThemeRole::BorderSubtle);
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

        let core_new = core.clone();
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
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("New agent chat")
                            .width(Size::px(24.0))
                            .height(Size::px(24.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_new.invoke_or_log(Command::AgentNew { tab_id: active_tab_id });
                                core_new.invoke_or_log(Command::PanelOpen {
                                    panel_id: SidePanelId::new(AGENT_PANEL_ID),
                                    focus: true,
                                });
                            })
                            .child(Icon::new("plus").size(12.5).role(ThemeRole::TextMuted)),
                    )
                    .child(DockLayoutControls {
                        panel_id: self.resolved.panel_id.clone(),
                        prefs: self.resolved.prefs,
                        split_fell_back: self.resolved.split_fell_back,
                        accent: theme_accent,
                    })
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Close agents panel")
                            .width(Size::px(24.0))
                            .height(Size::px(24.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_close.invoke_or_log(Command::PanelClose {
                                    panel_id: SidePanelId::new(AGENTS_PANEL_ID),
                                });
                            })
                            .child(Icon::new("close").size(12.0).role(ThemeRole::TextMuted)),
                    ),
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
                .child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("Retry AgentServer")
                        .height(Size::px(22.0))
                        .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                        .corner_radius(CornerRadius::new_all(6.0))
                        .background(theme.rgba(ThemeRole::SurfaceRaised).with_alpha(0.85).to_color())
                        .center()
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_retry.invoke_or_log(Command::DebugToggleAgentService);
                        })
                        .child(
                            label()
                                .text("Retry")
                                .font_size(10.5)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(theme.color(ThemeRole::Text)),
                        ),
                )
        });

        let body: Element = if rows.is_empty() {
            let core_empty_new = core.clone();
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .padding(Gaps::new(24.0, 28.0, 24.0, 28.0))
                .center()
                .spacing(10.0)
                .child(Icon::new("layers").size(32.0).role(ThemeRole::TextMuted))
                .child(
                    label()
                        .width(Size::fill())
                        .text("No agents at work")
                        .font_size(14.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .text_align(TextAlign::Center)
                        .color(theme.color(ThemeRole::Text)),
                )
                .child(
                    label()
                        .width(Size::fill())
                        .text("Open Agent Chat on any tab or press a to start a new run.")
                        .font_size(11.5)
                        .line_height(1.45)
                        .text_align(TextAlign::Center)
                        .color(theme.color(ThemeRole::TextMuted)),
                )
                .child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("Start new agent chat")
                        .margin(Gaps::new(4.0, 0.0, 0.0, 0.0))
                        .height(Size::px(28.0))
                        .padding(Gaps::new(0.0, 12.0, 0.0, 10.0))
                        .corner_radius(CornerRadius::new_all(999.0))
                        .background(theme.rgba(ThemeRole::Accent).with_alpha(0.16).to_color())
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(theme.rgba(ThemeRole::Accent).with_alpha(0.35).to_color()),
                        )
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(5.0)
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_empty_new.invoke_or_log(Command::AgentNew { tab_id: active_tab_id });
                            core_empty_new.invoke_or_log(Command::PanelOpen {
                                panel_id: SidePanelId::new(AGENT_PANEL_ID),
                                focus: true,
                            });
                        })
                        .child(Icon::new("plus").size(11.5).role(ThemeRole::Accent))
                        .child(
                            label()
                                .text("New chat")
                                .font_size(11.5)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(theme_accent),
                        ),
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
                    .filter_map(|(idx, r)| (r.group == group).then_some(idx))
                    .collect();
                if group_indices.is_empty() {
                    continue;
                }
                let heading_color = match group {
                    RosterGroup::NeedsYou => theme.color(ThemeRole::Warning),
                    _ => theme.color(ThemeRole::TextMuted),
                };
                let mut section = rect().width(Size::fill()).spacing(6.0).child(
                    rect().padding(Gaps::new(0.0, 4.0, 0.0, 4.0)).child(
                        label()
                            .text(format!("{} · {}", group.heading(), group_indices.len()))
                            .font_size(9.5)
                            .font_weight(FontWeight::BOLD)
                            .color(heading_color),
                    ),
                );

                for idx in group_indices {
                    let row = &rows[idx];
                    let is_selected = flat_idx == selected_idx;
                    let is_peeked = peeked.read().contains(&row.conv.id.0);
                    section = section.child(render_roster_row(
                        row,
                        flat_idx,
                        is_selected,
                        is_peeked,
                        &theme,
                        &core,
                        cursor,
                        peeked,
                    ));
                    flat_idx += 1;
                }
                list_col = list_col.child(section);
            }

            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .child(
                    ScrollView::new()
                        .width(Size::fill())
                        .height(Size::fill())
                        .child(list_col),
                )
                .into_element()
        };

        // --- Keyboard navigation (j/k move, Enter open, Space peek, x stop, d dismiss, a new, y/n confirm) ---
        let key_rows: Vec<(
            ConversationId,
            Option<zendia_browser_core::model::TabId>,
            ConversationStatus,
            Option<StepId>,
        )> = rows
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
                        e.stop_propagation();
                    }
                }
                Key::Character(ch) if ch == " " => {
                    if let Some((conv_id, _, _, _)) = key_rows.get((*cursor.peek()).min(count.saturating_sub(1))) {
                        let mut set = peeked.peek().clone();
                        if !set.remove(&conv_id.0) {
                            set.insert(conv_id.0);
                        }
                        peeked.set(set);
                        e.stop_propagation();
                    }
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
                Key::Character(ch) if ch == "d" => {
                    if let Some((conv_id, _, _, _)) = key_rows.get((*cursor.peek()).min(count.saturating_sub(1))) {
                        core_keys.invoke_or_log(Command::AgentDismiss {
                            conversation_id: *conv_id,
                        });
                        e.stop_propagation();
                    }
                }
                Key::Character(ch) if ch == "a" => {
                    core_keys.invoke_or_log(Command::AgentNew { tab_id: active_tab_id });
                    core_keys.invoke_or_log(Command::PanelOpen {
                        panel_id: SidePanelId::new(AGENT_PANEL_ID),
                        focus: true,
                    });
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
                _ => {}
            }
        };

        // --- Footer key legend ---
        let footer = rect()
            .width(Size::fill())
            .padding(Gaps::new(7.0, 10.0, 8.0, 10.0))
            .spacing(4.0)
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(9.0)
                    .child(legend_pair(&["j", "k"], "move", &theme))
                    .child(legend_pair(&["↵"], "open", &theme))
                    .child(legend_pair(&["␣"], "peek", &theme))
                    .child(legend_pair(&["x"], "stop", &theme))
                    .child(legend_pair(&["d"], "dismiss", &theme)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .child(legend_pair(&["a"], "new", &theme)),
            );

        rect()
            .a11y_id(panel_a11y_id)
            .a11y_focusable(true)
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .on_key_down(on_key_down)
            .child(header)
            .child(rect().width(Size::fill()).height(Size::px(1.0)).background(border_col))
            .maybe_child(service_banner)
            .child(body)
            .child(rect().width(Size::fill()).height(Size::px(1.0)).background(border_col))
            .child(footer)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[allow(clippy::too_many_arguments)]
fn render_roster_row(
    row: &RosterRowData,
    index: usize,
    is_selected: bool,
    is_peeked: bool,
    theme: &crate::ui::theme::ResolvedTheme,
    core: &crate::ui::core_view::CoreView,
    mut cursor: State<usize>,
    mut peeked: State<HashSet<u64>>,
) -> Element {
    let conv_id = row.conv.id;
    let tab_id = row.conv.tab_id;
    let is_needs_you = row.group == RosterGroup::NeedsYou;
    let is_working = row.group == RosterGroup::Working;
    let is_idle = row.group == RosterGroup::Idle;

    let (status_word, status_color, dot_color) = match row.group {
        RosterGroup::NeedsYou => (
            "Asks permission",
            theme.color(ThemeRole::Warning),
            theme.color(ThemeRole::Warning),
        ),
        RosterGroup::Working => ("Working", Color::from_rgb(52, 211, 153), Color::from_rgb(52, 211, 153)),
        RosterGroup::Finished => (
            "Finished",
            theme.color(ThemeRole::Success),
            theme.color(ThemeRole::Success),
        ),
        RosterGroup::Idle => (
            "Idle",
            theme.color(ThemeRole::TextMuted),
            theme.color(ThemeRole::TextMuted),
        ),
    };

    let card_bg = if is_needs_you {
        theme
            .rgba(ThemeRole::Warning)
            .with_alpha(match theme.scheme {
                ResolvedScheme::Dark => 0.11,
                ResolvedScheme::Light => 0.14,
            })
            .to_color()
    } else if is_selected {
        match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(20, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(14, 20, 24, 38),
        }
    } else {
        Color::TRANSPARENT
    };
    let card_border = if is_needs_you {
        theme.rgba(ThemeRole::Warning).with_alpha(0.32).to_color()
    } else if is_selected {
        theme.color(ThemeRole::BorderSubtle)
    } else {
        Color::TRANSPARENT
    };

    let time_label = if is_working {
        format_duration_secs(row.conv.elapsed_secs)
    } else {
        format_relative_age(row.conv.elapsed_secs.max(7))
    };

    let core_open = core.clone();
    let core_stop = core.clone();
    let core_dismiss = core.clone();

    let avatar = rect()
        .width(Size::px(24.0))
        .height(Size::px(24.0))
        .opacity(if is_idle { 0.65 } else { 1.0 })
        .child(agent_badge_emblem(&row.conv.agent_id.0, 24.0, 15.0, 999.0, false))
        .child(
            rect()
                .position(Position::new_absolute().right(-1.0).bottom(-1.0))
                .width(Size::px(8.0))
                .height(Size::px(8.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(dot_color)
                .border(
                    Border::new()
                        .width(1.5)
                        .alignment(BorderAlignment::Outer)
                        .fill(theme.color(ThemeRole::Surface)),
                ),
        );

    let top_right: Element = if is_needs_you || is_selected {
        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(3.0)
            .child(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Peek steps")
                    .width(Size::px(20.0))
                    .height(Size::px(20.0))
                    .corner_radius(CornerRadius::new_all(5.0))
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        let mut set = peeked.peek().clone();
                        if !set.remove(&conv_id.0) {
                            set.insert(conv_id.0);
                        }
                        peeked.set(set);
                    })
                    .child(if is_peeked {
                        chevron_down_icon(10.0, theme.color(ThemeRole::TextMuted))
                    } else {
                        chevron_right_icon(10.0, theme.color(ThemeRole::TextMuted))
                    }),
            )
            .child(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Dismiss agent run")
                    .width(Size::px(20.0))
                    .height(Size::px(20.0))
                    .corner_radius(CornerRadius::new_all(5.0))
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        core_dismiss.invoke_or_log(Command::AgentDismiss {
                            conversation_id: conv_id,
                        });
                    })
                    .child(Icon::new("trash").size(11.0).role(ThemeRole::TextMuted)),
            )
            .into_element()
    } else {
        label()
            .text(time_label)
            .font_size(10.5)
            .color(theme.color(ThemeRole::TextMuted))
            .max_lines(1)
            .into_element()
    };

    let stop_btn = is_working.then(|| {
        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Stop agent")
            .width(Size::px(26.0))
            .height(Size::px(26.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(theme.rgba(ThemeRole::Danger).with_alpha(0.18).to_color())
            .center()
            .cursor(CursorIcon::Pointer)
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                core_stop.invoke_or_log(Command::AgentStop {
                    conversation_id: Some(conv_id),
                });
            })
            .child(
                rect()
                    .width(Size::px(8.0))
                    .height(Size::px(8.0))
                    .corner_radius(CornerRadius::new_all(2.0))
                    .background(theme.color(ThemeRole::Danger)),
            )
    });

    let inline_confirm = row.pending_confirm.as_ref().map(|(step_id, prompt)| {
        let sid = *step_id;
        let core_deny = core.clone();
        let core_allow = core.clone();
        let inner_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(46, 20, 22, 34),
            ResolvedScheme::Light => Color::from_argb(30, 120, 88, 28),
        };
        let deny_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(42, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(26, 20, 24, 38),
        };
        let allow_fg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_rgb(24, 24, 38),
            ResolvedScheme::Light => Color::WHITE,
        };
        let keycap_bg_deny = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(36, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(28, 20, 24, 38),
        };
        let keycap_bg_allow = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(36, 24, 24, 38),
            ResolvedScheme::Light => Color::from_argb(56, 255, 255, 255),
        };

        rect()
            .width(Size::fill())
            .margin(Gaps::new(6.0, 0.0, 0.0, 0.0))
            .padding(Gaps::new(9.0, 10.0, 9.0, 10.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(inner_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.rgba(ThemeRole::Warning).with_alpha(0.26).to_color()),
            )
            .spacing(8.0)
            .child(
                label()
                    .width(Size::fill())
                    .text(prompt.clone())
                    .font_size(11.5)
                    .line_height(1.4)
                    .color(theme.color(ThemeRole::Text)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::end())
                    .cross_align(Alignment::center())
                    .spacing(7.0)
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Deny permission")
                            .height(Size::px(26.0))
                            .padding(Gaps::new(0.0, 8.0, 0.0, 10.0))
                            .corner_radius(CornerRadius::new_all(7.0))
                            .background(deny_bg)
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(6.0)
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_deny.invoke_or_log(Command::AgentConfirm {
                                    conversation_id: conv_id,
                                    step_id: sid,
                                    allow: false,
                                });
                            })
                            .child(
                                label()
                                    .text("Deny")
                                    .font_size(11.5)
                                    .font_weight(FontWeight::MEDIUM)
                                    .color(theme.color(ThemeRole::Text)),
                            )
                            .child(
                                rect()
                                    .width(Size::px(16.0))
                                    .height(Size::px(16.0))
                                    .corner_radius(CornerRadius::new_all(4.0))
                                    .background(keycap_bg_deny)
                                    .center()
                                    .child(
                                        label()
                                            .text("n")
                                            .font_size(9.5)
                                            .color(theme.color(ThemeRole::TextMuted)),
                                    ),
                            ),
                    )
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Allow permission")
                            .height(Size::px(26.0))
                            .padding(Gaps::new(0.0, 8.0, 0.0, 10.0))
                            .corner_radius(CornerRadius::new_all(7.0))
                            .background(theme.color(ThemeRole::Accent))
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(6.0)
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_allow.invoke_or_log(Command::AgentConfirm {
                                    conversation_id: conv_id,
                                    step_id: sid,
                                    allow: true,
                                });
                            })
                            .child(
                                label()
                                    .text("Allow")
                                    .font_size(11.5)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(allow_fg),
                            )
                            .child(
                                rect()
                                    .width(Size::px(16.0))
                                    .height(Size::px(16.0))
                                    .corner_radius(CornerRadius::new_all(4.0))
                                    .background(keycap_bg_allow)
                                    .center()
                                    .child(label().text("y").font_size(9.5).color(allow_fg)),
                            ),
                    ),
            )
    });

    let peek_drawer = is_peeked.then(|| {
        let mut col = rect()
            .width(Size::fill())
            .margin(Gaps::new(6.0, 0.0, 0.0, 28.0))
            .padding(Gaps::new(6.0, 8.0, 6.0, 8.0))
            .corner_radius(CornerRadius::new_all(7.0))
            .background(match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(28, 255, 255, 255),
                ResolvedScheme::Light => Color::from_argb(14, 20, 24, 38),
            })
            .spacing(4.0);
        let tail_start = row.conv.steps.len().saturating_sub(3);
        for step in &row.conv.steps[tail_start..] {
            let icon_el: Element = match step.status {
                StepStatus::Running => WorkingRing::new(11.0)
                    .color(theme.color(ThemeRole::Accent))
                    .into_element(),
                StepStatus::Completed => Icon::new("check").size(10.5).role(ThemeRole::Success).into_element(),
                StepStatus::Failed => Icon::new("close").size(10.5).role(ThemeRole::Danger).into_element(),
                StepStatus::WaitingForUser => Icon::new("warning").size(10.5).role(ThemeRole::Warning).into_element(),
                StepStatus::Pending => Icon::new("info").size(10.5).role(ThemeRole::TextMuted).into_element(),
            };
            col = col.child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .child(icon_el)
                    .child(
                        rect().width(Size::flex(1.0)).child(
                            label()
                                .text(step.summary.clone())
                                .font_size(10.5)
                                .color(theme.color(ThemeRole::TextMuted))
                                .max_lines(1),
                        ),
                    ),
            );
        }
        col
    });

    rect()
        .a11y_role(AccessibilityRole::Button)
        .a11y_alt(format!("Open agent run: {}", row.conv.title))
        .width(Size::fill())
        .padding(Gaps::new(9.0, 9.0, 9.0, 9.0))
        .corner_radius(CornerRadius::new_all(10.0))
        .background(card_bg)
        .border(
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(card_border),
        )
        .cursor(CursorIcon::Pointer)
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
                .spacing(9.0)
                .child(avatar)
                .child(
                    rect()
                        .width(Size::flex(1.0))
                        .spacing(3.0)
                        .child(
                            rect()
                                .width(Size::fill())
                                .direction(Direction::Horizontal)
                                .content(Content::Flex)
                                .cross_align(Alignment::center())
                                .main_align(Alignment::SpaceBetween)
                                .spacing(6.0)
                                .child(
                                    rect().width(Size::flex(1.0)).overflow(Overflow::Clip).child(
                                        label()
                                            .text(row.conv.title.clone())
                                            .font_size(12.0)
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
                                .spacing(5.0)
                                .child(
                                    label()
                                        .text(status_word)
                                        .font_size(11.0)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .color(status_color)
                                        .max_lines(1),
                                )
                                .child(
                                    rect().width(Size::flex(1.0)).overflow(Overflow::Clip).child(
                                        label()
                                            .text(row.snippet.clone())
                                            .font_size(11.0)
                                            .color(theme.color(ThemeRole::TextMuted))
                                            .max_lines(1)
                                            .text_overflow(TextOverflow::Ellipsis),
                                    ),
                                ),
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
                                        .child(
                                            rect()
                                                .width(Size::px(12.0))
                                                .height(Size::px(12.0))
                                                .corner_radius(CornerRadius::new_all(3.0))
                                                .background(row.tab_color)
                                                .center()
                                                .child(
                                                    rect()
                                                        .width(Size::px(5.0))
                                                        .height(Size::px(5.0))
                                                        .corner_radius(CornerRadius::new_all(999.0))
                                                        .background(Color::WHITE),
                                                ),
                                        )
                                        .child(
                                            rect().width(Size::flex(1.0)).overflow(Overflow::Clip).child(
                                                label()
                                                    .text(row.tab_title.clone())
                                                    .font_size(10.5)
                                                    .color(theme.color(ThemeRole::TextMuted))
                                                    .max_lines(1)
                                                    .text_overflow(TextOverflow::Ellipsis),
                                            ),
                                        ),
                                )
                                .child(
                                    label()
                                        .text(row.agent_name.clone())
                                        .font_size(10.5)
                                        .color(theme.color(ThemeRole::TextMuted))
                                        .max_lines(1),
                                ),
                        ),
                )
                .maybe_child(stop_btn),
        )
        .maybe_child(inline_confirm)
        .maybe_child(peek_drawer)
        .into_element()
}

fn legend_pair(keys: &[&str], desc: &str, theme: &crate::ui::theme::ResolvedTheme) -> Element {
    let cap_bg = match theme.scheme {
        ResolvedScheme::Dark => Color::from_argb(26, 255, 255, 255),
        ResolvedScheme::Light => Color::from_argb(18, 20, 24, 38),
    };
    let mut row = rect()
        .direction(Direction::Horizontal)
        .cross_align(Alignment::center())
        .spacing(3.0);
    for k in keys {
        row = row.child(
            rect()
                .min_width(Size::px(15.0))
                .height(Size::px(15.0))
                .padding(Gaps::new(0.0, 3.0, 0.0, 3.0))
                .corner_radius(CornerRadius::new_all(3.5))
                .background(cap_bg)
                .center()
                .child(
                    label()
                        .text((*k).to_string())
                        .font_size(9.0)
                        .font_weight(FontWeight::MEDIUM)
                        .color(theme.color(ThemeRole::TextMuted)),
                ),
        );
    }
    row.child(
        label()
            .text(desc.to_string())
            .font_size(9.5)
            .color(theme.color(ThemeRole::TextMuted)),
    )
    .into_element()
}

fn tab_badge_color(seed: u64) -> Color {
    match seed % 4 {
        0 => Color::from_rgb(59, 130, 246), // blue
        1 => Color::from_rgb(16, 185, 129), // emerald
        2 => Color::from_rgb(236, 72, 153), // pink
        _ => Color::from_rgb(249, 115, 22), // orange
    }
}
