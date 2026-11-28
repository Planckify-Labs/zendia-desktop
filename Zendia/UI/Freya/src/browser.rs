/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The browser window: its tabs, its toolbar, and the page stage hosting one or more [`PageSurface`]s.

use crate::extensions::{ExtensionAdapter, ExtensionUiEvent};
use crate::hub::{self, startup};
use crate::hub_adapter::HubTabAdapter;
use crate::input;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::address_bar::{AddressBarLayer, use_provide_address_bar_state};
use crate::ui::chrome::menus::{
    MenuLayer, MenuViewState, open_hub_context_menu, open_hub_select_dropdown, use_provide_menu_state,
};
use crate::ui::chrome::sidebar::use_provide_sidebar_state;
use crate::ui::core_view::use_provide_core_with_observer;
use crate::ui::extensions::{
    ExtensionPopupOverlay, ExtensionPromptOverlay, match_extension_shortcut, use_provide_extension_popup_state,
};
use crate::ui::kit::{is_text_editing_key, use_provide_text_focus};
use crate::ui::library::media_preview::{MediaPreviewOverlay, use_provide_media_preview_state};
use crate::ui::library::sections::space_routing::use_provide_space_routing_view_state;
use crate::ui::library::sections::spaces::use_provide_spaces_view_state;
use crate::ui::library::use_provide_library_state;
use crate::ui::onboarding::{OnboardingOverlay, handle_onboarding_key_down, use_provide_onboarding_state};
use crate::ui::page::download_arc::{DownloadArcLayer, use_provide_download_arc};
use crate::ui::page::{
    PageArea, PageStage, PageSurfaceContext, PaneMode, VimOverlayLayer, use_provide_keymap, use_provide_page_surfaces,
};
use crate::ui::panels::use_provide_dock_state;
use crate::ui::settings::use_provide_settings_host;
use crate::ui::share::{ShareConfirmOverlay, use_provide_share_state};
use crate::ui::spaces::{
    GradientGeneratorOverlay, SpaceDeleteConfirmOverlay, SpaceIconPickerOverlay, use_provide_space_editing_state,
};
use crate::ui::theme::{ThemeRole, use_provide_theme, use_theme};
use crate::ui::vim::use_provide_vim_statusline_state;
use crate::ui::vim::{VimPickerOverlay, use_provide_region_picker_state, use_provide_vim_picker_state};
use crate::ui::webchat::use_provide_webchat_views;
use freya::prelude::*;
use hub_protocol::ipc_types::hub_client::messages as client;
use hub_protocol::ipc_types::hub_server::messages as server;
use hub_protocol::*;
use std::sync::{Arc, Mutex, OnceLock};

/// Most recent rich clipboard item (`text/html` + `text/plain`) inserted by a page copy or visual yank (`VIS-02`).
fn cached_rich_clipboard() -> &'static Mutex<Option<Web_Clipboard_SystemClipboardItem>> {
    static RICH_CLIPBOARD: OnceLock<Mutex<Option<Web_Clipboard_SystemClipboardItem>>> = OnceLock::new();
    RICH_CLIPBOARD.get_or_init(|| Mutex::new(None))
}
use zendia_browser_core::BrowserCore;
use zendia_browser_core::commands::Command;
use zendia_browser_core::events::{Event as CoreEvent, ViewRequest};
use zendia_browser_core::extension_source::ExtensionSource;
use zendia_browser_core::model::TabId;
use zendia_browser_core::stub::StubBackend;
use zendia_browser_core::tab_source::TabSource;
use zendia_browser_core::vim_keys::VimCommand;

fn cursor_icon(cursor: &Gfx_Cursor) -> CursorIcon {
    let Gfx_Cursor::Standard(cursor) = cursor else {
        // FIXME: Show the page's own cursor images.
        return CursorIcon::Default;
    };
    match cursor {
        Gfx_StandardCursor::None | Gfx_StandardCursor::Arrow => CursorIcon::Default,
        Gfx_StandardCursor::Hidden => CursorIcon::Default,
        Gfx_StandardCursor::Crosshair => CursorIcon::Crosshair,
        Gfx_StandardCursor::IBeam => CursorIcon::Text,
        Gfx_StandardCursor::ResizeHorizontal => CursorIcon::EwResize,
        Gfx_StandardCursor::ResizeVertical => CursorIcon::NsResize,
        Gfx_StandardCursor::ResizeDiagonalTLBR => CursorIcon::NwseResize,
        Gfx_StandardCursor::ResizeDiagonalBLTR => CursorIcon::NeswResize,
        Gfx_StandardCursor::ResizeColumn => CursorIcon::ColResize,
        Gfx_StandardCursor::ResizeRow => CursorIcon::RowResize,
        Gfx_StandardCursor::Hand => CursorIcon::Pointer,
        Gfx_StandardCursor::Help => CursorIcon::Help,
        Gfx_StandardCursor::OpenHand => CursorIcon::Grab,
        Gfx_StandardCursor::Drag => CursorIcon::Grabbing,
        Gfx_StandardCursor::DragCopy => CursorIcon::Copy,
        Gfx_StandardCursor::Move => CursorIcon::Move,
        Gfx_StandardCursor::Wait => CursorIcon::Wait,
        Gfx_StandardCursor::Disallowed => CursorIcon::NotAllowed,
        Gfx_StandardCursor::Eyedropper => CursorIcon::Crosshair,
        Gfx_StandardCursor::ZoomIn => CursorIcon::ZoomIn,
        Gfx_StandardCursor::ZoomOut => CursorIcon::ZoomOut,
    }
}

/// Routes incoming Hub messages to the Hub tab adapter (browser state) or the view-local [`PageSurfaceContext`].
fn handle_message(
    core: &StubBackend,
    adapter: &HubTabAdapter,
    page_ctx: &PageSurfaceContext,
    menu_state: &MenuViewState,
    message: HubMessage,
) {
    let hub = hub::connection();
    let mut surfaces = page_ctx.surfaces();
    let tracker = page_ctx.tracker();

    match message {
        HubMessage::ViewInstallBackingStores(client::ViewInstallBackingStores {
            view_id,
            page_id,
            bitmap_ids,
            backing_stores,
        }) => {
            if !adapter.has_view(view_id) {
                if let Some(front) = bitmap_ids.first() {
                    hub.post_or_log(server::ReleaseBackingStore {
                        page_id,
                        bitmap_id: *front,
                    });
                }
                return;
            }
            tracker
                .borrow_mut()
                .install_backing_stores(view_id, page_id, bitmap_ids, backing_stores);
        }
        HubMessage::ViewAddBackingStores(client::ViewAddBackingStores {
            view_id,
            bitmap_ids,
            backing_stores,
        }) => {
            tracker
                .borrow_mut()
                .add_backing_stores(view_id, bitmap_ids, backing_stores);
        }
        HubMessage::ViewRetireBackingStores(client::ViewRetireBackingStores { view_id, bitmap_ids }) => {
            tracker.borrow_mut().retire_backing_stores(view_id, &bitmap_ids);
        }
        HubMessage::ViewClearBackingStores(client::ViewClearBackingStores { view_id }) => {
            tracker.borrow_mut().clear_backing_stores(view_id);
        }
        HubMessage::ViewPresentBackingStore(present) => {
            let presentation = tracker.borrow_mut().present_backing_store(
                present.view_id,
                present.page_id,
                present.bitmap_id,
                present.size.clone(),
            );
            if let Some(bitmap_id) = presentation.release {
                hub.post_or_log(server::ReleaseBackingStore {
                    page_id: present.page_id,
                    bitmap_id,
                });
            }
            if let Some(frame) = presentation.frame {
                let mut surfaces = surfaces.write();
                let surface = surfaces.entry(present.view_id).or_default();
                surface.frame = Some(frame);
                surface.frame_seq = surface.frame_seq.wrapping_add(1);
                surface.frame_size = (present.size.width, present.size.height);
                surface.frame_device_pixel_ratio = present.device_pixel_ratio;
            }
        }
        HubMessage::ViewAcceptPresentedBackingStore(_) => {}
        HubMessage::ViewStateChanged(client::ViewStateChanged { view_id, state }) => {
            surfaces
                .write()
                .entry(view_id)
                .or_default()
                .set_background_from_color(&state.page_background_color);
            adapter.on_view_state_changed(view_id, &state);
        }
        HubMessage::ViewTitleChange(client::ViewTitleChange { view_id, title }) => {
            adapter.on_title_changed(view_id, title.to_string_lossy());
        }
        HubMessage::ViewUrlChange(client::ViewUrlChange { view_id, url }) => {
            adapter.on_url_changed(view_id, url.as_str().to_string());
        }
        HubMessage::ViewLoadingStateChange(client::ViewLoadingStateChange { view_id, is_loading }) => {
            adapter.note_auxiliary_loading(view_id, is_loading);
            adapter.on_loading_state_changed(view_id, is_loading);
        }
        HubMessage::ViewLoadStart(start) => {
            adapter.on_load_start(start.view_id);
        }
        HubMessage::ViewLoadFinish(finish) => {
            adapter.on_load_finish(finish.view_id);
        }
        HubMessage::ViewPageBackgroundColorChange(client::ViewPageBackgroundColorChange { view_id, color }) => {
            surfaces
                .write()
                .entry(view_id)
                .or_default()
                .set_background_from_color(&color);
        }
        HubMessage::ViewCursorChange(client::ViewCursorChange { view_id, cursor }) => {
            surfaces.write().entry(view_id).or_default().cursor = cursor_icon(&cursor);
        }
        HubMessage::ViewLinkHover(client::ViewLinkHover { view_id, url }) => {
            surfaces.write().entry(view_id).or_default().hovered_link = Some(url.as_str().to_string());
        }
        HubMessage::ViewLinkUnhover(client::ViewLinkUnhover { view_id }) => {
            surfaces.write().entry(view_id).or_default().hovered_link = None;
        }
        HubMessage::ViewFinishHandlingKeyEvent(_)
        | HubMessage::ViewFinishHandlingMouseEvent(_)
        | HubMessage::ViewReleaseInput(_) => {}
        HubMessage::ViewOpenNewView(client::ViewOpenNewView {
            view_id,
            new_view_id,
            new_view_state,
            activate_tab,
            ..
        }) => {
            surfaces
                .write()
                .entry(new_view_id)
                .or_default()
                .set_background_from_color(&new_view_state.page_background_color);
            adapter.adopt_view(
                new_view_id,
                &new_view_state,
                Some(TabId(view_id)),
                activate_tab == Web_HTML_ActivateTab::Yes,
            );
        }
        HubMessage::AppOpenViewInNewTab(client::AppOpenViewInNewTab {
            view_id,
            source_view_id,
            activate_tab,
            ..
        }) => match hub.send_sync(server::ViewDescription { view_id }) {
            Ok(description) => {
                surfaces
                    .write()
                    .entry(view_id)
                    .or_default()
                    .set_background_from_color(&description.state.page_background_color);
                adapter.adopt_view(
                    view_id,
                    &description.state,
                    source_view_id.map(TabId),
                    activate_tab == Web_HTML_ActivateTab::Yes,
                );
            }
            Err(error) => eprintln!("zendia: Unable to show view {view_id}: {error}"),
        },
        HubMessage::AppOpenUrlInNewTab(client::AppOpenUrlInNewTab {
            url,
            activate_tab,
            source_view_id,
            ..
        }) => {
            let _ = adapter.open_url_tab(
                Some(url),
                source_view_id.map(TabId),
                activate_tab == Web_HTML_ActivateTab::Yes,
            );
        }
        // FIXME: Open new windows, once this UI has more than one.
        HubMessage::AppOpenUrlInNewWindow(client::AppOpenUrlInNewWindow { url, .. }) => {
            let _ = adapter.open_url_tab(Some(url), None, true);
        }
        HubMessage::AppOpenNewTab(client::AppOpenNewTab { urls })
        | HubMessage::AppOpenNewWindow(client::AppOpenNewWindow { urls }) => {
            if urls.is_empty() {
                let _ = adapter.open_url_tab(None, None, true);
            }
            for url in urls {
                let _ = adapter.open_url_tab(Some(url), None, true);
            }
        }
        HubMessage::ViewActivateTab(client::ViewActivateTab { view_id }) => {
            adapter.activate_view(view_id);
        }
        HubMessage::ViewClose(client::ViewClose { view_id }) => {
            adapter.finish_close_tab(view_id);
        }
        HubMessage::AppSetClipboardText(client::AppSetClipboardText { text, .. }) => {
            if let Ok(text) = text.into_string() {
                *cached_rich_clipboard()
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = None;
                let _ = freya::clipboard::Clipboard::set(text);
            }
        }
        HubMessage::AppReadClipboardText(client::AppReadClipboardText { request_id, .. }) => {
            let text = freya::clipboard::Clipboard::get().unwrap_or_default();
            hub.post_or_log(server::AnswerClipboardText {
                request_id,
                text: libipc_rust::wire::Utf16String::from(text.as_str()),
            });
        }
        // A paste asks for the clipboard as an item of all its forms (`text/html`, `text/plain`, etc.); preserve rich
        // representations from the last page copy/yank when the system clipboard text still matches (`VIS-02`).
        HubMessage::AppReadClipboardItem(client::AppReadClipboardItem { request_id }) => {
            let text = freya::clipboard::Clipboard::get().unwrap_or_default();
            let rich = cached_rich_clipboard()
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .as_ref()
                .filter(|saved| !text.is_empty() && saved.plain_text_of().as_deref() == Some(text.as_str()))
                .map(Web_Clipboard_SystemClipboardItem::clone_bytes_only);
            let item = rich.unwrap_or_else(|| {
                if text.is_empty() {
                    Web_Clipboard_SystemClipboardItem::default()
                } else {
                    Web_Clipboard_SystemClipboardItem::plain_text(&text)
                }
            });
            hub.post_or_log(server::AnswerClipboardItem { request_id, item });
        }
        // What a page puts on the clipboard (a copy or visual yank, `navigator.clipboard.write`): keep all MIME forms
        // (`text/html` + `text/plain`) for rich pastes and sync plain text to the system clipboard (`VIS-02`).
        HubMessage::AppInsertClipboardItem(client::AppInsertClipboardItem { item }) => {
            let cloned = item.clone_bytes_only();
            if let Some(text) = cloned.plain_text_of() {
                let _ = freya::clipboard::Clipboard::set(text);
            }
            *cached_rich_clipboard()
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(cloned);
        }
        HubMessage::AppQuitForSignal(_) => Platform::get().exit(),
        HubMessage::ViewRequestContextMenu(client::ViewRequestContextMenu { view_id, request }) => {
            let caps = adapter.view_capabilities(view_id);
            let page_url = core
                .model()
                .tab_by_view_id(view_id)
                .map(|t| t.url.clone())
                .unwrap_or_default();
            let media_type = match request.kind {
                WebView_ContextMenuKind::Image => Some("image"),
                WebView_ContextMenuKind::Media => {
                    if request.media.as_ref().is_some_and(|m| m.is_video) {
                        Some("video")
                    } else {
                        Some("audio")
                    }
                }
                _ => None,
            };
            let mut target = serde_json::json!({
                "tabId": view_id as i64,
                "pageUrl": page_url,
                "editable": request.is_editable,
            });
            if let Some(link) = request.link_navigation.as_ref() {
                target["linkUrl"] = serde_json::json!(link.url.as_str());
            }
            if let Some(sel) = request.selected_text.as_ref().and_then(|s| s.as_str().ok())
                && !sel.is_empty()
            {
                target["selectionText"] = serde_json::json!(sel);
            }
            if let Some(mt) = media_type {
                target["mediaType"] = serde_json::json!(mt);
                if let Some(m) = request.media.as_ref().and_then(|m| m.media_url.as_ref()) {
                    target["srcUrl"] = serde_json::json!(m.as_str());
                } else if let Some(nav) = request.navigation.as_ref() {
                    target["srcUrl"] = serde_json::json!(nav.url.as_str());
                }
            }
            let target_json = target.to_string();
            let extension_items = core.extension_context_menu_items(&target_json);
            open_hub_context_menu(
                core,
                page_ctx,
                menu_state,
                view_id,
                &request,
                caps.can_view_source,
                caps.can_report_website_issue,
                adapter.search_engine_name(),
                extension_items,
                &target_json,
            );
        }
        HubMessage::ViewRequestSelectDropdown(client::ViewRequestSelectDropdown {
            view_id,
            content_position,
            minimum_width,
            items,
        }) => {
            open_hub_select_dropdown(
                core,
                page_ctx,
                menu_state,
                view_id,
                &content_position,
                minimum_width,
                &items,
            );
        }
        HubMessage::ViewSelectedText(client::ViewSelectedText { text, .. }) => {
            if let Some(bytes) = text {
                let text = String::from_utf8_lossy(&bytes.0).into_owned();
                if !text.is_empty() {
                    let _ = freya::clipboard::Clipboard::set(text);
                }
            }
        }
        HubMessage::ViewFindInPage(client::ViewFindInPage {
            view_id,
            current_match_index,
            total_match_count,
        }) => {
            adapter.on_find_in_page(view_id, current_match_index, total_match_count);
        }
        HubMessage::ViewEmbedderReply(client::ViewEmbedderReply {
            view_id,
            request_id,
            result_json,
            ..
        }) => {
            if let Ok(json) = result_json.as_str() {
                adapter.on_embedder_reply(view_id, request_id, json);
            }
        }
    }
}

pub fn app() -> impl IntoElement {
    let (stub_core, hub_adapter, ext_adapter) = use_hook(|| {
        let device_pixel_ratio = *Platform::get().current_window().scale_factor.peek();
        let stub = Arc::new(StubBackend::new_for_external_tabs());
        let adapter = Arc::new(HubTabAdapter::new(Arc::clone(&stub), device_pixel_ratio));
        stub.attach_tab_source(Arc::clone(&adapter) as Arc<dyn TabSource>);
        let ext = ExtensionAdapter::new(Arc::clone(&stub), Arc::clone(&adapter));
        stub.attach_extension_source(Arc::clone(&ext) as Arc<dyn ExtensionSource>);
        if let Err(err) = ext.start() {
            eprintln!("zendia: ExtensionServer failed to start: {err}");
        }
        stub.attach_clipboard_reader(Arc::new(|| freya::clipboard::Clipboard::get().ok()));
        if let Some(path) = zendia_browser_core::yank_webchat_actions::yank_ring_profile_path()
            && let Some(ring) = zendia_browser_core::yank_webchat_actions::load_yank_ring_from(&path)
        {
            stub.set_yank_ring(ring);
        }
        let onboarding_profile = zendia_browser_core::onboarding::onboarding_profile_path()
            .and_then(|path| zendia_browser_core::onboarding::load_profile_record_from(&path));
        stub.apply_onboarding_profile(onboarding_profile);
        let shell_profile = zendia_browser_core::shell_layout::shell_layout_profile_path()
            .and_then(|path| zendia_browser_core::shell_layout::load_shell_layout_from(&path));
        stub.apply_shell_layout_profile(shell_profile);
        let user_json_meta =
            zendia_browser_core::settings::load_user_keymap_json().map(|(_, raw)| stub.set_user_keymap_json(Some(raw)));
        let vim_cfg = zendia_browser_core::settings::vim_config_profile_path()
            .and_then(|path| zendia_browser_core::settings::load_vim_config_from(&path));
        stub.apply_vim_config_profile(vim_cfg);
        if let Some(meta) = user_json_meta
            && let Some(err) = meta.error
        {
            let _ = stub.invoke(Command::ToastShow {
                message: format!("Invalid zendia.json keymap config: {err}"),
                action_label: None,
                action_command: None,
                timeout_ms: Some(5000),
            });
        }
        let prev_session = zendia_browser_core::session::rotate_session_on_startup();
        stub.set_last_session(prev_session);
        (stub, adapter, ext)
    });

    use_drop({
        let ext_adapter = Arc::clone(&ext_adapter);
        move || {
            ext_adapter.shutdown();
        }
    });

    let mut viewport_known = use_state(|| false);
    let mut location = use_state(String::new);
    let location_a11y_id = use_hook(AccessibilityId::new_unique);

    let page_ctx = use_provide_page_surfaces();
    let keymap = use_provide_keymap();
    use_hook({
        let stub_core = Arc::clone(&stub_core);
        let page_ctx = page_ctx.clone();
        let keymap = keymap.clone();
        move || {
            page_ctx.set_core(Arc::clone(&stub_core) as Arc<dyn BrowserCore>);
            keymap.reload_vim_profile(stub_core.model().settings.vim_timeoutlen);
        }
    });
    let text_focus = use_provide_text_focus();
    let menu_state = use_provide_menu_state();
    let sidebar_state = use_provide_sidebar_state();
    let address_bar_state = use_provide_address_bar_state();
    let download_arc = use_provide_download_arc();
    let mut last_click_pos = use_state(|| None::<(f32, f32)>);
    let vim_statusline_state = use_provide_vim_statusline_state();
    let vim_picker_state = use_provide_vim_picker_state();
    let region_picker_state = use_provide_region_picker_state();
    let mut dock_state = use_provide_dock_state();
    let library_state = use_provide_library_state();
    let library_spaces_state = use_provide_spaces_view_state();
    let library_routing_state = use_provide_space_routing_view_state();
    let media_preview_state = use_provide_media_preview_state();
    let space_editing_state = use_provide_space_editing_state();
    let share_state = use_provide_share_state();
    let extension_popup_state = use_provide_extension_popup_state();
    use_hook({
        let stub_core = Arc::clone(&stub_core);
        let hub_adapter = Arc::clone(&hub_adapter);
        let ext_adapter = Arc::clone(&ext_adapter);
        let page_ctx = page_ctx.clone();
        let keymap = keymap.clone();
        move || {
            let close_core = Arc::clone(&stub_core);
            let close_hub = Arc::clone(&hub_adapter);
            let close_ext = Arc::clone(&ext_adapter);
            let close_ctx = page_ctx.clone();
            let close_km = keymap.clone();
            extension_popup_state.set_close_callback(move |view_id| {
                close_core.clear_extension_popup_keyboard_mark();
                close_ext.set_popup_view(view_id, false);
                close_km.remove_page(TabId(view_id));
                close_ctx.remove_closed_view(view_id);
                close_hub.destroy_auxiliary_view(view_id);
                close_ctx.focus_effective_model_surface(&close_core.model());
            });
            let scan_hub = Arc::clone(&hub_adapter);
            extension_popup_state.set_scan_callback(move |view_id| {
                scan_hub.post_automation_request(
                    view_id,
                    zendia_automation::AutomationRequest::JumpScan {
                        jump: zendia_automation::JumpKind::Hints,
                        pattern: String::new(),
                        top_only: true,
                        include_fields: true,
                        watch_mutations: true,
                    },
                );
            });
            let act_hub = Arc::clone(&hub_adapter);
            extension_popup_state.set_activate_callback(move |view_id, target| {
                if target.kind == "field" {
                    act_hub.post_automation_request(
                        view_id,
                        zendia_automation::AutomationRequest::JumpPick {
                            id: target.id.clone(),
                            new_tab: false,
                            extend: false,
                        },
                    );
                } else {
                    use zendia_browser_core::tab_source::PointerStep;
                    let cx = f64::from(target.x + target.width * 0.5);
                    let cy = f64::from(target.y + target.height * 0.5);
                    let source: Arc<dyn TabSource> = Arc::clone(&act_hub) as Arc<dyn TabSource>;
                    let _ = source.dispatch_pointer_steps(
                        TabId(view_id),
                        &[
                            PointerStep::move_to(cx, cy),
                            PointerStep::press(cx, cy),
                            PointerStep::release(cx, cy),
                        ],
                    );
                }
            });
        }
    });
    let find_bar = crate::ui::page::find_bar::use_provide_find_bar();
    let webchat_views = use_provide_webchat_views(Some(Arc::clone(&hub_adapter)));
    let onboarding_state = use_provide_onboarding_state(false);
    use_hook({
        let stub_core = Arc::clone(&stub_core);
        move || {
            if !stub_core.model().onboarding.seen {
                onboarding_state.open();
            }
        }
    });
    let settings_host = use_provide_settings_host();
    let core_slot: std::rc::Rc<std::cell::RefCell<Option<crate::ui::core_view::CoreView>>> =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(None)));

    let yank_save_seq = use_hook(|| Arc::new(std::sync::atomic::AtomicU64::new(0)));
    let core = use_provide_core_with_observer(Arc::clone(&stub_core) as Arc<dyn BrowserCore>, {
        let stub_core = Arc::clone(&stub_core);
        let hub_adapter = Arc::clone(&hub_adapter);
        let ext_adapter = Arc::clone(&ext_adapter);
        let page_ctx = page_ctx.clone();
        let keymap = keymap.clone();
        let core_slot = std::rc::Rc::clone(&core_slot);
        let yank_save_seq = Arc::clone(&yank_save_seq);
        let save_live_session = |stub: &StubBackend| {
            if let Some((session_path, _)) = zendia_browser_core::session::session_profile_paths() {
                let snap = zendia_browser_core::session::snapshot_from_model(&stub.model());
                let _ = zendia_browser_core::session::save_session_to(&session_path, &snap);
            }
        };
        let save_vim_cfg = |stub: &StubBackend| {
            if let Some(path) = zendia_browser_core::settings::vim_config_profile_path() {
                let _ = zendia_browser_core::settings::save_vim_config_to(&path, &stub.vim_config_record());
            }
        };
        move |event| match event {
            CoreEvent::TabOpened { .. } | CoreEvent::TabUpdated { .. } => {
                ext_adapter.push_browser_state();
                save_live_session(&stub_core);
            }
            CoreEvent::ZoomChanged { .. } => {
                ext_adapter.push_browser_state();
            }
            CoreEvent::SpaceSwitched { .. }
            | CoreEvent::SpacesUpdated { .. }
            | CoreEvent::TabsReordered { .. }
            | CoreEvent::PinnedTabsChanged { .. }
            | CoreEvent::TabPinnedUrlsChanged { .. }
            | CoreEvent::TabUnloadedChanged { .. }
            | CoreEvent::EssentialsChanged { .. }
            | CoreEvent::FolderCreated { .. }
            | CoreEvent::FolderUpdated { .. }
            | CoreEvent::FolderDeleted { .. }
            | CoreEvent::FolderHierarchyChanged { .. } => {
                save_live_session(&stub_core);
            }
            CoreEvent::TabClosed { tab_id } => {
                keymap.remove_page(*tab_id);
                page_ctx.remove_closed_view(tab_id.0);
                ext_adapter.push_browser_state();
                save_live_session(&stub_core);
            }
            CoreEvent::QuitRequested => {
                Platform::get().exit();
            }
            CoreEvent::ClipboardCopyRequested { text } if !text.is_empty() => {
                *cached_rich_clipboard()
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = None;
                let _ = freya::clipboard::Clipboard::set(text.clone());
            }
            CoreEvent::DownloadStarted { .. } => {
                // Launch the icon from the user's last click position (falling back to window center if unknown).
                let size = *Platform::get().current_window().root_size.peek();
                let start = (*last_click_pos.peek()).unwrap_or((size.width / 2.0, size.height / 2.0));
                download_arc.launch(start, (24.0, size.height - 24.0));
            }
            CoreEvent::TabActivated { .. } => {
                // NB: The vim mode follows whether a field of the newly active tab has the focus, and keyboard focus
                //     moves to the newly active page surface (`INS-04`).
                let _ = stub_core.invoke(Command::Vim(VimCommand::SyncPageMode));
                page_ctx.focus_effective_model_surface(&stub_core.model());
                ext_adapter.push_browser_state();
                save_live_session(&stub_core);
            }
            CoreEvent::GlanceChanged { .. } => {
                page_ctx.focus_effective_model_surface(&stub_core.model());
            }
            CoreEvent::FocusChanged { focus } => {
                if focus.surface.is_none() {
                    page_ctx.focus_effective_model_surface(&stub_core.model());
                }
            }
            CoreEvent::PageMoved { tab_id, scrolled } => {
                if *scrolled {
                    vim_statusline_state.notify_page_scroll();
                }
                extension_popup_state.on_view_updated(tab_id.0);
            }
            CoreEvent::PopupTargetsUpdated { tab_id, targets } => {
                extension_popup_state.set_targets(tab_id.0, targets.clone());
            }
            CoreEvent::YankRingChanged => {
                let seq = yank_save_seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                let seq_ref = Arc::clone(&yank_save_seq);
                let stub_save = Arc::clone(&stub_core);
                spawn(async move {
                    timer(std::time::Duration::from_millis(2000)).await;
                    if seq_ref.load(std::sync::atomic::Ordering::Relaxed) == seq
                        && let Some(path) = zendia_browser_core::yank_webchat_actions::yank_ring_profile_path()
                    {
                        let ring = stub_save.yank_ring_snapshot();
                        let _ = zendia_browser_core::yank_webchat_actions::save_yank_ring_to(&path, &ring);
                    }
                });
            }
            CoreEvent::VimPageStateChanged {
                mode_transition_pending: false,
                ..
            } => {
                let maybe_cv = core_slot.borrow().clone();
                if let Some(cv) = maybe_cv {
                    keymap.replay_held_keys(&cv, &page_ctx);
                }
            }
            CoreEvent::RegionCaptured { .. } => {
                region_picker_state.stop();
            }
            CoreEvent::VimModeChanged { enabled: false } => {
                keymap.reset_vim();
                vim_statusline_state.close_all_keymaps();
                vim_picker_state.close();
                region_picker_state.stop();
                save_vim_cfg(&stub_core);
            }
            CoreEvent::VimModeChanged { enabled: true } => {
                vim_statusline_state.close_all_keymaps();
                save_vim_cfg(&stub_core);
            }
            CoreEvent::ModeChanged { .. } | CoreEvent::WhichKeyChanged { .. } => {
                vim_statusline_state.close_all_keymaps();
            }
            CoreEvent::StatuslineSettingsChanged { .. } | CoreEvent::ShieldChanged { .. } => {
                save_vim_cfg(&stub_core);
            }
            CoreEvent::BrowserSettingsChanged { settings } => {
                keymap.reload_vim_profile(settings.vim_timeoutlen);
                save_vim_cfg(&stub_core);
            }
            CoreEvent::OnboardingChanged { state } => {
                if let Some(path) = zendia_browser_core::onboarding::onboarding_profile_path() {
                    let _ = zendia_browser_core::onboarding::save_profile_record_to(&path, &state.to_profile_record());
                }
                save_vim_cfg(&stub_core);
            }
            CoreEvent::LayoutSettingsChanged { .. } | CoreEvent::PanelPrefsChanged { .. } => {
                if let Some(path) = zendia_browser_core::shell_layout::shell_layout_profile_path() {
                    let model = stub_core.model();
                    let record = zendia_browser_core::shell_layout::ShellLayoutRecord {
                        layout: model.layout,
                        panel_prefs: model.side_panel_prefs,
                        user_customized_compact: stub_core.user_customized_compact(),
                    };
                    let _ = zendia_browser_core::shell_layout::save_shell_layout_to(&path, &record);
                }
            }
            CoreEvent::ViewRequested { request, .. } => match request {
                ViewRequest::ExtensionsMenuToggle => {
                    // Extensions live inside the Site Panel (matching Zen/Zendia), so invoking extensions.menu.toggle
                    // from the command palette opens the Site Panel rather than an unanchored puzzle-button popover.
                    address_bar_state.close();
                    menu_state.close_all();
                    share_state.close();
                    extension_popup_state.request_close();
                    address_bar_state.toggle_site_panel(None);
                }
                ViewRequest::ExtensionPopupOpen {
                    extension_id,
                    popup_url,
                    from_keyboard,
                } => {
                    menu_state.close_all();
                    address_bar_state.close();
                    address_bar_state.close_site_panel();
                    share_state.close();
                    extension_popup_state.request_close();
                    let ext_btn_id = format!("ext.{}", extension_id.0);
                    let anchor = menu_state
                        .toolbar_item_anchor(&ext_btn_id)
                        .map(|a| (a.x + a.width * 0.5, a.y + a.height))
                        .or_else(|| {
                            menu_state
                                .toolbar_item_anchor("builtin.extensions")
                                .or_else(|| menu_state.toolbar_item_anchor("builtin.url_bar"))
                                .map(|a| (a.x + a.width - 18.0, a.y + a.height))
                        })
                        .or_else(|| {
                            let a = *address_bar_state.site_panel_anchor.peek();
                            (a.width > 0.0).then_some((a.x + a.width * 0.5, a.y + a.height))
                        })
                        .or_else(|| *last_click_pos.peek());
                    let ext_ref = Arc::clone(&ext_adapter);
                    if let Some(view_id) = hub_adapter.create_extension_popup_view(popup_url, move |vid| {
                        ext_ref.set_popup_view(vid, true);
                    }) {
                        extension_popup_state.open_popup_with_hints(
                            extension_id.clone(),
                            view_id,
                            anchor,
                            *from_keyboard,
                        );
                        let _ = stub_core.invoke(Command::Vim(VimCommand::KeyedPageChanged {
                            tab_id: Some(TabId(view_id)),
                        }));
                        page_ctx.focus_view(view_id);
                    }
                }
                ViewRequest::ExtensionPopupClose => {
                    extension_popup_state.request_close();
                }
                ViewRequest::LibraryToggle { .. } | ViewRequest::LibraryOpen { .. } | ViewRequest::LibraryClose => {
                    library_state.handle_view_request(request);
                    let is_open = *library_state.open.peek();
                    let _ =
                        stub_core.invoke(zendia_browser_core::commands::Command::LibraryFocusSet { focused: is_open });
                    if !is_open {
                        page_ctx.focus_effective_model_surface(&stub_core.model());
                    }
                }
                ViewRequest::SpaceRename { space_id } => {
                    if let Some(space) = stub_core.model().space(*space_id) {
                        if library_state.is_open() {
                            library_spaces_state.start_rename(*space_id, space.name.clone());
                        } else {
                            sidebar_state.start_space_rename(*space_id, space.name.clone());
                        }
                    }
                }
                ViewRequest::SpaceIconPicker { space_id } => {
                    if library_state.is_open() {
                        library_spaces_state.open_icon_picker(*space_id, None);
                    } else {
                        let sidebar_w = stub_core.model().layout.effective_sidebar_width();
                        space_editing_state.open_live_icon_picker(*space_id, Some((sidebar_w + 10.0, 86.0)));
                    }
                }
                ViewRequest::SpaceDeleteConfirm {
                    space_id,
                    space_name,
                    tab_count,
                } => {
                    menu_state.close_all();
                    address_bar_state.close();
                    address_bar_state.close_site_panel();
                    space_editing_state.open_delete_confirm(*space_id, space_name.clone(), *tab_count);
                }
                ViewRequest::SpaceRoutingEdit { rule_id } => {
                    if let Some(rule) = stub_core.model().space_routing.rules.iter().find(|r| r.id == *rule_id) {
                        library_routing_state.start_edit(rule);
                    }
                }
                ViewRequest::LibraryMediaPreviewOpen { .. }
                | ViewRequest::LibraryMediaPreviewClose
                | ViewRequest::LibraryMediaPreviewStep { .. } => {
                    let items = stub_core.model().library_media;
                    media_preview_state.handle_view_request(request, &items);
                }
                ViewRequest::LocationFocus => {
                    menu_state.close_all();
                    share_state.close();
                    let current_url = stub_core
                        .model()
                        .active_tab()
                        .map(|tab| tab.url.clone())
                        .filter(|url| !url.is_empty() && url != "about:blank" && url != "about:newtab");
                    address_bar_state.open_floating(
                        false,
                        current_url,
                        zendia_browser_core::address_bar::AddressBarFilter::All,
                    );
                }
                ViewRequest::AddressBarOpen {
                    new_tab,
                    initial_query,
                    filter,
                } => {
                    menu_state.close_all();
                    share_state.close();
                    address_bar_state.open_floating(*new_tab, initial_query.clone(), *filter);
                }
                ViewRequest::PaletteOpen { initial_query } => {
                    menu_state.close_all();
                    share_state.close();
                    address_bar_state.open_actions(initial_query.clone());
                }
                ViewRequest::SitePanelToggle => {
                    menu_state.close_all();
                    share_state.close();
                    address_bar_state.toggle_site_panel(None);
                }
                ViewRequest::AppMenuToggle => {
                    address_bar_state.close();
                    address_bar_state.close_site_panel();
                    share_state.close();
                    menu_state.toggle_app_menu(None);
                }
                ViewRequest::AboutDialog => {
                    address_bar_state.close();
                    address_bar_state.close_site_panel();
                    share_state.close();
                    menu_state.open_about();
                }
                ViewRequest::TabRename { tab_id } => {
                    if let Some(tab) = stub_core.model().tab(*tab_id) {
                        sidebar_state.start_tab_rename(*tab_id, tab.display_title());
                    }
                }
                ViewRequest::CompactSidebarReveal => {
                    if stub_core.model().layout.compact_hides_sidebar() {
                        let mut pinned = sidebar_state.compact_pinned;
                        let next = !*pinned.peek();
                        pinned.set(next);
                        let mut revealed = sidebar_state.compact_revealed;
                        revealed.set(next);
                    }
                }
                ViewRequest::FolderRename { folder_id } => {
                    if let Some(folder) = stub_core.model().folder(*folder_id) {
                        sidebar_state.start_folder_rename(*folder_id, folder.name.clone());
                    }
                }
                ViewRequest::PickerOpen { request } => {
                    if stub_core.model().vim_mode_enabled {
                        menu_state.close_all();
                        address_bar_state.close();
                        address_bar_state.close_site_panel();
                        region_picker_state.stop();
                        vim_picker_state.open(request.clone());
                    }
                }
                ViewRequest::PickerClosed { .. } => {
                    vim_picker_state.close();
                    page_ctx.focus_effective_model_surface(&stub_core.model());
                }
                ViewRequest::RegionPick { tab_id } => {
                    if stub_core.model().vim_mode_enabled {
                        menu_state.close_all();
                        address_bar_state.close();
                        address_bar_state.close_site_panel();
                        vim_picker_state.close();
                        region_picker_state.start(*tab_id);
                    }
                }
                ViewRequest::RegionPickCancelled => {
                    region_picker_state.stop();
                }
                ViewRequest::WhichKeyAll { mode } => {
                    vim_statusline_state.open_all_keymaps(*mode);
                }
                ViewRequest::PanelOpen { .. }
                | ViewRequest::PanelClose { .. }
                | ViewRequest::PanelToggle { .. }
                | ViewRequest::AgentModelCard
                | ViewRequest::AgentInstructions
                | ViewRequest::WebChatAccountsToggle => {
                    if dock_state.apply_request(request) {
                        page_ctx.request_focus_active_surface();
                    }
                }
                ViewRequest::PanelFocusChanged { panel_id } => {
                    let mut ds = dock_state;
                    ds.set_focus(panel_id.clone());
                    if panel_id.is_none() {
                        page_ctx.request_focus_active_surface();
                    } else if panel_id
                        .as_ref()
                        .is_some_and(|p| p.0 == zendia_browser_core::webchat::WEBCHAT_PANEL_ID)
                    {
                        let model = stub_core.model();
                        if let Some(site) = model.active_chat_site()
                            && let Some(vid) = webchat_views.view_for_site(&site.id)
                        {
                            page_ctx.focus_view(vid);
                        }
                    }
                }
                ViewRequest::WebChatReload { site_id } => {
                    webchat_views.reload_site(site_id);
                }
                ViewRequest::WebChatNavigateHome { site_id } => {
                    if let Some(site) = stub_core.model().chat_sites.iter().find(|site| &site.id == site_id) {
                        webchat_views.navigate_home(site);
                    }
                }
                ViewRequest::OnboardingOpen => {
                    menu_state.close_all();
                    address_bar_state.close();
                    address_bar_state.close_site_panel();
                    vim_picker_state.close();
                    region_picker_state.stop();
                    onboarding_state.open();
                }
                ViewRequest::OnboardingClose => {
                    onboarding_state.close();
                }
                ViewRequest::SpaceCreateFormOpen => {
                    menu_state.close_all();
                    address_bar_state.close();
                    address_bar_state.close_site_panel();
                    space_editing_state.open_create_form();
                }
                ViewRequest::SpaceCreateFormClose => {
                    space_editing_state.close_create_form();
                }
                ViewRequest::ThemePickerOpen { space_id } => {
                    menu_state.close_all();
                    address_bar_state.close();
                    address_bar_state.close_site_panel();
                    let sidebar_w = stub_core.model().layout.effective_sidebar_width();
                    space_editing_state.open_theme_picker(*space_id, Some((sidebar_w + 10.0, 86.0)));
                }
                ViewRequest::ThemePickerClose => {
                    space_editing_state.close_theme_picker();
                }
                ViewRequest::ShareConfirmOpen { payload } => {
                    menu_state.close_all();
                    address_bar_state.close();
                    address_bar_state.close_site_panel();
                    share_state.open(payload.clone());
                }
                ViewRequest::ShareConfirmClose => {
                    share_state.close();
                }
                ViewRequest::SettingsOpen { group } => {
                    menu_state.close_all();
                    address_bar_state.close();
                    address_bar_state.close_site_panel();
                    settings_host.open_os_window(Arc::clone(&stub_core) as Arc<dyn BrowserCore>, *group);
                }
                ViewRequest::SettingsClose => {
                    settings_host.close_os_window();
                }
                ViewRequest::TabMenu { tab_id } => {
                    address_bar_state.close();
                    address_bar_state.close_site_panel();
                    share_state.close();
                    let pos = sidebar_state
                        .tab_anchor(*tab_id)
                        .map(|a| (a.x + a.width.min(180.0) * 0.5, a.y + a.height))
                        .unwrap_or((240.0, 96.0));
                    crate::ui::chrome::menus::open_tab_context_menu(&menu_state, &stub_core.model(), *tab_id, pos);
                }
                ViewRequest::FolderMenu { folder_id } => {
                    address_bar_state.close();
                    address_bar_state.close_site_panel();
                    share_state.close();
                    let pos = sidebar_state
                        .folder_anchor(*folder_id)
                        .map(|a| (a.x + a.width.min(180.0) * 0.5, a.y + a.height))
                        .unwrap_or((240.0, 96.0));
                    crate::ui::chrome::menus::open_folder_context_menu(
                        &menu_state,
                        &stub_core.model(),
                        *folder_id,
                        pos,
                    );
                }
                ViewRequest::CreateNewMenu => {
                    address_bar_state.close();
                    address_bar_state.close_site_panel();
                    share_state.close();
                    menu_state.toggle_create_new(None);
                }
                ViewRequest::ToolbarItemMenu { item_id } => {
                    let anchor = menu_state.toolbar_item_anchor(item_id.as_str());
                    let pos = anchor.map(|a| (a.x, a.y + a.height + 4.0)).unwrap_or((240.0, 48.0));
                    match item_id.as_str() {
                        "builtin.app_menu" => {
                            address_bar_state.close();
                            address_bar_state.close_site_panel();
                            share_state.close();
                            menu_state.toggle_app_menu(anchor);
                        }
                        "builtin.new_tab" => {
                            address_bar_state.close();
                            address_bar_state.close_site_panel();
                            share_state.close();
                            menu_state.toggle_create_new(anchor);
                        }
                        "builtin.extensions" | "builtin.url_bar" => {
                            menu_state.close_all();
                            share_state.close();
                            address_bar_state.toggle_site_panel(anchor);
                        }
                        "builtin.back" | "builtin.forward" => {
                            address_bar_state.close();
                            address_bar_state.close_site_panel();
                            share_state.close();
                            let direction = if item_id.as_str() == "builtin.back" { -1 } else { 1 };
                            let model = stub_core.model();
                            let history_entries = model
                                .active_tab
                                .map(|tid| hub_adapter.session_history_items(tid, direction))
                                .unwrap_or_default();
                            let items: Vec<zendia_browser_core::menus::ResolvedMenuItem> = if history_entries.is_empty()
                            {
                                vec![
                                    zendia_browser_core::menus::ResolvedMenuItem::action(
                                        "history.empty",
                                        "No history entries",
                                        "",
                                    )
                                    .with_enabled(false),
                                ]
                            } else {
                                history_entries
                                    .into_iter()
                                    .enumerate()
                                    .map(|(idx, (step, url))| {
                                        let mut entry = zendia_browser_core::menus::ResolvedMenuItem::action(
                                            format!("history.step.{idx}"),
                                            url,
                                            "navigation.history_step",
                                        );
                                        entry.command_args = vec![("step".to_string(), step.to_string())];
                                        entry
                                    })
                                    .collect()
                            };
                            menu_state.open_context_menu(crate::ui::chrome::menus::OpenContextMenu {
                                position: pos,
                                items,
                                tab_id: model.active_tab,
                                open_submenu: None,
                            });
                        }
                        _ => {
                            address_bar_state.close();
                            address_bar_state.close_site_panel();
                            share_state.close();
                            let model = stub_core.model();
                            let ext_id = model
                                .contributions
                                .toolbar_items
                                .iter()
                                .find(|it| it.id == *item_id)
                                .and_then(|it| match &it.source {
                                    zendia_browser_core::contributions::ContributionSource::Extension {
                                        extension_id,
                                    } => Some(extension_id.clone()),
                                    _ => None,
                                });
                            if let Some(extension_id) = ext_id {
                                crate::ui::chrome::menus::open_extension_context_menu(
                                    &menu_state,
                                    &model,
                                    &extension_id,
                                    pos,
                                );
                            } else {
                                crate::ui::chrome::menus::open_sidebar_context_menu(&menu_state, &model, pos);
                            }
                        }
                    }
                }
                ViewRequest::UrlPaste { new_tab } => {
                    if let Ok(raw) = freya::clipboard::Clipboard::get() {
                        let _ =
                            stub_core.invoke(Command::Tabs(zendia_browser_core::tab_actions::TabAction::UrlPaste {
                                new_tab: *new_tab,
                                text: Some(raw),
                            }));
                    }
                }
                ViewRequest::PageFind => {
                    find_bar.open(&stub_core.model().page_find.query);
                }
                ViewRequest::WindowMinimize => {
                    Platform::get().with_window(Platform::window_id(), |window| {
                        window.set_minimized(true);
                    });
                }
                ViewRequest::WindowToggleMaximize => {
                    Platform::get().with_window(Platform::window_id(), |window| {
                        window.set_maximized(!window.is_maximized());
                    });
                }
                ViewRequest::WindowClose => {
                    Platform::get().close_window(Platform::window_id());
                }
                ViewRequest::LibraryVimAction { action, count } => {
                    use crate::ui::library::nav::NavMove;
                    use zendia_browser_core::chrome_actions::LibraryVimAction;
                    use zendia_browser_core::library::LibrarySection;
                    use zendia_browser_core::model::{DownloadId, FolderId, HistoryEntryId, LibraryMediaId, SpaceId};
                    use zendia_browser_core::space_routing::SpaceRouteId;

                    if !*library_state.open.peek() {
                        return;
                    }
                    let steps = (*count).max(1) as usize;
                    let active_section = *library_state.section.peek();
                    let cursor_key = library_state.nav.cursor.peek().clone();

                    let resolve_cursor_space = |model: &zendia_browser_core::model::BrowserModel| -> Option<SpaceId> {
                        if active_section != LibrarySection::Spaces {
                            return None;
                        }
                        let key = cursor_key.as_deref()?;
                        if let Some(sid) = key.strip_prefix("space:").and_then(|s| s.parse::<u64>().ok()) {
                            return Some(SpaceId(sid));
                        }
                        if let Some(fid) = key.strip_prefix("folder:").and_then(|s| s.parse::<u64>().ok()) {
                            return model.folder(FolderId(fid)).map(|f| f.space_id);
                        }
                        if let Some(tid) = key.strip_prefix("tab:").and_then(|s| s.parse::<u64>().ok()) {
                            let tab_id = TabId(tid);
                            return model.tab(tab_id).and_then(|t| t.space_id).or_else(|| {
                                model
                                    .pinned_tabs
                                    .iter()
                                    .find(|p| p.tab_id == tab_id)
                                    .map(|p| p.space_id)
                            });
                        }
                        None
                    };

                    match action {
                        LibraryVimAction::Down => library_state.nav.move_cursor_by(NavMove::Down, steps),
                        LibraryVimAction::Up => library_state.nav.move_cursor_by(NavMove::Up, steps),
                        LibraryVimAction::Left => library_state.nav.move_cursor_by(NavMove::Left, steps),
                        LibraryVimAction::Right => library_state.nav.move_cursor_by(NavMove::Right, steps),
                        LibraryVimAction::First => library_state.nav.move_cursor_by(NavMove::First, 1),
                        LibraryVimAction::Last => library_state.nav.move_cursor_by(NavMove::Last, 1),
                        LibraryVimAction::HalfPageDown => {
                            library_state.nav.move_cursor_by(NavMove::HalfPageDown, steps)
                        }
                        LibraryVimAction::HalfPageUp => library_state.nav.move_cursor_by(NavMove::HalfPageUp, steps),
                        LibraryVimAction::Activate => {
                            let maybe_cv = core_slot.borrow().clone();
                            if let Some(cv) = maybe_cv {
                                let _ = library_state.nav.activate_cursor(&cv);
                            }
                        }
                        LibraryVimAction::Quit => {
                            if library_spaces_state.icon_picker_space.peek().is_some() {
                                library_spaces_state.close_icon_picker();
                            } else if !library_state.nav.close_subview_if_open()
                                && !library_state.nav.close_filter_panel_if_open()
                            {
                                let _ = stub_core.invoke(Command::LibraryClose);
                            }
                        }
                        LibraryVimAction::Jump => {
                            let _ = library_state.nav.start_jump_hints();
                        }
                        LibraryVimAction::Search => {
                            let _ = library_state.nav.focus_search();
                        }
                        LibraryVimAction::Filter => {
                            let _ = library_state.nav.open_filter_panel();
                        }
                        LibraryVimAction::Menu => {
                            let Some(ref key) = cursor_key else {
                                return;
                            };
                            let pos = library_state.nav.menu_position_for(key);
                            let model = stub_core.model();
                            if let Some(id) = key.strip_prefix("history:").and_then(|s| s.parse::<u64>().ok()) {
                                if let Some(entry) = model
                                    .history
                                    .iter()
                                    .chain(model.closed_tabs.iter())
                                    .find(|e| e.id == HistoryEntryId(id))
                                {
                                    menu_state.open_context_menu(crate::ui::chrome::menus::OpenContextMenu {
                                        position: pos,
                                        items: zendia_browser_core::history::build_history_row_context_menu(entry),
                                        tab_id: None,
                                        open_submenu: None,
                                    });
                                }
                            } else if let Some(id) = key.strip_prefix("download:").and_then(|s| s.parse::<u64>().ok()) {
                                if let Some(dl) = model.downloads.iter().find(|d| d.id == DownloadId(id)) {
                                    menu_state.open_context_menu(crate::ui::chrome::menus::OpenContextMenu {
                                        position: pos,
                                        items: zendia_browser_core::downloads::build_download_context_menu(dl),
                                        tab_id: None,
                                        open_submenu: None,
                                    });
                                }
                            } else if let Some(id) = key.strip_prefix("media:").and_then(|s| s.parse::<u64>().ok()) {
                                if let Some(item) = model.library_media.iter().find(|m| m.id == LibraryMediaId(id)) {
                                    menu_state.open_context_menu(crate::ui::chrome::menus::OpenContextMenu {
                                        position: pos,
                                        items: zendia_browser_core::library_media::build_media_context_menu(item),
                                        tab_id: None,
                                        open_submenu: None,
                                    });
                                }
                            } else if let Some(id) = key.strip_prefix("space:").and_then(|s| s.parse::<u64>().ok()) {
                                let key_table = zendia_keymap::KeyTable::standard();
                                let items = zendia_browser_core::space_routing::build_library_space_menu(
                                    &model,
                                    SpaceId(id),
                                    Some(&key_table),
                                );
                                menu_state.open_context_menu(crate::ui::chrome::menus::OpenContextMenu {
                                    position: pos,
                                    items,
                                    tab_id: None,
                                    open_submenu: None,
                                });
                            } else if let Some(id) = key.strip_prefix("tab:").and_then(|s| s.parse::<u64>().ok()) {
                                crate::ui::chrome::menus::open_tab_context_menu(&menu_state, &model, TabId(id), pos);
                            } else if let Some(id) = key.strip_prefix("folder:").and_then(|s| s.parse::<u64>().ok()) {
                                crate::ui::chrome::menus::open_folder_context_menu(
                                    &menu_state,
                                    &model,
                                    FolderId(id),
                                    pos,
                                );
                            } else if let Some(id) = key.strip_prefix("rule:").and_then(|s| s.parse::<u64>().ok())
                                && let Some((idx, rule)) = model
                                    .space_routing
                                    .rules
                                    .iter()
                                    .enumerate()
                                    .find(|(_, r)| r.id == SpaceRouteId(id))
                            {
                                use zendia_browser_core::menus::ResolvedMenuItem;
                                let mut edit =
                                    ResolvedMenuItem::action("route.edit", "Edit Route", "space_routing.start_edit");
                                edit.command_args = vec![("rule_id".to_string(), rule.id.0.to_string())];
                                let mut copy = ResolvedMenuItem::action("route.copy", "Copy Link", "page.copy_link");
                                copy.command_args = vec![("url".to_string(), rule.pattern.clone())];
                                let mut items = vec![edit, copy];
                                if idx > 0 {
                                    let mut up =
                                        ResolvedMenuItem::action("route.up", "Move Up", "space_routing.reorder");
                                    up.command_args = vec![
                                        ("rule_id".to_string(), rule.id.0.to_string()),
                                        ("index".to_string(), (idx - 1).to_string()),
                                    ];
                                    items.push(up);
                                }
                                if idx + 1 < model.space_routing.rules.len() {
                                    let mut down =
                                        ResolvedMenuItem::action("route.down", "Move Down", "space_routing.reorder");
                                    down.command_args = vec![
                                        ("rule_id".to_string(), rule.id.0.to_string()),
                                        ("index".to_string(), (idx + 1).to_string()),
                                    ];
                                    items.push(down);
                                }
                                items.push(ResolvedMenuItem::separator("route.sep"));
                                let mut del =
                                    ResolvedMenuItem::action("route.delete", "Delete Route", "space_routing.delete");
                                del.command_args = vec![("rule_id".to_string(), rule.id.0.to_string())];
                                items.push(del);
                                menu_state.open_context_menu(crate::ui::chrome::menus::OpenContextMenu {
                                    position: pos,
                                    items,
                                    tab_id: None,
                                    open_submenu: None,
                                });
                            }
                        }
                        LibraryVimAction::Delete => {
                            let Some(ref key) = cursor_key else {
                                return;
                            };
                            match active_section {
                                LibrarySection::History => {
                                    if let Some(id) = key.strip_prefix("history:").and_then(|s| s.parse::<u64>().ok()) {
                                        let _ = stub_core.invoke(Command::HistoryDelete {
                                            entry_id: HistoryEntryId(id),
                                        });
                                    }
                                }
                                LibrarySection::Downloads => {
                                    if let Some(id) = key.strip_prefix("download:").and_then(|s| s.parse::<u64>().ok())
                                    {
                                        let _ = stub_core.invoke(Command::DownloadRemove {
                                            download_id: DownloadId(id),
                                        });
                                    }
                                }
                                LibrarySection::Spaces => {
                                    if let Some(id) = key.strip_prefix("tab:").and_then(|s| s.parse::<u64>().ok()) {
                                        let _ = stub_core.invoke(Command::TabClose {
                                            tab_id: Some(TabId(id)),
                                            force: false,
                                        });
                                    }
                                }
                                LibrarySection::SpaceRouting => {
                                    if let Some(id) = key.strip_prefix("rule:").and_then(|s| s.parse::<u64>().ok()) {
                                        let _ = stub_core.invoke(Command::SpaceRoutingDelete {
                                            rule_id: SpaceRouteId(id),
                                        });
                                    }
                                }
                                LibrarySection::Media => {}
                            }
                        }
                        LibraryVimAction::Yank => {
                            let Some(ref key) = cursor_key else {
                                return;
                            };
                            match active_section {
                                LibrarySection::History => {
                                    if let Some(id) = key.strip_prefix("history:").and_then(|s| s.parse::<u64>().ok()) {
                                        let _ = stub_core.invoke(Command::HistoryCopyUrl {
                                            entry_id: HistoryEntryId(id),
                                        });
                                    }
                                }
                                LibrarySection::Downloads => {
                                    if let Some(id) = key.strip_prefix("download:").and_then(|s| s.parse::<u64>().ok())
                                    {
                                        let _ = stub_core.invoke(Command::DownloadCopyLink {
                                            download_id: DownloadId(id),
                                        });
                                    }
                                }
                                LibrarySection::Media => {
                                    if let Some(id) = key.strip_prefix("media:").and_then(|s| s.parse::<u64>().ok()) {
                                        let _ = stub_core.invoke(Command::LibraryMediaCopy {
                                            media_id: LibraryMediaId(id),
                                        });
                                    }
                                }
                                LibrarySection::SpaceRouting => {
                                    if let Some(id) = key.strip_prefix("rule:").and_then(|s| s.parse::<u64>().ok())
                                        && let Some(rule) = stub_core
                                            .model()
                                            .space_routing
                                            .rules
                                            .iter()
                                            .find(|r| r.id == SpaceRouteId(id))
                                    {
                                        let text = rule.pattern.trim().to_string();
                                        if !text.is_empty() {
                                            let _ = stub_core.invoke(Command::PageCopyLink { url: text });
                                        }
                                    }
                                }
                                LibrarySection::Spaces => {}
                            }
                        }
                        LibraryVimAction::Add => match active_section {
                            LibrarySection::Spaces => {
                                let _ = stub_core.invoke(Command::SpaceCreate { name: None, icon: None });
                            }
                            LibrarySection::SpaceRouting => {
                                library_routing_state.start_new();
                            }
                            _ => {}
                        },
                        LibraryVimAction::RenameSpace => {
                            let model = stub_core.model();
                            if let Some(space_id) = resolve_cursor_space(&model)
                                && let Some(space) = model.space(space_id)
                            {
                                library_spaces_state.start_rename(space_id, space.name.clone());
                            }
                        }
                        LibraryVimAction::SpaceTheme => {
                            let model = stub_core.model();
                            if let Some(space_id) = resolve_cursor_space(&model) {
                                let _ = stub_core.invoke(Command::SpaceOpenThemePicker { space_id });
                            }
                        }
                        LibraryVimAction::SpaceIcon => {
                            let model = stub_core.model();
                            if let Some(space_id) = resolve_cursor_space(&model) {
                                let anchor = library_spaces_state
                                    .card_areas
                                    .peek()
                                    .get(&space_id)
                                    .map(|&(x, y, _, _)| (x + 12.0, y + 48.0));
                                library_spaces_state.open_icon_picker(space_id, anchor);
                            }
                        }
                        LibraryVimAction::MoveSpaceLeft | LibraryVimAction::MoveSpaceRight => {
                            let model = stub_core.model();
                            if let Some(space_id) = resolve_cursor_space(&model)
                                && let Some(pos) = model.spaces.iter().position(|s| s.id == space_id)
                            {
                                let delta: isize = if matches!(action, LibraryVimAction::MoveSpaceRight) {
                                    steps as isize
                                } else {
                                    -(steps as isize)
                                };
                                let target = (pos as isize + delta)
                                    .clamp(0, model.spaces.len().saturating_sub(1) as isize)
                                    as usize;
                                if target != pos
                                    && stub_core
                                        .invoke(Command::SpaceReorder {
                                            space_id,
                                            index: target,
                                        })
                                        .is_ok()
                                {
                                    let mut cursor = library_state.nav.cursor;
                                    cursor.set_if_modified(Some(format!("space:{}", space_id.0)));
                                }
                            }
                        }
                        LibraryVimAction::NextSection | LibraryVimAction::PrevSection => {
                            let delta: isize = if matches!(action, LibraryVimAction::NextSection) {
                                steps as isize
                            } else {
                                -(steps as isize)
                            };
                            if let Some(next) = library_state.adjacent_visible_section(delta) {
                                library_state.open_keeping_list_focus(Some(next));
                            }
                        }
                        LibraryVimAction::GotoSection(section) => {
                            library_state.open_keeping_list_focus(Some(*section));
                        }
                    }
                }
            },
            _ => {}
        }
    });
    *core_slot.borrow_mut() = Some(core.clone());
    let _theme = use_provide_theme(&core);
    let theme = use_theme();

    // NB: What the Hub sends arrives on the IPC reader thread, which hands it to this task on the UI thread.
    use_hook({
        let stub_core = Arc::clone(&stub_core);
        let adapter = Arc::clone(&hub_adapter);
        let page_ctx = page_ctx.clone();
        move || {
            spawn_in_window(async move {
                let incoming = startup().incoming.clone();
                while let Ok(incoming) = incoming.recv().await {
                    match incoming {
                        Incoming::Message(message) => {
                            handle_message(stub_core.as_ref(), &adapter, &page_ctx, &menu_state, *message)
                        }
                        Incoming::Disconnected => {
                            eprintln!("zendia: The Hub went away");
                            Platform::get().exit();
                            break;
                        }
                    }
                }
            });
        }
    });

    // NB: ExtensionServer events (popup lifecycle, host.* calls, crashes) arrive on its IPC reader thread and execute
    //     on the UI thread here.
    use_hook({
        let stub_core = Arc::clone(&stub_core);
        let ext_adapter = Arc::clone(&ext_adapter);
        move || {
            spawn_in_window(async move {
                let rx = ext_adapter.receiver();
                let mut consecutive_restarts = 0u32;
                while let Ok(event) = rx.recv().await {
                    match event {
                        ExtensionUiEvent::OpenPopup {
                            extension_id,
                            popup_url,
                        } => {
                            stub_core.open_extension_popup(extension_id, popup_url);
                        }
                        ExtensionUiEvent::PopupSize { view_id, width, height } => {
                            extension_popup_state.resize_popup(view_id, width, height);
                        }
                        ExtensionUiEvent::PopupReady { view_id } => {
                            extension_popup_state.mark_ready(view_id);
                        }
                        ExtensionUiEvent::ClosePopup { view_id } => {
                            let matches_open = extension_popup_state
                                .open
                                .peek()
                                .as_ref()
                                .is_some_and(|p| p.view_id == view_id);
                            if matches_open {
                                extension_popup_state.request_close();
                            }
                        }
                        ExtensionUiEvent::HostCall {
                            token,
                            extension_id,
                            function,
                            args,
                        } => {
                            ext_adapter.handle_host_call(token, &extension_id, &function, &args);
                        }
                        ExtensionUiEvent::ServiceDisconnected { reason } => {
                            extension_popup_state.request_close();
                            let is_manual = reason == "Restarting ExtensionServer";
                            if is_manual {
                                consecutive_restarts = 0;
                            } else {
                                consecutive_restarts = consecutive_restarts.saturating_add(1);
                            }
                            if consecutive_restarts <= 5 {
                                if !is_manual {
                                    eprintln!(
                                        "zendia: {reason}; restarting ExtensionServer (attempt {consecutive_restarts})"
                                    );
                                }
                                if let Err(err) = ext_adapter.start() {
                                    eprintln!("zendia: ExtensionServer restart failed: {err}");
                                }
                            } else {
                                eprintln!(
                                    "zendia: ExtensionServer crashed too many times; keeping paused until manual restart"
                                );
                            }
                        }
                    }
                }
            });
        }
    });

    // The tabs the browser starts with, once the page stage knows its size.
    let mut did_open_initial_tabs = use_state(|| false);
    if *viewport_known.read() && !*did_open_initial_tabs.peek() {
        did_open_initial_tabs.set(true);
        hub_adapter.open_initial_tabs();
    }

    // The location bar shows the address of the current tab, other than while the user edits it.
    let (active_url, active_tab_id) = {
        let model = core.model().read();
        let active = model.active_tab();
        (active.map(|tab| tab.url.clone()).unwrap_or_default(), model.active_tab)
    };
    let shown_url = use_state(String::new);
    if *shown_url.peek() != active_url && !location_a11y_id.is_focused() {
        let mut shown_url = shown_url;
        shown_url.set(active_url.clone());
        location.set(active_url);
    }

    // NB: Mouse events carry no modifiers, so each page surface's mouse events take those of the last key event.
    let track_modifiers = {
        let page_ctx = page_ctx.clone();
        move |is_down: bool, e: &KeyboardEventData| {
            let modifiers = input::modifiers_after_key(is_down, &e.key, e.modifiers);
            page_ctx
                .tracker()
                .borrow_mut()
                .set_modifiers(input::key_modifiers(modifiers));
        }
    };
    let mut window_dictation_held = use_state(|| false);
    let track_modifiers_on_up = track_modifiers.clone();
    let on_global_key_up = move |e: Event<KeyboardEventData>| {
        track_modifiers_on_up(false, &e);
        if *window_dictation_held.peek() {
            window_dictation_held.set(false);
            if *dock_state.dictation.peek() == crate::ui::panels::DictationState::Listening {
                let mut ds = dock_state;
                ds.dictation.set(crate::ui::panels::DictationState::Processing);
                spawn(async move {
                    timer(std::time::Duration::from_millis(450)).await;
                    if *ds.dictation.peek() == crate::ui::panels::DictationState::Processing {
                        let mut ct = ds.composer_text;
                        let prev = ct.peek().clone();
                        let next = if prev.trim().is_empty() {
                            "Summarize the key takeaways on this page".to_string()
                        } else {
                            format!("{prev} and highlight action items")
                        };
                        ct.set(next);
                        ds.dictation.set(crate::ui::panels::DictationState::Idle);
                    }
                });
            }
        }
    };

    // Keep core's panel-row focus state in sync with pointer clicks inside panels or on the page (`WIN-06`).
    let mut last_synced_panel = use_state(|| None::<zendia_browser_core::contributions::SidePanelId>);
    let current_focused_panel = dock_state.focused_panel.read().clone();
    if *last_synced_panel.peek() != current_focused_panel {
        last_synced_panel.set(current_focused_panel.clone());
        core.invoke_or_log(Command::Vim(
            zendia_browser_core::vim_keys::VimCommand::PanelFocusChanged {
                panel_id: current_focused_panel,
            },
        ));
    }

    let on_global_key_down = {
        let core = core.clone();
        let keymap = keymap.clone();
        move |e: Event<KeyboardEventData>| {
            track_modifiers(true, &e);
            if onboarding_state.is_open() {
                let _ = handle_onboarding_key_down(&e, &core, &onboarding_state);
                return;
            }
            // AGK-03: Window-wide configurable dictation hold key (`zendia.dictation.hold-key`), taken before vim keys.
            let dictation_hold_spec = core.model().peek().settings.dictation_hold_key.clone();
            if crate::ui::agent::panel::matches_dictation_hold_key(&e, &dictation_hold_spec) {
                e.stop_propagation();
                e.prevent_default();
                keymap.consume_key_up(e.code);
                if !dock_state.is_agent_open() {
                    core.invoke_or_log(Command::PanelOpen {
                        panel_id: zendia_browser_core::contributions::SidePanelId::new(
                            zendia_browser_core::agent::AGENT_PANEL_ID,
                        ),
                        focus: true,
                    });
                    window_dictation_held.set(true);
                    let mut ds = dock_state;
                    ds.dictation.set(crate::ui::panels::DictationState::Listening);
                }
                return;
            }
            // While the floating address bar is open, let Ctrl+N / Ctrl+P navigate its list instead of triggering
            // window.new / page.print.
            if address_bar_state.is_open()
                && e.modifiers == Modifiers::CONTROL
                && matches!(e.code, Code::KeyN | Code::KeyP)
            {
                return;
            }
            if vim_picker_state.is_open()
                || region_picker_state.is_active()
                || media_preview_state.is_open()
                || share_state.is_open()
                || space_editing_state.has_modal_open()
                || core.model().peek().pending_extension_prompt.is_some()
            {
                return;
            }
            let (vim_enabled, mode_transition_pending) = {
                let model = core.model().peek();
                (model.vim_mode_enabled, model.mode_transition_pending)
            };
            let try_extension_shortcut = |e: &Event<KeyboardEventData>| -> bool {
                if text_focus.is_any_focused() || address_bar_state.is_open() || menu_state.has_open_overlay() {
                    return false;
                }
                let matched_ext_cmd = {
                    let model = core.model().peek();
                    match_extension_shortcut(e, &model.extension_commands)
                        .map(|c| (c.extension_id.clone(), c.name.clone()))
                };
                if let Some((extension_id, name)) = matched_ext_cmd {
                    e.stop_propagation();
                    e.prevent_default();
                    core.invoke_or_log(Command::ExtensionInvokeCommand { extension_id, name });
                    return true;
                }
                false
            };
            // With vim off, extension shortcuts run before standard shortcuts; with vim on, vim mappings win (`EXT-07`).
            if !vim_enabled && try_extension_shortcut(&e) {
                return;
            }
            if text_focus.is_any_focused() && is_text_editing_key(&e, vim_enabled) {
                return;
            }
            if (dock_state.captures_text_input() || space_editing_state.captures_text_input())
                && !e
                    .modifiers
                    .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
            {
                return;
            }
            let library_space_renaming = library_state.is_open() && library_spaces_state.is_renaming();
            if !address_bar_state.is_open()
                && !sidebar_state.is_renaming()
                && !library_space_renaming
                && !menu_state.has_open_overlay()
                && crate::ui::library::nav::handle_library_key_down(&e, &core, &library_state)
            {
                return;
            }
            if !address_bar_state.is_open()
                && !sidebar_state.is_renaming()
                && !library_space_renaming
                && !dock_state.captures_text_input()
                && crate::ui::vim::handle_focus_key_down(&e, &core)
            {
                return;
            }
            if mode_transition_pending && !address_bar_state.is_open() && !menu_state.has_open_overlay() {
                keymap.hold_key_down(e.data().clone());
                return;
            }
            // XCM-03: Block compact-mode keyboard toggles while Library is open (`ZenCompactMode.mjs:167-169`).
            if library_state.is_open()
                && e.code == Code::KeyS
                && e.modifiers.intersects(Modifiers::CONTROL | Modifiers::META)
            {
                return;
            }
            let library_field_focused = library_state.is_open()
                && (library_state.nav.is_search_focused() || library_state.nav.is_form_input_focused());
            if library_field_focused
                && !e.modifiers.intersects(Modifiers::ALT | Modifiers::META)
                && !(e.modifiers.contains(Modifiers::CONTROL) && e.modifiers.contains(Modifiers::SHIFT))
            {
                return;
            }
            if address_bar_state.is_open()
                || find_bar.is_open()
                || sidebar_state.is_renaming()
                || library_space_renaming
                || menu_state.has_open_overlay()
                || space_editing_state.captures_text_input()
            {
                return;
            }
            if extension_popup_state.is_open() {
                let consumed = {
                    let model = core.model().peek();
                    extension_popup_state.handle_key_down(&e, &model)
                };
                if consumed {
                    e.stop_propagation();
                    e.prevent_default();
                    keymap.consume_key_up(e.code);
                    return;
                }
            }
            if !keymap.handle_global_key_down(&e, &core) {
                crate::ui::vim::leave_window_mode_on_unused_key(&e, &core, &keymap);
                if extension_popup_state.is_open() && e.code == Code::Escape {
                    e.stop_propagation();
                    e.prevent_default();
                    extension_popup_state.request_close();
                    return;
                }
                if vim_enabled {
                    let _ = try_extension_shortcut(&e);
                }
            }
        }
    };

    let on_stage_sized = {
        let adapter = Arc::clone(&hub_adapter);
        move |e: Event<SizedEventData>| {
            let dpr = *Platform::get().current_window().scale_factor.peek();
            let width = (e.area.width() as f64 * dpr).round() as i32;
            let height = (e.area.height() as f64 * dpr).round() as i32;
            if adapter.set_default_viewport((width, height), dpr) && width > 0 && height > 0 {
                viewport_known.set_if_modified(true);
            }
            Platform::get().with_window(Platform::window_id(), move |window| {
                vim_statusline_state.set_fullscreen(window.fullscreen().is_some());
            });
        }
    };

    let page_content: Element = if active_tab_id.is_some() {
        PageArea::new(PaneMode::Live).into_element()
    } else {
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(theme.color(ThemeRole::Surface))
            .main_align(Alignment::center())
            .cross_align(Alignment::center())
            .child(
                label()
                    .text("No tabs open in this space")
                    .font_size(theme.font_sizes.md)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
            .into_element()
    };

    let page_stage = PageStage::new()
        .on_sized(on_stage_sized)
        .page(page_content)
        .overlay(VimOverlayLayer::new())
        .overlay(crate::ui::page::find_bar::FindBar::default());

    rect()
        .width(Size::fill())
        .height(Size::fill())
        .on_global_pointer_down(move |e: Event<PointerEventData>| {
            let loc = e.global_location();
            last_click_pos.set(Some((loc.x as f32, loc.y as f32)));
        })
        .on_global_key_down(on_global_key_down)
        .on_global_key_up(on_global_key_up)
        .child(WindowShell::new(location, location_a11y_id, page_stage))
        .child(MenuLayer)
        .child(AddressBarLayer)
        .child(VimPickerOverlay)
        .child(MediaPreviewOverlay)
        .child(GradientGeneratorOverlay)
        .child(SpaceIconPickerOverlay)
        .child(SpaceDeleteConfirmOverlay)
        .child(ShareConfirmOverlay)
        .child(ExtensionPopupOverlay)
        .child(ExtensionPromptOverlay)
        .child(DownloadArcLayer { pinned_progress: None })
        .child(OnboardingOverlay)
}
