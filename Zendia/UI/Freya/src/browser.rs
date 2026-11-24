/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The browser window: its tabs, its toolbar and the page of the current tab.

use crate::frames::PageBackingStores;
use crate::hub::{self, startup};
use crate::input::{self, MouseState};
use freya::elements::image::{ImageHandle, image};
use freya::prelude::*;
use hub_protocol::ipc_types::hub_client::messages as client;
use hub_protocol::ipc_types::hub_server::messages as server;
use hub_protocol::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const TOOLBAR_BACKGROUND: (u8, u8, u8) = (238, 238, 242);
const TAB_STRIP_BACKGROUND: (u8, u8, u8) = (222, 222, 228);
const ACTIVE_TAB_BACKGROUND: (u8, u8, u8) = (238, 238, 242);
const TEXT_COLOR: (u8, u8, u8) = (30, 30, 34);
const MUTED_TEXT_COLOR: (u8, u8, u8) = (96, 96, 104);

#[derive(Clone, PartialEq)]
pub struct Tab {
    view_id: u64,
    title: String,
    url: String,
    is_loading: bool,
    can_navigate_back: bool,
    can_navigate_forward: bool,
    frame: Option<ImageHandle>,
    frame_size: (i32, i32),
    frame_device_pixel_ratio: f64,
    background: (u8, u8, u8),
    cursor: CursorIcon,
    hovered_link: Option<String>,
    closing: bool,
}

impl Tab {
    fn new(view_id: u64, state: &Hub_ViewState) -> Self {
        let mut tab = Self {
            view_id,
            title: String::new(),
            url: String::new(),
            is_loading: false,
            can_navigate_back: false,
            can_navigate_forward: false,
            frame: None,
            frame_size: (0, 0),
            frame_device_pixel_ratio: 1.0,
            background: (255, 255, 255),
            cursor: CursorIcon::Default,
            hovered_link: None,
            closing: false,
        };
        tab.apply_state(state);
        tab
    }

    fn apply_state(&mut self, state: &Hub_ViewState) {
        self.title = state.title.to_string_lossy();
        self.url = state.displayed_url.as_str().to_string();
        self.is_loading = state.is_loading;
        self.can_navigate_back = state.can_navigate_back;
        self.can_navigate_forward = state.can_navigate_forward;
        let color = state.page_background_color;
        self.background = (color.red(), color.green(), color.blue());
    }

    fn display_title(&self) -> String {
        if !self.title.is_empty() {
            self.title.clone()
        } else if !self.url.is_empty() && self.url != "about:blank" {
            self.url.clone()
        } else {
            "New Tab".to_string()
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct Browser {
    tabs: Vec<Tab>,
    active: Option<u64>,
    /// The page's size in device pixels, and how many of those make a logical pixel.
    viewport: (i32, i32),
    device_pixel_ratio: f64,
}

impl Browser {
    fn tab_mut(&mut self, view_id: u64) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.view_id == view_id)
    }

    fn active_tab(&self) -> Option<&Tab> {
        self.tabs.iter().find(|tab| Some(tab.view_id) == self.active)
    }
}

/// What the UI keeps of each view that does not change how anything looks.
#[derive(Default)]
struct ViewResources {
    backing_stores: HashMap<u64, PageBackingStores>,
    mouse: MouseState,
}

type Resources = Rc<RefCell<ViewResources>>;

fn is_private() -> WebView_IsPrivate {
    WebView_IsPrivate::No
}

/// Gets a view that the UI is about to show ready to be shown in this window.
fn set_up_view(browser: &Browser, view_id: u64) {
    let hub = hub::connection();
    hub.post_or_log(server::ViewSetPresentsFromCpuMemory {
        view_id,
        presents_from_cpu_memory: true,
    });
    hub.post_or_log(server::ViewSetHostsRemoteLayer {
        view_id,
        hosts_remote_layer: false,
    });
    if browser.viewport.0 > 0 && browser.viewport.1 > 0 {
        hub.post_or_log(server::ViewSetViewport {
            view_id,
            size: Web_DevicePixelSize {
                width: browser.viewport.0,
                height: browser.viewport.1,
            },
            device_pixel_ratio: browser.device_pixel_ratio,
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

fn activate(browser: &mut Browser, view_id: u64) {
    let hub = hub::connection();
    if let Some(previous) = browser.active.filter(|previous| *previous != view_id) {
        hub.post_or_log(server::ViewSetSystemVisibilityState {
            view_id: previous,
            visibility_state: Web_HTML_VisibilityState::Hidden,
        });
        hub.post_or_log(server::ViewSetCurrentTabOfWindow {
            view_id: previous,
            current_tab_of_window: false,
        });
        hub.post_or_log(server::ViewSetHasSystemFocus {
            view_id: previous,
            has_system_focus: false,
        });
    }
    browser.active = Some(view_id);
    hub.post_or_log(server::ViewSetSystemVisibilityState {
        view_id,
        visibility_state: Web_HTML_VisibilityState::Visible,
    });
    hub.post_or_log(server::ViewSetCurrentTabOfWindow {
        view_id,
        current_tab_of_window: true,
    });
    hub.post_or_log(server::ViewSetHasSystemFocus {
        view_id,
        has_system_focus: true,
    });
    hub.post_or_log(server::ActiveViewChanged { view_id: Some(view_id) });
}

/// Shows a view, which the Hub made, in a new tab placed after the tab that opened it, if any.
fn adopt_view(browser: &mut Browser, view_id: u64, state: &Hub_ViewState, after: Option<u64>, activate_tab: bool) {
    if browser.tabs.iter().any(|tab| tab.view_id == view_id) {
        return;
    }
    set_up_view(browser, view_id);
    let index = after
        .and_then(|after| browser.tabs.iter().position(|tab| tab.view_id == after))
        .map(|index| index + 1)
        .unwrap_or(browser.tabs.len());
    browser.tabs.insert(index, Tab::new(view_id, state));
    if activate_tab || browser.active.is_none() {
        activate(browser, view_id);
    }
}

fn open_tab(browser: &mut Browser, url: Option<URL_URL>, after: Option<u64>, activate_tab: bool) {
    let hub = hub::connection();
    let created = hub.send_sync(server::CreateView {
        request: Hub_NewViewRequest {
            is_private: is_private(),
            device_pixel_ratio: browser.device_pixel_ratio,
            maximum_frames_per_second: 60.0,
            display_id: None,
            screen_shows_high_dynamic_range: false,
            system_theme: Core_AnonymousBuffer::default(),
            screen_rects: None,
            owner_view_id: None,
            websites_prefer_dark: false,
            is_tab: true,
        },
    });
    let created = match created {
        Ok(created) => created,
        Err(error) => {
            eprintln!("zendia: Unable to open a tab: {error}");
            return;
        }
    };
    adopt_view(browser, created.view_id, &created.state, after, activate_tab);
    let url = url.unwrap_or_else(|| URL_URL::from_serialization(startup().settings.new_tab_page_url.clone()));
    load(created.view_id, url);
}

fn load(view_id: u64, url: URL_URL) {
    hub::connection().post_or_log(server::ViewLoad {
        view_id,
        url,
        history_behavior: Web_Bindings_NavigationHistoryBehavior::Auto,
    });
}

fn request_close(browser: &mut Browser, view_id: u64) {
    if let Some(tab) = browser.tab_mut(view_id) {
        tab.closing = true;
    }
    // NB: The page may ask the user whether to leave first. Once it may close, the Hub tells us to close the tab.
    hub::connection().post_or_log(server::ViewRequestClose { view_id });
}

fn close_tab(browser: &mut Browser, resources: &Resources, view_id: u64) {
    let Some(index) = browser.tabs.iter().position(|tab| tab.view_id == view_id) else {
        return;
    };
    browser.tabs.remove(index);
    resources.borrow_mut().backing_stores.remove(&view_id);
    hub::connection().post_or_log(server::DestroyView { view_id });

    if browser.active == Some(view_id) {
        browser.active = None;
        if let Some(next) = browser.tabs.get(index.min(browser.tabs.len().saturating_sub(1))) {
            let next = next.view_id;
            activate(browser, next);
        }
    }
    if browser.tabs.is_empty() {
        Platform::get().exit();
    }
}

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

/// Does what the Hub tells the UI to do.
fn handle_message(browser: &mut State<Browser>, resources: &Resources, message: HubMessage) {
    let hub = hub::connection();
    match message {
        HubMessage::ViewInstallBackingStores(client::ViewInstallBackingStores {
            view_id,
            page_id,
            bitmap_ids,
            backing_stores,
        }) => {
            if browser.peek().tabs.iter().all(|tab| tab.view_id != view_id) {
                if let Some(front) = bitmap_ids.first() {
                    hub.post_or_log(server::ReleaseBackingStore {
                        page_id,
                        bitmap_id: *front,
                    });
                }
                return;
            }
            let mut resources = resources.borrow_mut();
            resources
                .backing_stores
                .entry(view_id)
                .or_default()
                .install(page_id, bitmap_ids, backing_stores);
        }
        HubMessage::ViewAddBackingStores(client::ViewAddBackingStores {
            view_id,
            bitmap_ids,
            backing_stores,
        }) => {
            let mut resources = resources.borrow_mut();
            resources
                .backing_stores
                .entry(view_id)
                .or_default()
                .add(bitmap_ids, backing_stores);
        }
        HubMessage::ViewRetireBackingStores(client::ViewRetireBackingStores { view_id, bitmap_ids }) => {
            if let Some(stores) = resources.borrow_mut().backing_stores.get_mut(&view_id) {
                stores.retire(&bitmap_ids);
            }
        }
        HubMessage::ViewClearBackingStores(client::ViewClearBackingStores { view_id }) => {
            if let Some(stores) = resources.borrow_mut().backing_stores.get_mut(&view_id) {
                stores.clear();
            }
        }
        HubMessage::ViewPresentBackingStore(present) => {
            let presentation = match resources.borrow_mut().backing_stores.get_mut(&present.view_id) {
                Some(stores) if stores.page_id() == present.page_id => {
                    stores.present(present.bitmap_id, present.size.clone())
                }
                _ => crate::frames::Presentation {
                    frame: None,
                    release: Some(present.bitmap_id),
                },
            };
            if let Some(bitmap_id) = presentation.release {
                hub.post_or_log(server::ReleaseBackingStore {
                    page_id: present.page_id,
                    bitmap_id,
                });
            }
            if let Some(frame) = presentation.frame {
                let mut browser = browser.write();
                if let Some(tab) = browser.tab_mut(present.view_id) {
                    tab.frame = Some(frame);
                    tab.frame_size = (present.size.width, present.size.height);
                    tab.frame_device_pixel_ratio = present.device_pixel_ratio;
                }
            }
        }
        HubMessage::ViewAcceptPresentedBackingStore(_) => {}
        HubMessage::ViewStateChanged(client::ViewStateChanged { view_id, state }) => {
            if let Some(tab) = browser.write().tab_mut(view_id) {
                tab.apply_state(&state);
            }
        }
        HubMessage::ViewTitleChange(client::ViewTitleChange { view_id, title }) => {
            if let Some(tab) = browser.write().tab_mut(view_id) {
                tab.title = title.to_string_lossy();
            }
        }
        HubMessage::ViewUrlChange(client::ViewUrlChange { view_id, url }) => {
            if let Some(tab) = browser.write().tab_mut(view_id) {
                tab.url = url.as_str().to_string();
            }
        }
        HubMessage::ViewLoadingStateChange(client::ViewLoadingStateChange { view_id, is_loading }) => {
            if let Some(tab) = browser.write().tab_mut(view_id) {
                tab.is_loading = is_loading;
            }
        }
        HubMessage::ViewLoadStart(_) | HubMessage::ViewLoadFinish(_) => {}
        HubMessage::ViewPageBackgroundColorChange(client::ViewPageBackgroundColorChange { view_id, color }) => {
            if let Some(tab) = browser.write().tab_mut(view_id) {
                tab.background = (color.red(), color.green(), color.blue());
            }
        }
        HubMessage::ViewCursorChange(client::ViewCursorChange { view_id, cursor }) => {
            let icon = cursor_icon(&cursor);
            if let Some(tab) = browser.write().tab_mut(view_id) {
                tab.cursor = icon;
            }
        }
        HubMessage::ViewLinkHover(client::ViewLinkHover { view_id, url }) => {
            if let Some(tab) = browser.write().tab_mut(view_id) {
                tab.hovered_link = Some(url.as_str().to_string());
            }
        }
        HubMessage::ViewLinkUnhover(client::ViewLinkUnhover { view_id }) => {
            if let Some(tab) = browser.write().tab_mut(view_id) {
                tab.hovered_link = None;
            }
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
            let mut browser = browser.write();
            adopt_view(
                &mut browser,
                new_view_id,
                &new_view_state,
                Some(view_id),
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
                let mut browser = browser.write();
                adopt_view(
                    &mut browser,
                    view_id,
                    &description.state,
                    source_view_id,
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
            open_tab(
                &mut browser.write(),
                Some(url),
                source_view_id,
                activate_tab == Web_HTML_ActivateTab::Yes,
            );
        }
        // FIXME: Open new windows, once this UI has more than one.
        HubMessage::AppOpenUrlInNewWindow(client::AppOpenUrlInNewWindow { url, .. }) => {
            open_tab(&mut browser.write(), Some(url), None, true);
        }
        HubMessage::AppOpenNewTab(client::AppOpenNewTab { urls })
        | HubMessage::AppOpenNewWindow(client::AppOpenNewWindow { urls }) => {
            let mut browser = browser.write();
            if urls.is_empty() {
                open_tab(&mut browser, None, None, true);
            }
            for url in urls {
                open_tab(&mut browser, Some(url), None, true);
            }
        }
        HubMessage::ViewActivateTab(client::ViewActivateTab { view_id }) => {
            activate(&mut browser.write(), view_id);
        }
        HubMessage::ViewClose(client::ViewClose { view_id }) => {
            close_tab(&mut browser.write(), resources, view_id);
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

fn device_point(location: CursorPoint, device_pixel_ratio: f64) -> Web_DevicePixelPoint {
    Web_DevicePixelPoint {
        x: (location.x * device_pixel_ratio).round() as i32,
        y: (location.y * device_pixel_ratio).round() as i32,
    }
}

fn is_browser_shortcut(key: &Key, modifiers: Modifiers) -> bool {
    let command = modifiers.contains(Modifiers::CONTROL) || modifiers.contains(Modifiers::META);
    match key {
        Key::Character(text) if command => matches!(text.to_ascii_lowercase().as_str(), "t" | "w" | "l" | "r"),
        Key::Named(NamedKey::F5) => true,
        Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight) => modifiers.contains(Modifiers::ALT),
        _ => false,
    }
}

pub fn app() -> impl IntoElement {
    let mut browser = use_state(|| Browser {
        tabs: Vec::new(),
        active: None,
        viewport: (0, 0),
        device_pixel_ratio: *Platform::get().current_window().scale_factor.peek(),
    });
    let resources: Resources = use_hook(|| Rc::new(RefCell::new(ViewResources::default())));
    let mut location = use_state(String::new);
    let page_a11y_id = use_hook(AccessibilityId::new_unique);
    let location_a11y_id = use_hook(AccessibilityId::new_unique);

    // NB: What the Hub sends arrives on the IPC reader thread, which hands it to this task on the UI thread.
    use_hook({
        let resources = resources.clone();
        move || {
            spawn_in_window(async move {
                let incoming = startup().incoming.clone();
                while let Ok(incoming) = incoming.recv().await {
                    match incoming {
                        Incoming::Message(message) => handle_message(&mut browser, &resources, *message),
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

    // The tabs the browser starts with, once the page area knows its size.
    let mut did_open_initial_tabs = use_state(|| false);
    let viewport_known = browser.read().viewport.0 > 0;
    if viewport_known && !*did_open_initial_tabs.peek() {
        did_open_initial_tabs.set(true);
        let mut browser = browser.write();
        let urls = startup().initial_urls.clone();
        if urls.is_empty() {
            open_tab(&mut browser, None, None, true);
        }
        for url in urls {
            open_tab(&mut browser, Some(url), None, true);
        }
        hub::connection().post_or_log(server::UiDidStart);
    }

    // The location bar shows the address of the current tab, other than while the user edits it.
    let active_url = browser
        .read()
        .active_tab()
        .map(|tab| tab.url.clone())
        .unwrap_or_default();
    let shown_url = use_state(String::new);
    if *shown_url.peek() != active_url && !location_a11y_id.is_focused() {
        let mut shown_url = shown_url;
        shown_url.set(active_url.clone());
        location.set(active_url.clone());
    }

    let snapshot = browser.read().clone();
    let active = snapshot.active_tab().cloned();

    // NB: Mouse events carry no modifiers, so the page's mouse events take those of the last key event, wherever the
    //     focus is.
    let track_modifiers = {
        let resources = resources.clone();
        move |is_down: bool, e: &KeyboardEventData| {
            let modifiers = input::modifiers_after_key(is_down, &e.key, e.modifiers);
            resources.borrow_mut().mouse.modifiers = input::key_modifiers(modifiers);
        }
    };
    let track_modifiers_on_up = track_modifiers.clone();
    let on_global_key_up = move |e: Event<KeyboardEventData>| track_modifiers_on_up(false, &e);

    let on_global_key_down = move |e: Event<KeyboardEventData>| {
        track_modifiers(true, &e);
        let modifiers = e.modifiers;
        let command = modifiers.contains(Modifiers::CONTROL) || modifiers.contains(Modifiers::META);
        match &e.key {
            Key::Character(text) if command => match text.to_ascii_lowercase().as_str() {
                "t" => open_tab(&mut browser.write(), None, None, true),
                "w" => {
                    let active = browser.peek().active;
                    if let Some(view_id) = active {
                        request_close(&mut browser.write(), view_id);
                    }
                }
                "l" => location_a11y_id.request_focus(),
                "r" => {
                    if let Some(view_id) = browser.peek().active {
                        hub::connection().post_or_log(server::ViewReload {
                            view_id,
                            bypass_cache: WebView_BypassCache::No,
                        });
                    }
                }
                _ => {}
            },
            Key::Named(NamedKey::F5) => {
                if let Some(view_id) = browser.peek().active {
                    hub::connection().post_or_log(server::ViewReload {
                        view_id,
                        bypass_cache: WebView_BypassCache::No,
                    });
                }
            }
            Key::Named(NamedKey::ArrowLeft) if modifiers.contains(Modifiers::ALT) => {
                if let Some(view_id) = browser.peek().active {
                    hub::connection().post_or_log(server::ViewTraverseTheHistoryByDelta { view_id, delta: -1 });
                }
            }
            Key::Named(NamedKey::ArrowRight) if modifiers.contains(Modifiers::ALT) => {
                if let Some(view_id) = browser.peek().active {
                    hub::connection().post_or_log(server::ViewTraverseTheHistoryByDelta { view_id, delta: 1 });
                }
            }
            _ => {}
        }
    };

    rect()
        .width(Size::fill())
        .height(Size::fill())
        .background(TOOLBAR_BACKGROUND)
        .color(TEXT_COLOR)
        .on_global_key_down(on_global_key_down)
        .on_global_key_up(on_global_key_up)
        .child(tab_strip(browser, resources.clone()))
        .child(toolbar(
            browser,
            location,
            location_a11y_id,
            page_a11y_id,
            active.as_ref(),
        ))
        .child(page_view(browser, resources, page_a11y_id, active))
}

fn tab_strip(mut browser: State<Browser>, resources: Resources) -> impl IntoElement {
    let snapshot = browser.read().clone();
    let tabs = snapshot.tabs.iter().map(|tab| {
        let view_id = tab.view_id;
        let is_active = snapshot.active == Some(view_id);
        let resources = resources.clone();
        let title = if tab.is_loading {
            format!("⟳ {}", tab.display_title())
        } else {
            tab.display_title()
        };
        rect()
            .key(view_id)
            .width(Size::px(200.))
            .height(Size::fill())
            .padding(Gaps::new(0., 6., 0., 12.))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(6.))
            .background(if is_active {
                ACTIVE_TAB_BACKGROUND
            } else {
                TAB_STRIP_BACKGROUND
            })
            .on_press(move |_| activate(&mut browser.write(), view_id))
            .child(
                label()
                    .text(title)
                    .width(Size::flex(1.))
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .font_size(13.)
                    .color(if is_active { TEXT_COLOR } else { MUTED_TEXT_COLOR }),
            )
            .child(
                rect()
                    .width(Size::px(22.))
                    .height(Size::px(22.))
                    .main_align(Alignment::center())
                    .cross_align(Alignment::center())
                    .corner_radius(CornerRadius::new_all(11.))
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        let tab_is_closing = browser
                            .peek()
                            .tabs
                            .iter()
                            .any(|tab| tab.view_id == view_id && tab.closing);
                        if tab_is_closing {
                            // NB: A page that does not answer is closed without asking it again.
                            close_tab(&mut browser.write(), &resources, view_id);
                        } else {
                            request_close(&mut browser.write(), view_id);
                        }
                    })
                    .child(label().text("×").font_size(15.)),
            )
    });

    rect()
        .width(Size::fill())
        .height(Size::px(38.))
        .padding(Gaps::new(4., 8., 0., 8.))
        .spacing(4.)
        .direction(Direction::Horizontal)
        .background(TAB_STRIP_BACKGROUND)
        .children(tabs)
        .child(
            rect()
                .width(Size::px(30.))
                .height(Size::fill())
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .on_press(move |_| open_tab(&mut browser.write(), None, None, true))
                .child(label().text("+").font_size(18.)),
        )
}

fn toolbar_button(
    text: &'static str,
    enabled: bool,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> impl IntoElement {
    Button::new()
        .flat()
        .compact()
        .enabled(enabled)
        .on_press(on_press)
        .child(label().text(text).font_size(16.))
}

fn toolbar(
    browser: State<Browser>,
    location: State<String>,
    location_a11y_id: AccessibilityId,
    page_a11y_id: AccessibilityId,
    active: Option<&Tab>,
) -> impl IntoElement {
    let active_view = active.map(|tab| tab.view_id);
    let can_back = active.is_some_and(|tab| tab.can_navigate_back);
    let can_forward = active.is_some_and(|tab| tab.can_navigate_forward);
    let is_loading = active.is_some_and(|tab| tab.is_loading);

    let on_submit = move |text: String| {
        let Some(view_id) = browser.peek().active else { return };
        if let Some(url) = location::url_for_location(&text, &startup().settings.search_url) {
            load(view_id, url);
            page_a11y_id.request_focus();
        }
    };

    rect()
        .width(Size::fill())
        .height(Size::px(44.))
        .padding(Gaps::new(6., 8., 6., 8.))
        .spacing(4.)
        .direction(Direction::Horizontal)
        .cross_align(Alignment::center())
        .child(toolbar_button("←", can_back, move |_| {
            if let Some(view_id) = active_view {
                hub::connection().post_or_log(server::ViewTraverseTheHistoryByDelta { view_id, delta: -1 });
            }
        }))
        .child(toolbar_button("→", can_forward, move |_| {
            if let Some(view_id) = active_view {
                hub::connection().post_or_log(server::ViewTraverseTheHistoryByDelta { view_id, delta: 1 });
            }
        }))
        .child(toolbar_button(
            if is_loading { "✕" } else { "↻" },
            active_view.is_some(),
            move |_| {
                let Some(view_id) = active_view else { return };
                if is_loading {
                    hub::connection().post_or_log(server::ViewStopLoading { view_id });
                } else {
                    hub::connection().post_or_log(server::ViewReload {
                        view_id,
                        bypass_cache: WebView_BypassCache::No,
                    });
                }
            },
        ))
        .child(
            Input::new(location)
                .a11y_id(location_a11y_id)
                .placeholder("Search or enter an address")
                .width(Size::flex(1.))
                .on_submit(on_submit),
        )
}

fn page_view(
    mut browser: State<Browser>,
    resources: Resources,
    page_a11y_id: AccessibilityId,
    active: Option<Tab>,
) -> impl IntoElement {
    let view_id = active.as_ref().map(|tab| tab.view_id);
    let device_pixel_ratio = browser.read().device_pixel_ratio;

    let on_sized = move |e: Event<SizedEventData>| {
        let device_pixel_ratio = *Platform::get().current_window().scale_factor.peek();
        let width = (e.area.width() as f64 * device_pixel_ratio).round() as i32;
        let height = (e.area.height() as f64 * device_pixel_ratio).round() as i32;
        let current = browser.peek().clone();
        if current.viewport == (width, height) && current.device_pixel_ratio == device_pixel_ratio {
            return;
        }
        let mut browser = browser.write();
        browser.viewport = (width, height);
        browser.device_pixel_ratio = device_pixel_ratio;
        if width <= 0 || height <= 0 {
            return;
        }
        for tab in &browser.tabs {
            hub::connection().post_or_log(server::ViewSetViewport {
                view_id: tab.view_id,
                size: Web_DevicePixelSize { width, height },
                device_pixel_ratio,
            });
        }
    };

    let send_mouse = move |event: Web_MouseEvent| {
        if let Some(view_id) = view_id {
            hub::connection().post_or_log(server::ViewEnqueueMouseEvent {
                view_id,
                input_id: 0,
                event,
            });
        }
    };

    let on_mouse_down = {
        let resources = resources.clone();
        move |e: Event<MouseEventData>| {
            page_a11y_id.request_focus();
            let Some(button) = e.button else { return };
            let button = input::mouse_button(button);
            let event = resources
                .borrow_mut()
                .mouse
                .down(device_point(e.element_location, device_pixel_ratio), button);
            send_mouse(event);
        }
    };
    let on_mouse_up = {
        let resources = resources.clone();
        move |e: Event<MouseEventData>| {
            let Some(button) = e.button else { return };
            let button = input::mouse_button(button);
            let event = resources
                .borrow_mut()
                .mouse
                .up(device_point(e.element_location, device_pixel_ratio), button);
            send_mouse(event);
        }
    };
    let on_mouse_move = {
        let resources = resources.clone();
        move |e: Event<MouseEventData>| {
            let event = resources
                .borrow()
                .mouse
                .moved(device_point(e.element_location, device_pixel_ratio));
            send_mouse(event);
        }
    };
    let on_pointer_leave = {
        let resources = resources.clone();
        move |_: Event<PointerEventData>| {
            let event = resources.borrow_mut().mouse.left();
            send_mouse(event);
        }
    };
    let on_wheel = {
        let resources = resources.clone();
        move |e: Event<WheelEventData>| {
            let event = resources.borrow().mouse.wheel(
                device_point(e.element_location, device_pixel_ratio),
                e.delta_x,
                e.delta_y,
                e.source,
                device_pixel_ratio,
            );
            send_mouse(event);
        }
    };
    let send_key = {
        move |is_down: bool, e: Event<KeyboardEventData>| {
            if is_browser_shortcut(&e.key, e.modifiers) {
                return;
            }
            let Some(view_id) = view_id else { return };
            let event = input::key_event(is_down, &e.key, e.code, e.modifiers);
            hub::connection().post_or_log(server::ViewEnqueueKeyEvent {
                view_id,
                input_id: 0,
                event,
            });
        }
    };
    let send_key_up = send_key;

    let (background, cursor, frame, hovered_link) = match &active {
        Some(tab) => (
            tab.background,
            tab.cursor,
            tab.frame
                .clone()
                .map(|frame| (frame, tab.frame_size, tab.frame_device_pixel_ratio)),
            tab.hovered_link.clone(),
        ),
        None => ((255, 255, 255), CursorIcon::Default, None, None),
    };

    rect()
        .width(Size::fill())
        .height(Size::flex(1.))
        .background(background)
        .a11y_id(page_a11y_id)
        .a11y_focusable(true)
        .a11y_role(AccessibilityRole::Document)
        .cursor(cursor)
        .on_sized(on_sized)
        .on_mouse_down(on_mouse_down)
        .on_mouse_up(on_mouse_up)
        .on_mouse_move(on_mouse_move)
        .on_pointer_leave(on_pointer_leave)
        .on_wheel(on_wheel)
        .on_key_down(move |e| send_key(true, e))
        .on_key_up(move |e| send_key_up(false, e))
        .maybe_child(frame.map(|(frame, (width, height), frame_device_pixel_ratio)| {
            image(frame)
                .width(Size::px((width as f64 / frame_device_pixel_ratio) as f32))
                .height(Size::px((height as f64 / frame_device_pixel_ratio) as f32))
        }))
        .maybe_child(hovered_link.map(|link| {
            rect()
                .position(Position::new_absolute().left(0.).bottom(0.))
                .padding(Gaps::new(3., 8., 3., 8.))
                .background(TOOLBAR_BACKGROUND)
                .corner_radius(CornerRadius::new_all(4.))
                .child(label().text(link).font_size(12.).max_lines(1))
        }))
}
