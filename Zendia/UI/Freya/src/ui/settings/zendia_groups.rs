/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Zendia-specific settings groups: Vim Mode, Agents, and Web Chat.

use crate::ui::core_view::use_core;
use crate::ui::kit::{KeyLabel, Pill, TextField, focus_ring_border, use_interactive_feedback};
use crate::ui::settings::controls::{
    ServiceStatusBanner, SettingsButton, SettingsChoiceRow, SettingsPalette, SettingsToggleRow, section_heading,
    settings_card, subsection_heading,
};
use crate::ui::theme::{Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::agent::{AGENT_PANEL_ID, AGENTS_PANEL_ID};
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{SidePanelId, SidePanelLayout, SidePanelSide};
use zendia_browser_core::input_state::StatuslineShow;
use zendia_browser_core::model::{Agent, ChatSite, ServiceId, ServiceStatus};
use zendia_browser_core::modules::service_status_in;
use zendia_browser_core::webchat::WEBCHAT_PANEL_ID;
use zendia_module_api::ui::UiTone;

/// Vim Mode settings group: opt-in vim mode, statusline resting style, and InputServer status.
#[derive(Clone, PartialEq)]
pub struct VimSettingsGroup;

impl Component for VimSettingsGroup {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let (vim_enabled, statusline, browser_settings, input_status) = {
            let model = core.model().read();
            (
                model.vim_mode_enabled,
                model.statusline.clone(),
                model.settings.clone(),
                service_status_in(&model.services, ServiceId::InputServer),
            )
        };

        let mut leader_draft = use_state(|| browser_settings.vim_leader.clone());
        let mut last_model_leader = use_state(|| browser_settings.vim_leader.clone());
        if *last_model_leader.peek() != browser_settings.vim_leader {
            last_model_leader.set(browser_settings.vim_leader.clone());
            leader_draft.set(browser_settings.vim_leader.clone());
        }
        let mut timeout_draft = use_state(|| browser_settings.vim_timeoutlen.to_string());
        let mut last_model_timeout = use_state(|| browser_settings.vim_timeoutlen);
        if *last_model_timeout.peek() != browser_settings.vim_timeoutlen {
            last_model_timeout.set(browser_settings.vim_timeoutlen);
            timeout_draft.set(browser_settings.vim_timeoutlen.to_string());
        }

        let core_vim = core.clone();
        let core_sl_enabled = core.clone();
        let core_show = core.clone();
        let core_leader = core.clone();
        let core_timeout = core.clone();
        let core_restart = core.clone();
        let input_down = !matches!(input_status, ServiceStatus::Running);
        let user_json_hint = zendia_browser_core::settings::user_keymap_profile_paths()
            .into_iter()
            .next()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~/.config/zendia/zendia/zendia.json".to_string());

        rect()
            .width(Size::fill())
            .spacing(22.0)
            .child(section_heading(
                "Vim Mode",
                Some("Optional keyboard-driven browsing with modal navigation, explorer focus, hints, and which-key."),
            ))
            .maybe_child(input_down.then(|| ServiceStatusBanner {
                service: ServiceId::InputServer,
                status: input_status.clone(),
                on_restart: EventHandler::new(move |_| {
                    core_restart.invoke_or_log(Command::ServiceRestart {
                        service: ServiceId::InputServer,
                    });
                }),
            }))
            .child(settings_card(vec![
                subsection_heading("Keyboard scheme", None),
                SettingsToggleRow::new("Enable Vim mode", vim_enabled, move |next| {
                    core_vim.invoke_or_log(Command::VimSetEnabled { enabled: next });
                })
                .description("Off by default so Zendia behaves like a standard browser until you turn it on.")
                .key("vim-enabled")
                .into_element(),
                SettingsToggleRow::new("Show mode indicator", statusline.enabled, move |next| {
                    core_sl_enabled.invoke_or_log(Command::StatuslineSetEnabled { enabled: next });
                })
                .description("Display Zendia's mode mascot and pending keys in the bottom corner when Vim mode is on.")
                .key("vim-statusline-enabled")
                .into_element(),
                SettingsChoiceRow::new(
                    "Mode indicator resting style",
                    statusline.show.as_str(),
                    vec![
                        ("shrink".to_string(), "Shrink to badge when quiet".to_string()),
                        ("always".to_string(), "Always visible".to_string()),
                        ("never".to_string(), "Hide when quiet".to_string()),
                    ],
                    move |val: String| {
                        if let Some(show) = StatuslineShow::parse(&val) {
                            core_show.invoke_or_log(Command::StatuslineSetShow { show });
                        }
                    },
                )
                .description("Controls how the mascot and mode pill rest after key activity settles.")
                .key("vim-statusline-show")
                .into_element(),
                settings_text_input_row(
                    "Leader key",
                    "Prefix token expanded by <leader> mappings (e.g. <Space>, , or \\).",
                    leader_draft,
                    "<Space>",
                    palette,
                    move |val| {
                        core_leader.invoke_or_log(Command::SettingsSet {
                            key: "vim.leader".to_string(),
                            value: val,
                        });
                    },
                ),
                settings_text_input_row(
                    "Sequence timeout (ms)",
                    "Milliseconds an ambiguous prefix waits before settling (10–10000 ms).",
                    timeout_draft,
                    "300",
                    palette,
                    move |val| {
                        core_timeout.invoke_or_log(Command::SettingsSet {
                            key: "vim.timeoutlen".to_string(),
                            value: val,
                        });
                    },
                ),
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(4.0, 8.0, 2.0, 8.0))
                    .child(
                        label()
                            .text(format!("User keymap overrides: {user_json_hint}"))
                            .font_size(11.5)
                            .font_family("monospace")
                            .color(theme.color(ThemeRole::TextMuted)),
                    )
                    .into_element(),
            ]))
            .child(settings_card(vec![
                subsection_heading(
                    "Quick Reference",
                    Some("Standard shortcuts remain available alongside vim mode."),
                ),
                rect()
                    .width(Size::fill())
                    .spacing(8.0)
                    .child(shortcut_hint_row("Open Command Palette", "<C-k>"))
                    .child(shortcut_hint_row("Open Settings", "<C-,>"))
                    .child(shortcut_hint_row("Toggle Agent Panel", "<C-S-a>"))
                    .child(shortcut_hint_row("Focus Sidebar Explorer (Vim)", "<C-A-e>"))
                    .child(shortcut_hint_row("Enter Window Mode (Vim)", "<C-A-w>"))
                    .into_element(),
            ]))
            .child(
                label()
                    .text(if vim_enabled {
                        "Active profile: vim.toml (compiled keymap table in UI process)"
                    } else {
                        "Active profile: standard.toml (default browser shortcuts)"
                    })
                    .font_size(theme.font_sizes.xs)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
    }
}

fn shortcut_hint_row(title: &str, keys: &str) -> Element {
    let theme = use_theme();
    rect()
        .width(Size::fill())
        .padding(Gaps::new(4.0, 8.0, 4.0, 8.0))
        .direction(Direction::Horizontal)
        .main_align(Alignment::SpaceBetween)
        .cross_align(Alignment::center())
        .child(
            label()
                .text(title.to_string())
                .font_size(12.5)
                .color(theme.color(ThemeRole::Text)),
        )
        .child(KeyLabel::new(keys))
        .into_element()
}

fn settings_text_input_row(
    title: &str,
    description: &str,
    draft: State<String>,
    placeholder: &str,
    palette: SettingsPalette,
    on_commit: impl Fn(String) + Clone + 'static,
) -> Element {
    let theme = use_theme();
    let commit_enter = on_commit.clone();
    let commit_btn = on_commit;
    rect()
        .width(Size::fill())
        .padding(Gaps::new(8.0, 8.0, 8.0, 8.0))
        .direction(Direction::Horizontal)
        .content(Content::Flex)
        .main_align(Alignment::SpaceBetween)
        .cross_align(Alignment::center())
        .spacing(12.0)
        .child(
            rect()
                .width(Size::flex(1.0))
                .spacing(2.0)
                .child(
                    label()
                        .text(title.to_string())
                        .font_size(13.5)
                        .font_weight(FontWeight::MEDIUM)
                        .color(theme.color(ThemeRole::Text)),
                )
                .child(
                    label()
                        .text(description.to_string())
                        .font_size(12.0)
                        .color(theme.color(ThemeRole::TextMuted)),
                ),
        )
        .child(
            rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(6.0)
                .child(
                    rect()
                        .width(Size::px(124.0))
                        .height(Size::px(32.0))
                        .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                        .corner_radius(CornerRadius::new_all(7.0))
                        .background(palette.input_bg)
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(palette.input_border),
                        )
                        .main_align(Alignment::center())
                        .font_size(12.5)
                        .child(
                            TextField::new(draft)
                                .placeholder(placeholder.to_string())
                                .width(Size::fill())
                                .padding(Gaps::new(0.0, 2.0, 0.0, 2.0))
                                .background(Color::TRANSPARENT)
                                .focus_background(Color::TRANSPARENT)
                                .color(theme.color(ThemeRole::Text))
                                .placeholder_color(theme.color(ThemeRole::TextMuted))
                                .on_submit(move |_| {
                                    let val = draft.peek().trim().to_string();
                                    if !val.is_empty() {
                                        commit_enter(val);
                                    }
                                }),
                        ),
                )
                .child(SettingsButton::new("Apply", move |_| {
                    let val = draft.peek().trim().to_string();
                    if !val.is_empty() {
                        commit_btn(val);
                    }
                })),
        )
        .into_element()
}

/// Agents settings group: default agent, dock layout/side, automation toggles, instructions link, and AgentServer health.
#[derive(Clone, PartialEq)]
pub struct AgentsSettingsGroup;

impl Component for AgentsSettingsGroup {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let (agents, default_agent_id, agent_prefs, agents_prefs, browser_settings, instruction_count, agent_status) = {
            let model = core.model().read();
            (
                model.agents.clone(),
                model.default_agent_id.clone(),
                model.panel_prefs(&SidePanelId::new(AGENT_PANEL_ID)),
                model.panel_prefs(&SidePanelId::new(AGENTS_PANEL_ID)),
                model.settings.clone(),
                model.active_instruction_count(),
                service_status_in(&model.services, ServiceId::AgentServer),
            )
        };

        let core_restart = core.clone();
        let core_agent_layout = core.clone();
        let core_agent_side = core.clone();
        let core_agents_layout = core.clone();
        let core_steal = core.clone();
        let core_loop = core.clone();
        let core_dictation = core.clone();
        let core_instr = core.clone();
        let agent_down = !matches!(agent_status, ServiceStatus::Running);
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let mut dict_key_draft = use_state(|| browser_settings.dictation_hold_key.clone());
        let mut last_dict_key = use_state(|| browser_settings.dictation_hold_key.clone());
        if *last_dict_key.peek() != browser_settings.dictation_hold_key {
            last_dict_key.set(browser_settings.dictation_hold_key.clone());
            dict_key_draft.set(browser_settings.dictation_hold_key.clone());
        }

        let agent_cards: Vec<Element> = agents
            .iter()
            .map(|agent| {
                let is_default = default_agent_id.as_ref() == Some(&agent.id);
                let sel_core = core.clone();
                let aid = agent.id.clone();
                AgentSelectCard {
                    agent: agent.clone(),
                    selected: is_default,
                    on_select: EventHandler::new(move |_| {
                        sel_core.invoke_or_log(Command::AgentSelect {
                            agent_id: Some(aid.clone()),
                        });
                    }),
                    key: DiffKey::from(&agent.id.0),
                }
                .into_element()
            })
            .collect();

        rect()
            .width(Size::fill())
            .spacing(22.0)
            .child(section_heading(
                "Agents",
                Some("Configure your default AI agent, side panel dock layout, and task automation preferences."),
            ))
            .maybe_child(agent_down.then(|| ServiceStatusBanner {
                service: ServiceId::AgentServer,
                status: agent_status.clone(),
                on_restart: EventHandler::new(move |_| {
                    core_restart.invoke_or_log(Command::ServiceRestart {
                        service: ServiceId::AgentServer,
                    });
                }),
            }))
            .child(settings_card(vec![
                subsection_heading(
                    "Default Agent",
                    Some("API keys stay inside AgentServer and never reach the UI."),
                ),
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(10.0)
                    .children(agent_cards)
                    .into_element(),
            ]))
            .child(settings_card(vec![
                subsection_heading(
                    "Side Panel Dock",
                    Some("Choose how Agent and Agents panels sit alongside web pages."),
                ),
                SettingsChoiceRow::new(
                    "Agent panel layout",
                    panel_layout_str(agent_prefs.layout),
                    vec![
                        ("float".to_string(), "Float".to_string()),
                        ("slide".to_string(), "Slide".to_string()),
                        ("split".to_string(), "Split".to_string()),
                    ],
                    move |val: String| {
                        core_agent_layout.invoke_or_log(Command::PanelSetLayout {
                            panel_id: SidePanelId::new(AGENT_PANEL_ID),
                            layout: parse_layout(&val),
                        });
                    },
                )
                .key("agent-panel-layout")
                .into_element(),
                SettingsChoiceRow::new(
                    "Agent panel side",
                    panel_side_str(agent_prefs.side),
                    vec![
                        ("left".to_string(), "Left".to_string()),
                        ("right".to_string(), "Right".to_string()),
                    ],
                    move |val: String| {
                        core_agent_side.invoke_or_log(Command::PanelSetSide {
                            panel_id: SidePanelId::new(AGENT_PANEL_ID),
                            side: parse_side(&val),
                        });
                    },
                )
                .key("agent-panel-side")
                .into_element(),
                SettingsChoiceRow::new(
                    "Agents roster layout",
                    panel_layout_str(agents_prefs.layout),
                    vec![
                        ("float".to_string(), "Float".to_string()),
                        ("slide".to_string(), "Slide".to_string()),
                        ("split".to_string(), "Split".to_string()),
                    ],
                    move |val: String| {
                        core_agents_layout.invoke_or_log(Command::PanelSetLayout {
                            panel_id: SidePanelId::new(AGENTS_PANEL_ID),
                            layout: parse_layout(&val),
                        });
                    },
                )
                .key("agents-panel-layout")
                .into_element(),
            ]))
            .child(settings_card(vec![
                subsection_heading("Automation & Instructions", None),
                SettingsToggleRow::new(
                    "Allow agents to focus tabs they drive",
                    browser_settings.agent_steal_focus,
                    move |next: bool| {
                        core_steal.invoke_or_log(Command::SettingsSet {
                            key: "agent.steal_focus".to_string(),
                            value: next.to_string(),
                        });
                    },
                )
                .description(
                    "When off, background agent tasks stay in their tabs without interrupting your current tab.",
                )
                .key("agent-steal-focus")
                .into_element(),
                SettingsToggleRow::new(
                    "Keep agents on their work list",
                    browser_settings.agent_loop_enabled,
                    move |next: bool| {
                        core_loop.invoke_or_log(Command::SettingsSet {
                            key: "agent.loop_enabled".to_string(),
                            value: next.to_string(),
                        });
                    },
                )
                .description("Prompt an agent to keep working until all items in its task list complete.")
                .key("agent-loop")
                .into_element(),
                settings_text_input_row(
                    "Hold-to-dictate key",
                    "Hold anywhere in the browser window to open the Agent panel and dictate (e.g. <A-Space>).",
                    dict_key_draft,
                    "<A-Space>",
                    palette,
                    move |val| {
                        core_dictation.invoke_or_log(Command::SettingsSet {
                            key: "agent.dictation_hold_key".to_string(),
                            value: val,
                        });
                    },
                ),
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(4.0, 8.0, 2.0, 8.0))
                    .direction(Direction::Horizontal)
                    .child(
                        SettingsButton::new(format!("Manage Instructions ({instruction_count} active)"), move |_| {
                            core_instr.invoke_or_log(Command::AgentOpenInstructions);
                        })
                        .icon("book")
                        .key("agent-instructions-btn"),
                    )
                    .into_element(),
            ]))
    }
}

#[derive(Clone, PartialEq)]
struct AgentSelectCard {
    agent: Agent,
    selected: bool,
    on_select: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for AgentSelectCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for AgentSelectCard {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let feedback = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_select = self.on_select.clone();
        let (r, g, b) = self.agent.accent_rgb;
        let accent = Rgba::rgb(r, g, b).to_color();

        let rest_bg = if self.selected {
            palette.selected_pill_bg
        } else {
            Color::TRANSPARENT
        };
        let bg = feedback.background(rest_bg, palette.row_hover_bg, palette.selected_pill_bg);
        let border = focus_ring_border(
            focus() == Focus::Keyboard,
            &theme,
            Border::new()
                .width(if self.selected { 1.5 } else { 1.0 })
                .alignment(BorderAlignment::Inner)
                .fill(if self.selected { accent } else { palette.card_border }),
        );

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Select {} agent", self.agent.name))
            .width(Size::px(280.0))
            .padding(Gaps::new(12.0, 14.0, 12.0, 14.0))
            .spacing(6.0)
            .corner_radius(CornerRadius::new_all(10.0))
            .background(bg)
            .border(border)
            .scale(feedback.scale())
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_press(move |_| on_select.call(()))
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if e.key == Key::Named(NamedKey::Enter) || matches!(&e.key, Key::Character(c) if c == " ") {
                    e.stop_propagation();
                }
            })
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::SpaceBetween)
                    .cross_align(Alignment::center())
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(8.0)
                            .child(
                                rect()
                                    .width(Size::px(10.0))
                                    .height(Size::px(10.0))
                                    .corner_radius(CornerRadius::new_all(999.0))
                                    .background(accent),
                            )
                            .child(
                                label()
                                    .text(self.agent.name.clone())
                                    .font_size(13.5)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(theme.color(ThemeRole::Text)),
                            ),
                    )
                    .maybe_child(self.selected.then(|| Pill::new("Default").tone(UiTone::Accent))),
            )
            .child(
                label()
                    .text(self.agent.description.clone())
                    .font_size(12.0)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Web Chat settings group: dock layout/side, site list, and add-custom-site form.
#[derive(Clone, PartialEq)]
pub struct WebChatSettingsGroup;

impl Component for WebChatSettingsGroup {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let mut custom_name = use_state(String::new);
        let mut custom_url = use_state(String::new);

        let (sites, active_site_id, prefs) = {
            let model = core.model().read();
            (
                model.chat_sites.clone(),
                model.active_chat_site_id.clone(),
                model.panel_prefs(&SidePanelId::new(WEBCHAT_PANEL_ID)),
            )
        };

        let core_layout = core.clone();
        let core_side = core.clone();
        let core_add = core.clone();

        let site_rows: Vec<Element> = sites
            .iter()
            .map(|site| {
                let sel_core = core.clone();
                let rem_core = core.clone();
                let sid_sel = site.id.clone();
                let sid_rem = site.id.clone();
                WebChatSiteRow {
                    site: site.clone(),
                    active: active_site_id.as_ref() == Some(&site.id),
                    on_select: EventHandler::new(move |_| {
                        sel_core.invoke_or_log(Command::WebChatSelectSite {
                            site_id: sid_sel.clone(),
                        });
                    }),
                    on_remove: EventHandler::new(move |_| {
                        rem_core.invoke_or_log(Command::WebChatRemoveSite {
                            site_id: sid_rem.clone(),
                        });
                    }),
                    key: DiffKey::from(&site.id.0),
                }
                .into_element()
            })
            .collect();

        rect()
            .width(Size::fill())
            .spacing(22.0)
            .child(section_heading(
                "Web Chat",
                Some("Configure the websites available in Zendia's web chat side panel."),
            ))
            .child(settings_card(vec![
                subsection_heading("Side Panel Dock", None),
                SettingsChoiceRow::new(
                    "Web chat panel layout",
                    panel_layout_str(prefs.layout),
                    vec![
                        ("float".to_string(), "Float".to_string()),
                        ("slide".to_string(), "Slide".to_string()),
                        ("split".to_string(), "Split".to_string()),
                    ],
                    move |val: String| {
                        core_layout.invoke_or_log(Command::PanelSetLayout {
                            panel_id: SidePanelId::new(WEBCHAT_PANEL_ID),
                            layout: parse_layout(&val),
                        });
                    },
                )
                .key("webchat-layout")
                .into_element(),
                SettingsChoiceRow::new(
                    "Web chat panel side",
                    panel_side_str(prefs.side),
                    vec![
                        ("left".to_string(), "Left".to_string()),
                        ("right".to_string(), "Right".to_string()),
                    ],
                    move |val: String| {
                        core_side.invoke_or_log(Command::PanelSetSide {
                            panel_id: SidePanelId::new(WEBCHAT_PANEL_ID),
                            side: parse_side(&val),
                        });
                    },
                )
                .key("webchat-side")
                .into_element(),
            ]))
            .child(settings_card(vec![
                subsection_heading(
                    "Configured Chat Sites",
                    Some("Chat sites load as regular websites using your browser cookies."),
                ),
                rect()
                    .width(Size::fill())
                    .spacing(4.0)
                    .children(site_rows)
                    .into_element(),
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(10.0, 0.0, 2.0, 0.0))
                    .spacing(8.0)
                    .child(
                        label()
                            .text("Add custom chat site")
                            .font_size(12.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .cross_align(Alignment::center())
                            .spacing(8.0)
                            .child(
                                rect()
                                    .width(Size::px(150.0))
                                    .height(Size::px(32.0))
                                    .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                                    .corner_radius(CornerRadius::new_all(7.0))
                                    .background(palette.input_bg)
                                    .border(
                                        Border::new()
                                            .width(1.0)
                                            .alignment(BorderAlignment::Inner)
                                            .fill(palette.input_border),
                                    )
                                    .main_align(Alignment::center())
                                    .child(
                                        TextField::new(custom_name)
                                            .placeholder("Name (optional)")
                                            .width(Size::fill())
                                            .padding(Gaps::new(0.0, 2.0, 0.0, 2.0))
                                            .background(Color::TRANSPARENT)
                                            .focus_background(Color::TRANSPARENT)
                                            .color(theme.color(ThemeRole::Text))
                                            .placeholder_color(theme.color(ThemeRole::TextMuted)),
                                    ),
                            )
                            .child(
                                rect()
                                    .width(Size::flex(1.0))
                                    .height(Size::px(32.0))
                                    .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                                    .corner_radius(CornerRadius::new_all(7.0))
                                    .background(palette.input_bg)
                                    .border(
                                        Border::new()
                                            .width(1.0)
                                            .alignment(BorderAlignment::Inner)
                                            .fill(palette.input_border),
                                    )
                                    .main_align(Alignment::center())
                                    .child(
                                        TextField::new(custom_url)
                                            .placeholder("https://chat.example.org")
                                            .width(Size::fill())
                                            .padding(Gaps::new(0.0, 2.0, 0.0, 2.0))
                                            .background(Color::TRANSPARENT)
                                            .focus_background(Color::TRANSPARENT)
                                            .color(theme.color(ThemeRole::Text))
                                            .placeholder_color(theme.color(ThemeRole::TextMuted))
                                            .on_submit({
                                                let core_submit = core_add.clone();
                                                move |raw_url: String| {
                                                    let url = raw_url.trim().to_string();
                                                    let name_raw = custom_name.peek().trim().to_string();
                                                    if !url.is_empty() {
                                                        let name = (!name_raw.is_empty()).then_some(name_raw);
                                                        if core_submit
                                                            .invoke_or_log(Command::WebChatAddSite { name, url })
                                                        {
                                                            custom_name.set(String::new());
                                                            custom_url.set(String::new());
                                                        }
                                                    }
                                                }
                                            }),
                                    ),
                            )
                            .child(
                                SettingsButton::new("Add", move |_| {
                                    let url = custom_url.peek().trim().to_string();
                                    let name_raw = custom_name.peek().trim().to_string();
                                    if !url.is_empty() {
                                        let name = (!name_raw.is_empty()).then_some(name_raw);
                                        if core_add.invoke_or_log(Command::WebChatAddSite { name, url }) {
                                            custom_name.set(String::new());
                                            custom_url.set(String::new());
                                        }
                                    }
                                })
                                .icon("plus")
                                .primary(true)
                                .key("add-webchat-btn"),
                            ),
                    )
                    .into_element(),
            ]))
    }
}

#[derive(Clone, PartialEq)]
struct WebChatSiteRow {
    site: ChatSite,
    active: bool,
    on_select: EventHandler<()>,
    on_remove: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for WebChatSiteRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for WebChatSiteRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let feedback = use_interactive_feedback(true);
        let on_select = self.on_select.clone();
        let on_remove = self.on_remove.clone();
        let dot_col = Rgba::from_hex(&self.site.accent_hex)
            .map(Rgba::to_color)
            .unwrap_or_else(|| theme.color(ThemeRole::Accent));

        rect()
            .width(Size::fill())
            .padding(Gaps::new(7.0, 10.0, 7.0, 10.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(feedback.background(Color::TRANSPARENT, palette.row_hover_bg, palette.row_hover_bg))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .spacing(10.0)
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .spacing(10.0)
                    .child(
                        rect()
                            .width(Size::px(10.0))
                            .height(Size::px(10.0))
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(dot_col),
                    )
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .spacing(1.0)
                            .child(
                                rect()
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(6.0)
                                    .child(
                                        label()
                                            .text(self.site.name.clone())
                                            .font_size(13.0)
                                            .font_weight(FontWeight::MEDIUM)
                                            .color(theme.color(ThemeRole::Text)),
                                    )
                                    .maybe_child(self.active.then(|| Pill::new("Active").tone(UiTone::Accent)))
                                    .maybe_child(
                                        self.site.is_custom.then(|| Pill::new("Custom").tone(UiTone::Neutral)),
                                    ),
                            )
                            .child(
                                label()
                                    .text(self.site.url.clone())
                                    .font_size(11.5)
                                    .color(theme.color(ThemeRole::TextMuted))
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis),
                            ),
                    ),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .maybe_child(
                        (!self.active)
                            .then(|| SettingsButton::new("Select", move |_| on_select.call(())).key("sel-site")),
                    )
                    .maybe_child(self.site.is_custom.then(|| {
                        SettingsButton::new("Remove", move |_| on_remove.call(()))
                            .danger(true)
                            .key("rm-site")
                    })),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn panel_layout_str(layout: SidePanelLayout) -> &'static str {
    match layout {
        SidePanelLayout::Float => "float",
        SidePanelLayout::Slide => "slide",
        SidePanelLayout::Split => "split",
    }
}

fn parse_layout(raw: &str) -> SidePanelLayout {
    match raw {
        "slide" => SidePanelLayout::Slide,
        "split" => SidePanelLayout::Split,
        _ => SidePanelLayout::Float,
    }
}

fn panel_side_str(side: SidePanelSide) -> &'static str {
    match side {
        SidePanelSide::Left => "left",
        SidePanelSide::Right => "right",
    }
}

fn parse_side(raw: &str) -> SidePanelSide {
    match raw {
        "left" => SidePanelSide::Left,
        _ => SidePanelSide::Right,
    }
}
