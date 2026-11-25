/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Application menu, Ladybird-style context menus, `<select>` dropdowns, and the window-root [`MenuLayer`].

pub mod app_menu;
pub mod context_menu;
pub mod select_dropdown;
pub mod state;

pub use app_menu::{AppMenuInteractive, AppMenuOverlay};
pub use context_menu::{ContextMenuCard, ContextMenuOverlay};
pub use select_dropdown::{SelectDropdownCard, SelectDropdownOverlay};
pub use state::{
    AnchorRect, MenuViewState, OpenContextMenu, OpenSelectDropdown, try_use_menu_state, use_menu_state,
    use_provide_menu_state,
};

use crate::ui::chrome::about::AboutDialogOverlay;
use crate::ui::page::PageSurfaceContext;
use freya::prelude::*;
use hub_protocol::{Gfx_IntPoint, Web_HTML_SelectItem, WebView_ContextMenuKind, WebView_ContextMenuRequest};
use zendia_browser_core::BrowserCore;
use zendia_browser_core::menus::{
    MediaMenuContext, PageContextMenuKind, PageContextMenuRequest, SelectDropdownItem, SelectOption,
    build_folder_context_menu, build_page_context_menu, build_space_context_menu, build_tab_context_menu,
};
use zendia_browser_core::model::{BrowserModel, FolderId, SpaceId, TabId};
use zendia_keymap::KeyTable;

/// Single overlay host placed once at the window root above all chrome and page surfaces.
///
/// Renders whichever of the application menu, context menu, `<select>` dropdown, or About dialog is currently open in
/// [`MenuViewState`].
#[derive(Clone, PartialEq)]
pub struct MenuLayer;

impl Component for MenuLayer {
    fn render(&self) -> impl IntoElement {
        rect()
            .child(AppMenuOverlay)
            .child(ContextMenuOverlay)
            .child(SelectDropdownOverlay)
            .child(AboutDialogOverlay)
    }
}

/// Converts a Hub `view_request_context_menu` payload into a core [`PageContextMenuRequest`] and opens it in
/// `menu_state`.
#[allow(clippy::too_many_arguments)]
pub fn open_hub_context_menu(
    core: &dyn BrowserCore,
    page_ctx: &PageSurfaceContext,
    menu_state: &MenuViewState,
    view_id: u64,
    request: &WebView_ContextMenuRequest,
    can_view_source: bool,
    can_report_website_issue: bool,
    search_engine_name: Option<String>,
) {
    let model = core.model();
    let tab = model.tab_by_view_id(view_id);
    let tab_id = tab.map(|t| t.id).or(Some(TabId(view_id)));

    let surface_rect = page_ctx.tracker().borrow().surface_rect(view_id);
    let window_pos = match surface_rect {
        Some(rect) => {
            let dpr = rect.device_pixel_ratio.max(0.5) as f32;
            (
                rect.x + request.widget_position.x as f32 / dpr,
                rect.y + request.widget_position.y as f32 / dpr,
            )
        }
        None => (request.widget_position.x as f32, request.widget_position.y as f32),
    };

    let kind = match request.kind {
        WebView_ContextMenuKind::Page => PageContextMenuKind::Page,
        WebView_ContextMenuKind::Link => PageContextMenuKind::Link,
        WebView_ContextMenuKind::Image => PageContextMenuKind::Image,
        WebView_ContextMenuKind::Media => PageContextMenuKind::Media,
    };

    let media = request.media.as_ref().map(|m| MediaMenuContext {
        media_url: m.media_url.as_ref().map(|u| u.as_str().to_string()),
        is_video: m.is_video,
        is_playing: m.is_playing,
        is_muted: m.is_muted,
        has_user_agent_controls: m.has_user_agent_controls,
        is_looping: m.is_looping,
        is_fullscreen: m.is_fullscreen,
        is_picture_in_picture: m.is_picture_in_picture,
        can_enter_picture_in_picture: m.can_enter_picture_in_picture,
    });

    let core_request = PageContextMenuRequest {
        tab_id,
        kind,
        position: (request.widget_position.x, request.widget_position.y),
        is_editable: request.is_editable,
        selected_text: request
            .selected_text
            .as_ref()
            .map(|s| s.as_str().unwrap_or_default().to_string()),
        has_dictionary_lookup: request.dictionary_lookup.is_some(),
        dictionary_lookup_text: request
            .dictionary_lookup
            .as_ref()
            .map(|lookup| lookup.text.as_str().unwrap_or_default().to_string()),
        misspelled_word: request.misspelled_word.as_ref().map(|w| w.to_string_lossy()),
        spelling_guesses: request
            .spelling_guesses
            .iter()
            .map(|guess| guess.to_string_lossy())
            .collect(),
        makes_link_to_highlight: request.makes_link_to_highlight,
        can_inspect_element: request.can_inspect_element,
        link_url: request.link_navigation.as_ref().map(|n| n.url.as_str().to_string()),
        has_link_navigation: request.link_navigation.is_some(),
        navigation_url: request.navigation.as_ref().map(|n| n.url.as_str().to_string()),
        has_navigation: request.navigation.is_some(),
        has_image_bitmap: request.image_bitmap.as_ref().is_some_and(|bmp| bmp.0.is_some()),
        image_mime_type: request
            .image_mime_type
            .as_ref()
            .map(|m| m.as_str().unwrap_or_default().to_string()),
        media,
        can_navigate_back: tab.is_some_and(|t| t.can_navigate_back),
        can_navigate_forward: tab.is_some_and(|t| t.can_navigate_forward),
        can_view_source,
        can_report_website_issue,
        is_private: false,
        supports_private_browsing_windows: true,
        search_engine_name,
    };

    let key_table = KeyTable::standard();
    let items = build_page_context_menu(&core_request, &model.contributions.menu_entries, Some(&key_table));
    if !items.is_empty() {
        menu_state.open_context_menu(OpenContextMenu {
            position: window_pos,
            items,
            tab_id,
            open_submenu: None,
        });
    }
}

/// Converts a Hub `view_request_select_dropdown` payload into [`OpenSelectDropdown`] and opens it in `menu_state`.
pub fn open_hub_select_dropdown(
    core: &dyn BrowserCore,
    page_ctx: &PageSurfaceContext,
    menu_state: &MenuViewState,
    view_id: u64,
    content_position: &Gfx_IntPoint,
    minimum_width: i32,
    items: &[Web_HTML_SelectItem],
) {
    let model = core.model();
    let tab_id = model.tab_by_view_id(view_id).map(|t| t.id).or(Some(TabId(view_id)));

    let surface_rect = page_ctx.tracker().borrow().surface_rect(view_id);
    let (window_pos, min_width) = match surface_rect {
        Some(rect) => {
            // NB: `content_position` and `minimum_width` arrive in CSS/logical pixels relative to the page viewport.
            (
                (rect.x + content_position.x as f32, rect.y + content_position.y as f32),
                minimum_width.max(0) as f32,
            )
        }
        None => (
            (content_position.x as f32, content_position.y as f32),
            minimum_width.max(0) as f32,
        ),
    };

    let converted: Vec<SelectDropdownItem> = items
        .iter()
        .map(|item| match item {
            Web_HTML_SelectItem::Option(opt) => SelectDropdownItem::Option(SelectOption {
                id: opt.id,
                label: opt.label.to_string_lossy(),
                value: opt.value.to_string_lossy(),
                selected: opt.selected,
                disabled: opt.disabled,
            }),
            Web_HTML_SelectItem::OptionGroup(group) => SelectDropdownItem::Group {
                label: group.label.to_string_lossy(),
                items: group
                    .items
                    .iter()
                    .map(|opt| SelectOption {
                        id: opt.id,
                        label: opt.label.to_string_lossy(),
                        value: opt.value.to_string_lossy(),
                        selected: opt.selected,
                        disabled: opt.disabled,
                    })
                    .collect(),
            },
            Web_HTML_SelectItem::Separator(_) => SelectDropdownItem::Separator,
        })
        .collect();

    menu_state.open_select_dropdown(OpenSelectDropdown {
        position: window_pos,
        minimum_width: min_width,
        items: converted,
        tab_id,
    });
}

/// Opens the tab context menu for `tab_id` at `position`, built from the core's tab menu contributions.
pub fn open_tab_context_menu(menu_state: &MenuViewState, model: &BrowserModel, tab_id: TabId, position: (f32, f32)) {
    let key_table = KeyTable::standard();
    let items = build_tab_context_menu(model, tab_id, Some(&key_table));
    if !items.is_empty() {
        menu_state.open_context_menu(OpenContextMenu {
            position,
            items,
            tab_id: Some(tab_id),
            open_submenu: None,
        });
    }
}

/// Opens the folder context menu for `folder_id` at `position`, built from the core's folder menu contributions.
pub fn open_folder_context_menu(
    menu_state: &MenuViewState,
    model: &BrowserModel,
    folder_id: FolderId,
    position: (f32, f32),
) {
    let key_table = KeyTable::standard();
    let items = build_folder_context_menu(model, folder_id, Some(&key_table));
    if !items.is_empty() {
        menu_state.open_context_menu(OpenContextMenu {
            position,
            items,
            tab_id: None,
            open_submenu: None,
        });
    }
}

/// Opens the space context menu for `space_id` at `position`, built from the core's space menu contributions.
pub fn open_space_context_menu(
    menu_state: &MenuViewState,
    model: &BrowserModel,
    space_id: SpaceId,
    position: (f32, f32),
) {
    let key_table = KeyTable::standard();
    let items = build_space_context_menu(model, space_id, Some(&key_table));
    if !items.is_empty() {
        menu_state.open_context_menu(OpenContextMenu {
            position,
            items,
            tab_id: None,
            open_submenu: None,
        });
    }
}
