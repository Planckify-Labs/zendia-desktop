/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Adapter boundary between the browser core and a real tab engine (such as Ladybird's Hub).

use crate::commands::{CommandError, CommandResult, MediaAction};
use crate::model::TabId;

/// Receives tab and navigation commands from the core and forwards them to the engine.
pub trait TabSource: Send + Sync {
    fn new_tab(&self, url: Option<&str>, after: Option<TabId>, activate: bool) -> CommandResult;
    fn close_tab(&self, tab_id: TabId, force: bool) -> CommandResult;
    fn activate_tab(&self, tab_id: TabId) -> CommandResult;
    fn navigate_back(&self, tab_id: TabId) -> CommandResult;
    fn navigate_forward(&self, tab_id: TabId) -> CommandResult;
    fn reload(&self, tab_id: TabId, bypass_cache: bool) -> CommandResult;
    fn stop_loading(&self, tab_id: TabId) -> CommandResult;
    fn load_url(&self, tab_id: TabId, input: &str) -> CommandResult;

    // Context menu, select dropdown, and zoom actions routed to the engine when supported.
    fn inspect_element(&self, _tab_id: TabId) -> CommandResult {
        Err(CommandError::BackendUnavailable("inspect_element".to_string()))
    }
    fn cut_selection(&self, _tab_id: TabId) -> CommandResult {
        Err(CommandError::BackendUnavailable("cut_selection".to_string()))
    }
    fn copy_selection(&self, _tab_id: TabId) -> CommandResult {
        Err(CommandError::BackendUnavailable("copy_selection".to_string()))
    }
    fn paste(&self, _tab_id: TabId, _match_style: bool) -> CommandResult {
        Err(CommandError::BackendUnavailable("paste".to_string()))
    }
    fn select_all(&self, _tab_id: TabId) -> CommandResult {
        Err(CommandError::BackendUnavailable("select_all".to_string()))
    }
    fn view_source(&self, _tab_id: TabId) -> CommandResult {
        Err(CommandError::BackendUnavailable("view_source".to_string()))
    }
    fn replace_misspelling(&self, _tab_id: TabId, _replacement: &str) -> CommandResult {
        Err(CommandError::BackendUnavailable("replace_misspelling".to_string()))
    }
    fn ignore_misspelling(&self, _tab_id: TabId, _word: &str) -> CommandResult {
        Err(CommandError::BackendUnavailable("ignore_misspelling".to_string()))
    }
    fn learn_misspelling(&self, _tab_id: TabId, _word: &str) -> CommandResult {
        Err(CommandError::BackendUnavailable("learn_misspelling".to_string()))
    }
    fn media_action(&self, _tab_id: TabId, _action: MediaAction) -> CommandResult {
        Err(CommandError::BackendUnavailable("media_action".to_string()))
    }
    fn select_dropdown_choose(&self, _tab_id: TabId, _item_id: Option<u32>) -> CommandResult {
        Err(CommandError::BackendUnavailable("select_dropdown_choose".to_string()))
    }
    fn zoom_in(&self, _tab_id: TabId) -> CommandResult {
        Err(CommandError::BackendUnavailable("zoom_in".to_string()))
    }
    fn zoom_out(&self, _tab_id: TabId) -> CommandResult {
        Err(CommandError::BackendUnavailable("zoom_out".to_string()))
    }
    fn zoom_reset(&self, _tab_id: TabId) -> CommandResult {
        Err(CommandError::BackendUnavailable("zoom_reset".to_string()))
    }
    fn open_url_in_new_tab(&self, tab_id: TabId, url: &str, activate: bool) -> CommandResult {
        self.new_tab(Some(url), Some(tab_id), activate)
    }
}

/// Incremental field updates pushed into the core by a [`TabSource`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabUpdate {
    pub title: Option<String>,
    pub url: Option<String>,
    pub is_loading: Option<bool>,
    pub can_navigate_back: Option<bool>,
    pub can_navigate_forward: Option<bool>,
    pub is_audible: Option<bool>,
    pub is_muted: Option<bool>,
    pub closing: Option<bool>,
}
