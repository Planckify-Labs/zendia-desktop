/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Thin adapter bridging Ladybird's Hub messages and the browser core's tab model and commands.

use crate::hub::{self, startup};
use hub_protocol::ipc_types::hub_server::messages as server;
use hub_protocol::*;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use zendia_automation::{AutomationRequest, AutomationResponse};
use zendia_browser_core::BrowserCore;
use zendia_browser_core::address_bar::SearchEngine;
use zendia_browser_core::commands::{Command, CommandError, CommandResult, MediaAction};
use zendia_browser_core::model::{Tab, TabId, TabKind};
use zendia_browser_core::stub::StubBackend;
use zendia_browser_core::tab_source::{TabSource, TabUpdate};
use zendia_browser_core::vim_keys::{FieldEdit, VimCommand};

/// Per-view capability flags cached from `Hub::ViewState` for building context menus.
#[derive(Clone, Copy, Debug, Default)]
pub struct ViewCapabilities {
    pub can_view_source: bool,
    pub can_report_website_issue: bool,
    pub zoom_level: f32,
}

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
    capabilities: Mutex<HashMap<u64, ViewCapabilities>>,
    auxiliary_views: Mutex<HashSet<u64>>,
    auxiliary_loading: Mutex<HashMap<u64, bool>>,
    next_request_id: AtomicU64,
    /// Automation World requests sent without the runtime, kept until answered in case the page has none yet.
    automation_requests: Mutex<HashMap<u64, (u64, AutomationRequest)>>,
}

impl HubTabAdapter {
    pub fn new(core: Arc<StubBackend>, device_pixel_ratio: f64) -> Self {
        // Seed the browser core with the search engine Ladybird's settings configured so result providers and
        // address_bar.submit use the user's real default engine.
        let engine = SearchEngine::from_url(&startup().settings.search_url);
        core.set_search_engine(engine);
        Self {
            core,
            viewport: Mutex::new(ViewportState {
                size: (0, 0),
                device_pixel_ratio,
            }),
            capabilities: Mutex::new(HashMap::new()),
            auxiliary_views: Mutex::new(HashSet::new()),
            auxiliary_loading: Mutex::new(HashMap::new()),
            next_request_id: AtomicU64::new(1),
            automation_requests: Mutex::new(HashMap::new()),
        }
    }

    pub fn view_capabilities(&self, view_id: u64) -> ViewCapabilities {
        self.capabilities
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&view_id)
            .copied()
            .unwrap_or(ViewCapabilities {
                can_view_source: true,
                can_report_website_issue: false,
                zoom_level: 1.0,
            })
    }

    pub fn search_engine_name(&self) -> Option<String> {
        let name = self.core.model().search_engine.name;
        if name.is_empty() { None } else { Some(name) }
    }

    fn record_capabilities(&self, view_id: u64, state: &Hub_ViewState) {
        let caps = ViewCapabilities {
            can_view_source: state.can_view_source,
            can_report_website_issue: state.can_report_website_issue,
            zoom_level: if state.zoom_level > 0.0 {
                state.zoom_level as f32
            } else {
                1.0
            },
        };
        self.capabilities
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(view_id, caps);
        if self.core.model().active_tab == Some(TabId(view_id)) {
            self.core.feed_zoom_level(caps.zoom_level);
        }
    }

    pub fn has_view(&self, view_id: u64) -> bool {
        self.core.model().tab_by_view_id(view_id).is_some()
            || self
                .auxiliary_views
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .contains(&view_id)
    }

    /// Takes an Automation World reply from the Hub's embedder channel. A document that has no runtime yet gets the
    /// request again with it; any other answer goes to the core.
    pub fn on_embedder_reply(&self, view_id: u64, request_id: u64, result_json: &str) {
        let request = self
            .automation_requests
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&request_id);
        let response = match zendia_automation::parse_response(result_json) {
            Ok(response) => response,
            Err(error) => {
                eprintln!("zendia: unreadable automation reply: {error}");
                return;
            }
        };
        if response == AutomationResponse::NeedRuntime {
            if let Some((view_id, request)) = request {
                self.post_automation(view_id, request, true);
            }
            return;
        }
        self.core.feed_automation_response(TabId(view_id), response);
    }

    /// The page counted find in page's matches, or moved to one.
    pub fn on_find_in_page(&self, view_id: u64, current: usize, total: Option<usize>) {
        self.core.feed_find_result(TabId(view_id), current, total);
    }

    fn post_automation(&self, view_id: u64, request: AutomationRequest, with_runtime: bool) {
        let script = match zendia_automation::build_eval_script(&request, with_runtime) {
            Ok(script) => script,
            Err(error) => {
                eprintln!("zendia: failed to build automation script: {error}");
                return;
            }
        };
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        if !with_runtime {
            self.automation_requests
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(request_id, (view_id, request));
        }
        hub::connection().post_or_log(server::ViewEmbedderMessage {
            view_id,
            request_id,
            world: libipc_rust::wire::Utf8String::from(zendia_automation::AUTOMATION_WORLD_NAME),
            script: libipc_rust::wire::Utf8String::from(script),
        });
    }

    /// Creates a persistent non-tab Hub view (used by Web chat sites) and navigates it to `url`.
    ///
    /// Auxiliary views share the browser's cookies like any tab without appearing in the tab sidebar.
    pub fn create_auxiliary_view(&self, url: &str) -> Option<u64> {
        let (_, device_pixel_ratio) = self.viewport();
        let hub = hub::connection();
        let created = match hub.send_sync(server::CreateView {
            request: Hub_NewViewRequest {
                is_private: WebView_IsPrivate::No,
                device_pixel_ratio,
                maximum_frames_per_second: 60.0,
                display_id: None,
                screen_shows_high_dynamic_range: false,
                system_theme: crate::page_style::system_theme(),
                screen_rects: None,
                owner_view_id: None,
                websites_prefer_dark: false,
                is_tab: false,
            },
        }) {
            Ok(created) => created,
            Err(error) => {
                eprintln!("zendia: Unable to create web chat view for `{url}`: {error}");
                return None;
            }
        };

        let view_id = created.view_id;
        self.auxiliary_views
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(view_id);
        self.auxiliary_loading
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(view_id, true);
        self.set_up_view(view_id);
        self.record_capabilities(view_id, &created.state);
        load_in_hub(view_id, URL_URL::from_serialization(url.to_string()));
        Some(view_id)
    }

    /// Reloads an auxiliary Hub view (e.g. an active Web chat site).
    pub fn reload_auxiliary_view(&self, view_id: u64) {
        if !self.has_view(view_id) {
            return;
        }
        self.auxiliary_loading
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(view_id, true);
        hub::connection().post_or_log(server::ViewReload {
            view_id,
            bypass_cache: WebView_BypassCache::No,
        });
    }

    /// Navigates an auxiliary Hub view to `url` (e.g. starting a new chat at the site's home URL).
    pub fn navigate_auxiliary_view(&self, view_id: u64, url: &str) {
        if !self.has_view(view_id) {
            return;
        }
        self.auxiliary_loading
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(view_id, true);
        load_in_hub(view_id, URL_URL::from_serialization(url.to_string()));
    }

    /// Records loading state changes for auxiliary views so the web chat progress bar settles on load finish.
    pub fn note_auxiliary_loading(&self, view_id: u64, is_loading: bool) {
        let is_aux = self
            .auxiliary_views
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains(&view_id);
        if is_aux {
            self.auxiliary_loading
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(view_id, is_loading);
        }
    }

    /// Returns whether an auxiliary Hub view is currently loading.
    pub fn is_auxiliary_loading(&self, view_id: u64) -> bool {
        self.auxiliary_loading
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&view_id)
            .copied()
            .unwrap_or(false)
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
        hub.post_or_log(server::ViewSetUserStyle {
            view_id,
            source: libipc_rust::wire::Utf8String::from(crate::page_style::PAGE_USER_STYLE),
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
        self.record_capabilities(view_id, state);
        let should_activate = activate_tab || self.core.model().active_tab.is_none();
        let tab = Tab {
            id: TabId(view_id),
            view_id: Some(view_id),
            space_id: None,
            folder_id: None,
            split_group_id: None,
            kind: TabKind::Normal,
            title: state.title.to_string_lossy(),
            custom_title: None,
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
                    system_theme: crate::page_style::system_theme(),
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
        self.capabilities
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&view_id);
        hub::connection().post_or_log(server::DestroyView { view_id });
        if let Some(next) = self.core.feed_tab_closed(TabId(view_id)) {
            self.activate_view(next.0);
        }
    }

    pub fn on_view_state_changed(&self, view_id: u64, state: &Hub_ViewState) {
        self.record_capabilities(view_id, state);
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
        // NB: The page turns its input method on while a field has the focus, which is when vim mode types into it.
        let editing = state.input_method_state.is_enabled;
        if self.core.page_is_editing(TabId(view_id)) != editing {
            let _ = self.core.invoke(Command::Vim(VimCommand::EditableFocusChanged {
                tab_id: Some(TabId(view_id)),
                editing,
            }));
        }
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

/// A press of the primary button at `position`, as a user's click would make it.
fn click_event(r#type: Web_MouseEvent_Type, position: Web_DevicePixelPoint, held: bool) -> Web_MouseEvent {
    Web_MouseEvent {
        r#type,
        position: position.clone(),
        screen_position: position,
        button: if r#type == Web_MouseEvent_Type::MouseMove {
            Web_UIEvents_MouseButton::None
        } else {
            Web_UIEvents_MouseButton::Primary
        },
        buttons: if held {
            Web_UIEvents_MouseButton::Primary
        } else {
            Web_UIEvents_MouseButton::None
        },
        modifiers: Web_UIEvents_KeyModifier::Mod_None,
        wheel_delta_x: 0.0,
        wheel_delta_y: 0.0,
        wheel_tick_120ths_x: 0.0,
        wheel_tick_120ths_y: 0.0,
        wheel_delta_precision: Web_WheelDeltaPrecision::Discrete,
        scroll_gesture_phase: Web_ScrollGesturePhase::None,
        click_count: if r#type == Web_MouseEvent_Type::MouseMove { 0 } else { 1 },
        async_scroll_performed_default_action: false,
        id: 0,
        scrollbar_dragged_by_compositor: None,
        pointer_data: Web_PointerInputData { properties: None },
        wheel_gesture_mark: None,
    }
}

/// A Backspace press that runs `commands` in the page's focused field, as a platform's editing key binding would.
fn editing_key_event(
    is_down: bool,
    modifiers: Web_UIEvents_KeyModifier,
    commands: Vec<Web_TextEditingCommand>,
) -> Web_KeyEvent {
    Web_KeyEvent {
        r#type: if is_down {
            Web_KeyEvent_Type::KeyDown
        } else {
            Web_KeyEvent_Type::KeyUp
        },
        key: Web_UIEvents_KeyCode::Key_Backspace,
        modifiers,
        code_point: 0x08,
        repeat: false,
        should_insert_text: false,
        async_scroll_performed_default_action: false,
        user_input_scroll_count_with_nothing_to_scroll: None,
        id: 0,
        native_editing_binding_resolved: is_down,
        editing_commands: if is_down { commands } else { Vec::new() },
    }
}

impl TabSource for HubTabAdapter {
    fn click_at(&self, tab_id: TabId, x: f64, y: f64) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "click")?;
        let (_, device_pixel_ratio) = self.viewport();
        let position = Web_DevicePixelPoint {
            x: (x * device_pixel_ratio).round() as i32,
            y: (y * device_pixel_ratio).round() as i32,
        };
        for (r#type, held) in [
            (Web_MouseEvent_Type::MouseMove, false),
            (Web_MouseEvent_Type::MouseDown, true),
            (Web_MouseEvent_Type::MouseUp, false),
        ] {
            hub::connection().post_or_log(server::ViewEnqueueMouseEvent {
                view_id,
                input_id: 0,
                event: click_event(r#type, position.clone(), held),
            });
        }
        Ok(())
    }

    fn edit_field(&self, tab_id: TabId, edit: FieldEdit) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, edit.name())?;
        let command = |r#type, extend_selection| Web_TextEditingCommand {
            r#type,
            extend_selection,
        };
        let (modifiers, commands) = match edit {
            FieldEdit::DeleteWordBackward => (
                Web_UIEvents_KeyModifier::Mod_Ctrl,
                vec![command(Web_TextEditingCommand_Type::DeleteWordBackward, false)],
            ),
            FieldEdit::DeleteToLineStart => (
                Web_UIEvents_KeyModifier::Mod_None,
                vec![
                    command(Web_TextEditingCommand_Type::MoveToBeginningOfLine, true),
                    command(Web_TextEditingCommand_Type::DeleteBackward, false),
                ],
            ),
            FieldEdit::DeleteCharBackward => (
                Web_UIEvents_KeyModifier::Mod_None,
                vec![command(Web_TextEditingCommand_Type::DeleteBackward, false)],
            ),
        };
        for is_down in [true, false] {
            hub::connection().post_or_log(server::ViewEnqueueKeyEvent {
                view_id,
                input_id: 0,
                event: editing_key_event(is_down, modifiers, commands.clone()),
            });
        }
        Ok(())
    }

    fn run_automation(&self, tab_id: TabId, request: AutomationRequest) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "automation")?;
        self.post_automation(view_id, request, false);
        Ok(())
    }

    fn find_in_page(&self, tab_id: TabId, query: &str) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "page.find")?;
        hub::connection().post_or_log(server::ViewFindInPage {
            view_id,
            query: libipc_rust::wire::Utf16String::from(query),
            case_sensitivity: AK_CaseSensitivity::CaseInsensitive,
            highlight_all_matches: true,
        });
        Ok(())
    }

    fn find_in_page_step(&self, tab_id: TabId, backwards: bool) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "search.step")?;
        if backwards {
            hub::connection().post_or_log(server::ViewFindInPagePreviousMatch { view_id });
        } else {
            hub::connection().post_or_log(server::ViewFindInPageNextMatch { view_id });
        }
        Ok(())
    }

    fn find_in_page_end(&self, tab_id: TabId) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "page.find_end")?;
        hub::connection().post_or_log(server::ViewFindInPageEnd { view_id });
        Ok(())
    }

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

    fn inspect_element(&self, tab_id: TabId) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "page.inspect_element")?;
        hub::connection().post_or_log(server::ViewInspectContextMenuElement { view_id });
        Ok(())
    }

    fn cut_selection(&self, tab_id: TabId) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "page.cut")?;
        hub::connection().post_or_log(server::ViewCutSelection { view_id });
        Ok(())
    }

    fn copy_selection(&self, tab_id: TabId) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "page.copy")?;
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        hub::connection().post_or_log(server::ViewRequestSelectedText { view_id, request_id });
        Ok(())
    }

    fn paste(&self, tab_id: TabId, match_style: bool) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "page.paste")?;
        hub::connection().post_or_log(server::ViewPaste {
            view_id,
            paste_mode: if match_style {
                Web_Clipboard_PasteMode::PlainTextOnly
            } else {
                Web_Clipboard_PasteMode::AllMimeTypes
            },
        });
        Ok(())
    }

    fn select_all(&self, tab_id: TabId) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "page.select_all")?;
        hub::connection().post_or_log(server::ViewSelectAll { view_id });
        Ok(())
    }

    fn view_source(&self, tab_id: TabId) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "page.view_source")?;
        hub::connection().post_or_log(server::ViewGetSource { view_id });
        Ok(())
    }

    fn replace_misspelling(&self, tab_id: TabId, replacement: &str) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "page.replace_misspelling")?;
        hub::connection().post_or_log(server::ViewReplaceContextMenuMisspelling {
            view_id,
            replacement: libipc_rust::wire::Utf16String::from(replacement),
        });
        Ok(())
    }

    fn ignore_misspelling(&self, tab_id: TabId, word: &str) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "page.ignore_misspelling")?;
        hub::connection().post_or_log(server::ViewIgnoreMisspelling {
            view_id,
            word: libipc_rust::wire::Utf16String::from(word),
        });
        Ok(())
    }

    fn learn_misspelling(&self, tab_id: TabId, word: &str) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "page.learn_misspelling")?;
        hub::connection().post_or_log(server::ViewLearnMisspelling {
            view_id,
            word: libipc_rust::wire::Utf16String::from(word),
        });
        Ok(())
    }

    fn media_action(&self, tab_id: TabId, action: MediaAction) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "media.action")?;
        let ipc_action = match action {
            MediaAction::TogglePlay => WebView_ContextMenuMediaAction::TogglePlay,
            MediaAction::ToggleMute => WebView_ContextMenuMediaAction::ToggleMute,
            MediaAction::ToggleControls => WebView_ContextMenuMediaAction::ToggleControls,
            MediaAction::ToggleLoop => WebView_ContextMenuMediaAction::ToggleLoop,
            MediaAction::TogglePictureInPicture => WebView_ContextMenuMediaAction::TogglePictureInPicture,
            MediaAction::ToggleFullscreen => WebView_ContextMenuMediaAction::ToggleFullscreen,
        };
        hub::connection().post_or_log(server::ViewDoContextMenuMediaAction {
            view_id,
            action: ipc_action,
        });
        Ok(())
    }

    fn select_dropdown_choose(&self, tab_id: TabId, item_id: Option<u32>) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "page.select_dropdown_choose")?;
        hub::connection().post_or_log(server::ViewSelectDropdownClosed {
            view_id,
            selected_item_id: item_id,
        });
        Ok(())
    }

    fn zoom_in(&self, tab_id: TabId) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "zoom.in")?;
        hub::connection().post_or_log(server::ViewZoomIn { view_id });
        Ok(())
    }

    fn zoom_out(&self, tab_id: TabId) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "zoom.out")?;
        hub::connection().post_or_log(server::ViewZoomOut { view_id });
        Ok(())
    }

    fn zoom_reset(&self, tab_id: TabId) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "zoom.reset")?;
        hub::connection().post_or_log(server::ViewResetZoom { view_id });
        Ok(())
    }

    fn open_url_in_new_tab(&self, tab_id: TabId, url: &str, activate: bool) -> CommandResult {
        let view_id = self.view_id_for_tab(tab_id, "page.open_link_in_new_tab")?;
        let Some(parsed) = location::url_for_location(url, &startup().settings.search_url) else {
            return Err(CommandError::InvalidArgument {
                command: "page.open_link_in_new_tab".to_string(),
                message: "url is empty".to_string(),
            });
        };
        hub::connection().post_or_log(server::ViewOpenUrlInNewTab {
            view_id,
            url: parsed,
            activate_tab: if activate {
                Web_HTML_ActivateTab::Yes
            } else {
                Web_HTML_ActivateTab::No
            },
        });
        Ok(())
    }
}
