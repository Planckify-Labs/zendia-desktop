/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Thin adapter bridging Ladybird's Hub messages and the browser core's tab model and commands.

use crate::hub::{self, startup};
use hub_protocol::ipc_types::hub_server::messages as server;
use hub_protocol::*;
use std::sync::{Arc, Mutex};
use zendia_browser_core::BrowserCore;
use zendia_browser_core::commands::{CommandError, CommandResult};
use zendia_browser_core::model::{Tab, TabId, TabKind};
use zendia_browser_core::stub::StubBackend;
use zendia_browser_core::tab_source::{TabSource, TabUpdate};

#[derive(Clone, Copy, Debug)]
struct ViewportState {
    size: (i32, i32),
    device_pixel_ratio: f64,
}

/// Bridges the Hub IPC connection to [`StubBackend`]: forwards core tab/navigation commands to the Hub and translates
/// Hub view events into core tab updates.
pub struct HubTabAdapter {
    core: Arc<StubBackend>,
    viewport: Mutex<ViewportState>,
}

impl HubTabAdapter {
    pub fn new(core: Arc<StubBackend>, device_pixel_ratio: f64) -> Self {
        Self {
            core,
            viewport: Mutex::new(ViewportState {
                size: (0, 0),
                device_pixel_ratio,
            }),
        }
    }

    pub fn has_view(&self, view_id: u64) -> bool {
        self.core.model().tab_by_view_id(view_id).is_some()
    }

    pub fn viewport(&self) -> ((i32, i32), f64) {
        let vp = *self.viewport.lock().unwrap_or_else(|error| error.into_inner());
        (vp.size, vp.device_pixel_ratio)
    }

    /// Records the default page stage dimensions used when creating new Hub views before their [`PageSurface`]
    /// measures its own layout rect. Never broadcasts `ViewSetViewport` to existing views; each mounted surface
    /// manages its own viewport independently.
    pub fn set_default_viewport(&self, size: (i32, i32), device_pixel_ratio: f64) -> bool {
        let mut vp = self.viewport.lock().unwrap_or_else(|error| error.into_inner());
        if vp.size == size && vp.device_pixel_ratio == device_pixel_ratio {
            return false;
        }
        vp.size = size;
        vp.device_pixel_ratio = device_pixel_ratio;
        true
    }

    /// Opens the initial tabs passed from the Hub at startup and notifies the Hub that the UI started.
    pub fn open_initial_tabs(&self) {
        let urls = startup().initial_urls.clone();
        if urls.is_empty() {
            let _ = self.open_url_tab(None, None, true);
        }
        for url in urls {
            let _ = self.open_url_tab(Some(url), None, true);
        }
        hub::connection().post_or_log(server::UiDidStart);
    }

    fn set_up_view(&self, view_id: u64) {
        let (viewport, device_pixel_ratio) = self.viewport();
        let hub = hub::connection();
        hub.post_or_log(server::ViewSetPresentsFromCpuMemory {
            view_id,
            presents_from_cpu_memory: true,
        });
        hub.post_or_log(server::ViewSetHostsRemoteLayer {
            view_id,
            hosts_remote_layer: false,
        });
        if viewport.0 > 0 && viewport.1 > 0 {
            hub.post_or_log(server::ViewSetViewport {
                view_id,
                size: Web_DevicePixelSize {
                    width: viewport.0,
                    height: viewport.1,
                },
                device_pixel_ratio,
            });
        }
        hub.post_or_log(server::ViewSetSystemVisibilityState {
            view_id,
            visibility_state: Web_HTML_VisibilityState::Hidden,
        });
        hub.post_or_log(server::ViewSetCurrentTabOfWindow {
            view_id,
            current_tab_of_window: false,
        });
    }

    /// Makes a Hub view the active tab in both the Hub and the browser core.
    ///
    /// Visibility (`ViewSetSystemVisibilityState`) and `ViewSetCurrentTabOfWindow` are managed per mounted
    /// [`PageSurface`], so activating one pane in a split view does not hide sibling panes that remain on screen.
    pub fn activate_view(&self, view_id: u64) {
        hub::connection().post_or_log(server::ActiveViewChanged { view_id: Some(view_id) });
        self.core.feed_tab_activated(Some(TabId(view_id)));
    }

    /// Adopts an existing Hub view into the core's tab list, placed after `after` when given.
    pub fn adopt_view(&self, view_id: u64, state: &Hub_ViewState, after: Option<TabId>, activate_tab: bool) {
        if self.has_view(view_id) {
            return;
        }
        self.set_up_view(view_id);
        let should_activate = activate_tab || self.core.model().active_tab.is_none();
        let tab = Tab {
            id: TabId(view_id),
            view_id: Some(view_id),
            space_id: None,
            folder_id: None,
            split_group_id: None,
            kind: TabKind::Normal,
            title: state.title.to_string_lossy(),
            url: state.displayed_url.as_str().to_string(),
            favicon: None,
            is_loading: state.is_loading,
            can_navigate_back: state.can_navigate_back,
            can_navigate_forward: state.can_navigate_forward,
            is_audible: false,
            is_muted: false,
            closing: false,
        };
        self.core.feed_tab_opened(tab, after, false);
        if should_activate {
            self.activate_view(view_id);
        }
    }

    /// Creates a new Hub view, adopts it as a tab, and loads `url` (or the configured new tab page).
    pub fn open_url_tab(&self, url: Option<URL_URL>, after: Option<TabId>, activate_tab: bool) -> CommandResult {
        let (_, device_pixel_ratio) = self.viewport();
        let hub = hub::connection();
        let created = hub
            .send_sync(server::CreateView {
                request: Hub_NewViewRequest {
                    is_private: WebView_IsPrivate::No,
                    device_pixel_ratio,
                    maximum_frames_per_second: 60.0,
                    display_id: None,
                    screen_shows_high_dynamic_range: false,
                    system_theme: Core_AnonymousBuffer::default(),
                    screen_rects: None,
                    owner_view_id: None,
                    websites_prefer_dark: false,
                    is_tab: true,
                },
            })
            .map_err(|error| {
                eprintln!("zendia: Unable to open a tab: {error}");
                CommandError::Failed(format!("unable to create Hub view: {error}"))
            })?;

        self.adopt_view(created.view_id, &created.state, after, activate_tab);
        let url = url.unwrap_or_else(|| URL_URL::from_serialization(startup().settings.new_tab_page_url.clone()));
        load_in_hub(created.view_id, url);
        Ok(())
    }

    /// Destroys a closed Hub view, removes the tab from the core, and activates the next tab if needed.
    pub fn finish_close_tab(&self, view_id: u64) {
        if !self.has_view(view_id) {
            return;
        }
        hub::connection().post_or_log(server::DestroyView { view_id });
        if let Some(next) = self.core.feed_tab_closed(TabId(view_id)) {
            self.activate_view(next.0);
        }
    }

    pub fn on_view_state_changed(&self, view_id: u64, state: &Hub_ViewState) {
        self.core.feed_tab_updated(
            TabId(view_id),
            TabUpdate {
                title: Some(state.title.to_string_lossy()),
                url: Some(state.displayed_url.as_str().to_string()),
                is_loading: Some(state.is_loading),
                can_navigate_back: Some(state.can_navigate_back),
                can_navigate_forward: Some(state.can_navigate_forward),
                ..TabUpdate::default()
            },
        );
    }

    pub fn on_title_changed(&self, view_id: u64, title: String) {
        self.core.feed_tab_updated(
            TabId(view_id),
            TabUpdate {
                title: Some(title),
                ..TabUpdate::default()
            },
        );
    }

    pub fn on_url_changed(&self, view_id: u64, url: String) {
        self.core.feed_tab_updated(
            TabId(view_id),
            TabUpdate {
                url: Some(url),
                ..TabUpdate::default()
            },
        );
    }

    pub fn on_loading_state_changed(&self, view_id: u64, is_loading: bool) {
        self.core.feed_tab_updated(
            TabId(view_id),
            TabUpdate {
                is_loading: Some(is_loading),
                ..TabUpdate::default()
            },
        );
    }

    fn view_id_for_tab(&self, tab_id: TabId, command: &str) -> Result<u64, CommandError> {
        self.core
            .model()
            .tab(tab_id)
            .and_then(|tab| tab.view_id)
            .ok_or_else(|| CommandError::TargetNotFound {
                command: command.to_string(),
                target: format!("Hub view for tab {}", tab_id.0),
            })
    }
}

fn load_in_hub(view_id: u64, url: URL_URL) {
    hub::connection().post_or_log(server::ViewLoad {
        view_id,
        url,
        history_behavior: Web_Bindings_NavigationHistoryBehavior::Auto,
    });
}

impl TabSource for HubTabAdapter {
    fn new_tab(&self, url: Option<&str>, after: Option<TabId>, activate: bool) -> CommandResult {
        let parsed = url.and_then(|input| location::url_for_location(input, &startup().settings.search_url));
        self.open_url_tab(parsed, after, activate)
    }

    fn close_tab(&self, tab_id: TabId, force: bool) -> CommandResult {
        let model = self.core.model();
        let Some(tab) = model.tab(tab_id) else {
            return Err(CommandError::TargetNotFound {
                command: "tab.close".to_string(),
                target: format!("tab {}", tab_id.0),
            });
        };
        let Some(view_id) = tab.view_id else {
            return Err(CommandError::TargetNotFound {
                command: "tab.close".to_string(),
                target: format!("Hub view for tab {}", tab_id.0),
            });
        };
        if force || tab.closing {
            // NB: A page that does not answer is closed without asking it again.
            self.finish_close_tab(view_id);
        } else {
            self.core.feed_tab_updated(
                tab_id,
                TabUpdate {
                    closing: Some(true),
                    ..TabUpdate::default()
                },
            );
            // NB: The page may ask the user whether to leave first. Once it may close, the Hub tells us to close the tab.
            hub::connection().post_or_log(server::ViewRequestClose { view_id });
        }
        Ok(())
    }

    fn activate_tab(&self, tab_id: TabId) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "tab.activate")?;
        self.activate_view(view_id);
        Ok(())
    }

    fn navigate_back(&self, tab_id: TabId) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "navigation.back")?;
        hub::connection().post_or_log(server::ViewTraverseTheHistoryByDelta { view_id, delta: -1 });
        Ok(())
    }

    fn navigate_forward(&self, tab_id: TabId) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "navigation.forward")?;
        hub::connection().post_or_log(server::ViewTraverseTheHistoryByDelta { view_id, delta: 1 });
        Ok(())
    }

    fn reload(&self, tab_id: TabId, bypass_cache: bool) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "navigation.reload")?;
        hub::connection().post_or_log(server::ViewReload {
            view_id,
            bypass_cache: if bypass_cache {
                WebView_BypassCache::Yes
            } else {
                WebView_BypassCache::No
            },
        });
        Ok(())
    }

    fn stop_loading(&self, tab_id: TabId) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "navigation.stop")?;
        hub::connection().post_or_log(server::ViewStopLoading { view_id });
        Ok(())
    }

    fn load_url(&self, tab_id: TabId, input: &str) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "navigation.load_url")?;
        let Some(url) = location::url_for_location(input, &startup().settings.search_url) else {
            return Err(CommandError::InvalidArgument {
                command: "navigation.load_url".to_string(),
                message: "location input is empty".to_string(),
            });
        };
        load_in_hub(view_id, url);
        Ok(())
    }
}
