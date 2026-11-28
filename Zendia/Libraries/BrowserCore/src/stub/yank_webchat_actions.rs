/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for [`crate::yank_webchat_actions`].

use super::StubBackend;
use crate::BrowserCore;
use crate::commands::{Command, CommandResult};
use crate::contributions::SidePanelId;
use crate::events::{Event, ViewRequest};
use crate::model::{ChatSiteId, InputMode, TabId};
use crate::picker::{PickerItem, PickerLayout, PickerRequest};
use crate::webchat::WEBCHAT_PANEL_ID;
use crate::yank_webchat_actions::{YankRing, YankWebChatAction, format_yank_age, now_unix_ms};
use zendia_automation::AutomationRequest;

/// How much of a yank's first line the history shows.
const YANK_LABEL_MAX: usize = 100;

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then(|| host.to_string())
}

impl StubBackend {
    /// Seeds the yank ring from disk on startup (`YNK-01`).
    pub fn set_yank_ring(&self, ring: YankRing) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.vim.yank_ring = ring;
    }

    /// Clones the current yank ring snapshot for debounced persistence (`YNK-01`).
    pub fn yank_ring_snapshot(&self) -> YankRing {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .vim
            .yank_ring
            .clone()
    }

    /// Marks the window or `tab_id` as private browsing so yanks and clipboard sync are excluded (`YNK-04`).
    pub fn set_private_browsing(&self, is_private: bool) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.is_private = is_private;
    }

    /// Adds external system clipboard text to the yank ring unless in a private window (`YNK-03`, `YNK-04`).
    pub fn sync_clipboard_to_yank_ring(&self, text: &str) {
        let changed = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.is_private {
                false
            } else {
                state.vim.yank_ring.add(text.to_string(), None, None)
            }
        };
        if changed {
            self.bus.publish(Event::YankRingChanged);
        }
    }

    pub(super) fn record_yank(&self, text: &str) {
        let tab_id = self.effective_keyed_tab();
        let changed = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.is_private {
                return;
            }
            let (url, title) = tab_id
                .and_then(|id| state.model.tab(id))
                .map(|tab| (Some(tab.url.clone()), Some(tab.display_title())))
                .unwrap_or((None, None));
            state.vim.yank_ring.add(text.to_string(), url, title)
        };
        if changed {
            self.bus.publish(Event::YankRingChanged);
        }
    }

    pub(super) fn record_yank_from_page(&self, text: &str, tab_id: TabId, host: Option<String>) {
        let changed = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.is_private {
                return;
            }
            let (url, title) = state
                .model
                .tab(tab_id)
                .map(|tab| (Some(tab.url.clone()), Some(tab.display_title())))
                .unwrap_or_else(|| {
                    let fallback_url = host.filter(|h| !h.is_empty()).map(|h| format!("https://{h}/"));
                    (fallback_url, None)
                });
            state.vim.yank_ring.add(text.to_string(), url, title)
        };
        if changed {
            self.bus.publish(Event::YankRingChanged);
        }
    }

    pub(super) fn handle_yank_webchat_actions(&self, action: YankWebChatAction) -> CommandResult {
        match action {
            YankWebChatAction::YankHistory => self.handle_yank_history(),
            YankWebChatAction::YankPut { text, index } => {
                if self.current_tab_source().is_some() && self.effective_keyed_tab().is_some() {
                    self.ask_page(AutomationRequest::YankPut {
                        text,
                        index,
                        replace: false,
                    })
                } else {
                    self.bus.publish(Event::ClipboardCopyRequested { text });
                    self.show_toast("Copied to the clipboard.".to_string(), None, None);
                    Ok(())
                }
            }
            YankWebChatAction::YankClear => {
                let mut changed = false;
                self.with_yank_ring(|ring| {
                    changed = ring.clear();
                });
                if changed {
                    self.bus.publish(Event::YankRingChanged);
                }
                self.show_toast("Yank history cleared.".to_string(), None, None);
                Ok(())
            }
            // Older yanks are further from the front of the ring.
            YankWebChatAction::YankPutPrev => self.ask_page(AutomationRequest::YankCycle { step: 1 }),
            YankWebChatAction::YankPutNext => self.ask_page(AutomationRequest::YankCycle { step: -1 }),
            YankWebChatAction::YankUrlMarkdown => {
                let md = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state
                        .model
                        .effective_keyed_tab()
                        .and_then(|tid| state.model.tab(tid))
                        .or_else(|| state.model.active_tab())
                        .filter(|tab| !tab.url.is_empty())
                        .map(|tab| format!("[{}]({})", tab.display_title(), tab.url))
                };
                if let Some(text) = md {
                    self.record_yank(&text);
                    self.bus.publish(Event::ClipboardCopyRequested { text });
                    self.show_toast(crate::toasts::messages::YANK_COPIED.to_string(), None, None);
                }
                Ok(())
            }
            YankWebChatAction::WebChatOpen { site_id } => {
                if let Some(raw_id) = site_id {
                    self.handle_webchat_select_site(ChatSiteId::new(raw_id))?;
                }
                self.invoke(Command::PanelOpen {
                    panel_id: SidePanelId::new(WEBCHAT_PANEL_ID),
                    focus: true,
                })
            }
            YankWebChatAction::WebChatSelect => self.handle_webchat_select(),
            YankWebChatAction::WebChatAccounts => {
                self.invoke(Command::PanelOpen {
                    panel_id: SidePanelId::new(WEBCHAT_PANEL_ID),
                    focus: true,
                })?;
                self.request_view(ViewRequest::WebChatAccountsToggle);
                Ok(())
            }
            YankWebChatAction::WebChatSync => self.invoke(Command::WebChatReload { site_id: None }),
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
        if let Some(reader) = self
            .clipboard_reader
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            && let Some(clip) = reader()
        {
            self.sync_clipboard_to_yank_ring(&clip);
        }
        let now_ms = now_unix_ms();
        let items: Vec<PickerItem> = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state
                .vim
                .yank_ring
                .entries()
                .iter()
                .enumerate()
                .map(|(idx, entry)| {
                    // The first line with text, cut at 100 characters, and `host · age` (`YNK-05`).
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
                    let age = format_yank_age(now_ms, entry.time_ms);
                    let detail = match entry.url.as_deref().and_then(host_of) {
                        Some(host) => format!("{host} \u{00b7} {age}"),
                        None => age,
                    };
                    PickerItem::new(format!("yank-{idx}"), title, "yank.put")
                        .detail(detail)
                        .arg("text", entry.text.clone())
                        .arg("index", idx.to_string())
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
        if let Some(tab_id) = self.effective_keyed_tab() {
            let in_visual = {
                let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                state
                    .vim
                    .pages
                    .get(&tab_id)
                    .is_some_and(|p| p.mode == InputMode::Visual)
            };
            if in_visual {
                self.set_page_mode_for_tab(tab_id, InputMode::Normal);
            }
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

    #[test]
    fn yank_ring_persists_excludes_private_syncs_clipboard_and_formats_age() {
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();

        // Private window exclusion (`YNK-04`).
        core.set_private_browsing(true);
        core.record_yank("secret-token");
        core.sync_clipboard_to_yank_ring("private-clip");
        assert!(core.yank_ring_snapshot().entries().is_empty());

        // Normal window + external clipboard sync on `yank.history` (`YNK-03`, `YNK-05`).
        core.set_private_browsing(false);
        core.record_yank("first line\nsecond line");
        core.attach_clipboard_reader(std::sync::Arc::new(|| Some("system clipboard text".to_string())));
        core.invoke(Command::from_name_and_args("yank.history", &[]).unwrap())
            .unwrap();
        let picker = core.active_picker().expect("yank history picker");
        assert_eq!(picker.items.len(), 2);
        assert_eq!(picker.items[0].title, "system clipboard text");
        assert_eq!(picker.items[0].detail.as_deref(), Some("now"));
        assert_eq!(picker.items[1].title, "first line");
        assert!(
            picker.items[1]
                .detail
                .as_deref()
                .unwrap_or("")
                .ends_with("\u{00b7} now")
        );

        // JSON persistence round-trip (`YNK-01`).
        let tmp = std::env::temp_dir().join(format!("zendia-yank-test-{}.json", std::process::id()));
        let snap = core.yank_ring_snapshot();
        crate::yank_webchat_actions::save_yank_ring_to(&tmp, &snap).unwrap();
        let loaded = crate::yank_webchat_actions::load_yank_ring_from(&tmp).expect("loaded yank ring");
        let _ = std::fs::remove_file(&tmp);
        assert_eq!(loaded.entries().len(), 2);
        assert_eq!(loaded.entries()[0].text, "system clipboard text");

        // `webchat.open` selects site and opens panel (`AGK-08`).
        let first_site = core.model().chat_sites[0].id.0.clone();
        core.invoke(Command::from_name_and_args("webchat.open", &[("site_id".into(), first_site.clone())]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_chat_site_id.unwrap().0, first_site);
    }
}
