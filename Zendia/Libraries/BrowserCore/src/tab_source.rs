/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Adapter boundary between the browser core and a real tab engine (such as Ladybird's Hub).

use crate::commands::CommandResult;
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
