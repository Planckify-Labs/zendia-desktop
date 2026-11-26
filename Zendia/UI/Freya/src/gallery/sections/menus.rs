/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Gallery section showcasing the Phase 1 menu surfaces: Zen-styled app menu (vim off/on, submenu open),
//! Ladybird-styled context menus (page, editable with spelling guesses, selected text, link, image, media
//! playing/paused, tab), HTML `<select>` dropdown, and About Zendia dialog.

use crate::gallery::sections::{demo_card, state_caption};
use crate::ui::chrome::about::AboutDialogCard;
use crate::ui::chrome::menus::{AnchorRect, AppMenuInteractive, ContextMenuCard, SelectDropdownCard, use_menu_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, KitChip};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::MenuLocation;
use zendia_browser_core::menus::{
    APP_MENU_GROUPS, AboutUpdateStatus, MediaMenuContext, PageContextMenuKind, PageContextMenuRequest,
    SelectDropdownItem, SelectOption, build_page_context_menu, build_tab_context_menu, builtin_app_menu_entries,
    builtin_page_context_entries, resolve_contributed_menu,
};
use zendia_browser_core::model::TabId;
use zendia_keymap::KeyTable;

#[derive(Clone, PartialEq)]
pub struct MenusSection {
    pub state_id: String,
}

impl Component for MenusSection {
    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.state_id)
    }

    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let core = use_core();
        let menu_state = use_menu_state();
        let key_table = use_hook(KeyTable::standard);
        let mut preview_vim = use_state(|| self.state_id == "app-menu-vim-on");
        let mut last_state_id = use_state(|| self.state_id.clone());
        if *last_state_id.peek() != self.state_id {
            last_state_id.set(self.state_id.clone());
            preview_vim.set(self.state_id == "app-menu-vim-on");
        }

        let content: Element = match self.state_id.as_str() {
            "app-menu-vim-off" => render_app_menu_state(&theme, &core, &menu_state, &key_table, preview_vim, None),
            "app-menu-vim-on" => render_app_menu_state(&theme, &core, &menu_state, &key_table, preview_vim, None),
            "app-menu-submenu" => render_app_menu_state(
                &theme,
                &core,
                &menu_state,
                &key_table,
                preview_vim,
                Some("zendia.app.help"),
            ),
            "context-page" => render_page_context_state(&key_table),
            "context-editable" => render_editable_context_state(&key_table),
            "context-selection" => render_selection_context_state(&key_table),
            "context-link" => render_link_context_state(&key_table),
            "context-image" => render_image_context_state(&key_table),
            "context-media" => render_media_context_state(&key_table),
            "context-tab" => render_tab_context_state(&key_table),
            "select-dropdown" => render_select_dropdown_state(),
            "about-dialog" => render_about_dialog_state(&theme, &menu_state),
            _ => render_app_menu_state(&theme, &core, &menu_state, &key_table, preview_vim, None),
        };

        ScrollView::new().width(Size::fill()).height(Size::fill()).child(
            rect()
                .width(Size::fill())
                .padding(Gaps::new_all(20.0))
                .spacing(theme.spacing.xl)
                .child(content),
        )
    }
}

fn sample_app_toolbar_bar(
    theme: &crate::ui::theme::ResolvedTheme,
    menu_state: crate::ui::chrome::menus::MenuViewState,
) -> Element {
    let mut trigger_area = use_state(|| None::<Area>);
    rect()
        .width(Size::fill())
        .height(Size::px(38.0))
        .padding(Gaps::new(4.0, 10.0, 4.0, 10.0))
        .spacing(6.0)
        .direction(Direction::Horizontal)
        .cross_align(Alignment::center())
        .corner_radius(CornerRadius::new_all(8.0))
        .background(theme.color(ThemeRole::Surface))
        .border(
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::BorderSubtle)),
        )
        .child(
            rect()
                .width(Size::px(28.0))
                .height(Size::px(28.0))
                .center()
                .corner_radius(CornerRadius::new_all(6.0))
                .background(theme.color(ThemeRole::Hover))
                .cursor(CursorIcon::Pointer)
                .on_sized(move |e: Event<SizedEventData>| trigger_area.set_if_modified(Some(e.area)))
                .on_press(move |_| {
                    let anchor = (*trigger_area.peek()).map(AnchorRect::from_area);
                    menu_state.toggle_app_menu(anchor);
                })
                .child(Icon::new("menu").size(14.0).role(ThemeRole::Text)),
        )
        .child(Icon::new("sidebar").size(14.0).role(ThemeRole::TextMuted))
        .child(Icon::new("back").size(14.0).role(ThemeRole::TextMuted))
        .child(Icon::new("forward").size(14.0).role(ThemeRole::TextMuted))
        .child(Icon::new("reload").size(14.0).role(ThemeRole::TextMuted))
        .child(
            label()
                .text("Sample toolbar anchor (click ⋯ or press Alt+F / F10 for live anchored overlay)")
                .font_size(theme.font_sizes.xs)
                .color(theme.color(ThemeRole::TextMuted)),
        )
        .into_element()
}

fn render_app_menu_state(
    theme: &crate::ui::theme::ResolvedTheme,
    core: &crate::ui::core_view::CoreView,
    menu_state: &crate::ui::chrome::menus::MenuViewState,
    key_table: &KeyTable,
    mut preview_vim: State<bool>,
    initial_submenu: Option<&str>,
) -> Element {
    let vim_enabled = *preview_vim.read();
    let entries = builtin_app_menu_entries(vim_enabled);
    let items = resolve_contributed_menu(&entries, MenuLocation::AppMenu, Some(key_table), APP_MENU_GROUPS);
    let zoom_level = core.model().read().zoom_level;

    let mut panel = AppMenuInteractive::new(items, zoom_level)
        .on_toggle(move |(cmd, next): (String, bool)| {
            if cmd == "vim.set_enabled" {
                preview_vim.set(next);
            }
        })
        .key(initial_submenu.unwrap_or("main"));
    if let Some(sub) = initial_submenu {
        panel = panel.initial_open_submenu(sub);
    }

    let title = match (vim_enabled, initial_submenu) {
        (_, Some(_)) => "Application Menu — Help and Report Submenu Open",
        (true, None) => "Application Menu — Vim Mode On",
        (false, None) => "Application Menu — Vim Mode Off",
    };

    demo_card(
        title,
        "Zen-styled panel anchored to the app menu button with keymap-resolved shortcuts, zoom row, and core-backed Vim switch.",
        rect()
            .width(Size::fill())
            .spacing(8.0)
            .child(sample_app_toolbar_bar(theme, *menu_state))
            .child(rect().padding(Gaps::new(0.0, 0.0, 0.0, 4.0)).child(panel)),
    )
}

fn render_page_context_state(key_table: &KeyTable) -> Element {
    let req = PageContextMenuRequest {
        tab_id: Some(TabId(4)),
        kind: PageContextMenuKind::Page,
        can_navigate_back: true,
        can_navigate_forward: false,
        can_view_source: true,
        can_report_website_issue: true,
        can_inspect_element: true,
        ..PageContextMenuRequest::default()
    };
    let contribs = builtin_page_context_entries("ChatGPT");
    let items = build_page_context_menu(&req, &contribs, Some(key_table));

    demo_card(
        "Page Context Menu (Ladybird look + Zendia contributions)",
        "Follows UI/Qt/ContextMenus.cpp order and enabled states (Back enabled, Forward disabled) with Zendia's Ask, Split, and Glance entries.",
        ContextMenuCard::new(items).key("ctx-page"),
    )
}

fn render_editable_context_state(key_table: &KeyTable) -> Element {
    let req = PageContextMenuRequest {
        tab_id: Some(TabId(4)),
        kind: PageContextMenuKind::Page,
        is_editable: true,
        misspelled_word: Some("browsr".to_string()),
        spelling_guesses: vec!["browser".to_string(), "browsed".to_string(), "browses".to_string()],
        can_report_website_issue: true,
        can_inspect_element: true,
        ..PageContextMenuRequest::default()
    };
    let items = build_page_context_menu(&req, &[], Some(key_table));

    demo_card(
        "Editable Field Context Menu — Spelling Guesses",
        "Shows spelling suggestions, Ignore/Learn Spelling, Cut/Copy/Paste/Paste and Match Style/Select All, and Inspect Element.",
        ContextMenuCard::new(items).key("ctx-editable"),
    )
}

fn render_selection_context_state(key_table: &KeyTable) -> Element {
    let req = PageContextMenuRequest {
        tab_id: Some(TabId(4)),
        kind: PageContextMenuKind::Page,
        selected_text: Some("Ladybird browser engine".to_string()),
        has_dictionary_lookup: true,
        dictionary_lookup_text: Some("Ladybird".to_string()),
        search_engine_name: Some("Google".to_string()),
        makes_link_to_highlight: true,
        can_report_website_issue: true,
        can_inspect_element: true,
        ..PageContextMenuRequest::default()
    };
    let contribs = builtin_page_context_entries("ChatGPT");
    let items = build_page_context_menu(&req, &contribs, Some(key_table));

    demo_card(
        "Selected Text Context Menu",
        "Omits page navigation items when text is selected; offers Look Up, Copy, Search Google, Copy Link to Highlight, Zendia Ask, and Inspect.",
        ContextMenuCard::new(items).key("ctx-selection"),
    )
}

fn render_link_context_state(key_table: &KeyTable) -> Element {
    let req = PageContextMenuRequest {
        tab_id: Some(TabId(4)),
        kind: PageContextMenuKind::Link,
        link_url: Some("https://ladybird.org/docs/architecture".to_string()),
        has_link_navigation: true,
        supports_private_browsing_windows: true,
        can_inspect_element: true,
        ..PageContextMenuRequest::default()
    };
    let contribs = builtin_page_context_entries("ChatGPT");
    let items = build_page_context_menu(&req, &contribs, Some(key_table));

    demo_card(
        "Link Context Menu",
        "Open in New Tab/Window/Private Window, Copy Link, Download Linked File, Save Link As…, contextualized Zendia link actions, and Inspect.",
        ContextMenuCard::new(items).key("ctx-link"),
    )
}

fn render_image_context_state(key_table: &KeyTable) -> Element {
    let req = PageContextMenuRequest {
        tab_id: Some(TabId(4)),
        kind: PageContextMenuKind::Image,
        link_url: Some("https://ladybird.org".to_string()),
        has_link_navigation: true,
        navigation_url: Some("https://ladybird.org/assets/hero.png".to_string()),
        has_navigation: true,
        has_image_bitmap: true,
        image_mime_type: Some("image/png".to_string()),
        supports_private_browsing_windows: true,
        can_inspect_element: true,
        ..PageContextMenuRequest::default()
    };
    let contribs = builtin_page_context_entries("ChatGPT");
    let items = build_page_context_menu(&req, &contribs, Some(key_table));

    demo_card(
        "Image Context Menu (Linked Image)",
        "Combines link actions (when the image is inside an anchor) with Open Image in New Tab, Open Image, Copy Image, Copy Image Address, and Save Image As….",
        ContextMenuCard::new(items).key("ctx-image"),
    )
}

fn render_media_context_state(key_table: &KeyTable) -> Element {
    let playing_req = PageContextMenuRequest {
        tab_id: Some(TabId(4)),
        kind: PageContextMenuKind::Media,
        has_navigation: true,
        media: Some(MediaMenuContext {
            media_url: Some("https://example.org/demo.mp4".to_string()),
            is_video: true,
            is_playing: true,
            is_muted: false,
            has_user_agent_controls: true,
            is_looping: true,
            is_fullscreen: false,
            is_picture_in_picture: false,
            can_enter_picture_in_picture: true,
        }),
        can_inspect_element: true,
        ..PageContextMenuRequest::default()
    };

    let paused_req = PageContextMenuRequest {
        tab_id: Some(TabId(4)),
        kind: PageContextMenuKind::Media,
        has_navigation: true,
        media: Some(MediaMenuContext {
            media_url: Some("https://example.org/podcast.ogg".to_string()),
            is_video: false,
            is_playing: false,
            is_muted: true,
            has_user_agent_controls: false,
            is_looping: false,
            is_fullscreen: false,
            is_picture_in_picture: false,
            can_enter_picture_in_picture: false,
        }),
        can_inspect_element: true,
        ..PageContextMenuRequest::default()
    };

    let playing_items = build_page_context_menu(&playing_req, &[], Some(key_table));
    let paused_items = build_page_context_menu(&paused_req, &[], Some(key_table));

    demo_card(
        "Media Context Menus — Playing Video & Paused Audio",
        "Dynamic labels (Pause/Play, Mute/Unmute, Hide/Show Controls), checkable Loop item, Picture in Picture, Full Screen, and Open/Copy address.",
        rect()
            .direction(Direction::Horizontal)
            .spacing(28.0)
            .child(
                rect()
                    .spacing(8.0)
                    .child(state_caption("PLAYING VIDEO (LOOP CHECKED)"))
                    .child(ContextMenuCard::new(playing_items).key("ctx-media-playing")),
            )
            .child(
                rect()
                    .spacing(8.0)
                    .child(state_caption("PAUSED MUTED AUDIO"))
                    .child(ContextMenuCard::new(paused_items).key("ctx-media-paused")),
            ),
    )
}

fn render_tab_context_state(key_table: &KeyTable) -> Element {
    let core = use_core();
    let model = core.model().read().clone();
    let tab_id = model
        .active_tab
        .or_else(|| model.tabs.first().map(|t| t.id))
        .unwrap_or(TabId(1));
    let items = build_tab_context_menu(&model, tab_id, Some(key_table));

    demo_card(
        "Tab Context Menu (Zendia entries in Ladybird native menu look)",
        "Contributed via MenuLocation::TabContext with Ask ChatGPT site-picker row, Move to Folder, Split View, Essentials quota badge, and tab actions.",
        ContextMenuCard::new(items)
            .key("ctx-tab")
            .initial_open_submenu("zendia.tab.ask"),
    )
}

fn render_select_dropdown_state() -> Element {
    let items = vec![
        SelectDropdownItem::Option(SelectOption {
            id: 1,
            label: "Auto (Follow System)".to_string(),
            value: "system".to_string(),
            selected: false,
            disabled: false,
        }),
        SelectDropdownItem::Separator,
        SelectDropdownItem::Group {
            label: "Rendering Backends".to_string(),
            items: vec![
                SelectOption {
                    id: 2,
                    label: "Skia GPU (Vulkan)".to_string(),
                    value: "vulkan".to_string(),
                    selected: true,
                    disabled: false,
                },
                SelectOption {
                    id: 3,
                    label: "Skia GPU (OpenGL)".to_string(),
                    value: "gl".to_string(),
                    selected: false,
                    disabled: false,
                },
                SelectOption {
                    id: 4,
                    label: "Software Rasterizer (Unavailable)".to_string(),
                    value: "cpu".to_string(),
                    selected: false,
                    disabled: true,
                },
            ],
        },
        SelectDropdownItem::Separator,
        SelectDropdownItem::Group {
            label: "Profiles".to_string(),
            items: vec![
                SelectOption {
                    id: 5,
                    label: "Work Space".to_string(),
                    value: "work".to_string(),
                    selected: false,
                    disabled: false,
                },
                SelectOption {
                    id: 6,
                    label: "Research Space".to_string(),
                    value: "research".to_string(),
                    selected: false,
                    disabled: false,
                },
            ],
        },
    ];

    demo_card(
        "HTML <select> Dropdown Popup",
        "Ladybird native popup style supporting checkmarked selected option, optgroup headings, separators, disabled options, and keyboard navigation.",
        SelectDropdownCard::new(items).minimum_width(240.0).key("select-demo"),
    )
}

fn render_about_dialog_state(
    theme: &crate::ui::theme::ResolvedTheme,
    menu_state: &crate::ui::chrome::menus::MenuViewState,
) -> Element {
    let core = use_core();
    let modal_state = *menu_state;
    let cmd_core = core.clone();
    let current_status = core.model().read().about_update_status.clone();

    let preset_chips = [
        ("up-to-date", "Up to date"),
        ("checking", "Checking"),
        ("available", "Available"),
        ("downloading", "Downloading"),
        ("ready", "Restart ready"),
        ("failed", "Failed"),
        ("blocked", "Blocked"),
    ]
    .into_iter()
    .map(|(preset, caption)| {
        let chip_core = core.clone();
        let active = current_status.preset_name() == preset;
        KitChip::new(caption)
            .selected(active)
            .on_press(move |_| {
                if let Some(status) = AboutUpdateStatus::from_preset(preset) {
                    chip_core.invoke_or_log(Command::AppSetUpdateStatus { status });
                }
            })
            .key(preset)
            .into_element()
    });

    demo_card(
        "About Zendia Dialog",
        "Matches Zen's compact About layout with Living Emblem mascot, timed version copy feedback, update status section, Ladybird attribution, and keyboard-focusable links.",
        rect()
            .width(Size::fill())
            .spacing(12.0)
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .spacing(10.0)
                    .child(KitChip::new("Open as Modal Overlay").icon("info").on_press(move |_| {
                        modal_state.open_about();
                    }))
                    .child(
                        KitChip::new("Invoke app.about Command")
                            .icon("sparkles")
                            .on_press(move |_| {
                                cmd_core.invoke_or_log(Command::AppAbout);
                            }),
                    ),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .spacing(6.0)
                    .children(preset_chips),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(28.0, 20.0, 28.0, 20.0))
                    .center()
                    .corner_radius(CornerRadius::new_all(10.0))
                    .background(theme.color(ThemeRole::Surface))
                    .child(AboutDialogCard::new()),
            ),
    )
}
