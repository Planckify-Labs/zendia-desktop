/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 8 gallery section: Web chat side panel, site strip, accounts menu, add custom site, Ask submenu, and
//! dictation listening indicator.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::menus::{OpenContextMenu, try_use_menu_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::{KitChip, Pill};
use crate::ui::page::{PageArea, PaneMode};
use crate::ui::panels::try_use_dock_state;
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::BrowserCore;
use zendia_browser_core::agent::SidePanelPrefs;
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{SidePanelId, SidePanelLayout, SidePanelSide};
use zendia_browser_core::menus::build_tab_context_menu;
use zendia_browser_core::model::{BrowserModel, ChatSiteId};
use zendia_browser_core::webchat::{WEBCHAT_DEFAULT_WIDTH, WEBCHAT_PANEL_ID};
use zendia_module_api::ui::UiTone;

/// Gallery section showcasing all Phase 8 Web chat states.
#[derive(Clone)]
pub struct WebChatSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
    key: DiffKey,
}

impl PartialEq for WebChatSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

impl KeyExt for WebChatSection {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl WebChatSection {
    pub fn new(state_id: impl Into<String>, gallery_core: Arc<GalleryCore>) -> Self {
        Self {
            state_id: state_id.into(),
            gallery_core,
            key: DiffKey::None,
        }
    }
}

fn build_fixture_model(state_id: &str) -> BrowserModel {
    let mut model = super::gallery_sample_model();
    model.active_chat_site_id = Some(ChatSiteId::new("chatgpt"));
    model.webchat_dictating = state_id == "dictation";
    if state_id == "dictation" {
        model.webchat_pending_prompt = Some((ChatSiteId::new("chatgpt"), "Summarize this page in English".to_string()));
    }

    let prefs = SidePanelPrefs {
        width: WEBCHAT_DEFAULT_WIDTH,
        side: SidePanelSide::Right,
        layout: if state_id == "panel-split" {
            SidePanelLayout::Split
        } else {
            SidePanelLayout::Float
        },
        ..Default::default()
    };
    model.side_panel_prefs.insert(SidePanelId::new(WEBCHAT_PANEL_ID), prefs);
    model
}

impl Component for WebChatSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let menu_state = try_use_menu_state();
        let location = use_state(|| "https://zendia.local/docs".to_string());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut applied_state = use_state(String::new);

        if *applied_state.peek() != self.state_id {
            applied_state.set(self.state_id.clone());
            let mut fixture = build_fixture_model(&self.state_id);
            fixture.theme = self.gallery_core.model().theme;
            let active_tab = fixture.active_tab;
            let tab_menu_items = active_tab.map(|tid| build_tab_context_menu(&fixture, tid, None));
            self.gallery_core.stub().replace_model(fixture);

            if let Some(mut ds) = dock_state {
                let webchat_id = SidePanelId::new(WEBCHAT_PANEL_ID);
                if self.state_id == "ask-submenu" {
                    ds.open_panels.set(Vec::new());
                    ds.focused_panel.set(None);
                } else {
                    ds.open_panels.set(vec![webchat_id.clone()]);
                    ds.focused_panel.set(Some(webchat_id));
                }
                ds.model_card_open.set(false);
                ds.instructions_open.set(false);
                ds.webchat_accounts_open.set(self.state_id == "account-menu");
                ds.webchat_add_site_open.set(self.state_id == "add-site");
            }

            if let Some(ms) = menu_state {
                if self.state_id == "ask-submenu"
                    && let Some(items) = tab_menu_items
                {
                    ms.open_context_menu(OpenContextMenu {
                        position: (310.0, 290.0),
                        items,
                        tab_id: active_tab,
                        open_submenu: Some("zendia.tab.ask".to_string()),
                    });
                } else {
                    ms.close_all();
                }
            }
        }

        let core_toggle = core.clone();
        let core_ask = core.clone();
        let core_dict = core.clone();
        let is_open = dock_state.is_some_and(|ds| ds.is_webchat_open());
        let is_dictating = core.model().read().webchat_dictating;
        let status_label = match self.state_id.as_str() {
            "panel-split" => "Split layout · site strip beside page",
            "account-menu" => "Accounts menu · shared browser cookies",
            "add-site" => "Add custom site · inline URL input",
            "ask-submenu" => "Tab context menu · Ask ChatGPT submenu",
            "dictation" => "Voice dictation · listening indicator",
            _ => "Float layout · 8-site strip + waiting emblem",
        };

        let controls = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .child(Pill::new(status_label).tone(UiTone::Accent))
            .child(
                KitChip::new(if is_open { "Close Web Chat" } else { "Open Web Chat" })
                    .icon("webchat")
                    .selected(is_open)
                    .on_press(move |_| {
                        core_toggle.invoke_or_log(Command::WebChatToggle);
                    }),
            )
            .child(KitChip::new("Send Ask Prompt").icon("sparkles").on_press(move |_| {
                core_ask.invoke_or_log(Command::WebChatAsk {
                    site_id: ChatSiteId::new("chatgpt"),
                    prompt: "Summarize this page in English".to_string(),
                });
            }))
            .child(
                KitChip::new(if is_dictating {
                    "Stop Dictation"
                } else {
                    "Toggle Dictation"
                })
                .icon("mic")
                .selected(is_dictating)
                .on_press(move |_| {
                    core_dict.invoke_or_log(Command::WebChatSetDictating {
                        listening: !is_dictating,
                    });
                }),
            );

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
