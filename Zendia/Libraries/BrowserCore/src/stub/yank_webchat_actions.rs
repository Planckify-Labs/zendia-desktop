/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for [`crate::yank_webchat_actions`].

use super::StubBackend;
use crate::BrowserCore;
use crate::commands::{Command, CommandResult};
use crate::events::ViewRequest;
use crate::model::{ChatSiteId, InputMode};
use crate::picker::{PickerItem, PickerLayout, PickerRequest};
use crate::yank_webchat_actions::YankWebChatAction;
use zendia_automation::AutomationRequest;

/// How much of a yank's first line the history shows.
const YANK_LABEL_MAX: usize = 100;

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then(|| host.to_string())
}

impl StubBackend {
    pub(super) fn record_yank(&self, text: &str) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let (url, title) = state
            .model
            .active_tab()
            .map(|tab| (Some(tab.url.clone()), Some(tab.display_title())))
            .unwrap_or((None, None));
        state.vim.yank_ring.add(text.to_string(), url, title);
    }

    pub(super) fn handle_yank_webchat_actions(&self, action: YankWebChatAction) -> CommandResult {
        match action {
            YankWebChatAction::YankHistory => self.handle_yank_history(),
            YankWebChatAction::YankPut { text, index } => self.ask_page(AutomationRequest::YankPut {
                text,
                index,
                replace: false,
            }),
            YankWebChatAction::YankClear => {
                self.with_yank_ring(|ring| ring.clear());
                self.show_toast("Yank history cleared.".to_string(), None, None);
                Ok(())
            }
            // Older yanks are further from the front of the ring.
            YankWebChatAction::YankPutPrev => self.ask_page(AutomationRequest::YankCycle { step: 1 }),
            YankWebChatAction::YankPutNext => self.ask_page(AutomationRequest::YankCycle { step: -1 }),
            YankWebChatAction::WebChatSelect => self.handle_webchat_select(),
            YankWebChatAction::WebChatNewChatAll => self.handle_webchat_new_chat_all(),
            YankWebChatAction::WebChatAskRegion => self.invoke(Command::RegionPick { tab_id: None }),
            YankWebChatAction::WebChatPutReply => {
                self.show_toast("No Web Chat reply to insert".to_string(), None, None);
                Ok(())
            }
            other => self.handle_webchat_prompt_action(other),
        }
    }

    fn handle_yank_history(&self) -> CommandResult {
        let items: Vec<PickerItem> = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state
                .vim
                .yank_ring
                .entries()
                .iter()
                .enumerate()
                .map(|(idx, entry)| {
                    // The first line with text, cut at 100 characters, and where it came from.
                    let line = entry
                        .text
                        .lines()
                        .find(|line| !line.trim().is_empty())
                        .unwrap_or("")
                        .trim();
                    let title = if line.chars().count() > YANK_LABEL_MAX {
                        let cut: String = line.chars().take(YANK_LABEL_MAX - 1).collect();
                        format!("{cut}\u{2026}")
                    } else {
                        line.to_string()
                    };
                    let mut item = PickerItem::new(format!("yank-{idx}"), title, "yank.put")
                        .arg("text", entry.text.clone())
                        .arg("index", idx.to_string());
                    if let Some(host) = entry.url.as_deref().and_then(host_of) {
                        item = item.detail(host);
                    }
                    item
                })
                .collect()
        };
        if items.is_empty() {
            self.show_toast("Nothing yanked yet.".to_string(), None, None);
            return Ok(());
        }
        let request = PickerRequest::new("yank-history", "Yank History", PickerLayout::List, items);
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.active_picker = Some(request.clone());
        }
        self.request_view(ViewRequest::PickerOpen { request });
        Ok(())
    }

    fn with_yank_ring(&self, change: impl FnOnce(&mut crate::yank_webchat_actions::YankRing)) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        change(&mut state.vim.yank_ring);
    }

    fn handle_webchat_select(&self) -> CommandResult {
        let items: Vec<PickerItem> = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state
                .model
                .chat_sites
                .iter()
                .map(|site| {
                    PickerItem::new(site.id.0.clone(), site.name.clone(), "webchat.open")
                        .detail(site.url.clone())
                        .arg("site_id", site.id.0.clone())
                })
                .collect()
        };
        if items.is_empty() {
            self.show_toast("No Web Chat sites configured".to_string(), None, None);
            return Ok(());
        }
        let request = PickerRequest::new("webchat-sites", "Select Web Chat Site", PickerLayout::List, items);
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.active_picker = Some(request.clone());
        }
        self.request_view(ViewRequest::PickerOpen { request });
        Ok(())
    }

    fn handle_webchat_new_chat_all(&self) -> CommandResult {
        let site_ids: Vec<ChatSiteId> = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.chat_sites.iter().map(|s| s.id.clone()).collect()
        };
        for site_id in site_ids {
            self.request_view(ViewRequest::WebChatNavigateHome { site_id });
        }
        Ok(())
    }

    fn handle_webchat_prompt_action(&self, action: YankWebChatAction) -> CommandResult {
        // Leave visual mode once the selection is handed to Web Chat, matching `leaveVisual`.
        let in_visual = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.input_mode == InputMode::Visual
        };
        if in_visual {
            self.set_input_mode(InputMode::Normal);
        }

        let (site_id, page_info) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let site_id = state
                .model
                .active_chat_site()
                .map(|s| s.id.clone())
                .or_else(|| state.model.chat_sites.first().map(|s| s.id.clone()));
            let page_info = state
                .model
                .active_tab()
                .map(|tab| format!("{} ({})", tab.display_title(), tab.url))
                .unwrap_or_default();
            (site_id, page_info)
        };
        let Some(site_id) = site_id else {
            self.show_toast("No Web Chat sites configured".to_string(), None, None);
            return Ok(());
        };
        let prefix = action.prompt_prefix().unwrap_or("About this page:");
        let prompt = if page_info.is_empty() {
            prefix.to_string()
        } else {
            format!("{prefix} {page_info}")
        };
        self.invoke(Command::WebChatAsk { site_id, prompt })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yank_history_and_webchat_select_open_pickers() {
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        core.record_yank("https://zendia.browser");
        core.invoke(Command::from_name_and_args("yank.history", &[]).unwrap())
            .unwrap();
        assert_eq!(core.active_picker().unwrap().id.as_str(), "yank-history");

        core.invoke(Command::from_name_and_args("webchat.select", &[]).unwrap())
            .unwrap();
        assert_eq!(core.active_picker().unwrap().id.as_str(), "webchat-sites");

        core.invoke(Command::from_name_and_args("webchat.ask", &[]).unwrap())
            .unwrap();
    }
}
