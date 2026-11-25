/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Persistent per-site Hub view registry for the Web chat panel.
//!
//! Each chat site gets its own real Hub view when first selected and keeps it alive across site switches so in-flight
//! replies, drafts, and scroll position stay intact. In `zendia-gallery` (where no Hub is connected), the registry
//! carries `None` and the Web chat panel renders its native waiting/emblem state instead.

use crate::hub_adapter::HubTabAdapter;
use freya::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use zendia_browser_core::model::{ChatSite, ChatSiteId};

/// Window-local registry mapping each [`ChatSiteId`] to its persistent Hub `view_id`.
#[derive(Clone, Copy)]
pub struct WebChatViewRegistry {
    adapter: State<Option<Arc<HubTabAdapter>>>,
    views: State<HashMap<ChatSiteId, u64>>,
}

impl PartialEq for WebChatViewRegistry {
    fn eq(&self, other: &Self) -> bool {
        self.views == other.views
    }
}

impl WebChatViewRegistry {
    /// Returns the existing Hub `view_id` for `site`, lazily creating and navigating it on first selection when a live
    /// Hub adapter is available.
    pub fn ensure_view_for_site(&mut self, site: &ChatSite) -> Option<u64> {
        if let Some(view_id) = self.views.peek().get(&site.id).copied() {
            return Some(view_id);
        }
        let adapter = self.adapter.peek().clone()?;
        let view_id = adapter.create_auxiliary_view(&site.url)?;
        let mut map = self.views.peek().clone();
        map.insert(site.id.clone(), view_id);
        self.views.set(map);
        Some(view_id)
    }

    /// Returns the currently allocated `view_id` for `site_id` without creating one.
    pub fn view_for_site(&self, site_id: &ChatSiteId) -> Option<u64> {
        self.views.read().get(site_id).copied()
    }

    /// Returns whether the live Hub view for `site_id` is currently loading.
    pub fn is_loading(&self, site_id: &ChatSiteId) -> bool {
        let Some(view_id) = self.views.read().get(site_id).copied() else {
            return true;
        };
        let Some(adapter) = self.adapter.read().clone() else {
            return true;
        };
        adapter.is_auxiliary_loading(view_id)
    }

    /// Reloads the live Hub view for `site_id`, if created.
    pub fn reload_site(&self, site_id: &ChatSiteId) {
        let Some(view_id) = self.views.peek().get(site_id).copied() else {
            return;
        };
        if let Some(adapter) = self.adapter.peek().clone() {
            adapter.reload_auxiliary_view(view_id);
        }
    }

    /// Navigates the live Hub view for `site` back to its home URL for a fresh conversation.
    pub fn navigate_home(&self, site: &ChatSite) {
        let Some(view_id) = self.views.peek().get(&site.id).copied() else {
            return;
        };
        if let Some(adapter) = self.adapter.peek().clone() {
            adapter.navigate_auxiliary_view(view_id, &site.url);
        }
    }
}

/// Installs [`WebChatViewRegistry`] into Freya context for the current window.
pub fn use_provide_webchat_views(adapter: Option<Arc<HubTabAdapter>>) -> WebChatViewRegistry {
    let adapter_state = use_state(move || adapter);
    let views_state = use_state(HashMap::<ChatSiteId, u64>::new);
    let registry = WebChatViewRegistry {
        adapter: adapter_state,
        views: views_state,
    };
    use_provide_context(move || registry);
    registry
}

/// Retrieves [`WebChatViewRegistry`] if installed in the current Freya tree.
pub fn try_use_webchat_views() -> Option<WebChatViewRegistry> {
    try_consume_context::<WebChatViewRegistry>()
}
