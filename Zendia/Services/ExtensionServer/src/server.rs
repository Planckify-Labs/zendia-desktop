/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Single-threaded core event loop for `ExtensionServer`, hosting `webext::Dispatcher`, `StoreState`,
//! `ExtensionStorage`, and the IPC bridges to the UI process and Ladybird's Hub.

use crate::storage::ExtensionStorage;
use crate::store::{
    StoreState, amo_event, amo_install_progress, cws_installed_event, cws_uninstalled_event, store_display_name,
    store_page_key,
};
use base64::Engine;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use webext::bus::ContextInfo;
use webext::content_scripts::{Target, matching_content_scripts_json};
use webext::dispatch::{Dispatcher, InstallOptions};
use webext::host::{ApiCall, ApiError, ContextKind, ErrorKind, ExtensionHost, Reply};
use webext::loader::{Files, load_extension};
use webext::manifest::Severity;
use webext::package::Package;
use webext::permissions::PermissionRequest;
use webext::registry::Registry;
use webext::store::{
    STORE_BRIDGE_KEY, StoreExpectation, check_store_package, chrome_web_store_download_url, permission_warnings,
    preview_manifest, store_for_url,
};
use zendia_sandbox::{SandboxOutcome, SandboxProfile, enter_sandbox};
use zendia_service::{Frame, FrameKind, read_frame, write_frame};

/// Wire protocol version spoken between the UI process and `ExtensionServer`.
pub const PROTOCOL_VERSION: u32 = 1;
pub const SERVICE_NAME: &str = "extension_server";

const REPORTED_CHROME_VERSION: &str = "153.0.0.0";

enum Inbox {
    Ui(Frame),
    UiDisconnected,
    Hub(Frame),
    HubDisconnected,
}

enum QueuedAction {
    ReloadExtension(String),
    RemoveExtension { id: String, confirm: bool },
    SetEnabled { id: String, enabled: bool },
    FinishCwsBegin { id: String, accepted: bool },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StartReason {
    Startup,
    Install,
    Update,
}

type PromptCallback = Box<dyn FnOnce(bool)>;
type DownloadCallback = Box<dyn FnOnce(Result<Vec<u8>, String>)>;

struct SharedState {
    ui_writer: UnixStream,
    hub_writer: Option<UnixStream>,
    storage: Option<ExtensionStorage>,
    locale: String,
    auto_accept_prompts: bool,
    next_token: u64,
    next_alarm_at_ms: Option<i64>,
    ui_dirty: bool,
    network_interest_dirty: bool,
    pending_ui_calls: HashMap<u64, Reply>,
    pending_hub_calls: HashMap<u64, Reply>,
    pending_prompts: HashMap<u64, PromptCallback>,
    pending_downloads: HashMap<u64, DownloadCallback>,
    queued_actions: Vec<QueuedAction>,
}

impl SharedState {
    fn alloc_token(&mut self) -> u64 {
        let token = self.next_token;
        self.next_token = self.next_token.wrapping_add(1).max(1);
        token
    }

    fn send_ui(&mut self, frame: &Frame) {
        if let Err(error) = write_frame(&mut self.ui_writer, frame) {
            eprintln!("zendia-extension-server: failed writing to UI: {error}");
        }
    }

    fn send_hub(&mut self, frame: &Frame) {
        if let Some(hub) = self.hub_writer.as_mut()
            && let Err(error) = write_frame(hub, frame)
        {
            eprintln!("zendia-extension-server: failed writing to Hub: {error}");
        }
    }

    fn request_prompt(
        &mut self,
        title: String,
        message: String,
        details: Vec<String>,
        accept_label: String,
        on_answer: PromptCallback,
    ) {
        if self.auto_accept_prompts {
            on_answer(true);
            return;
        }
        let token = self.alloc_token();
        self.pending_prompts.insert(token, on_answer);
        self.send_ui(&Frame::event(json!({
            "op": "prompt",
            "token": token,
            "title": title,
            "message": message,
            "details": details,
            "acceptLabel": accept_label,
        })));
    }

    fn request_hub_download(&mut self, url: String, on_done: DownloadCallback) {
        if self.hub_writer.is_none() {
            on_done(Err("Hub embedder channel is not connected".to_string()));
            return;
        }
        let token = self.alloc_token();
        self.pending_downloads.insert(token, on_done);
        self.send_hub(&Frame::event(json!({
            "op": "download_url",
            "token": token,
            "url": url,
        })));
    }
}

struct ServerHost {
    shared: Rc<RefCell<SharedState>>,
}

impl ExtensionHost for ServerHost {
    fn read_storage(&self, extension_id: &str, key: &str) -> Option<String> {
        self.shared
            .borrow()
            .storage
            .as_ref()
            .and_then(|storage| storage.read(extension_id, key))
    }

    fn write_storage(&self, extension_id: &str, key: &str, json: &str) {
        if let Some(storage) = self.shared.borrow().storage.as_ref() {
            storage.write(extension_id, key, json);
        }
    }

    fn now_ms(&self) -> i64 {
        now_ms()
    }

    fn ui_locale(&self) -> String {
        self.shared.borrow().locale.clone()
    }

    fn dispatch_event(&self, extension_id: &str, event: &str, args: Vec<Value>) {
        let mut shared = self.shared.borrow_mut();
        if event.starts_with("__ui.") {
            shared.ui_dirty = true;
            return;
        }
        if event == "__network.changed" {
            shared.network_interest_dirty = true;
            return;
        }
        let payload = json!({
            "kind": "event",
            "name": event,
            "args": args,
        });
        shared.send_hub(&Frame::event(json!({
            "op": "deliver_to_extension",
            "extensionId": extension_id,
            "json": payload.to_string(),
        })));
    }

    fn deliver_to_context(&self, context_id: u64, message: Value) {
        let payload = json!({
            "kind": "envelope",
            "envelope": message,
        });
        self.shared.borrow_mut().send_hub(&Frame::event(json!({
            "op": "deliver_to_context",
            "contextId": context_id,
            "json": payload.to_string(),
        })));
    }

    fn call_browser(&self, call: &ApiCall, reply: Reply) {
        let function = call.function.as_str();
        let args = call.args.first().cloned().unwrap_or_else(|| json!({}));

        if function == "host.runtime.reload" {
            self.shared
                .borrow_mut()
                .queued_actions
                .push(QueuedAction::ReloadExtension(call.extension_id.clone()));
            reply(Ok(Value::Null));
            return;
        }
        if function == "host.management.uninstall" {
            let target_id = args
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or(&call.extension_id)
                .to_string();
            self.shared
                .borrow_mut()
                .queued_actions
                .push(QueuedAction::RemoveExtension {
                    id: target_id,
                    confirm: true,
                });
            reply(Ok(Value::Null));
            return;
        }
        if function == "host.management.setEnabled" {
            let target_id = args
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or(&call.extension_id)
                .to_string();
            let enabled = args.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            self.shared
                .borrow_mut()
                .queued_actions
                .push(QueuedAction::SetEnabled { id: target_id, enabled });
            reply(Ok(Value::Null));
            return;
        }

        if is_hub_host_call(function) {
            let mut shared = self.shared.borrow_mut();
            if shared.hub_writer.is_none() {
                drop(shared);
                reply(Err(ApiError::runtime("Hub embedder channel is not connected")));
                return;
            }
            let token = shared.alloc_token();
            shared.pending_hub_calls.insert(token, reply);
            shared.send_hub(&Frame::event(json!({
                "op": "hub_host_call",
                "token": token,
                "extensionId": call.extension_id,
                "function": call.function,
                "args": args,
            })));
            return;
        }

        if function.starts_with("host.") {
            let mut shared = self.shared.borrow_mut();
            let token = shared.alloc_token();
            shared.pending_ui_calls.insert(token, reply);
            shared.send_ui(&Frame::event(json!({
                "op": "host_call",
                "token": token,
                "extensionId": call.extension_id,
                "function": call.function,
                "args": args,
            })));
            return;
        }

        reply(Err(ApiError::new(
            ErrorKind::NotAvailable,
            format!("{} is not implemented yet in Zendia", call.function),
        )));
    }

    fn request_permissions(&self, extension_id: &str, request: &PermissionRequest, reply: Box<dyn FnOnce(bool)>) {
        let mut details = Vec::new();
        for perm in &request.permissions {
            details.push(format!("Permission: {perm}"));
        }
        for origin in &request.origins {
            details.push(format!("Access site: {origin}"));
        }
        self.shared.borrow_mut().request_prompt(
            format!("Allow additional permissions for \"{extension_id}\"?"),
            "This extension is requesting additional access:".to_string(),
            details,
            "Allow".to_string(),
            reply,
        );
    }

    fn schedule_alarm_check(&self, at_ms: i64) {
        let mut shared = self.shared.borrow_mut();
        shared.next_alarm_at_ms = Some(match shared.next_alarm_at_ms {
            Some(existing) => existing.min(at_ms),
            None => at_ms,
        });
    }
}

fn is_hub_host_call(function: &str) -> bool {
    matches!(
        function,
        "host.tabs.frames"
            | "host.scripting.inject"
            | "host.cookies.get"
            | "host.cookies.getAll"
            | "host.cookies.set"
            | "host.cookies.remove"
            | "host.browsingData.remove"
            | "host.search.engines"
    )
}

#[derive(Clone, Debug)]
struct ContextEntry {
    extension_id: String,
    kind: ContextKind,
    is_store: bool,
    store_name: Option<String>,
}

struct ServerState {
    dispatcher: Dispatcher,
    shared: Rc<RefCell<SharedState>>,
    store: Option<StoreState>,
    initialized: bool,
    next_context_id: u64,
    contexts: HashMap<u64, ContextEntry>,
    loaded_cli_ids: HashSet<String>,
    disabled_ids: HashSet<String>,
    pending_start_reasons: HashMap<String, StartReason>,
    active_tab_id: Option<i64>,
    last_network_interest: u32,
}

impl ServerState {
    fn new(ui_writer: UnixStream, hub_writer: Option<UnixStream>) -> Self {
        let shared = Rc::new(RefCell::new(SharedState {
            ui_writer,
            hub_writer,
            storage: None,
            locale: "en-US".to_string(),
            auto_accept_prompts: false,
            next_token: 1,
            next_alarm_at_ms: None,
            ui_dirty: false,
            network_interest_dirty: false,
            pending_ui_calls: HashMap::new(),
            pending_hub_calls: HashMap::new(),
            pending_prompts: HashMap::new(),
            pending_downloads: HashMap::new(),
            queued_actions: Vec::new(),
        }));
        let host = Rc::new(ServerHost {
            shared: Rc::clone(&shared),
        });
        let dispatcher = Dispatcher::new(host, Registry::builtin());
        Self {
            dispatcher,
            shared,
            store: None,
            initialized: false,
            next_context_id: 1,
            contexts: HashMap::new(),
            loaded_cli_ids: HashSet::new(),
            disabled_ids: HashSet::new(),
            pending_start_reasons: HashMap::new(),
            active_tab_id: None,
            last_network_interest: 0,
        }
    }

    fn flush_side_effects(&mut self) {
        loop {
            let actions: Vec<QueuedAction> = std::mem::take(&mut self.shared.borrow_mut().queued_actions);
            if actions.is_empty() {
                break;
            }
            for action in actions {
                match action {
                    QueuedAction::ReloadExtension(id) => self.reload_extension(&id),
                    QueuedAction::RemoveExtension { id, confirm } => self.remove_extension(&id, confirm),
                    QueuedAction::SetEnabled { id, enabled } => self.set_extension_enabled(&id, enabled),
                    QueuedAction::FinishCwsBegin { id, accepted } => {
                        if let Some(store) = self.store.as_mut() {
                            if accepted {
                                if let Some(manifest) = store.approved_cws_installs.remove(&format!("pending:{id}")) {
                                    store.approved_cws_installs.insert(id, manifest);
                                }
                            } else {
                                store.approved_cws_installs.remove(&format!("pending:{id}"));
                                store.installs_in_progress.remove(&id);
                            }
                        }
                    }
                }
            }
        }

        let (ui_dirty, net_dirty) = {
            let mut shared = self.shared.borrow_mut();
            let ui = std::mem::take(&mut shared.ui_dirty);
            let net = std::mem::take(&mut shared.network_interest_dirty);
            (ui, net)
        };
        if net_dirty {
            self.push_network_interest();
        }
        if ui_dirty {
            self.push_ui_state();
        }
    }

    fn handle_ui_frame(&mut self, frame: Frame) {
        let op = frame.op().unwrap_or("").to_string();
        match op.as_str() {
            "hello" => {
                let reply = Frame::response_ok(
                    frame.id,
                    json!({
                        "service": SERVICE_NAME,
                        "protocolVersion": PROTOCOL_VERSION,
                    }),
                );
                self.shared.borrow_mut().send_ui(&reply);
            }
            "init" => {
                self.handle_init(&frame.json);
                if frame.kind == FrameKind::Request {
                    self.shared
                        .borrow_mut()
                        .send_ui(&Frame::response_ok(frame.id, json!({ "ok": true })));
                }
            }
            "browser_state" => {
                if let Some(state) = frame.json.get("state") {
                    self.active_tab_id = find_active_tab_id(state);
                    let _ = self.dispatcher.host_event("browserState", state);
                    self.shared.borrow_mut().ui_dirty = true;
                }
            }
            "get_ui_state" => {
                let tab_id = frame.json.get("tabId").and_then(Value::as_i64).or(self.active_tab_id);
                let payload = self.build_ui_snapshot(tab_id);
                self.shared.borrow_mut().send_ui(&Frame::response_ok(frame.id, payload));
            }
            "action_clicked" => {
                let ext_id = frame.json.get("extensionId").and_then(Value::as_str).unwrap_or("");
                let tab_id = frame.json.get("tabId").and_then(Value::as_i64).or(self.active_tab_id);
                let mut args = json!({ "extensionId": ext_id });
                if let Some(t) = tab_id {
                    args["tabId"] = json!(t);
                }
                let result = if self.disabled_ids.contains(ext_id) {
                    Value::Null
                } else {
                    self.dispatcher.host_event("actionClicked", &args)
                };
                if frame.kind == FrameKind::Request {
                    self.shared.borrow_mut().send_ui(&Frame::response_ok(frame.id, result));
                } else if let Some(popup) = result.get("popup").and_then(Value::as_str) {
                    self.shared.borrow_mut().send_ui(&Frame::event(json!({
                        "op": "open_popup",
                        "extensionId": ext_id,
                        "popupUrl": popup,
                    })));
                }
            }
            "set_action_pinned" => {
                let ext_id = frame.json.get("extensionId").and_then(Value::as_str).unwrap_or("");
                let pinned = frame.json.get("pinned").and_then(Value::as_bool).unwrap_or(true);
                let _ = self
                    .dispatcher
                    .host_event("setActionPinned", &json!({ "extensionId": ext_id, "pinned": pinned }));
                self.shared.borrow_mut().ui_dirty = true;
            }
            "set_enabled" => {
                let ext_id = frame
                    .json
                    .get("extensionId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let enabled = frame.json.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                self.set_extension_enabled(&ext_id, enabled);
            }
            "install_package" => {
                let keep = frame.json.get("keep").and_then(Value::as_bool).unwrap_or(true);
                let result = self.install_package_bytes(frame.binary, "", "auto", keep);
                if frame.kind == FrameKind::Request {
                    match result {
                        Ok(info) => self.shared.borrow_mut().send_ui(&Frame::response_ok(frame.id, info)),
                        Err(err) => self.shared.borrow_mut().send_ui(&Frame::response_err(frame.id, err)),
                    }
                }
            }
            "install_unpacked_files" => {
                let seed = frame
                    .json
                    .get("sourcePath")
                    .and_then(Value::as_str)
                    .unwrap_or("unpacked");
                let result = self.install_unpacked_map(&frame.json, seed);
                if frame.kind == FrameKind::Request {
                    match result {
                        Ok(info) => self.shared.borrow_mut().send_ui(&Frame::response_ok(frame.id, info)),
                        Err(err) => self.shared.borrow_mut().send_ui(&Frame::response_err(frame.id, err)),
                    }
                }
            }
            "remove_extension" => {
                let ext_id = frame
                    .json
                    .get("extensionId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let confirm = frame.json.get("confirm").and_then(Value::as_bool).unwrap_or(false);
                self.remove_extension(&ext_id, confirm);
            }
            "reload_extension" => {
                let ext_id = frame
                    .json
                    .get("extensionId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                self.reload_extension(&ext_id);
            }
            "context_menu_items" => {
                let result = self.dispatcher.host_event("contextMenuItems", &frame.json);
                self.shared.borrow_mut().send_ui(&Frame::response_ok(frame.id, result));
            }
            "context_menu_clicked" => {
                let _ = self.dispatcher.host_event("contextMenuClicked", &frame.json);
            }
            "command" => {
                let ext_id = frame.json.get("extensionId").and_then(Value::as_str).unwrap_or("");
                let result = if self.disabled_ids.contains(ext_id) {
                    Value::Null
                } else {
                    self.dispatcher.host_event("command", &frame.json)
                };
                if frame.kind == FrameKind::Request {
                    self.shared.borrow_mut().send_ui(&Frame::response_ok(frame.id, result));
                } else if let Some(popup) = result.get("popup").and_then(Value::as_str) {
                    self.shared.borrow_mut().send_ui(&Frame::event(json!({
                        "op": "open_popup",
                        "extensionId": ext_id,
                        "popupUrl": popup,
                    })));
                }
            }
            "set_popup_view" => {
                self.shared.borrow_mut().send_hub(&Frame::event(json!({
                    "op": "set_popup_view",
                    "viewId": frame.json.get("viewId").and_then(Value::as_u64).unwrap_or(0),
                    "isPopup": frame.json.get("isPopup").and_then(Value::as_bool).unwrap_or(false),
                })));
                if frame.kind == FrameKind::Request {
                    self.shared
                        .borrow_mut()
                        .send_ui(&Frame::response_ok(frame.id, json!({ "ok": true })));
                }
            }
            "host_call_reply" => {
                let token = frame.json.get("token").and_then(Value::as_u64).unwrap_or(0);
                let ok = frame.json.get("ok").and_then(Value::as_bool).unwrap_or(false);
                let reply = self.shared.borrow_mut().pending_ui_calls.remove(&token);
                if let Some(reply) = reply {
                    if ok {
                        let data = frame.json.get("data").cloned().unwrap_or(Value::Null);
                        reply(Ok(data));
                    } else {
                        let err = frame
                            .json
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("Host call failed");
                        reply(Err(ApiError::runtime(err)));
                    }
                }
            }
            "prompt_reply" => {
                let token = frame.json.get("token").and_then(Value::as_u64).unwrap_or(0);
                let accepted = frame.json.get("accepted").and_then(Value::as_bool).unwrap_or(false);
                let cb = self.shared.borrow_mut().pending_prompts.remove(&token);
                if let Some(cb) = cb {
                    cb(accepted);
                }
            }
            other => {
                if frame.kind == FrameKind::Request {
                    self.shared
                        .borrow_mut()
                        .send_ui(&Frame::response_err(frame.id, format!("unknown UI op `{other}`")));
                }
            }
        }
    }

    fn handle_init(&mut self, args: &Value) {
        if self.initialized {
            return;
        }
        self.initialized = true;

        let data_dir = args
            .get("dataDir")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("zendia-extensions"));
        let locale = args
            .get("locale")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("en-US")
            .to_string();
        let auto_accept = args.get("autoAcceptPrompts").and_then(Value::as_bool).unwrap_or(false);
        let load_paths: Vec<PathBuf> = args
            .get("loadExtensions")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default();

        let _ = std::fs::create_dir_all(&data_dir);
        let storage_dir = data_dir.join("Extensions");
        let packages_dir = data_dir.join("ExtensionPackages");
        let _ = std::fs::create_dir_all(&storage_dir);
        let _ = std::fs::create_dir_all(&packages_dir);

        // Lock down the OS sandbox before parsing any untrusted extension files or packages.
        let profile = SandboxProfile::ExtensionServer {
            data_dir: data_dir.clone(),
            read_only_paths: load_paths.clone(),
        };
        match enter_sandbox(&profile) {
            Ok(SandboxOutcome::Enforced) => {}
            Ok(SandboxOutcome::Degraded { reason }) => {
                eprintln!("zendia-extension-server: sandbox degraded: {reason}");
            }
            Ok(SandboxOutcome::Disabled) => {}
            Err(error) => {
                eprintln!("zendia-extension-server: sandbox error: {error}");
            }
        }

        {
            let mut shared = self.shared.borrow_mut();
            shared.storage = Some(ExtensionStorage::new(storage_dir));
            shared.locale = locale;
            shared.auto_accept_prompts = auto_accept;
        }
        self.store = Some(StoreState::new(packages_dir));

        // 1. Load kept store packages from disk.
        let mut kept_ids = Vec::new();
        let kept_entries = self
            .store
            .as_ref()
            .map(StoreState::read_kept_packages)
            .unwrap_or_default();
        for (entry, bytes) in kept_entries {
            match self.install_package_internal(bytes, "", "auto") {
                Ok(id) => {
                    if let Some(store) = self.store.as_mut() {
                        store.kept.push(entry);
                    }
                    kept_ids.push(id);
                }
                Err(error) => {
                    eprintln!("zendia-extension-server: kept package `{}` failed: {error}", entry.file);
                }
            }
        }

        // 2. Load --load-extension paths (overriding kept packages with the same ID).
        for path in &load_paths {
            let result = if path.is_dir() {
                self.install_directory_internal(path, "", "auto")
            } else {
                match std::fs::read(path) {
                    Ok(bytes) => self.install_package_internal(bytes, "", "auto"),
                    Err(error) => Err(format!("cannot read {}: {error}", path.display())),
                }
            };
            match result {
                Ok(id) => {
                    self.loaded_cli_ids.insert(id.clone());
                    self.start_background_page(&id, StartReason::Install);
                }
                Err(error) => {
                    eprintln!(
                        "zendia-extension-server: could not load extension '{}': {error}",
                        path.display()
                    );
                }
            }
        }

        // 3. Start background pages for kept packages that were not overridden by --load-extension.
        for id in kept_ids {
            if !self.loaded_cli_ids.contains(&id) {
                self.start_background_page(&id, StartReason::Startup);
            }
        }

        self.push_network_interest();
        self.push_ui_state();
    }

    fn handle_hub_frame(&mut self, frame: Frame) {
        let op = frame.op().unwrap_or("").to_string();
        match op.as_str() {
            "serve" => {
                let url = frame.json.get("url").and_then(Value::as_str).unwrap_or("");
                let initiator = frame
                    .json
                    .get("initiator")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty());
                let reply = match self.dispatcher.serve(url, initiator) {
                    Some(served) if !self.disabled_ids.contains(&served.extension_id) => {
                        Frame::response_ok_with_binary(
                            frame.id,
                            json!({
                                "found": true,
                                "status": served.status,
                                "mime": served.mime,
                                "csp": served.csp,
                                "extensionId": served.extension_id,
                            }),
                            served.body,
                        )
                    }
                    _ => Frame::response_ok(frame.id, json!({ "found": false })),
                };
                self.shared.borrow_mut().send_hub(&reply);
            }
            "content_script_programs" => {
                let url = frame.json.get("url").and_then(Value::as_str).unwrap_or("");
                let is_top_frame = frame.json.get("isTopFrame").and_then(Value::as_bool).unwrap_or(true);
                let programs = if self.dispatcher.extension_for_url(url).is_some() {
                    Value::Array(Vec::new())
                } else {
                    let mut list = self.dispatcher.content_script_programs(url, is_top_frame, None);
                    if let Some(arr) = list.as_array_mut() {
                        arr.retain(|entry| {
                            let ext_id = entry.get("extensionId").and_then(Value::as_str).unwrap_or("");
                            !self.disabled_ids.contains(ext_id)
                        });
                    }
                    list
                };
                self.shared.borrow_mut().send_hub(&Frame::response_ok(
                    frame.id,
                    json!({ "programs": programs.to_string() }),
                ));
            }
            "context_init" => {
                let reply = self.handle_hub_context_init(frame.id, &frame.json);
                self.shared.borrow_mut().send_hub(&reply);
            }
            "context_destroyed" => {
                let context_id = frame.json.get("contextId").and_then(Value::as_u64).unwrap_or(0);
                if self.contexts.remove(&context_id).is_some() {
                    self.dispatcher.context_destroyed(context_id);
                }
            }
            "api_call_sync" => {
                let context_id = frame.json.get("contextId").and_then(Value::as_u64).unwrap_or(0);
                let name = frame.json.get("name").and_then(Value::as_str).unwrap_or("");
                let args_json = frame.json.get("argsJson").and_then(Value::as_str).unwrap_or("[]");
                let result = self.handle_api_call_sync(context_id, name, args_json);
                self.shared
                    .borrow_mut()
                    .send_hub(&Frame::response_ok(frame.id, json!({ "result": result })));
            }
            "api_call" => {
                let context_id = frame.json.get("contextId").and_then(Value::as_u64).unwrap_or(0);
                let request_id = frame.json.get("requestId").and_then(Value::as_u64).unwrap_or(0);
                let view_id = frame.json.get("viewId").and_then(Value::as_u64).unwrap_or(0);
                let name = frame.json.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                let args_json = frame
                    .json
                    .get("argsJson")
                    .and_then(Value::as_str)
                    .unwrap_or("[]")
                    .to_string();
                self.handle_api_call(context_id, request_id, view_id, &name, &args_json);
            }
            "network_stage" => {
                let token = frame.json.get("token").and_then(Value::as_u64).unwrap_or(0);
                let stage = frame.json.get("stage").and_then(Value::as_str).unwrap_or("");
                let is_tab = frame.json.get("isTab").and_then(Value::as_bool).unwrap_or(false);
                let details = frame.json.get("details").cloned().unwrap_or_else(|| json!({}));

                if stage == "navigation" {
                    if is_tab {
                        let _ = self.dispatcher.host_event("navigation", &details);
                    }
                    return;
                }
                let shared = Rc::clone(&self.shared);
                self.dispatcher.network_event(
                    stage,
                    &details,
                    Box::new(move |res| {
                        if token == 0 {
                            return;
                        }
                        let answer = match res {
                            Ok(val) => val.to_string(),
                            Err(_) => "{}".to_string(),
                        };
                        shared.borrow_mut().send_hub(&Frame::event(json!({
                            "op": "network_answer",
                            "token": token,
                            "answerJson": answer,
                        })));
                    }),
                );
            }
            "background_loaded" => {
                let ext_id = frame.json.get("extensionId").and_then(Value::as_str).unwrap_or("");
                if let Some(reason) = self.pending_start_reasons.remove(ext_id) {
                    match reason {
                        StartReason::Install => {
                            self.dispatcher.notify_installed(ext_id, "install", None);
                            if self.loaded_cli_ids.contains(ext_id) {
                                self.dispatcher.notify_startup(ext_id);
                            }
                        }
                        StartReason::Update => {
                            self.dispatcher.notify_installed(ext_id, "update", None);
                        }
                        StartReason::Startup => {
                            self.dispatcher.notify_startup(ext_id);
                        }
                    }
                }
            }
            "hub_host_call_reply" => {
                let token = frame.json.get("token").and_then(Value::as_u64).unwrap_or(0);
                let ok = frame.json.get("ok").and_then(Value::as_bool).unwrap_or(false);
                let reply = self.shared.borrow_mut().pending_hub_calls.remove(&token);
                if let Some(reply) = reply {
                    if ok {
                        let data = frame.json.get("data").cloned().unwrap_or(Value::Null);
                        reply(Ok(data));
                    } else {
                        let err = frame
                            .json
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("Hub host call failed");
                        reply(Err(ApiError::runtime(err)));
                    }
                }
            }
            "download_reply" => {
                let token = frame.json.get("token").and_then(Value::as_u64).unwrap_or(0);
                let ok = frame.json.get("ok").and_then(Value::as_bool).unwrap_or(false);
                let cb = self.shared.borrow_mut().pending_downloads.remove(&token);
                if let Some(cb) = cb {
                    if ok {
                        cb(Ok(frame.binary));
                    } else {
                        let err = frame
                            .json
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("Download failed")
                            .to_string();
                        cb(Err(err));
                    }
                }
            }
            other => {
                if frame.kind == FrameKind::Request {
                    self.shared
                        .borrow_mut()
                        .send_hub(&Frame::response_err(frame.id, format!("unknown Hub op `{other}`")));
                }
            }
        }
    }

    fn handle_hub_context_init(&mut self, frame_id: u64, req: &Value) -> Frame {
        let url = req.get("url").and_then(Value::as_str).unwrap_or("");
        let origin = req.get("origin").and_then(Value::as_str).unwrap_or("");
        let bridge_key = req.get("bridgeKey").and_then(Value::as_str).unwrap_or("");
        let is_top_frame = req.get("isTopFrame").and_then(Value::as_bool).unwrap_or(true);
        let frame_id_num = req.get("frameId").and_then(Value::as_i64).unwrap_or(0);
        let tab_id = req.get("tabId").and_then(Value::as_i64);
        let is_popup = req.get("isPopup").and_then(Value::as_bool).unwrap_or(false);
        let has_injection_grant = req.get("hasInjectionGrant").and_then(Value::as_bool).unwrap_or(false);

        if bridge_key == STORE_BRIDGE_KEY {
            let store = store_for_url(url);
            let origin_store = if origin.is_empty() || origin == "null" {
                None
            } else {
                store_for_url(&format!("{origin}/"))
            };
            let Some(store) = store.filter(|s| Some(*s) == origin_store) else {
                return Frame::response_ok(frame_id, json!({ "ok": false, "misbehaved": true }));
            };
            let context_id = self.alloc_context_id();
            let store_str = store.as_str().to_string();
            let store_key = store_page_key(&store_str);
            self.contexts.insert(
                context_id,
                ContextEntry {
                    extension_id: store_key.clone(),
                    kind: ContextKind::Other,
                    is_store: true,
                    store_name: Some(store_str.clone()),
                },
            );
            let init_json = json!({ "store": store_str }).to_string();
            return Frame::response_ok(
                frame_id,
                json!({
                    "ok": true,
                    "contextId": context_id,
                    "extensionId": store_key,
                    "initJson": init_json,
                }),
            );
        }

        let origin_ext = if origin.starts_with("chrome-extension://") || origin.starts_with("moz-extension://") {
            self.dispatcher.extension_for_url(&format!("{origin}/"))
        } else {
            None
        };

        let (extension_id, kind) = if bridge_key.is_empty() {
            let Some(owner) = origin_ext else {
                return Frame::response_ok(frame_id, json!({ "ok": false, "misbehaved": true }));
            };
            if self.disabled_ids.contains(&owner) {
                return Frame::response_ok(frame_id, json!({ "ok": false }));
            }
            let kind = if self.dispatcher.is_background_url(&owner, url) {
                ContextKind::Background
            } else {
                ContextKind::ExtensionPage
            };
            (owner, kind)
        } else {
            let ext_id = bridge_key.to_string();
            if self.disabled_ids.contains(&ext_id) || origin_ext.is_some() {
                return Frame::response_ok(frame_id, json!({ "ok": false }));
            }
            let target = Target {
                url,
                is_top_frame,
                fallback_url: None,
            };
            let matches = self
                .dispatcher
                .with_extension(&ext_id, |e| matching_content_scripts_json(&e.manifest, &target))
                .and_then(|v| v.as_array().map(|a| !a.is_empty()))
                .unwrap_or(false);
            if !has_injection_grant && !matches {
                return Frame::response_ok(frame_id, json!({ "ok": false, "misbehaved": true }));
            }
            (ext_id, ContextKind::ContentScript)
        };

        let context_tab_id = if kind != ContextKind::Background && !is_popup {
            tab_id
        } else {
            None
        };
        let context_frame_id = if kind == ContextKind::ContentScript {
            Some(frame_id_num)
        } else {
            None
        };

        let context_id = self.alloc_context_id();
        let created = self.dispatcher.context_created(ContextInfo {
            id: context_id,
            extension_id: extension_id.clone(),
            kind,
            tab_id: context_tab_id,
            frame_id: context_frame_id,
            url: Some(url.to_string()),
        });
        if created.is_err() {
            return Frame::response_ok(frame_id, json!({ "ok": false }));
        }

        self.contexts.insert(
            context_id,
            ContextEntry {
                extension_id: extension_id.clone(),
                kind,
                is_store: false,
                store_name: None,
            },
        );

        let Some(mut init) = self.dispatcher.init_data_for_context(context_id) else {
            return Frame::response_ok(frame_id, json!({ "ok": false }));
        };
        if is_popup && kind == ContextKind::ExtensionPage {
            init["isPopup"] = json!(true);
        }

        Frame::response_ok(
            frame_id,
            json!({
                "ok": true,
                "contextId": context_id,
                "extensionId": extension_id,
                "initJson": init.to_string(),
            }),
        )
    }

    fn handle_api_call_sync(&mut self, context_id: u64, name: &str, args_json: &str) -> Option<String> {
        let entry = self.contexts.get(&context_id)?.clone();
        if entry.is_store || self.disabled_ids.contains(&entry.extension_id) {
            return None;
        }
        let args = serde_json::from_str::<Value>(args_json)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let call = ApiCall {
            extension_id: entry.extension_id,
            function: name.to_string(),
            args,
            caller: webext::host::Caller {
                context: entry.kind,
                context_id: Some(context_id),
                tab_id: None,
                frame_id: None,
                url: None,
            },
        };
        let result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let slot = Rc::clone(&result);
        self.dispatcher.dispatch(
            call,
            Box::new(move |res| {
                if let Ok(val) = res {
                    *slot.borrow_mut() = Some(val.to_string());
                }
            }),
        );
        result.borrow_mut().take()
    }

    fn handle_api_call(&mut self, context_id: u64, request_id: u64, view_id: u64, name: &str, args_json: &str) {
        let Some(entry) = self.contexts.get(&context_id).cloned() else {
            self.send_api_reply(context_id, request_id, false, "Extension context invalidated.");
            return;
        };

        if name.starts_with("__ui.") {
            self.send_api_reply(context_id, request_id, true, "null");
            if entry.kind == ContextKind::ExtensionPage {
                self.shared.borrow_mut().send_ui(&Frame::event(json!({
                    "op": "popup_request",
                    "viewId": view_id,
                    "extensionId": entry.extension_id,
                    "name": name,
                    "argsJson": args_json,
                })));
            }
            return;
        }

        if entry.is_store {
            let store_name = entry.store_name.unwrap_or_default();
            self.handle_store_api_call(context_id, request_id, &store_name, name, args_json);
            return;
        }

        if self.disabled_ids.contains(&entry.extension_id) {
            self.send_api_reply(context_id, request_id, false, "Extension is disabled.");
            return;
        }

        let args = serde_json::from_str::<Value>(args_json)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let call = ApiCall {
            extension_id: entry.extension_id,
            function: name.to_string(),
            args,
            caller: webext::host::Caller {
                context: entry.kind,
                context_id: Some(context_id),
                tab_id: None,
                frame_id: None,
                url: None,
            },
        };
        let shared = Rc::clone(&self.shared);
        self.dispatcher.dispatch(
            call,
            Box::new(move |res| match res {
                Ok(val) => {
                    shared.borrow_mut().send_hub(&Frame::event(json!({
                        "op": "api_reply",
                        "contextId": context_id,
                        "requestId": request_id,
                        "ok": true,
                        "json": val.to_string(),
                    })));
                }
                Err(err) => {
                    shared.borrow_mut().send_hub(&Frame::event(json!({
                        "op": "api_reply",
                        "contextId": context_id,
                        "requestId": request_id,
                        "ok": false,
                        "json": err.message,
                    })));
                }
            }),
        );
    }

    fn handle_store_api_call(&mut self, context_id: u64, request_id: u64, store: &str, name: &str, args_json: &str) {
        let args = serde_json::from_str::<Value>(args_json)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let str_arg = |idx: usize| -> String { args.get(idx).and_then(Value::as_str).unwrap_or("").to_string() };

        if store == "chrome" && name.starts_with("cws.") {
            let id = str_arg(0);
            match name {
                "cws.getAll" => {
                    let list = self.dispatcher.management_list().to_string();
                    self.send_api_reply(context_id, request_id, true, &list);
                }
                "cws.get" => match self.dispatcher.management_info(&id) {
                    Some(info) => self.send_api_reply(context_id, request_id, true, &info.to_string()),
                    None => self.send_api_reply(
                        context_id,
                        request_id,
                        false,
                        &format!("Failed to find extension with id {id}."),
                    ),
                },
                "cws.getExtensionStatus" => {
                    let status = if self.dispatcher.management_info(&id).is_some() {
                        if self.disabled_ids.contains(&id) {
                            "disabled"
                        } else {
                            "enabled"
                        }
                    } else {
                        "installable"
                    };
                    self.send_api_reply(context_id, request_id, true, &json!(status).to_string());
                }
                "cws.permissionWarningsByManifest" => {
                    let preview = preview_manifest(&id, "");
                    if preview.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                        let warnings = preview.get("warnings").cloned().unwrap_or_else(|| json!([]));
                        self.send_api_reply(context_id, request_id, true, &warnings.to_string());
                    } else {
                        self.send_api_reply(context_id, request_id, false, "Could not read the manifest.");
                    }
                }
                "cws.permissionWarningsById" => {
                    match self
                        .dispatcher
                        .with_extension(&id, |e| permission_warnings(&e.manifest))
                    {
                        Some(warnings) => {
                            self.send_api_reply(context_id, request_id, true, &json!(warnings).to_string())
                        }
                        None => self.send_api_reply(context_id, request_id, false, "Could not read the manifest."),
                    }
                }
                "cws.beginInstall" => {
                    self.handle_cws_begin_install(context_id, request_id, &args);
                }
                "cws.completeInstall" => {
                    self.handle_cws_complete_install(context_id, request_id, &id);
                }
                "cws.setEnabled" => {
                    if self.dispatcher.management_info(&id).is_none() {
                        self.send_api_reply(
                            context_id,
                            request_id,
                            false,
                            &format!("Failed to find extension with id {id}."),
                        );
                    } else {
                        let enabled = args.get(1).and_then(Value::as_bool).unwrap_or(true);
                        self.set_extension_enabled(&id, enabled);
                        self.send_api_reply(context_id, request_id, true, "");
                    }
                }
                "cws.uninstall" => {
                    let Some(info) = self.dispatcher.management_info(&id) else {
                        self.send_api_reply(
                            context_id,
                            request_id,
                            false,
                            &format!("Failed to find extension with id {id}."),
                        );
                        return;
                    };
                    let confirm = args.get(1).and_then(Value::as_bool).unwrap_or(false);
                    if !confirm {
                        self.finish_uninstall(&id);
                        self.send_api_reply(context_id, request_id, true, "");
                        return;
                    }
                    let display_name = info.get("name").and_then(Value::as_str).unwrap_or(&id).to_string();
                    let shared = Rc::clone(&self.shared);
                    let target_id = id.clone();
                    self.shared.borrow_mut().request_prompt(
                        format!("Remove \"{display_name}\"?"),
                        "The extension and the data it stored will be removed.".to_string(),
                        Vec::new(),
                        "Remove".to_string(),
                        Box::new(move |accepted| {
                            if !accepted {
                                shared.borrow_mut().send_hub(&Frame::event(json!({
                                    "op": "api_reply",
                                    "contextId": context_id,
                                    "requestId": request_id,
                                    "ok": false,
                                    "json": "User cancelled uninstall",
                                })));
                                return;
                            }
                            shared.borrow_mut().queued_actions.push(QueuedAction::RemoveExtension {
                                id: target_id,
                                confirm: false,
                            });
                            shared.borrow_mut().send_hub(&Frame::event(json!({
                                "op": "api_reply",
                                "contextId": context_id,
                                "requestId": request_id,
                                "ok": true,
                                "json": "",
                            })));
                        }),
                    );
                }
                _ => {
                    self.send_api_reply(
                        context_id,
                        request_id,
                        false,
                        &format!("{name} is not supported by this browser."),
                    );
                }
            }
            return;
        }

        if store == "firefox" && name.starts_with("amo.") {
            let id = str_arg(0);
            match name {
                "amo.getAddonByID" => {
                    let payload = if id.is_empty() {
                        "null".to_string()
                    } else {
                        self.dispatcher
                            .amo_addon(&id)
                            .map_or_else(|| "null".to_string(), |v| v.to_string())
                    };
                    self.send_api_reply(context_id, request_id, true, &payload);
                }
                "amo.install" => {
                    let Some(install_id) = args.first().and_then(Value::as_i64) else {
                        self.send_api_reply(context_id, request_id, false, "Bad install request.");
                        return;
                    };
                    let url = str_arg(1);
                    let hash = str_arg(2);
                    self.handle_amo_install(context_id, request_id, install_id, url, hash);
                }
                "amo.cancel" => self.send_api_reply(context_id, request_id, true, ""),
                "amo.uninstall" => {
                    let removed = self.finish_uninstall(&id);
                    self.send_api_reply(context_id, request_id, true, if removed { "true" } else { "false" });
                }
                "amo.setEnabled" => {
                    let enabled = args.get(1).and_then(Value::as_bool).unwrap_or(true);
                    self.set_extension_enabled(&id, enabled);
                    self.send_api_reply(context_id, request_id, true, "");
                }
                _ => self.send_api_reply(
                    context_id,
                    request_id,
                    false,
                    &format!("{name} is not supported by this browser."),
                ),
            }
            return;
        }

        self.send_api_reply(context_id, request_id, false, "This store API is not available here.");
    }

    fn handle_cws_begin_install(&mut self, context_id: u64, request_id: u64, args: &[Value]) {
        let Some(details) = args.first().and_then(Value::as_object) else {
            self.send_api_reply(context_id, request_id, true, &json!("unknown_error").to_string());
            return;
        };
        let id = details.get("id").and_then(Value::as_str).unwrap_or("").to_string();
        let manifest = details
            .get("manifest")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let localized_name = details.get("localizedName").and_then(Value::as_str).unwrap_or("");

        if chrome_web_store_download_url(&id, REPORTED_CHROME_VERSION).is_none() {
            self.send_api_reply(context_id, request_id, true, &json!("invalid_id").to_string());
            return;
        }
        if self
            .store
            .as_ref()
            .is_some_and(|s| s.installs_in_progress.contains(&id))
        {
            self.send_api_reply(context_id, request_id, true, &json!("install_in_progress").to_string());
            return;
        }

        let preview = preview_manifest(&manifest, localized_name);
        if !preview.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            self.send_api_reply(context_id, request_id, true, &json!("manifest_error").to_string());
            return;
        }
        let display_name = preview
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(&id)
            .to_string();
        let warnings: Vec<String> = preview
            .get("warnings")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default();
        let is_update = self.dispatcher.management_info(&id).is_some();

        if let Some(store) = self.store.as_mut() {
            store.installs_in_progress.insert(id.clone());
        }

        let title = if is_update {
            format!("Reinstall \"{display_name}\" from the Chrome Web Store?")
        } else {
            format!("Add \"{display_name}\"?")
        };
        let message = if warnings.is_empty() {
            String::new()
        } else {
            "It can:".to_string()
        };

        if let Some(store) = self.store.as_mut() {
            store.approved_cws_installs.insert(format!("pending:{id}"), manifest);
        }
        let shared = Rc::clone(&self.shared);
        let ext_id = id.clone();
        self.shared.borrow_mut().request_prompt(
            title,
            message,
            warnings,
            "Add extension".to_string(),
            Box::new(move |accepted| {
                let mut s = shared.borrow_mut();
                s.queued_actions
                    .push(QueuedAction::FinishCwsBegin { id: ext_id, accepted });
                s.send_hub(&Frame::event(json!({
                    "op": "api_reply",
                    "contextId": context_id,
                    "requestId": request_id,
                    "ok": true,
                    "json": if accepted { "\"\"" } else { "\"user_cancelled\"" },
                })));
            }),
        );
    }

    fn handle_cws_complete_install(&mut self, context_id: u64, request_id: u64, extension_id: &str) {
        let approved = self
            .store
            .as_mut()
            .and_then(|s| s.approved_cws_installs.remove(extension_id));
        let Some(approved_manifest) = approved else {
            if let Some(s) = self.store.as_mut() {
                s.installs_in_progress.remove(extension_id);
            }
            self.send_api_reply(context_id, request_id, false, "No install of this item was approved.");
            return;
        };

        let Some(url) = chrome_web_store_download_url(extension_id, REPORTED_CHROME_VERSION) else {
            if let Some(s) = self.store.as_mut() {
                s.installs_in_progress.remove(extension_id);
            }
            self.send_api_reply(context_id, request_id, false, "Invalid extension id.");
            return;
        };

        let ext_id = extension_id.to_string();
        let dispatcher = self.dispatcher.clone();
        let shared = Rc::clone(&self.shared);
        let packages_dir = self
            .store
            .as_ref()
            .map(|s| s.packages_dir().to_path_buf())
            .unwrap_or_else(|| std::env::temp_dir().join("zendia-ext-packages"));

        self.shared.borrow_mut().request_hub_download(
            url,
            Box::new(move |result| match result {
                Err(error) => {
                    shared.borrow_mut().send_hub(&Frame::event(json!({
                        "op": "api_reply",
                        "contextId": context_id,
                        "requestId": request_id,
                        "ok": false,
                        "json": error,
                    })));
                }
                Ok(bytes) => {
                    let expectation = StoreExpectation {
                        id: Some(&ext_id),
                        hash: None,
                        approved_manifest: Some(&approved_manifest),
                    };
                    if let Err(err) = check_store_package(&bytes, &expectation) {
                        shared.borrow_mut().send_hub(&Frame::event(json!({
                            "op": "api_reply",
                            "contextId": context_id,
                            "requestId": request_id,
                            "ok": false,
                            "json": format!("Package is invalid: {err}"),
                        })));
                        return;
                    }
                    let was_installed = dispatcher.management_info(&ext_id).is_some();
                    match install_package_into_dispatcher(&dispatcher, bytes.clone(), "", "auto") {
                        Ok(installed_id) => {
                            let mut store = StoreState::new(packages_dir);
                            store.kept = store.read_kept_packages().into_iter().map(|(k, _)| k).collect();
                            let _ = store.save_kept_package(&installed_id, "chrome", &bytes);
                            if let Some(bg_url) = dispatcher.background_url(&installed_id) {
                                shared.borrow_mut().send_hub(&Frame::event(json!({
                                    "op": "stop_background_page",
                                    "extensionId": installed_id,
                                })));
                                shared.borrow_mut().send_hub(&Frame::event(json!({
                                    "op": "start_background_page",
                                    "extensionId": installed_id,
                                    "url": bg_url,
                                })));
                            }
                            dispatcher.notify_installed(
                                &installed_id,
                                if was_installed { "update" } else { "install" },
                                None,
                            );
                            if let Some(info) = dispatcher.management_info(&installed_id) {
                                shared.borrow_mut().send_hub(&Frame::event(json!({
                                    "op": "deliver_to_extension",
                                    "extensionId": store_page_key("chrome"),
                                    "json": cws_installed_event(&info).to_string(),
                                })));
                            }
                            shared.borrow_mut().send_hub(&Frame::event(json!({
                                "op": "api_reply",
                                "contextId": context_id,
                                "requestId": request_id,
                                "ok": true,
                                "json": "",
                            })));
                            shared.borrow_mut().ui_dirty = true;
                            shared.borrow_mut().network_interest_dirty = true;
                        }
                        Err(error) => {
                            shared.borrow_mut().send_hub(&Frame::event(json!({
                                "op": "api_reply",
                                "contextId": context_id,
                                "requestId": request_id,
                                "ok": false,
                                "json": error,
                            })));
                        }
                    }
                }
            }),
        );
    }

    fn handle_amo_install(&mut self, context_id: u64, request_id: u64, install_id: i64, url: String, hash: String) {
        if !url.starts_with("https://") {
            self.send_api_reply(
                context_id,
                request_id,
                false,
                "Extension packages are only downloaded over HTTPS.",
            );
            return;
        }
        self.send_to_context(
            context_id,
            &amo_install_progress(install_id, "onDownloadStarted", "STATE_DOWNLOADING", None, None),
        );

        let dispatcher = self.dispatcher.clone();
        let shared = Rc::clone(&self.shared);
        let packages_dir = self
            .store
            .as_ref()
            .map(|s| s.packages_dir().to_path_buf())
            .unwrap_or_else(|| std::env::temp_dir().join("zendia-ext-packages"));

        self.shared.borrow_mut().request_hub_download(
            url,
            Box::new(move |result| {
                let bytes = match result {
                    Ok(b) => b,
                    Err(error) => {
                        shared.borrow_mut().send_hub(&Frame::event(json!({
                            "op": "deliver_to_context",
                            "contextId": context_id,
                            "json": amo_install_progress(install_id, "onDownloadFailed", "STATE_DOWNLOAD_FAILED", Some("ERROR_NETWORK_FAILURE"), None).to_string(),
                        })));
                        shared.borrow_mut().send_hub(&Frame::event(json!({
                            "op": "api_reply",
                            "contextId": context_id,
                            "requestId": request_id,
                            "ok": false,
                            "json": error,
                        })));
                        return;
                    }
                };

                let size = bytes.len();
                shared.borrow_mut().send_hub(&Frame::event(json!({
                    "op": "deliver_to_context",
                    "contextId": context_id,
                    "json": amo_install_progress(install_id, "onDownloadEnded", "STATE_DOWNLOADED", None, Some(size)).to_string(),
                })));

                let expectation = StoreExpectation {
                    id: None,
                    hash: (!hash.is_empty()).then_some(hash.as_str()),
                    approved_manifest: None,
                };
                let summary = match check_store_package(&bytes, &expectation) {
                    Ok(s) => s,
                    Err(error) => {
                        shared.borrow_mut().send_hub(&Frame::event(json!({
                            "op": "deliver_to_context",
                            "contextId": context_id,
                            "json": amo_install_progress(install_id, "onDownloadFailed", "STATE_DOWNLOAD_FAILED", Some("ERROR_CORRUPT_FILE"), None).to_string(),
                        })));
                        shared.borrow_mut().send_hub(&Frame::event(json!({
                            "op": "api_reply",
                            "contextId": context_id,
                            "requestId": request_id,
                            "ok": false,
                            "json": format!("Package is invalid: {error}"),
                        })));
                        return;
                    }
                };

                let is_update = dispatcher.management_info(&summary.id).is_some();
                let title = if is_update {
                    format!("Reinstall \"{}\" from {}?", summary.name, store_display_name("firefox"))
                } else {
                    format!("Add \"{}\"?", summary.name)
                };
                let message = if summary.warnings.is_empty() {
                    String::new()
                } else {
                    "It can:".to_string()
                };
                let shared_for_prompt = Rc::clone(&shared);
                shared.borrow_mut().request_prompt(
                    title,
                    message,
                    summary.warnings,
                    "Add extension".to_string(),
                    Box::new(move |accepted| {
                        if !accepted {
                            shared_for_prompt.borrow_mut().send_hub(&Frame::event(json!({
                                "op": "deliver_to_context",
                                "contextId": context_id,
                                "json": amo_install_progress(install_id, "onInstallCancelled", "STATE_CANCELLED", None, None).to_string(),
                            })));
                            shared_for_prompt.borrow_mut().send_hub(&Frame::event(json!({
                                "op": "api_reply",
                                "contextId": context_id,
                                "requestId": request_id,
                                "ok": false,
                                "json": "User cancelled install",
                            })));
                            return;
                        }

                        shared_for_prompt.borrow_mut().send_hub(&Frame::event(json!({
                            "op": "deliver_to_context",
                            "contextId": context_id,
                            "json": amo_install_progress(install_id, "onInstallStarted", "STATE_INSTALLING", None, Some(size)).to_string(),
                        })));

                        match install_package_into_dispatcher(&dispatcher, bytes.clone(), "", "auto") {
                            Ok(installed_id) => {
                                let mut store = StoreState::new(packages_dir);
                                store.kept = store
                                    .read_kept_packages()
                                    .into_iter()
                                    .map(|(k, _)| k)
                                    .collect();
                                let _ = store.save_kept_package(&installed_id, "firefox", &bytes);
                                if let Some(bg_url) = dispatcher.background_url(&installed_id) {
                                    shared_for_prompt.borrow_mut().send_hub(&Frame::event(json!({
                                        "op": "stop_background_page",
                                        "extensionId": installed_id,
                                    })));
                                    shared_for_prompt.borrow_mut().send_hub(&Frame::event(json!({
                                        "op": "start_background_page",
                                        "extensionId": installed_id,
                                        "url": bg_url,
                                    })));
                                }
                                dispatcher.notify_installed(
                                    &installed_id,
                                    if is_update { "update" } else { "install" },
                                    None,
                                );
                                shared_for_prompt.borrow_mut().send_hub(&Frame::event(json!({
                                    "op": "deliver_to_context",
                                    "contextId": context_id,
                                    "json": amo_install_progress(install_id, "onInstallEnded", "STATE_INSTALLED", None, Some(size)).to_string(),
                                })));
                                for ev in ["onInstalling", "onInstalled"] {
                                    shared_for_prompt.borrow_mut().send_hub(&Frame::event(json!({
                                        "op": "deliver_to_extension",
                                        "extensionId": store_page_key("firefox"),
                                        "json": amo_event(ev, &installed_id).to_string(),
                                    })));
                                }
                                shared_for_prompt.borrow_mut().send_hub(&Frame::event(json!({
                                    "op": "api_reply",
                                    "contextId": context_id,
                                    "requestId": request_id,
                                    "ok": true,
                                    "json": "true",
                                })));
                                shared_for_prompt.borrow_mut().ui_dirty = true;
                                shared_for_prompt.borrow_mut().network_interest_dirty = true;
                            }
                            Err(error) => {
                                shared_for_prompt.borrow_mut().send_hub(&Frame::event(json!({
                                    "op": "deliver_to_context",
                                    "contextId": context_id,
                                    "json": amo_install_progress(install_id, "onInstallFailed", "STATE_INSTALL_FAILED", Some("ERROR_CORRUPT_FILE"), None).to_string(),
                                })));
                                shared_for_prompt.borrow_mut().send_hub(&Frame::event(json!({
                                    "op": "api_reply",
                                    "contextId": context_id,
                                    "requestId": request_id,
                                    "ok": false,
                                    "json": error,
                                })));
                            }
                        }
                    }),
                );
            }),
        );
    }

    fn send_api_reply(&mut self, context_id: u64, request_id: u64, ok: bool, payload: &str) {
        self.shared.borrow_mut().send_hub(&Frame::event(json!({
            "op": "api_reply",
            "contextId": context_id,
            "requestId": request_id,
            "ok": ok,
            "json": payload,
        })));
    }

    fn send_to_context(&mut self, context_id: u64, payload: &Value) {
        self.shared.borrow_mut().send_hub(&Frame::event(json!({
            "op": "deliver_to_context",
            "contextId": context_id,
            "json": payload.to_string(),
        })));
    }

    fn alloc_context_id(&mut self) -> u64 {
        let id = self.next_context_id;
        self.next_context_id = self.next_context_id.wrapping_add(1).max(1);
        id
    }

    fn install_package_bytes(
        &mut self,
        bytes: Vec<u8>,
        id_override: &str,
        base_url: &str,
        keep: bool,
    ) -> Result<Value, String> {
        let was_installed = false;
        let id = self.install_package_internal(bytes.clone(), id_override, base_url)?;
        if keep && let Some(store) = self.store.as_mut() {
            let store_kind = if self
                .dispatcher
                .info_for(&id)
                .and_then(|v| {
                    v.get("baseUrl")
                        .and_then(Value::as_str)
                        .map(|u| u.starts_with("moz-extension://"))
                })
                .unwrap_or(false)
            {
                "firefox"
            } else {
                "chrome"
            };
            let _ = store.save_kept_package(&id, store_kind, &bytes);
        }
        self.disabled_ids.remove(&id);
        self.stop_background_page(&id);
        self.start_background_page(
            &id,
            if was_installed {
                StartReason::Update
            } else {
                StartReason::Install
            },
        );
        self.push_network_interest();
        self.push_ui_state();
        Ok(self
            .dispatcher
            .management_info(&id)
            .unwrap_or_else(|| json!({ "id": id })))
    }

    fn install_unpacked_map(&mut self, req: &Value, seed: &str) -> Result<Value, String> {
        let mut files = Files::new();
        if let Some(entries) = req.get("files").and_then(Value::as_array) {
            for entry in entries {
                let path = entry
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "file entry missing `path`".to_string())?;
                let data_b64 = entry
                    .get("data")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "file entry missing `data`".to_string())?;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data_b64)
                    .map_err(|error| format!("invalid base64 for `{path}`: {error}"))?;
                files.insert(path, bytes);
            }
        } else if let Some(map) = req.get("files").and_then(Value::as_object) {
            for (path, val) in map {
                let data_b64 = val
                    .as_str()
                    .ok_or_else(|| format!("file entry `{path}` must be base64 string"))?;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data_b64)
                    .map_err(|error| format!("invalid base64 for `{path}`: {error}"))?;
                files.insert(path.as_str(), bytes);
            }
        } else {
            return Err("missing `files` for unpacked install".to_string());
        }
        let id = self.install_files_with_zendia_defaults(files, None, "", "auto", Some(seed))?;
        self.loaded_cli_ids.insert(id.clone());
        self.disabled_ids.remove(&id);
        self.stop_background_page(&id);
        self.start_background_page(&id, StartReason::Install);
        self.push_network_interest();
        self.push_ui_state();
        Ok(self
            .dispatcher
            .management_info(&id)
            .unwrap_or_else(|| json!({ "id": id })))
    }

    fn install_package_internal(
        &mut self,
        bytes: Vec<u8>,
        id_override: &str,
        base_url: &str,
    ) -> Result<String, String> {
        let id = install_package_into_dispatcher(&self.dispatcher, bytes, id_override, base_url)?;
        self.apply_default_unpinned_if_new(&id);
        Ok(id)
    }

    fn install_directory_internal(&mut self, path: &Path, id_override: &str, base_url: &str) -> Result<String, String> {
        let canonical = std::fs::canonicalize(path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned());
        let files = Files::from_dir(path).map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
        self.install_files_with_zendia_defaults(files, None, id_override, base_url, Some(&canonical))
    }

    fn install_files_with_zendia_defaults(
        &mut self,
        files: Files,
        crx_id: Option<&str>,
        id_override: &str,
        base_url: &str,
        derive_id_from: Option<&str>,
    ) -> Result<String, String> {
        let id = install_files_into_dispatcher(&self.dispatcher, files, crx_id, id_override, base_url, derive_id_from)?;
        self.apply_default_unpinned_if_new(&id);
        Ok(id)
    }

    /// Matches Zen/Zendia's `CustomizableUI` default (`AREA_ADDONS`): newly installed extensions live in the Site Data
    /// Panel's Extensions grid until the user explicitly pins them to the toolbar.
    fn apply_default_unpinned_if_new(&self, id: &str) {
        let has_saved_pinned = self
            .shared
            .borrow()
            .storage
            .as_ref()
            .and_then(|s| s.read(id, "ui"))
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|v| v.get("pinned").and_then(Value::as_bool))
            .is_some();
        if !has_saved_pinned {
            let _ = self
                .dispatcher
                .host_event("setActionPinned", &json!({ "extensionId": id, "pinned": false }));
        }
    }

    fn start_background_page(&mut self, id: &str, reason: StartReason) {
        if self.disabled_ids.contains(id) {
            return;
        }
        let Some(url) = self.dispatcher.background_url(id) else {
            if reason != StartReason::Startup {
                self.dispatcher.notify_installed(
                    id,
                    if reason == StartReason::Update {
                        "update"
                    } else {
                        "install"
                    },
                    None,
                );
            }
            return;
        };
        self.pending_start_reasons.insert(id.to_string(), reason);
        self.shared.borrow_mut().send_hub(&Frame::event(json!({
            "op": "start_background_page",
            "extensionId": id,
            "url": url,
        })));
    }

    fn stop_background_page(&mut self, id: &str) {
        self.pending_start_reasons.remove(id);
        self.shared.borrow_mut().send_hub(&Frame::event(json!({
            "op": "stop_background_page",
            "extensionId": id,
        })));
    }

    fn reload_extension(&mut self, id: &str) {
        self.stop_background_page(id);
        self.start_background_page(id, StartReason::Startup);
        let base_url = self
            .dispatcher
            .info_for(id)
            .and_then(|v| v.get("baseUrl").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        self.shared.borrow_mut().send_hub(&Frame::event(json!({
            "op": "reload_extension_pages",
            "extensionId": id,
            "baseUrl": base_url,
        })));
    }

    fn set_extension_enabled(&mut self, id: &str, enabled: bool) {
        if self.dispatcher.management_info(id).is_none() {
            return;
        }
        if enabled {
            if self.disabled_ids.remove(id) {
                self.start_background_page(id, StartReason::Startup);
            }
        } else if self.disabled_ids.insert(id.to_string()) {
            self.stop_background_page(id);
        }
        self.push_network_interest();
        self.push_ui_state();
    }

    fn remove_extension(&mut self, id: &str, confirm: bool) {
        if self.dispatcher.management_info(id).is_none() {
            return;
        }
        if !confirm {
            self.finish_uninstall(id);
            return;
        }
        let name = self
            .dispatcher
            .management_info(id)
            .and_then(|v| v.get("name").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| id.to_string());
        let target_id = id.to_string();
        let shared = Rc::clone(&self.shared);
        self.shared.borrow_mut().request_prompt(
            format!("Remove \"{name}\"?"),
            "The extension and the data it stored will be removed.".to_string(),
            Vec::new(),
            "Remove".to_string(),
            Box::new(move |accepted| {
                if accepted {
                    shared.borrow_mut().queued_actions.push(QueuedAction::RemoveExtension {
                        id: target_id,
                        confirm: false,
                    });
                }
            }),
        );
    }

    fn finish_uninstall(&mut self, id: &str) -> bool {
        if self.dispatcher.management_info(id).is_none() {
            return false;
        }
        self.stop_background_page(id);
        self.dispatcher.uninstall(id);
        self.disabled_ids.remove(id);
        self.loaded_cli_ids.remove(id);
        if let Some(store) = self.store.as_mut() {
            // Re-sync kept list from disk in case a store install updated it in a callback.
            store.kept = store.read_kept_packages().into_iter().map(|(k, _)| k).collect();
            store.remove_kept_package(id);
        }
        if let Some(storage) = self.shared.borrow().storage.as_ref() {
            storage.remove_all(id);
        }
        // Notify open store pages.
        {
            let mut shared = self.shared.borrow_mut();
            shared.send_hub(&Frame::event(json!({
                "op": "deliver_to_extension",
                "extensionId": store_page_key("chrome"),
                "json": cws_uninstalled_event(id).to_string(),
            })));
            for ev in ["onUninstalling", "onUninstalled"] {
                shared.send_hub(&Frame::event(json!({
                    "op": "deliver_to_extension",
                    "extensionId": store_page_key("firefox"),
                    "json": amo_event(ev, id).to_string(),
                })));
            }
        }
        self.push_network_interest();
        self.push_ui_state();
        true
    }

    fn push_network_interest(&mut self) {
        let interest = self.dispatcher.host_event("networkInterest", &json!({}));
        let mut stages: u32 = 0;
        const STAGE_NAMES: [&str; 9] = [
            "beforeRequest",
            "beforeSendHeaders",
            "sendHeaders",
            "headersReceived",
            "authRequired",
            "beforeRedirect",
            "responseStarted",
            "completed",
            "errorOccurred",
        ];
        if let Some(names) = interest.get("stages").and_then(Value::as_array) {
            for name in names.iter().filter_map(Value::as_str) {
                if let Some(idx) = STAGE_NAMES.iter().position(|s| *s == name) {
                    stages |= 1u32 << idx;
                }
            }
        }
        if interest.get("navigation").and_then(Value::as_bool).unwrap_or(false) {
            stages |= 1u32 << 16;
        }
        // If every extension is disabled, tell renderers not to report network stages.
        if self
            .dispatcher
            .installed_ids()
            .iter()
            .all(|id| self.disabled_ids.contains(id))
        {
            stages = 0;
        }
        if stages != self.last_network_interest {
            self.last_network_interest = stages;
            self.shared.borrow_mut().send_hub(&Frame::event(json!({
                "op": "set_network_interest",
                "stages": stages,
            })));
        }
    }

    fn push_ui_state(&mut self) {
        let snapshot = self.build_ui_snapshot(self.active_tab_id);
        self.shared.borrow_mut().send_ui(&Frame::event(json!({
            "op": "ui_state",
            "snapshot": snapshot,
        })));
    }

    fn build_ui_snapshot(&self, tab_id: Option<i64>) -> Value {
        let mut args = json!({});
        if let Some(t) = tab_id {
            args["tabId"] = json!(t);
        }
        let ui_list = self.dispatcher.host_event("uiState", &args);
        let mut extensions = Vec::new();

        if let Some(items) = ui_list.as_array() {
            for item in items {
                let Some(ext_id) = item.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let mgmt = self.dispatcher.management_info(ext_id).unwrap_or(Value::Null);
                let enabled =
                    !self.disabled_ids.contains(ext_id) && mgmt.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                let description = mgmt
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let mut permissions: Vec<String> = mgmt
                    .get("permissions")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                    .unwrap_or_default();
                if let Some(hosts) = mgmt.get("hostPermissions").and_then(Value::as_array) {
                    for h in hosts.iter().filter_map(Value::as_str) {
                        permissions.push(h.to_string());
                    }
                }

                let icon_data = self.resolve_icon_data(ext_id, item, 32);

                let mut enriched = item.clone();
                if let Some(obj) = enriched.as_object_mut() {
                    obj.insert("enabled".to_string(), json!(enabled));
                    obj.insert("description".to_string(), json!(description));
                    obj.insert("permissions".to_string(), json!(permissions));
                    if let Some(icon) = icon_data {
                        obj.insert("iconData".to_string(), icon);
                    }
                }
                extensions.push(enriched);
            }
        }

        let commands = self.dispatcher.host_event("commands", &json!({}));
        json!({
            "extensions": extensions,
            "commands": commands,
        })
    }

    /// Resolves the best icon for `ext_id` (checking `action.icon` first, then manifest `icons`) into an inline
    /// base64-encoded image or RGBA pixel payload so the UI renders real extension icons with zero extra IPCs.
    fn resolve_icon_data(&self, ext_id: &str, ui_item: &Value, desired_px: i64) -> Option<Value> {
        if let Some(action_icon) = ui_item.get("action").and_then(|a| a.get("icon"))
            && let Some(resolved) = self.encode_core_icon(ext_id, action_icon, desired_px)
        {
            return Some(resolved);
        }
        let manifest_icons = ui_item.get("icons")?;
        let wrapped = json!({ "kind": "paths", "paths": manifest_icons });
        self.encode_core_icon(ext_id, &wrapped, desired_px)
    }

    fn encode_core_icon(&self, ext_id: &str, icon: &Value, desired_px: i64) -> Option<Value> {
        let obj = icon.as_object()?;
        let kind = obj.get("kind").and_then(Value::as_str)?;
        fn pick_best(list: &[Value], desired_px: i64) -> Option<&Value> {
            let mut best: Option<(&Value, i64)> = None;
            for entry in list {
                let sz = entry.get("size").and_then(Value::as_i64).unwrap_or(0);
                let better = match best {
                    None => true,
                    Some((_, best_sz)) => {
                        (sz >= desired_px && (best_sz < desired_px || sz < best_sz))
                            || (sz < desired_px && best_sz < desired_px && sz > best_sz)
                    }
                };
                if better {
                    best = Some((entry, sz));
                }
            }
            best.map(|(v, _)| v)
        }

        if kind == "paths" {
            let paths = obj.get("paths").and_then(Value::as_array)?;
            let chosen = pick_best(paths, desired_px)?;
            let path = chosen.get("path").and_then(Value::as_str)?;
            let bytes = self
                .dispatcher
                .with_extension(ext_id, |e| e.files.get(path).map(<[u8]>::to_vec))
                .flatten()?;
            let mime = mime_for_icon_path(path, &bytes);
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
            return Some(json!({
                "kind": "encoded",
                "mime": mime,
                "data": encoded,
            }));
        }

        if kind == "pixels" {
            let images = obj.get("images").and_then(Value::as_array)?;
            let chosen = pick_best(images, desired_px)?;
            let width = chosen.get("width").and_then(Value::as_u64)?;
            let height = chosen.get("height").and_then(Value::as_u64)?;
            let rgba = chosen.get("rgba").and_then(Value::as_str)?;
            return Some(json!({
                "kind": "rgba",
                "width": width,
                "height": height,
                "data": rgba,
            }));
        }

        None
    }
}

fn mime_for_icon_path(path: &str, bytes: &[u8]) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".svg") || bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
        "image/svg+xml"
    } else if lower.ends_with(".png") || bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

fn find_active_tab_id(browser_state: &Value) -> Option<i64> {
    let last_win = browser_state.get("lastFocusedWindowId").and_then(Value::as_i64);
    let windows = browser_state.get("windows").and_then(Value::as_array)?;
    let pick_in = |win: &Value| -> Option<i64> {
        win.get("tabs")?
            .as_array()?
            .iter()
            .find(|t| t.get("active").and_then(Value::as_bool).unwrap_or(false))
            .and_then(|t| t.get("id").and_then(Value::as_i64))
    };
    if let Some(wid) = last_win
        && let Some(win) = windows
            .iter()
            .find(|w| w.get("id").and_then(Value::as_i64) == Some(wid))
        && let Some(tab) = pick_in(win)
    {
        return Some(tab);
    }
    windows.iter().find_map(pick_in)
}

fn install_package_into_dispatcher(
    dispatcher: &Dispatcher,
    bytes: Vec<u8>,
    id_override: &str,
    base_url: &str,
) -> Result<String, String> {
    let seed: String = webext::encoding::sha256(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let pkg = Package::open(bytes).map_err(|e| e.to_string())?;
    let files = Files::from_package(&pkg).map_err(|e| e.to_string())?;
    let crx_id = pkg.crx_header().and_then(|c| c.extension_id());
    let signed_id = pkg.mozilla_signed_id();
    let effective_override = if id_override.is_empty() && crx_id.is_none() {
        signed_id.as_deref().unwrap_or("")
    } else {
        id_override
    };
    install_files_into_dispatcher(
        dispatcher,
        files,
        crx_id.as_deref(),
        effective_override,
        base_url,
        Some(&seed),
    )
}

fn install_files_into_dispatcher(
    dispatcher: &Dispatcher,
    files: Files,
    crx_id: Option<&str>,
    id_override: &str,
    base_url: &str,
    derive_id_from: Option<&str>,
) -> Result<String, String> {
    let ext = load_extension(&files, crx_id).map_err(|diags| {
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| format!("{}: {}", d.field, d.message))
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let id = if !id_override.is_empty() {
        id_override.to_string()
    } else if let Some(id) = ext.id.clone() {
        id
    } else if let Some(seed) = derive_id_from {
        webext::encoding::chrome_extension_id_from_public_key(seed.as_bytes())
    } else {
        return Err("the package declares no extension ID".to_string());
    };
    let resolved_base_url = if base_url.is_empty() || base_url == "auto" {
        webext::serve::default_base_url(&id, &ext.manifest)
    } else {
        base_url.to_string()
    };
    dispatcher.install(
        &id,
        ext.manifest,
        InstallOptions {
            base_url: resolved_base_url,
            locales: ext.locales,
            files,
        },
    );
    Ok(id)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Runs the `ExtensionServer` main loop until the UI connection closes.
pub fn run(ui_stream: UnixStream, hub_stream: Option<UnixStream>) -> io::Result<()> {
    let mut ui_reader = ui_stream.try_clone()?;
    let (tx, rx) = mpsc::channel::<Inbox>();

    let ui_tx = tx.clone();
    std::thread::Builder::new()
        .name("ext-ui-reader".to_string())
        .spawn(move || {
            while let Ok(Some(frame)) = read_frame(&mut ui_reader) {
                if ui_tx.send(Inbox::Ui(frame)).is_err() {
                    break;
                }
            }
            let _ = ui_tx.send(Inbox::UiDisconnected);
        })?;

    let hub_writer = if let Some(stream) = hub_stream {
        let mut hub_reader = stream.try_clone()?;
        let hub_tx = tx.clone();
        std::thread::Builder::new()
            .name("ext-hub-reader".to_string())
            .spawn(move || {
                while let Ok(Some(frame)) = read_frame(&mut hub_reader) {
                    if hub_tx.send(Inbox::Hub(frame)).is_err() {
                        break;
                    }
                }
                let _ = hub_tx.send(Inbox::HubDisconnected);
            })?;
        Some(stream)
    } else {
        None
    };

    let mut state = ServerState::new(ui_stream, hub_writer);

    loop {
        let wait_duration = state
            .shared
            .borrow()
            .next_alarm_at_ms
            .map(|at| Duration::from_millis((at - now_ms()).max(0) as u64));

        let msg = match wait_duration {
            Some(dur) => match rx.recv_timeout(dur) {
                Ok(m) => Some(m),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(m) => Some(m),
                Err(_) => break,
            },
        };

        if let Some(msg) = msg {
            match msg {
                Inbox::Ui(frame) => state.handle_ui_frame(frame),
                Inbox::UiDisconnected => break,
                Inbox::Hub(frame) => state.handle_hub_frame(frame),
                Inbox::HubDisconnected => {
                    state.shared.borrow_mut().hub_writer = None;
                }
            }
        }

        // Fire due alarms if any.
        let due = state.shared.borrow().next_alarm_at_ms.is_some_and(|at| now_ms() >= at);
        if due {
            state.shared.borrow_mut().next_alarm_at_ms = None;
            state.dispatcher.tick();
        }

        state.flush_side_effects();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_server_handshake_and_fixture_install_work() {
        let (ui_client, ui_server) = UnixStream::pair().unwrap();
        let (hub_client, hub_server) = UnixStream::pair().unwrap();

        let thread = std::thread::spawn(move || {
            run(ui_server, Some(hub_server)).unwrap();
        });

        let mut ui = ui_client;
        let mut hub = hub_client;

        // 1. Protocol version handshake.
        write_frame(
            &mut ui,
            &Frame::request(
                1,
                json!({ "op": "hello", "expectedService": SERVICE_NAME, "protocolVersion": PROTOCOL_VERSION }),
            ),
        )
        .unwrap();
        let hello = read_frame(&mut ui).unwrap().unwrap();
        assert_eq!(hello.id, 1);
        assert_eq!(hello.json["data"]["protocolVersion"], PROTOCOL_VERSION);

        // 2. Init with a temp data dir (sandbox disabled so test can clean up easily).
        let tmp = std::env::temp_dir().join(format!("zendia-extsrv-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../cgx-webext/Tests/Fixtures/storage-popup");
        let load_list = if fixture.is_dir() {
            vec![fixture.to_string_lossy().into_owned()]
        } else {
            Vec::new()
        };

        // SAFETY: test runs single-threaded for this env flag before server init reads it.
        unsafe { std::env::set_var("ZENDIA_DISABLE_SANDBOX", "1") };
        write_frame(
            &mut ui,
            &Frame::request(
                2,
                json!({
                    "op": "init",
                    "dataDir": tmp.to_string_lossy(),
                    "locale": "en-US",
                    "loadExtensions": load_list,
                    "autoAcceptPrompts": true,
                }),
            ),
        )
        .unwrap();

        // Expect ui_state event and init response.
        let mut saw_init_reply = false;
        let mut ui_snapshot = None;
        for _ in 0..4 {
            let f = read_frame(&mut ui).unwrap().unwrap();
            if f.kind == FrameKind::Response && f.id == 2 {
                saw_init_reply = true;
            } else if f.op() == Some("ui_state") {
                ui_snapshot = Some(f.json["snapshot"].clone());
            }
            if saw_init_reply && ui_snapshot.is_some() {
                break;
            }
        }
        assert!(saw_init_reply);
        if fixture.is_dir() {
            let exts = ui_snapshot.unwrap()["extensions"].as_array().unwrap().clone();
            assert_eq!(exts.len(), 1);
            let base_url = exts[0]["baseUrl"].as_str().unwrap();

            // 3. Hub can serve the extension's popup.html over the embedder channel!
            write_frame(
                &mut hub,
                &Frame::request(
                    10,
                    json!({ "op": "serve", "url": format!("{base_url}popup.html"), "initiator": "" }),
                ),
            )
            .unwrap();
            loop {
                let hf = read_frame(&mut hub).unwrap().unwrap();
                if hf.kind == FrameKind::Response && hf.id == 10 {
                    assert_eq!(hf.json["data"]["found"], true);
                    assert_eq!(hf.json["data"]["status"], 200);
                    assert!(!hf.binary.is_empty());
                    break;
                }
            }
        }

        drop(ui);
        drop(hub);
        thread.join().unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
