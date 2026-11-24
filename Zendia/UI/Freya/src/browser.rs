/*
 * Copyright (c) 2026-present, the Zendia developers.
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The browser window: its tabs, its toolbar, and the page stage hosting one or more [`PageSurface`]s.

use crate::hub::{self, startup};
use crate::hub_adapter::HubTabAdapter;
use crate::input;
use crate::ui::chrome::{WindowShell, use_provide_chrome_view_state};
use crate::ui::core_view::use_provide_core_with_observer;
use crate::ui::page::{
    DebugInputOverlay, PageStage, PageSurface, PageSurfaceContext, use_provide_keymap, use_provide_page_surfaces,
};
use crate::ui::theme::{ThemeRole, use_provide_theme, use_theme};
use freya::prelude::*;
use hub_protocol::ipc_types::hub_client::messages as client;
use hub_protocol::ipc_types::hub_server::messages as server;
use hub_protocol::*;
use std::sync::Arc;
use zendia_browser_core::BrowserCore;
use zendia_browser_core::events::Event as CoreEvent;
use zendia_browser_core::model::TabId;
use zendia_browser_core::stub::StubBackend;
use zendia_browser_core::tab_source::TabSource;

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
fn handle_message(adapter: &HubTabAdapter, page_ctx: &PageSurfaceContext, message: HubMessage) {
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
            adapter.on_loading_state_changed(view_id, is_loading);
        }
        HubMessage::ViewLoadStart(_) | HubMessage::ViewLoadFinish(_) => {}
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
        HubMessage::AppQuitForSignal(_) => Platform::get().exit(),
    }
}

fn debug_split_enabled() -> bool {
    std::env::var_os("ZENDIA_DEBUG_SPLIT").is_some_and(|value| !value.is_empty() && value != "0")
}

/// Chooses the pair of tabs to show side by side in debug split mode.
///
/// Defaults to the last two tabs (so launching with two URLs under `--temporary-profile` splits those two pages rather
/// than the first-run welcome tab), while keeping an earlier active tab visible if the user explicitly activates it.
fn debug_split_tabs(tab_ids: &[TabId], active_tab: Option<TabId>) -> Option<(TabId, TabId)> {
    let len = tab_ids.len();
    if len < 2 {
        return None;
    }
    let left = tab_ids[len - 2];
    let right = tab_ids[len - 1];
    match active_tab {
        Some(active) if active != left && active != right => Some((active, right)),
        _ => Some((left, right)),
    }
}

pub fn app() -> impl IntoElement {
    let (stub_core, hub_adapter) = use_hook(|| {
        let device_pixel_ratio = *Platform::get().current_window().scale_factor.peek();
        let stub = Arc::new(StubBackend::new_for_external_tabs());
        let adapter = Arc::new(HubTabAdapter::new(Arc::clone(&stub), device_pixel_ratio));
        stub.attach_tab_source(Arc::clone(&adapter) as Arc<dyn TabSource>);
        (stub, adapter)
    });

    let debug_split = use_hook(debug_split_enabled);
    let mut viewport_known = use_state(|| false);
    let mut location = use_state(String::new);
    let location_a11y_id = use_hook(AccessibilityId::new_unique);

    let page_ctx = use_provide_page_surfaces();
    let keymap = use_provide_keymap();

    let core = use_provide_core_with_observer(Arc::clone(&stub_core) as Arc<dyn BrowserCore>, {
        let stub_core = Arc::clone(&stub_core);
        let page_ctx = page_ctx.clone();
        move |event| match event {
            CoreEvent::TabClosed { tab_id } => {
                page_ctx.remove_closed_view(tab_id.0);
                if stub_core.model().tabs.is_empty() {
                    Platform::get().exit();
                }
            }
            CoreEvent::LocationFocusRequested => {
                location_a11y_id.request_focus();
            }
            _ => {}
        }
    });
    let _theme = use_provide_theme(&core);
    let _chrome_view_state = use_provide_chrome_view_state();
    let theme = use_theme();

    // NB: What the Hub sends arrives on the IPC reader thread, which hands it to this task on the UI thread.
    use_hook({
        let adapter = Arc::clone(&hub_adapter);
        let page_ctx = page_ctx.clone();
        move || {
            spawn_in_window(async move {
                let incoming = startup().incoming.clone();
                while let Ok(incoming) = incoming.recv().await {
                    match incoming {
                        Incoming::Message(message) => handle_message(&adapter, &page_ctx, *message),
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

    // The tabs the browser starts with, once the page stage knows its size.
    let mut did_open_initial_tabs = use_state(|| false);
    if *viewport_known.read() && !*did_open_initial_tabs.peek() {
        did_open_initial_tabs.set(true);
        hub_adapter.open_initial_tabs();
    }

    // The location bar shows the address of the current tab, other than while the user edits it.
    let (active_url, active_tab_id, tab_ids) = {
        let model = core.model().read();
        let active = model.active_tab();
        (
            active.map(|tab| tab.url.clone()).unwrap_or_default(),
            model.active_tab,
            model.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
        )
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
    let track_modifiers_on_up = track_modifiers.clone();
    let on_global_key_up = move |e: Event<KeyboardEventData>| track_modifiers_on_up(false, &e);

    let on_global_key_down = {
        let core = core.clone();
        let keymap = keymap.clone();
        move |e: Event<KeyboardEventData>| {
            track_modifiers(true, &e);
            let _ = keymap.handle_global_key_down(&e, &core);
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
        }
    };

    let page_content: Element =
        if debug_split && let Some((left_tab, right_tab)) = debug_split_tabs(&tab_ids, active_tab_id) {
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .child(
                    PageSurface::for_tab(left_tab)
                        .key(left_tab.0)
                        .width(Size::flex(1.))
                        .height(Size::fill())
                        .show_focus_ring(true),
                )
                .child(
                    rect()
                        .width(Size::px(2.))
                        .height(Size::fill())
                        .background(theme.color(ThemeRole::Border)),
                )
                .child(
                    PageSurface::for_tab(right_tab)
                        .key(right_tab.0)
                        .width(Size::flex(1.))
                        .height(Size::fill())
                        .show_focus_ring(true),
                )
                .into_element()
        } else if let Some(tab_id) = active_tab_id {
            PageSurface::for_tab(tab_id)
                .key(tab_id.0)
                .width(Size::fill())
                .height(Size::fill())
                .into_element()
        } else {
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .background((255, 255, 255))
                .into_element()
        };

    let page_stage = PageStage::new()
        .on_sized(on_stage_sized)
        .page(page_content)
        .overlay(DebugInputOverlay);

    rect()
        .width(Size::fill())
        .height(Size::fill())
        .on_global_key_down(on_global_key_down)
        .on_global_key_up(on_global_key_up)
        .child(WindowShell::new(location, location_a11y_id, page_stage))
}
