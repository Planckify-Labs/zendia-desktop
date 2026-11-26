/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Standalone component gallery (`zendia-gallery`) backed by `BrowserCore`'s stub backend with no Hub process.

pub mod registry;
pub mod sections;

pub use registry::{
    CommandLogEntry, CommandLogOutcome, GALLERY_SECTIONS, GalleryCore, GalleryLaunchOptions, GallerySectionSpec,
    GalleryStateSpec, default_state_id, find_section, find_state,
};

use crate::gallery::sections::render_section;
use crate::ui::chrome::address_bar::{AddressBarLayer, use_provide_address_bar_state};
use crate::ui::chrome::menus::{MenuLayer, use_provide_menu_state};
use crate::ui::chrome::sidebar::use_provide_sidebar_state;
use crate::ui::core_view::{use_core, use_provide_core_with_observer};
use crate::ui::kit::{Icon, KitChip, KitSwitch, ListRow, Pill, use_provide_text_focus};
use crate::ui::library::media_preview::{MediaPreviewOverlay, use_provide_media_preview_state};
use crate::ui::library::sections::space_routing::use_provide_space_routing_view_state;
use crate::ui::library::sections::spaces::use_provide_spaces_view_state;
use crate::ui::library::use_provide_library_state;
use crate::ui::onboarding::{OnboardingOverlay, use_provide_onboarding_state};
use crate::ui::page::download_arc::use_provide_download_arc;
use crate::ui::page::use_provide_keymap;
use crate::ui::panels::use_provide_dock_state;
use crate::ui::share::{ShareConfirmOverlay, use_provide_share_state};
use crate::ui::spaces::{GradientGeneratorOverlay, use_provide_space_editing_state};
use crate::ui::theme::{ResolvedScheme, ThemeRole, use_provide_theme, use_theme};
use crate::ui::vim::use_provide_vim_statusline_state;
use crate::ui::vim::{VimPickerOverlay, use_provide_region_picker_state, use_provide_vim_picker_state};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::BrowserCore;
use zendia_browser_core::commands::Command;
use zendia_browser_core::events::{Event as CoreEvent, ViewRequest};
use zendia_browser_core::model::ThemeScheme;
use zendia_module_api::ui::UiTone;

/// Launches the standalone `zendia-gallery` window with the given startup options.
pub fn launch_gallery(options: GalleryLaunchOptions) {
    let width = options.window_width as f64;
    let height = options.window_height as f64;
    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new_app(GalleryApp { options })
                .with_title("Zendia UI Gallery")
                .with_size(width, height),
        ),
    );
}

struct GalleryApp {
    options: GalleryLaunchOptions,
}

impl App for GalleryApp {
    fn render(&self) -> impl IntoElement {
        let initial_theme = self.options.initial_theme;
        let initial_section = self.options.initial_section.clone();
        let initial_state = self.options.resolved_initial_state().to_string();

        let gallery_core = use_hook(|| Arc::new(GalleryCore::new(initial_theme)));
        let _text_focus = use_provide_text_focus();
        let menu_state = use_provide_menu_state();
        let sidebar_state = use_provide_sidebar_state();
        let address_bar_state = use_provide_address_bar_state();
        let download_arc = use_provide_download_arc();
        let vim_statusline_state = use_provide_vim_statusline_state();
        let vim_picker_state = use_provide_vim_picker_state();
        let region_picker_state = use_provide_region_picker_state();
        let mut dock_state = use_provide_dock_state();
        let library_state = use_provide_library_state();
        let library_spaces_state = use_provide_spaces_view_state();
        let library_routing_state = use_provide_space_routing_view_state();
        let media_preview_state = use_provide_media_preview_state();
        let onboarding_state = use_provide_onboarding_state(true);
        let space_editing_state = use_provide_space_editing_state();
        let share_state = use_provide_share_state();
        let core = use_provide_core_with_observer(Arc::clone(&gallery_core) as Arc<dyn BrowserCore>, {
            let gallery_core = Arc::clone(&gallery_core);
            move |event| match event {
                CoreEvent::ClipboardCopyRequested { text } if !text.is_empty() => {
                    let _ = freya::clipboard::Clipboard::set(text.clone());
                }
                CoreEvent::DownloadStarted { .. } => download_arc.launch((980.0, 520.0), (60.0, 900.0)),
                CoreEvent::RegionCaptured { .. } => {
                    region_picker_state.stop();
                }
                CoreEvent::VimModeChanged { enabled: false } => {
                    vim_statusline_state.close_all_keymaps();
                    vim_picker_state.close();
                    region_picker_state.stop();
                }
                CoreEvent::ModeChanged { .. }
                | CoreEvent::WhichKeyChanged { .. }
                | CoreEvent::VimModeChanged { .. } => {
                    vim_statusline_state.close_all_keymaps();
                }
                CoreEvent::ViewRequested { request, .. } => match request {
                    ViewRequest::ExtensionsMenuToggle => {
                        address_bar_state.close();
                        address_bar_state.close_site_panel();
                        share_state.close();
                        menu_state.toggle_extensions_menu();
                    }
                    ViewRequest::LibraryToggle { .. } | ViewRequest::LibraryOpen { .. } | ViewRequest::LibraryClose => {
                        library_state.handle_view_request(request);
                        let _ = gallery_core.invoke(Command::LibraryFocusSet {
                            focused: *library_state.open.peek(),
                        });
                    }
                    ViewRequest::SpaceRename { space_id } => {
                        if let Some(space) = gallery_core.model().space(*space_id) {
                            library_spaces_state.start_rename(*space_id, space.name.clone());
                        }
                    }
                    ViewRequest::SpaceIconPicker { space_id } => {
                        library_spaces_state.open_icon_picker(*space_id, None);
                    }
                    ViewRequest::SpaceRoutingEdit { rule_id } => {
                        if let Some(rule) = gallery_core
                            .model()
                            .space_routing
                            .rules
                            .iter()
                            .find(|r| r.id == *rule_id)
                        {
                            library_routing_state.start_edit(rule);
                        }
                    }
                    ViewRequest::LibraryMediaPreviewOpen { .. }
                    | ViewRequest::LibraryMediaPreviewClose
                    | ViewRequest::LibraryMediaPreviewStep { .. } => {
                        let items = gallery_core.model().library_media;
                        media_preview_state.handle_view_request(request, &items);
                    }
                    ViewRequest::LocationFocus => {
                        menu_state.close_all();
                        share_state.close();
                        let current_url = gallery_core
                            .model()
                            .active_tab()
                            .map(|tab| tab.url.clone())
                            .filter(|url| !url.is_empty() && url != "about:blank" && url != "about:newtab");
                        address_bar_state.open_floating(false, current_url);
                    }
                    ViewRequest::AddressBarOpen { new_tab, initial_query } => {
                        menu_state.close_all();
                        share_state.close();
                        address_bar_state.open_floating(*new_tab, initial_query.clone());
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
                        if let Some(tab) = gallery_core.model().tab(*tab_id) {
                            sidebar_state.start_tab_rename(*tab_id, tab.display_title());
                        }
                    }
                    ViewRequest::CompactSidebarReveal => {
                        let mut pinned = sidebar_state.compact_pinned;
                        let next = !*pinned.peek();
                        pinned.set(next);
                        let mut revealed = sidebar_state.compact_revealed;
                        revealed.set(next);
                    }
                    ViewRequest::FolderRename { folder_id } => {
                        if let Some(folder) = gallery_core.model().folder(*folder_id) {
                            sidebar_state.start_folder_rename(*folder_id, folder.name.clone());
                        }
                    }
                    ViewRequest::PickerOpen { request } => {
                        if gallery_core.model().vim_mode_enabled {
                            menu_state.close_all();
                            address_bar_state.close();
                            address_bar_state.close_site_panel();
                            region_picker_state.stop();
                            vim_picker_state.open(request.clone());
                        }
                    }
                    ViewRequest::PickerClosed { .. } => {
                        vim_picker_state.close();
                    }
                    ViewRequest::RegionPick { tab_id } => {
                        if gallery_core.model().vim_mode_enabled {
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
                    | ViewRequest::AgentInstructions => {
                        dock_state.apply_request(request);
                    }
                    // The gallery has no Hub, so web chat sites have no views to reload or navigate.
                    ViewRequest::WebChatReload { .. } | ViewRequest::WebChatNavigateHome { .. } => {}
                    ViewRequest::OnboardingOpen => onboarding_state.open(),
                    ViewRequest::OnboardingClose => onboarding_state.close(),
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
                        space_editing_state.open_theme_picker(*space_id, Some((456.0, 180.0)));
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
                    ViewRequest::SettingsOpen { .. } | ViewRequest::SettingsClose => {}
                },
                _ => {}
            }
        });
        let _theme_state = use_provide_theme(&core);
        let keymap = use_provide_keymap();
        let theme = use_theme();

        let mut active_section_id = use_state(|| initial_section);
        let mut active_state_id = use_state(|| initial_state);
        let mut command_log = use_state(|| gallery_core.recent_log());
        let log_expanded = use_state(|| false);

        use_hook({
            let gallery_core = Arc::clone(&gallery_core);
            move || {
                let updates = gallery_core.log_updates();
                spawn_in_window(async move {
                    while let Ok(entry) = updates.recv().await {
                        let mut entries = command_log.write();
                        if entries.len() >= 32 {
                            entries.remove(0);
                        }
                        entries.push(entry);
                    }
                });
            }
        });

        let active_section = find_section(&active_section_id.read()).unwrap_or(&GALLERY_SECTIONS[0]);
        let current_state_id = active_state_id.read().clone();
        let active_state = find_state(active_section, &current_state_id).unwrap_or(&active_section.states[0]);

        let on_global_key_down = {
            let core = core.clone();
            let keymap = keymap.clone();
            move |e: Event<KeyboardEventData>| {
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
                {
                    return;
                }
                if (dock_state.captures_text_input() || space_editing_state.captures_text_input())
                    && !e
                        .modifiers
                        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
                {
                    return;
                }
                if !address_bar_state.is_open()
                    && !sidebar_state.is_renaming()
                    && !menu_state.has_open_overlay()
                    && crate::ui::library::nav::handle_library_key_down(&e, &core, &library_state)
                {
                    return;
                }
                if !address_bar_state.is_open()
                    && !sidebar_state.is_renaming()
                    && !dock_state.captures_text_input()
                    && crate::ui::vim::handle_focus_key_down(&e, &core)
                {
                    return;
                }
                let _ = keymap.handle_global_key_down(&e, &core);
            }
        };

        let is_dark = theme.scheme == ResolvedScheme::Dark;
        let theme_toggle_core = core.clone();

        let sidebar = rect()
            .width(Size::px(244.0))
            .height(Size::fill())
            .background(theme.color(ThemeRole::SurfaceRaised))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(16.0, 14.0, 14.0, 14.0))
                    .spacing(theme.spacing.md)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.color(ThemeRole::BorderSubtle)),
                    )
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(theme.spacing.md)
                            .child(Icon::new("sparkles").size(18.0).role(ThemeRole::Accent))
                            .child(
                                label()
                                    .text("Zendia Gallery")
                                    .font_size(theme.font_sizes.xl)
                                    .font_weight(FontWeight::BOLD)
                                    .color(theme.color(ThemeRole::Text)),
                            ),
                    )
                    .child(
                        label()
                            .text("Stub backend · No Hub required")
                            .font_size(theme.font_sizes.xs)
                            .color(theme.color(ThemeRole::TextMuted)),
                    )
                    .child(
                        KitSwitch::new(is_dark)
                            .label(if is_dark { "Dark theme" } else { "Light theme" })
                            .on_toggle(move |next_dark| {
                                let scheme = if next_dark {
                                    ThemeScheme::Dark
                                } else {
                                    ThemeScheme::Light
                                };
                                theme_toggle_core.invoke_or_log(Command::ThemeSet {
                                    scheme: Some(scheme),
                                    overrides: None,
                                });
                            }),
                    ),
            )
            .child(
                ScrollView::new().width(Size::fill()).height(Size::flex(1.0)).child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new_all(10.0))
                        .spacing(theme.spacing.xs)
                        .children(GALLERY_SECTIONS.iter().map(|section| {
                            let is_selected = section.id == active_section.id;
                            let section_id = section.id;
                            let default_state = default_state_id(section);
                            ListRow::new(section.title)
                                .key(section.id)
                                .icon(section.icon)
                                .detail(format!("P{}", section.phase))
                                .cursor(is_selected)
                                .selected(is_selected)
                                .on_press(move |_| {
                                    address_bar_state.close();
                                    address_bar_state.close_site_panel();
                                    vim_picker_state.close();
                                    region_picker_state.stop();
                                    media_preview_state.close();
                                    if section_id != "library-media" {
                                        library_state.close();
                                    }
                                    if section_id != "agent-panel" && section_id != "settings" {
                                        dock_state.open_panels.set(Vec::new());
                                        dock_state.focused_panel.set(None);
                                        dock_state.model_card_open.set(false);
                                        dock_state.instructions_open.set(false);
                                    }
                                    if !section_id.starts_with("library") {
                                        library_state.close();
                                        library_state.clear_gallery_overrides();
                                        library_spaces_state.reset();
                                        library_routing_state.reset();
                                    }
                                    if section_id != "onboarding" {
                                        onboarding_state.reset_for_gallery();
                                    }
                                    if section_id != "spaces-editing" {
                                        space_editing_state.reset();
                                    }
                                    if section_id != "share" {
                                        share_state.close();
                                    }
                                    active_section_id.set(section_id.to_string());
                                    active_state_id.set(default_state.to_string());
                                })
                        })),
                ),
            );

        let header = rect()
            .width(Size::fill())
            .padding(Gaps::new(14.0, 20.0, 14.0, 20.0))
            .spacing(theme.spacing.md)
            .background(theme.color(ThemeRole::Surface))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .main_align(Alignment::SpaceBetween)
                    .spacing(theme.spacing.md)
                    .child(
                        rect()
                            .spacing(theme.spacing.xxs)
                            .child(
                                rect()
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(theme.spacing.md)
                                    .child(
                                        label()
                                            .text(active_section.title)
                                            .font_size(theme.font_sizes.title)
                                            .font_weight(FontWeight::BOLD)
                                            .color(theme.color(ThemeRole::Text)),
                                    )
                                    .child(Pill::new(format!("Phase {}", active_section.phase)).tone(UiTone::Accent)),
                            )
                            .child(
                                label()
                                    .text(active_state.description)
                                    .font_size(theme.font_sizes.sm)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                    )
                    .child(ThemeQuickChips),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(theme.spacing.sm)
                    .children(active_section.states.iter().map(|state| {
                        let is_active = state.id == active_state.id;
                        let state_id = state.id;
                        KitChip::new(state.label)
                            .key(state.id)
                            .selected(is_active)
                            .on_press(move |_| active_state_id.set(state_id.to_string()))
                    })),
            );

        let log_entries = command_log.read().clone();
        let command_log_bar = CommandLogBar {
            entries: log_entries,
            expanded: log_expanded,
        };

        let body = render_section(active_section.id, active_state.id, &gallery_core);

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .background(theme.color(ThemeRole::Surface))
            .color(theme.color(ThemeRole::Text))
            .on_global_key_down(on_global_key_down)
            .child(sidebar)
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .height(Size::fill())
                    .content(Content::Flex)
                    .child(header)
                    .child(rect().width(Size::fill()).height(Size::flex(1.0)).child(body))
                    .child(command_log_bar),
            )
            .child(MenuLayer)
            .child(AddressBarLayer)
            .child(VimPickerOverlay)
            .child(MediaPreviewOverlay)
            .child(OnboardingOverlay)
            .child(GradientGeneratorOverlay)
            .child(ShareConfirmOverlay)
    }
}

#[derive(Clone, PartialEq)]
struct ThemeQuickChips;

impl Component for ThemeQuickChips {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let is_dark = theme.scheme == ResolvedScheme::Dark;

        let light_core = core.clone();
        let dark_core = core.clone();

        rect()
            .direction(Direction::Horizontal)
            .spacing(theme.spacing.xs)
            .child(KitChip::new("Light").icon("sun").selected(!is_dark).on_press(move |_| {
                light_core.invoke_or_log(Command::ThemeSet {
                    scheme: Some(ThemeScheme::Light),
                    overrides: None,
                });
            }))
            .child(KitChip::new("Dark").icon("moon").selected(is_dark).on_press(move |_| {
                dark_core.invoke_or_log(Command::ThemeSet {
                    scheme: Some(ThemeScheme::Dark),
                    overrides: None,
                });
            }))
    }
}

#[derive(Clone, PartialEq)]
struct CommandLogBar {
    entries: Vec<CommandLogEntry>,
    expanded: State<bool>,
}

impl Component for CommandLogBar {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut expanded = self.expanded;
        let is_expanded = expanded();

        let latest_summary = match self.entries.last() {
            Some(entry) => {
                let status = match &entry.outcome {
                    CommandLogOutcome::Handled => "handled",
                    CommandLogOutcome::Unhandled => "unhandled (logged by stub)",
                    CommandLogOutcome::Error(err) => err.as_str(),
                };
                if entry.detail.is_empty() {
                    format!("#{} {} — {}", entry.sequence, entry.command, status)
                } else {
                    format!("#{} {} ({}) — {}", entry.sequence, entry.command, entry.detail, status)
                }
            }
            None => "No commands invoked yet — interact with any control to see BrowserCore dispatch".to_string(),
        };

        let latest_tone = match self.entries.last().map(|e| &e.outcome) {
            Some(CommandLogOutcome::Handled) => UiTone::Success,
            Some(CommandLogOutcome::Unhandled) => UiTone::Warning,
            Some(CommandLogOutcome::Error(_)) => UiTone::Danger,
            None => UiTone::Neutral,
        };

        rect()
            .width(Size::fill())
            .background(theme.color(ThemeRole::SurfaceRaised))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .padding(Gaps::new(8.0, 16.0, 8.0, 16.0))
            .spacing(theme.spacing.sm)
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .main_align(Alignment::SpaceBetween)
                    .spacing(theme.spacing.md)
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(theme.spacing.md)
                            .child(Pill::new(format!("Stub Log ({})", self.entries.len())).tone(latest_tone))
                            .child(
                                label()
                                    .text(latest_summary)
                                    .font_size(theme.font_sizes.xs)
                                    .font_family("monospace")
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis)
                                    .color(theme.color(ThemeRole::Text)),
                            ),
                    )
                    .child(
                        KitChip::new(if is_expanded { "Collapse Log" } else { "Show History" })
                            .selected(is_expanded)
                            .on_press(move |_| {
                                let next = !*expanded.peek();
                                expanded.set(next);
                            }),
                    ),
            )
            .maybe_child(is_expanded.then(|| {
                let rows = self.entries.iter().rev().take(8).map(|entry| {
                    let (tag, role) = match &entry.outcome {
                        CommandLogOutcome::Handled => ("OK", ThemeRole::Success),
                        CommandLogOutcome::Unhandled => ("UNHANDLED", ThemeRole::Warning),
                        CommandLogOutcome::Error(_) => ("ERROR", ThemeRole::Danger),
                    };
                    let text = if entry.detail.is_empty() {
                        format!("#{} {}", entry.sequence, entry.command)
                    } else {
                        format!("#{} {} ({})", entry.sequence, entry.command, entry.detail)
                    };
                    rect()
                        .key(entry.sequence)
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(theme.spacing.md)
                        .child(
                            label()
                                .text(tag)
                                .font_size(theme.font_sizes.xs)
                                .font_weight(FontWeight::BOLD)
                                .font_family("monospace")
                                .color(theme.color(role)),
                        )
                        .child(
                            label()
                                .text(text)
                                .font_size(theme.font_sizes.xs)
                                .font_family("monospace")
                                .color(theme.color(ThemeRole::TextMuted)),
                        )
                });
                rect().width(Size::fill()).spacing(theme.spacing.xs).children(rows)
            }))
    }
}
