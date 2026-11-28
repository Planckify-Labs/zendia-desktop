/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Supervision and IPC adapter for the sandboxed `zendia-extension-server` process.
//!
//! Spawns `zendia-extension-server` with two inherited socket pairs: one to this UI process (`ZENDIA_UI_FD`) and one
//! passed straight to Ladybird's Hub via `HubServer::connect_embedder_channel("extensions", fd)` so WebContent's
//! synchronous extension seams bypass the UI thread completely.

use crate::hub::{self, startup};
use crate::hub_adapter::HubTabAdapter;
use base64::Engine as _;
use hub_protocol::URL_URL;
use hub_protocol::ipc_types::hub_server::messages as server;
use libipc_rust::wire::{File, Utf8String};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zendia_browser_core::BrowserCore;
use zendia_browser_core::commands::{Command, CommandError, CommandResult};
use zendia_browser_core::contributions::ContributionId;
use zendia_browser_core::extension_source::ExtensionSource;
use zendia_browser_core::history::HistoryClearRange;
use zendia_browser_core::model::{BrowserModel, ServiceStatus, Tab, TabId, TabKind, TabMoveTarget};
use zendia_browser_core::modules::{
    ExtensionCommand, ExtensionContextMenuItem, ExtensionIcon, ExtensionId, ExtensionInfo, ExtensionInstall,
    ExtensionInstallStage, ExtensionPrompt, ExtensionPromptDetail, ExtensionPromptSubject,
};
use zendia_browser_core::stub::StubBackend;
use zendia_service::{Frame, ServiceConnection, find_service_executable, spawn_service, verify_service_handshake};

const EXTENSION_SERVICE_NAME: &str = "extension_server";
const EXTENSION_PROTOCOL_VERSION: u32 = 1;
const EXTENSION_BIN_NAME: &str = "zendia-extension-server";
const MAX_UNPACKED_TOTAL_BYTES: usize = 50 * 1024 * 1024;
const MAX_UNPACKED_FILE_COUNT: usize = 2000;

/// Events pushed from `ExtensionServer` that need UI-thread execution (popups, `host.*` tab mutations, crashes).
#[derive(Clone, Debug)]
pub enum ExtensionUiEvent {
    OpenPopup {
        extension_id: ExtensionId,
        popup_url: String,
    },
    PopupSize {
        view_id: u64,
        width: f32,
        height: f32,
    },
    PopupReady {
        view_id: u64,
    },
    ClosePopup {
        view_id: u64,
    },
    HostCall {
        token: u64,
        extension_id: String,
        function: String,
        args: Value,
    },
    ServiceDisconnected {
        reason: String,
    },
}

struct LiveService {
    conn: Option<Arc<ServiceConnection>>,
    child: Option<Child>,
    shutting_down: bool,
    generation: u64,
}

/// Manages the `zendia-extension-server` child process and bridges it to [`StubBackend`] and [`HubTabAdapter`].
pub struct ExtensionAdapter {
    core: Arc<StubBackend>,
    hub_adapter: Arc<HubTabAdapter>,
    tx_ui: async_channel::Sender<ExtensionUiEvent>,
    rx_ui: async_channel::Receiver<ExtensionUiEvent>,
    live: Mutex<LiveService>,
}

impl ExtensionAdapter {
    pub fn new(core: Arc<StubBackend>, hub_adapter: Arc<HubTabAdapter>) -> Arc<Self> {
        let (tx_ui, rx_ui) = async_channel::unbounded();
        Arc::new(Self {
            core,
            hub_adapter,
            tx_ui,
            rx_ui,
            live: Mutex::new(LiveService {
                conn: None,
                child: None,
                shutting_down: false,
                generation: 0,
            }),
        })
    }

    pub fn receiver(&self) -> async_channel::Receiver<ExtensionUiEvent> {
        self.rx_ui.clone()
    }

    /// Spawns (or respawns) `zendia-extension-server`, hands its embedder socket to Hub, verifies the version
    /// handshake, sends `init`, and pushes the current browser state.
    pub fn start(self: &Arc<Self>) -> Result<(), String> {
        self.stop_child(false);
        self.core.feed_extension_service_status(ServiceStatus::Starting);

        let exe_path = find_extension_server_executable()?;
        let spawned = spawn_service(&exe_path, &[], true)
            .map_err(|e| format!("failed to spawn `{}`: {e}", exe_path.display()))?;
        let mut child = spawned.child;
        let ui_stream = spawned.ui_stream;
        let Some(hub_fd) = spawned.embedder_hub_fd else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("spawn_service did not return an embedder Hub socket".to_string());
        };

        // Hand the server's Hub socket directly to Hub before initializing extensions so background pages and
        // WebContent hooks can talk to ExtensionServer immediately without crossing the UI thread.
        hub::connection().post_or_log(server::ConnectEmbedderChannel {
            name: Utf8String::from("extensions"),
            socket: File(hub_fd),
        });

        let generation = {
            let mut live = self.live.lock().unwrap_or_else(|e| e.into_inner());
            live.generation = live.generation.wrapping_add(1);
            live.generation
        };

        let weak_self = Arc::downgrade(self);
        let conn = ServiceConnection::start(
            ui_stream,
            "zendia-ext-ui",
            {
                let weak_self = weak_self.clone();
                move |frame| {
                    if let Some(adapter) = weak_self.upgrade() {
                        adapter.on_service_event(frame);
                    }
                }
            },
            move || {
                if let Some(adapter) = weak_self.upgrade() {
                    adapter.on_service_disconnected(generation);
                }
            },
        )
        .map_err(|e| {
            let _ = child.kill();
            let _ = child.wait();
            format!("failed to attach ExtensionServer socket: {e}")
        })?;

        if let Err(err) = verify_service_handshake(&conn, EXTENSION_SERVICE_NAME, EXTENSION_PROTOCOL_VERSION) {
            let _ = child.kill();
            let _ = child.wait();
            let reason = format!("ExtensionServer handshake failed: {err}");
            self.core.feed_extension_service_status(ServiceStatus::Stopped {
                reason: Some(reason.clone()),
            });
            return Err(reason);
        }

        {
            let mut live = self.live.lock().unwrap_or_else(|e| e.into_inner());
            live.conn = Some(Arc::clone(&conn));
            live.child = Some(child);
        }

        let startup_info = startup();
        let init_payload = json!({
            "op": "init",
            "dataDir": startup_info.data_directory.to_string_lossy(),
            "locale": startup_info.locale,
            "loadExtensions": startup_info.load_extensions,
            "autoAcceptPrompts": startup_info.auto_accept_extension_prompts,
        });
        if let Err(err) = conn.request_sync(init_payload, Vec::new(), Duration::from_secs(10)) {
            let reason = format!("ExtensionServer init failed: {err}");
            self.stop_child(false);
            self.core.feed_extension_service_status(ServiceStatus::Stopped {
                reason: Some(reason.clone()),
            });
            return Err(reason);
        }

        self.core.feed_extension_service_status(ServiceStatus::Running);
        self.push_browser_state();
        Ok(())
    }

    pub fn shutdown(&self) {
        self.stop_child(true);
    }

    fn stop_child(&self, permanent: bool) {
        let mut live = self.live.lock().unwrap_or_else(|e| e.into_inner());
        if permanent {
            live.shutting_down = true;
        }
        live.conn = None;
        if let Some(mut child) = live.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn connection(&self) -> Result<Arc<ServiceConnection>, CommandError> {
        self.live
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .conn
            .clone()
            .ok_or_else(|| CommandError::BackendUnavailable("ExtensionServer is not running".to_string()))
    }

    fn on_service_disconnected(&self, generation: u64) {
        let should_notify = {
            let mut live = self.live.lock().unwrap_or_else(|e| e.into_inner());
            if live.shutting_down || live.generation != generation {
                false
            } else {
                live.conn = None;
                if let Some(mut child) = live.child.take() {
                    let _ = child.try_wait();
                }
                true
            }
        };
        if should_notify {
            let reason = "ExtensionServer process exited unexpectedly".to_string();
            self.core.feed_extension_prompt(None);
            self.core.feed_extension_service_status(ServiceStatus::Stopped {
                reason: Some(reason.clone()),
            });
            let _ = self
                .tx_ui
                .send_blocking(ExtensionUiEvent::ServiceDisconnected { reason });
        }
    }

    fn on_service_event(&self, frame: Frame) {
        let Some(op) = frame.op() else {
            return;
        };
        match op {
            "ui_state" => {
                if let Some(snapshot) = frame.json.get("snapshot") {
                    let (extensions, commands) = parse_ui_snapshot(snapshot);
                    self.core.feed_extensions_snapshot(extensions, commands);
                }
            }
            "prompt" => {
                let token = frame.json.get("token").and_then(Value::as_u64).unwrap_or(0);
                let title = frame
                    .json
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("Extension Request")
                    .to_string();
                let message = frame
                    .json
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let details = frame
                    .json
                    .get("details")
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().filter_map(parse_prompt_detail).collect())
                    .unwrap_or_default();
                let subject = frame.json.get("subject").and_then(Value::as_object).map(|subject| {
                    let text = |key: &str| subject.get(key).and_then(Value::as_str).unwrap_or("").to_string();
                    ExtensionPromptSubject {
                        name: text("name"),
                        version: text("version"),
                        source: text("source"),
                        icon: None,
                    }
                });
                let accept_label = frame
                    .json
                    .get("acceptLabel")
                    .and_then(Value::as_str)
                    .unwrap_or("Allow")
                    .to_string();
                self.core.feed_extension_prompt(Some(ExtensionPrompt {
                    token,
                    title,
                    message,
                    details,
                    accept_label,
                    subject,
                }));
            }
            "prompt_icon" => {
                let token = frame.json.get("token").and_then(Value::as_u64).unwrap_or(0);
                if let Some(icon) = parse_icon_data(Some(&frame.json)) {
                    self.core.feed_extension_prompt_icon(token, icon);
                }
            }
            "install_progress" => self.feed_install_progress(&frame.json),
            "open_popup" => {
                let ext_id = frame
                    .json
                    .get("extensionId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let popup_url = frame
                    .json
                    .get("popupUrl")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if !ext_id.is_empty() && !popup_url.is_empty() {
                    let _ = self.tx_ui.send_blocking(ExtensionUiEvent::OpenPopup {
                        extension_id: ExtensionId::new(ext_id),
                        popup_url,
                    });
                }
            }
            "popup_request" => {
                let view_id = frame.json.get("viewId").and_then(Value::as_u64).unwrap_or(0);
                let name = frame.json.get("name").and_then(Value::as_str).unwrap_or("");
                let args_json = frame.json.get("argsJson").and_then(Value::as_str).unwrap_or("[]");
                match name {
                    "__ui.popupSize" => {
                        if let Ok(Value::Array(args)) = serde_json::from_str::<Value>(args_json) {
                            let w = args.first().and_then(Value::as_f64).unwrap_or(320.0) as f32;
                            let h = args.get(1).and_then(Value::as_f64).unwrap_or(240.0) as f32;
                            let _ = self.tx_ui.send_blocking(ExtensionUiEvent::PopupSize {
                                view_id,
                                width: w.clamp(25.0, 800.0),
                                height: h.clamp(25.0, 600.0),
                            });
                        }
                    }
                    "__ui.popupReady" => {
                        let _ = self.tx_ui.send_blocking(ExtensionUiEvent::PopupReady { view_id });
                    }
                    "__ui.closePopup" => {
                        let _ = self.tx_ui.send_blocking(ExtensionUiEvent::ClosePopup { view_id });
                    }
                    _ => {}
                }
            }
            "host_call" => {
                let token = frame.json.get("token").and_then(Value::as_u64).unwrap_or(0);
                let extension_id = frame
                    .json
                    .get("extensionId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let function = frame
                    .json
                    .get("function")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let args = frame.json.get("args").cloned().unwrap_or_else(|| json!({}));
                let _ = self.tx_ui.send_blocking(ExtensionUiEvent::HostCall {
                    token,
                    extension_id,
                    function,
                    args,
                });
            }
            _ => {}
        }
    }

    /// Tells `ExtensionServer` (and Hub) whether `view_id` is hosting an extension popup.
    pub fn set_popup_view(&self, view_id: u64, is_popup: bool) {
        if let Ok(conn) = self.connection() {
            let _ = conn.request_sync(
                json!({
                    "op": "set_popup_view",
                    "viewId": view_id,
                    "isPopup": is_popup,
                }),
                Vec::new(),
                Duration::from_secs(2),
            );
        }
    }

    fn extension_tab_id_of(&self, tab_id: TabId) -> i64 {
        self.core
            .model()
            .tab(tab_id)
            .map(extension_tab_id)
            .unwrap_or((VIEWLESS_TAB_ID_BASE + tab_id.0) as i64)
    }

    /// Shows `ExtensionServer`'s report on an install as the install card.
    fn feed_install_progress(&self, json: &Value) {
        let install_id = json.get("installId").and_then(Value::as_u64).unwrap_or(0);
        let text = |key: &str| json.get(key).and_then(Value::as_str).unwrap_or("").to_string();
        let stage = match json.get("stage").and_then(Value::as_str).unwrap_or("") {
            "downloading" => ExtensionInstallStage::Downloading,
            "checking" => ExtensionInstallStage::Checking,
            "installing" => ExtensionInstallStage::Installing,
            "installed" => ExtensionInstallStage::Installed {
                updated: json.get("updated").and_then(Value::as_bool).unwrap_or(false),
            },
            "failed" => ExtensionInstallStage::Failed {
                reason: text("reason"),
                can_retry: json.get("canRetry").and_then(Value::as_bool).unwrap_or(false),
            },
            // Cancelled, or waiting on the install prompt, which takes the card's place.
            _ => {
                let current = self.core.model().extension_install;
                if current.is_some_and(|install| install.install_id == install_id) {
                    self.core.feed_extension_install(None);
                }
                return;
            }
        };
        let extension_id = Some(text("extensionId"))
            .filter(|id| !id.is_empty())
            .map(ExtensionId::new);
        self.core.feed_extension_install(Some(ExtensionInstall {
            install_id,
            extension_id,
            name: text("name"),
            source: text("source"),
            icon: parse_icon_data(json.get("icon")),
            stage,
        }));
    }

    /// Pushes the current window and tab snapshot from `BrowserCore` to `ExtensionServer`.
    pub fn push_browser_state(&self) {
        let Ok(conn) = self.connection() else {
            return;
        };
        let state = self.build_browser_state_json();
        let _ = conn.send_event(json!({
            "op": "browser_state",
            "state": state,
        }));
    }

    fn build_browser_state_json(&self) -> Value {
        let model = self.core.model();
        let window_id: i64 = model.windows.first().map(|w| w.id.0 as i64).unwrap_or(1);
        let active_tab = model.active_tab;

        let tabs: Vec<Value> = model
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| tab_to_json(tab, index, window_id, active_tab, &self.hub_adapter))
            .collect();

        // cgx-webext reads a window's tabs from the window itself.
        json!({
            "windows": [{
                "id": window_id,
                "focused": true,
                "incognito": false,
                "type": "normal",
                "state": "normal",
                "tabs": tabs,
            }],
            "lastFocusedWindowId": window_id,
        })
    }

    /// Executes an extension `host.*` API call on the UI thread and sends `host_call_reply` back to `ExtensionServer`.
    pub fn handle_host_call(&self, token: u64, extension_id: &str, function: &str, args: &Value) {
        let result = self.dispatch_host_call(extension_id, function, args);
        if let Ok(conn) = self.connection() {
            match result {
                Ok(data) => {
                    let _ = conn.send_event(json!({
                        "op": "host_call_reply",
                        "token": token,
                        "ok": true,
                        "data": data,
                    }));
                }
                Err(error) => {
                    let _ = conn.send_event(json!({
                        "op": "host_call_reply",
                        "token": token,
                        "ok": false,
                        "error": error,
                    }));
                }
            }
        }
    }

    fn dispatch_host_call(&self, extension_id: &str, function: &str, args: &Value) -> Result<Value, String> {
        let window_id: i64 = self.core.model().windows.first().map(|w| w.id.0 as i64).unwrap_or(1);

        match function {
            "host.tabs.create" => {
                let url = args
                    .get("url")
                    .and_then(Value::as_str)
                    .filter(|u| !u.is_empty())
                    .map(|u| URL_URL::from_serialization(u.to_string()));
                let active = args.get("active").and_then(Value::as_bool).unwrap_or(true);
                let tab_id = self
                    .hub_adapter
                    .open_url_tab_returning_id(url, None, active)
                    .map_err(|e| e.to_string())?;
                if args.get("pinned").and_then(Value::as_bool) == Some(true) {
                    let _ = self.core.invoke(Command::TabPin { tab_id: Some(tab_id) });
                }
                self.push_browser_state();
                let model = self.core.model();
                let tab_json = model
                    .tabs
                    .iter()
                    .enumerate()
                    .find(|(_, t)| t.id == tab_id)
                    .map(|(i, t)| tab_to_json(t, i, window_id, model.active_tab, &self.hub_adapter))
                    .unwrap_or_else(|| json!({ "id": self.extension_tab_id_of(tab_id) }));
                Ok(tab_json)
            }
            "host.tabs.update" => {
                let target_id = resolve_tab_id(&self.core, args)?;
                if let Some(url) = args.get("url").and_then(Value::as_str) {
                    self.core
                        .invoke(Command::NavigationLoadUrl {
                            tab_id: Some(target_id),
                            input: url.to_string(),
                        })
                        .map_err(|e| e.to_string())?;
                }
                if args.get("active").and_then(Value::as_bool) == Some(true) {
                    self.core
                        .invoke(Command::TabActivate { tab_id: target_id })
                        .map_err(|e| e.to_string())?;
                }
                if let Some(pinned) = args.get("pinned").and_then(Value::as_bool) {
                    let is_pinned = self
                        .core
                        .model()
                        .tab(target_id)
                        .is_some_and(|t| matches!(t.kind, TabKind::Pinned | TabKind::Essential));
                    if pinned != is_pinned {
                        let _ = self.core.invoke(Command::TabPin {
                            tab_id: Some(target_id),
                        });
                    }
                }
                if let Some(muted) = args.get("muted").and_then(Value::as_bool) {
                    let is_muted = self.core.model().tab(target_id).is_some_and(|t| t.is_muted);
                    if muted != is_muted {
                        let _ = self.core.invoke(Command::TabToggleMute {
                            tab_id: Some(target_id),
                        });
                    }
                }
                self.push_browser_state();
                let model = self.core.model();
                let tab_json = model
                    .tabs
                    .iter()
                    .enumerate()
                    .find(|(_, t)| t.id == target_id)
                    .map(|(i, t)| tab_to_json(t, i, window_id, model.active_tab, &self.hub_adapter))
                    .unwrap_or_else(|| json!({ "id": self.extension_tab_id_of(target_id) }));
                Ok(tab_json)
            }
            "host.tabs.remove" => {
                let mut ids = Vec::new();
                let model = self.core.model();
                if let Some(arr) = args.get("tabIds").and_then(Value::as_array) {
                    ids.extend(
                        arr.iter()
                            .filter_map(Value::as_u64)
                            .filter_map(|id| tab_for_extension_id(&model, id)),
                    );
                } else if let Some(id) = args.get("tabId").and_then(Value::as_u64) {
                    ids.extend(tab_for_extension_id(&model, id));
                }
                drop(model);
                for tab_id in ids {
                    let _ = self.core.invoke(Command::TabClose {
                        tab_id: Some(tab_id),
                        force: true,
                    });
                }
                self.push_browser_state();
                Ok(Value::Null)
            }
            "host.tabs.reload" => {
                let target_id = resolve_tab_id(&self.core, args)?;
                let bypass_cache = args.get("bypassCache").and_then(Value::as_bool).unwrap_or(false);
                self.core
                    .invoke(Command::NavigationReload {
                        tab_id: Some(target_id),
                        bypass_cache,
                    })
                    .map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            "host.tabs.navigateHistory" => {
                let target_id = resolve_tab_id(&self.core, args)?;
                let delta = args.get("delta").and_then(Value::as_i64).unwrap_or(0);
                let steps = Some((delta.unsigned_abs().min(u64::from(u32::MAX)) as u32).max(1));
                if delta < 0 {
                    let _ = self.core.invoke(Command::NavigationBack {
                        tab_id: Some(target_id),
                        count: steps,
                    });
                } else if delta > 0 {
                    let _ = self.core.invoke(Command::NavigationForward {
                        tab_id: Some(target_id),
                        count: steps,
                    });
                }
                Ok(Value::Null)
            }
            "host.tabs.duplicate" => {
                let target_id = resolve_tab_id(&self.core, args)?;
                let url = self
                    .core
                    .model()
                    .tab(target_id)
                    .map(|t| t.url.clone())
                    .filter(|u| !u.is_empty())
                    .map(URL_URL::from_serialization);
                let new_id = self
                    .hub_adapter
                    .open_url_tab_returning_id(url, Some(target_id), true)
                    .map_err(|e| e.to_string())?;
                self.push_browser_state();
                let model = self.core.model();
                let tab_json = model
                    .tabs
                    .iter()
                    .enumerate()
                    .find(|(_, t)| t.id == new_id)
                    .map(|(i, t)| tab_to_json(t, i, window_id, model.active_tab, &self.hub_adapter))
                    .unwrap_or_else(|| json!({ "id": self.extension_tab_id_of(new_id) }));
                Ok(tab_json)
            }
            "host.tabs.move" => {
                let mut ids = Vec::new();
                let model = self.core.model();
                if let Some(arr) = args.get("tabIds").and_then(Value::as_array) {
                    ids.extend(
                        arr.iter()
                            .filter_map(Value::as_u64)
                            .filter_map(|id| tab_for_extension_id(&model, id)),
                    );
                } else if let Some(id) = args.get("tabId").and_then(Value::as_u64) {
                    ids.extend(tab_for_extension_id(&model, id));
                }
                drop(model);
                let space_id = self.core.model().active_space;
                let index = args.get("index").and_then(Value::as_i64).unwrap_or(0).max(0) as usize;
                for (offset, tab_id) in ids.iter().enumerate() {
                    let _ = self.core.invoke(Command::TabMove {
                        tab_id: *tab_id,
                        target: TabMoveTarget::Normal {
                            space_id,
                            index: index.saturating_add(offset),
                        },
                    });
                }
                self.push_browser_state();
                let model = self.core.model();
                let moved: Vec<Value> = ids
                    .iter()
                    .filter_map(|id| {
                        model
                            .tabs
                            .iter()
                            .enumerate()
                            .find(|(_, t)| t.id == *id)
                            .map(|(i, t)| tab_to_json(t, i, window_id, model.active_tab, &self.hub_adapter))
                    })
                    .collect();
                Ok(Value::Array(moved))
            }
            "host.tabs.setZoom" => {
                let target_id = args
                    .get("tabId")
                    .and_then(Value::as_u64)
                    .and_then(|id| tab_for_extension_id(&self.core.model(), id));
                let factor = args.get("zoomFactor").and_then(Value::as_f64).unwrap_or(1.0);
                if (factor - 1.0).abs() < 0.01 || factor <= 0.0 {
                    let _ = self.core.invoke(Command::ZoomReset { tab_id: target_id });
                } else if factor > 1.0 {
                    let _ = self.core.invoke(Command::ZoomIn { tab_id: target_id });
                } else {
                    let _ = self.core.invoke(Command::ZoomOut { tab_id: target_id });
                }
                self.push_browser_state();
                Ok(Value::Null)
            }
            "host.tabs.capture" => Err("tabs.captureVisibleTab is not yet implemented".to_string()),
            "host.windows.create" => {
                let mut urls = Vec::new();
                if let Some(url) = args.get("url").and_then(Value::as_str) {
                    urls.push(url.to_string());
                } else if let Some(arr) = args.get("url").and_then(Value::as_array) {
                    for u in arr.iter().filter_map(Value::as_str) {
                        urls.push(u.to_string());
                    }
                }
                let mut opened = Vec::new();
                if urls.is_empty() {
                    if let Ok(id) = self.hub_adapter.open_url_tab_returning_id(None, None, true) {
                        opened.push(id);
                    }
                } else {
                    for (idx, url) in urls.into_iter().enumerate() {
                        if let Ok(id) = self.hub_adapter.open_url_tab_returning_id(
                            Some(URL_URL::from_serialization(url)),
                            None,
                            idx == 0,
                        ) {
                            opened.push(id);
                        }
                    }
                }
                self.push_browser_state();
                let model = self.core.model();
                let tab_values: Vec<Value> = opened
                    .iter()
                    .filter_map(|id| {
                        model
                            .tabs
                            .iter()
                            .enumerate()
                            .find(|(_, t)| t.id == *id)
                            .map(|(i, t)| tab_to_json(t, i, window_id, model.active_tab, &self.hub_adapter))
                    })
                    .collect();
                Ok(json!({
                    "id": window_id,
                    "focused": true,
                    "incognito": false,
                    "type": "normal",
                    "state": "normal",
                    "tabs": tab_values,
                }))
            }
            "host.windows.update" => Ok(json!({
                "id": window_id,
                "focused": true,
                "incognito": false,
                "type": "normal",
                "state": "normal",
            })),
            "host.windows.remove" => Ok(Value::Null),
            "host.action.openPopup" => {
                let ext_id = ExtensionId::new(extension_id.to_string());
                let popup_url = self
                    .core
                    .model()
                    .extension(&ext_id)
                    .and_then(|e| e.popup_url.clone())
                    .ok_or_else(|| "Extension has no popup configured".to_string())?;
                self.core.open_extension_popup(ext_id, popup_url);
                Ok(Value::Null)
            }
            "host.history.search" => {
                let query = args
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let max_results = args.get("maxResults").and_then(Value::as_u64).unwrap_or(100) as usize;
                let items: Vec<Value> = self
                    .core
                    .model()
                    .history
                    .iter()
                    .filter(|entry| {
                        query.is_empty()
                            || entry.url.to_ascii_lowercase().contains(&query)
                            || entry.title.to_ascii_lowercase().contains(&query)
                    })
                    .take(max_results)
                    .map(|entry| {
                        json!({
                            "id": entry.id.0.to_string(),
                            "url": entry.url,
                            "title": entry.title,
                            "lastVisitTime": (entry.visited_at_unix as f64) * 1000.0,
                            "visitCount": entry.visit_count,
                        })
                    })
                    .collect();
                Ok(Value::Array(items))
            }
            "host.history.deleteUrl" => {
                if let Some(url) = args.get("url").and_then(Value::as_str) {
                    let matching: Vec<_> = self
                        .core
                        .model()
                        .history
                        .iter()
                        .filter(|e| e.url == url)
                        .map(|e| e.id)
                        .collect();
                    for entry_id in matching {
                        let _ = self.core.invoke(Command::HistoryDelete { entry_id });
                    }
                }
                Ok(Value::Null)
            }
            "host.history.deleteAll" | "host.history.deleteRange" => {
                let _ = self.core.invoke(Command::HistoryClear {
                    range: HistoryClearRange::All,
                });
                Ok(Value::Null)
            }
            "host.bookmarks.get" | "host.bookmarks.getTree" | "host.bookmarks.search" => {
                let query = args
                    .get("query")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let items: Vec<Value> = self
                    .core
                    .model()
                    .bookmarks
                    .iter()
                    .filter(|b| {
                        query.is_empty()
                            || b.url.to_ascii_lowercase().contains(&query)
                            || b.title.to_ascii_lowercase().contains(&query)
                    })
                    .map(|b| {
                        json!({
                            "id": b.id.0.to_string(),
                            "parentId": "1",
                            "title": b.title,
                            "url": b.url,
                        })
                    })
                    .collect();
                if function == "host.bookmarks.getTree" {
                    Ok(json!([{
                        "id": "0",
                        "title": "",
                        "children": [{
                            "id": "1",
                            "parentId": "0",
                            "title": "Bookmarks",
                            "children": items,
                        }]
                    }]))
                } else {
                    Ok(Value::Array(items))
                }
            }
            "host.search.query" => {
                let text = args.get("text").and_then(Value::as_str).unwrap_or("");
                if text.is_empty() {
                    return Err("search.query requires non-empty text".to_string());
                }
                let search_url = self.core.model().search_engine.search_url_for(text);
                let disposition = args.get("disposition").and_then(Value::as_str).unwrap_or("CURRENT_TAB");
                if disposition == "NEW_TAB" || disposition == "NEW_WINDOW" {
                    let _ = self
                        .hub_adapter
                        .open_url_tab(Some(URL_URL::from_serialization(search_url)), None, true);
                } else {
                    let _ = self.core.invoke(Command::NavigationLoadUrl {
                        tab_id: None,
                        input: search_url,
                    });
                }
                Ok(Value::Null)
            }
            "host.fontSettings.getFontList" => Ok(json!([
                { "fontId": "sans-serif", "displayName": "Sans Serif" },
                { "fontId": "serif", "displayName": "Serif" },
                { "fontId": "monospace", "displayName": "Monospace" },
            ])),
            _ => Err(format!("Unsupported host call `{function}`")),
        }
    }
}

impl Drop for ExtensionAdapter {
    fn drop(&mut self) {
        self.stop_child(true);
    }
}

impl ExtensionSource for ExtensionAdapter {
    fn invoke_action(&self, extension_id: &ExtensionId) -> CommandResult {
        let conn = self.connection()?;
        let active_tab = self.core.model().active_tab.map(|t| self.extension_tab_id_of(t));
        let mut payload = json!({
            "op": "action_clicked",
            "extensionId": extension_id.as_str(),
        });
        if let Some(tab_id) = active_tab {
            payload["tabId"] = json!(tab_id);
        }
        let reply = conn
            .request_sync(payload, Vec::new(), Duration::from_secs(5))
            .map_err(|e| CommandError::Failed(format!("action_clicked failed: {e}")))?;
        if let Some(popup_url) = reply
            .json
            .get("data")
            .and_then(|d| d.get("popup"))
            .and_then(Value::as_str)
            && !popup_url.is_empty()
        {
            self.core
                .open_extension_popup(extension_id.clone(), popup_url.to_string());
        }
        Ok(())
    }

    fn set_enabled(&self, extension_id: &ExtensionId, enabled: bool) -> CommandResult {
        let conn = self.connection()?;
        conn.send_event(json!({
            "op": "set_enabled",
            "extensionId": extension_id.as_str(),
            "enabled": enabled,
        }))
        .map_err(|e| CommandError::Failed(format!("set_enabled failed: {e}")))
    }

    fn set_toolbar_pinned(&self, extension_id: &ExtensionId, pinned: bool) -> CommandResult {
        let conn = self.connection()?;
        conn.send_event(json!({
            "op": "set_action_pinned",
            "extensionId": extension_id.as_str(),
            "pinned": pinned,
        }))
        .map_err(|e| CommandError::Failed(format!("set_action_pinned failed: {e}")))
    }

    fn remove(&self, extension_id: &ExtensionId) -> CommandResult {
        let conn = self.connection()?;
        conn.send_event(json!({
            "op": "remove_extension",
            "extensionId": extension_id.as_str(),
            "confirm": true,
        }))
        .map_err(|e| CommandError::Failed(format!("remove_extension failed: {e}")))
    }

    fn reload(&self, extension_id: &ExtensionId) -> CommandResult {
        let conn = self.connection()?;
        conn.send_event(json!({
            "op": "reload_extension",
            "extensionId": extension_id.as_str(),
        }))
        .map_err(|e| CommandError::Failed(format!("reload_extension failed: {e}")))
    }

    fn install_path(&self, raw_path: &str) -> CommandResult {
        let conn = self.connection()?;
        let expanded = expand_tilde(raw_path.trim());
        let meta = std::fs::metadata(&expanded)
            .map_err(|e| CommandError::Failed(format!("cannot read `{}`: {e}", expanded.display())))?;

        let reply = if meta.is_dir() {
            let files = read_unpacked_directory(&expanded)?;
            conn.request_sync(
                json!({
                    "op": "install_unpacked_files",
                    "sourcePath": expanded.to_string_lossy(),
                    "files": files,
                }),
                Vec::new(),
                Duration::from_secs(15),
            )
            .map_err(|e| CommandError::Failed(format!("install failed: {e}")))?
        } else {
            let bytes = std::fs::read(&expanded)
                .map_err(|e| CommandError::Failed(format!("cannot read `{}`: {e}", expanded.display())))?;
            conn.request_sync(
                json!({
                    "op": "install_package",
                    "keep": true,
                }),
                bytes,
                Duration::from_secs(15),
            )
            .map_err(|e| CommandError::Failed(format!("install failed: {e}")))?
        };

        if reply.json.get("ok").and_then(Value::as_bool) != Some(true) {
            let msg = reply
                .json
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Extension installation failed");
            return Err(CommandError::Failed(msg.to_string()));
        }
        let name = reply
            .json
            .get("data")
            .and_then(|d| d.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("Extension");
        let _ = self.core.invoke(Command::ToastShow {
            message: format!("Installed {name}"),
            timeout_ms: Some(2600),
            action_label: None,
            action_command: None,
        });
        Ok(())
    }

    fn cancel_install(&self, install_id: u64) -> CommandResult {
        let conn = self.connection()?;
        conn.send_event(json!({ "op": "install_cancel", "installId": install_id }))
            .map_err(|e| CommandError::Failed(format!("install_cancel failed: {e}")))
    }

    fn retry_install(&self, install_id: u64) -> CommandResult {
        let conn = self.connection()?;
        conn.send_event(json!({ "op": "install_retry", "installId": install_id }))
            .map_err(|e| CommandError::Failed(format!("install_retry failed: {e}")))
    }

    fn answer_prompt(&self, token: u64, accepted: bool) -> CommandResult {
        let conn = self.connection()?;
        conn.send_event(json!({
            "op": "prompt_reply",
            "token": token,
            "accepted": accepted,
        }))
        .map_err(|e| CommandError::Failed(format!("prompt_reply failed: {e}")))
    }

    fn context_menu_items(&self, target_json: &str) -> Vec<ExtensionContextMenuItem> {
        let Ok(conn) = self.connection() else {
            return Vec::new();
        };
        let mut req: Value = serde_json::from_str(target_json).unwrap_or_else(|_| json!({}));
        if let Some(obj) = req.as_object_mut() {
            obj.insert("op".to_string(), json!("context_menu_items"));
        }
        let Ok(reply) = conn.request_sync(req, Vec::new(), Duration::from_millis(400)) else {
            return Vec::new();
        };
        reply
            .json
            .get("data")
            .and_then(Value::as_array)
            .map(|arr| parse_context_menu_items(arr, None))
            .unwrap_or_default()
    }

    fn context_menu_clicked(&self, extension_id: &ExtensionId, item_id: &str, target_json: &str) -> CommandResult {
        let conn = self.connection()?;
        let mut payload: Value = serde_json::from_str(target_json).unwrap_or_else(|_| json!({}));
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("op".to_string(), json!("context_menu_clicked"));
            obj.insert("extensionId".to_string(), json!(extension_id.as_str()));
            obj.insert("itemId".to_string(), json!(item_id));
        }
        conn.send_event(payload)
            .map_err(|e| CommandError::Failed(format!("context_menu_clicked failed: {e}")))
    }

    fn invoke_command(&self, extension_id: &ExtensionId, name: &str) -> CommandResult {
        let conn = self.connection()?;
        let mut payload = json!({
            "op": "command",
            "extensionId": extension_id.as_str(),
            "name": name,
        });
        if let Some(tab_id) = self.core.model().active_tab {
            payload["tabId"] = json!(self.extension_tab_id_of(tab_id));
        }
        let reply = conn
            .request_sync(payload, Vec::new(), Duration::from_secs(3))
            .map_err(|e| CommandError::Failed(format!("extension command failed: {e}")))?;
        if let Some(popup_url) = reply
            .json
            .get("data")
            .and_then(|d| d.get("popup"))
            .and_then(Value::as_str)
            && !popup_url.is_empty()
        {
            self.core
                .open_extension_popup(extension_id.clone(), popup_url.to_string());
        }
        Ok(())
    }

    fn restart_service(&self) -> CommandResult {
        let _ = self.tx_ui.send_blocking(ExtensionUiEvent::ServiceDisconnected {
            reason: "Restarting ExtensionServer".to_string(),
        });
        Ok(())
    }
}

fn find_extension_server_executable() -> Result<PathBuf, String> {
    find_service_executable(EXTENSION_BIN_NAME, Some("ZENDIA_EXTENSION_SERVER")).map_err(|e| e.to_string())
}

fn resolve_tab_id(core: &StubBackend, args: &Value) -> Result<TabId, String> {
    if let Some(id) = args.get("tabId").and_then(Value::as_u64) {
        return tab_for_extension_id(&core.model(), id).ok_or_else(|| format!("No tab with id: {id}."));
    }
    core.model().active_tab.ok_or_else(|| "No active tab".to_string())
}

// Extensions name a tab by the id of the Hub view that shows it, as the Hub does for content scripts, webRequest and
// injected scripts. A tab without a view yet gets an id above every view id.
const VIEWLESS_TAB_ID_BASE: u64 = 1 << 40;

fn extension_tab_id(tab: &Tab) -> i64 {
    tab.view_id.unwrap_or(VIEWLESS_TAB_ID_BASE + tab.id.0) as i64
}

fn tab_for_extension_id(model: &BrowserModel, id: u64) -> Option<TabId> {
    if id >= VIEWLESS_TAB_ID_BASE {
        let tab_id = TabId(id - VIEWLESS_TAB_ID_BASE);
        return model.tab(tab_id).filter(|tab| tab.view_id.is_none()).map(|tab| tab.id);
    }
    model.tab_by_view_id(id).map(|tab| tab.id)
}

fn tab_to_json(
    tab: &Tab,
    index: usize,
    window_id: i64,
    active_tab: Option<TabId>,
    hub_adapter: &HubTabAdapter,
) -> Value {
    let active = active_tab == Some(tab.id);
    let zoom = tab
        .view_id
        .map(|vid| hub_adapter.view_capabilities(vid).zoom_level as f64)
        .unwrap_or(1.0);
    json!({
        "id": extension_tab_id(tab),
        "windowId": window_id,
        "index": index,
        "url": tab.url,
        "title": tab.title,
        "active": active,
        "highlighted": active,
        "pinned": matches!(tab.kind, TabKind::Pinned | TabKind::Essential),
        "audible": tab.is_audible,
        "muted": tab.is_muted,
        "status": if tab.is_loading { "loading" } else { "complete" },
        "incognito": false,
        "zoomFactor": zoom,
    })
}

fn parse_rgba_color(value: Option<&Value>) -> Option<[u8; 4]> {
    let arr = value?.as_array()?;
    if arr.len() < 4 {
        return None;
    }
    Some([
        arr[0].as_u64()? as u8,
        arr[1].as_u64()? as u8,
        arr[2].as_u64()? as u8,
        arr[3].as_u64()? as u8,
    ])
}

/// A prompt detail: `{text, highRisk}`, or a plain string.
fn parse_prompt_detail(value: &Value) -> Option<ExtensionPromptDetail> {
    if let Some(text) = value.as_str() {
        return Some(ExtensionPromptDetail {
            text: text.to_string(),
            high_risk: false,
        });
    }
    Some(ExtensionPromptDetail {
        text: value.get("text")?.as_str()?.to_string(),
        high_risk: value.get("highRisk").and_then(Value::as_bool).unwrap_or(false),
    })
}

fn parse_icon_data(value: Option<&Value>) -> Option<ExtensionIcon> {
    let obj = value?.as_object()?;
    let kind = obj.get("kind").and_then(Value::as_str)?;
    match kind {
        "encoded" => {
            let mime = obj.get("mime").and_then(Value::as_str)?.to_string();
            let b64 = obj.get("data").and_then(Value::as_str)?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
            Some(ExtensionIcon::Encoded { mime, bytes })
        }
        "rgba" => {
            let width = obj.get("width").and_then(Value::as_u64)? as u32;
            let height = obj.get("height").and_then(Value::as_u64)? as u32;
            let b64 = obj.get("data").and_then(Value::as_str)?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
            Some(ExtensionIcon::Rgba { width, height, bytes })
        }
        _ => None,
    }
}

fn parse_ui_snapshot(snapshot: &Value) -> (Vec<ExtensionInfo>, Vec<ExtensionCommand>) {
    let mut extensions = Vec::new();
    if let Some(items) = snapshot.get("extensions").and_then(Value::as_array) {
        for item in items {
            let Some(id_str) = item.get("id").and_then(Value::as_str) else {
                continue;
            };
            let name = item.get("name").and_then(Value::as_str).unwrap_or(id_str).to_string();
            let version = item
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("0.0.0")
                .to_string();
            let description = item
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let enabled = item.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            let action = item.get("action");
            let pinned_to_toolbar = action
                .and_then(|a| a.get("pinned"))
                .and_then(Value::as_bool)
                .or_else(|| item.get("pinned").and_then(Value::as_bool))
                .unwrap_or(true);
            let site_access = item.get("siteAccess").and_then(Value::as_str).map(str::to_string);
            let has_site_access = matches!(site_access.as_deref(), Some("granted" | "onClick"));
            let permissions = item
                .get("permissions")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default();

            let badge_text = action
                .and_then(|a| a.get("badgeText"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let badge_bg = parse_rgba_color(
                action.and_then(|a| a.get("badgeBackgroundColor").or_else(|| a.get("badgeBackground"))),
            );
            let badge_fg = parse_rgba_color(action.and_then(|a| a.get("badgeTextColor")));
            let popup_url = action
                .and_then(|a| a.get("popupUrl").or_else(|| a.get("popup")))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let options_url = item
                .get("optionsUrl")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let icon_data = parse_icon_data(item.get("iconData"));
            let has_action = action.is_some();
            let is_system = item.get("isSystem").and_then(Value::as_bool).unwrap_or(false)
                || item.get("hidden").and_then(Value::as_bool).unwrap_or(false);

            extensions.push(ExtensionInfo {
                id: ExtensionId::new(id_str.to_string()),
                name,
                version,
                description,
                icon: "extension".to_string(),
                icon_data,
                enabled,
                pinned_to_toolbar,
                has_action,
                is_system,
                has_site_access,
                site_access,
                permissions,
                badge_text,
                badge_bg,
                badge_fg,
                options_url,
                popup_url,
                toolbar_item_id: ContributionId::new(format!("ext.{id_str}")),
                command: "extension.invoke_action".to_string(),
            });
        }
    }

    let mut commands = Vec::new();
    if let Some(items) = snapshot.get("commands").and_then(Value::as_array) {
        for item in items {
            let Some(ext_id) = item.get("extensionId").and_then(Value::as_str) else {
                continue;
            };
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                continue;
            };
            let shortcut = item.get("shortcut").and_then(Value::as_str).unwrap_or("").to_string();
            if shortcut.is_empty() {
                continue;
            }
            let description = item
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            commands.push(ExtensionCommand {
                extension_id: ExtensionId::new(ext_id.to_string()),
                name: name.to_string(),
                shortcut,
                description,
            });
        }
    }

    (extensions, commands)
}

fn parse_context_menu_items(items: &[Value], inherited_ext: Option<&str>) -> Vec<ExtensionContextMenuItem> {
    let mut out = Vec::new();
    for item in items {
        let Some(ext_id) = item.get("extensionId").and_then(Value::as_str).or(inherited_ext) else {
            continue;
        };
        let item_id = item.get("id").and_then(Value::as_str).unwrap_or("").to_string();
        let title = item.get("title").and_then(Value::as_str).unwrap_or("").to_string();
        let enabled = item.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        let is_separator = item.get("type").and_then(Value::as_str) == Some("separator");
        let children = item
            .get("children")
            .and_then(Value::as_array)
            .map(|c| parse_context_menu_items(c, Some(ext_id)))
            .unwrap_or_default();
        out.push(ExtensionContextMenuItem {
            extension_id: ExtensionId::new(ext_id.to_string()),
            item_id,
            title,
            enabled,
            is_separator,
            children,
        });
    }
    out
}

fn expand_tilde(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(raw)
}

fn read_unpacked_directory(root: &Path) -> Result<Map<String, Value>, CommandError> {
    let mut out = Map::new();
    let mut total_bytes = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| CommandError::Failed(format!("failed to read `{}`: {e}", dir.display())))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if name == ".git" || name == "node_modules" {
                continue;
            }
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() {
                if out.len() >= MAX_UNPACKED_FILE_COUNT {
                    return Err(CommandError::Failed(
                        "unpacked extension exceeds maximum file count".to_string(),
                    ));
                }
                let bytes = std::fs::read(&path)
                    .map_err(|e| CommandError::Failed(format!("failed to read `{}`: {e}", path.display())))?;
                total_bytes = total_bytes.saturating_add(bytes.len());
                if total_bytes > MAX_UNPACKED_TOTAL_BYTES {
                    return Err(CommandError::Failed(
                        "unpacked extension exceeds maximum size (50 MB)".to_string(),
                    ));
                }
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                out.insert(rel, Value::String(b64));
            }
        }
    }
    Ok(out)
}
