/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 2 Sidebar gallery section covering every sidebar state in light and dark themes:
//! populated sidebar, collapsed folder with active child, inline tab renamer, switched space ("Work"),
//! sidebar notification card, empty essentials promo, hovered/loading/audible tab states, drag reorder
//! preview, and collapsed icon-only sidebar.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::sidebar::{SidebarDragState, SidebarViewState, try_use_sidebar_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, KitChip, Pill};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{
    SIDEBAR_DEFAULT_WIDTH, SidebarSide, SpaceId, TabId, TabKind, TabMoveTarget, ToolbarLayout,
};
use zendia_browser_core::stub::{sample_model, sample_update_notification};
use zendia_module_api::ui::UiTone;

#[derive(Clone)]
pub struct SidebarSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
}

impl PartialEq for SidebarSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

fn configure_sidebar_fixture(state_id: &str, gallery_core: &GalleryCore, sidebar_state: Option<SidebarViewState>) {
    let stub = gallery_core.stub();
    if let Some(state) = sidebar_state {
        state.reset();
    }

    let mut model = sample_model(true);
    model.layout.toolbar_layout = ToolbarLayout::Single;
    model.layout.sidebar_side = SidebarSide::Left;
    model.layout.sidebar_expanded = true;
    model.layout.sidebar_width = SIDEBAR_DEFAULT_WIDTH;

    match state_id {
        "collapsed-folder-active" => {
            // Engine folder (FolderId(2)) is collapsed and its child TabId(8) is active.
            model.active_tab = Some(TabId(8));
            if let Some(space) = model.spaces.iter_mut().find(|s| s.id == SpaceId(1)) {
                space.last_active_tab = Some(TabId(8));
            }
            if let Some(window) = model.windows.first_mut() {
                window.active_tab = Some(TabId(8));
            }
            stub.replace_model(model);
        }
        "tab-renamer" => {
            // Activate Releases — Zendia (TabId(9)) and open its inline rename field.
            model.active_tab = Some(TabId(9));
            if let Some(space) = model.spaces.iter_mut().find(|s| s.id == SpaceId(1)) {
                space.last_active_tab = Some(TabId(9));
            }
            stub.replace_model(model);
            if let Some(state) = sidebar_state {
                state.start_tab_rename(TabId(9), "Releases — Zendia".to_string());
            }
        }
        "space-work" => {
            // Switch to the "Work" space (teal-blue gradient, briefcase header, shared essentials).
            model.active_space = Some(SpaceId(2));
            model.active_tab = Some(TabId(13));
            if let Some(window) = model.windows.first_mut() {
                window.active_space = Some(SpaceId(2));
                window.active_tab = Some(TabId(13));
            }
            stub.replace_model(model);
        }
        "notification-visible" => {
            // Clean Space with normal tabs and the "Update Complete!" card at the foot of the sidebar.
            model.essentials.clear();
            model.pinned_tabs.clear();
            model.folders.clear();
            model
                .tabs
                .retain(|tab| tab.kind == TabKind::Normal && tab.folder_id.is_none());
            model.active_tab = Some(TabId(12));
            if let Some(space) = model.spaces.iter_mut().find(|s| s.id == SpaceId(1)) {
                space.last_active_tab = Some(TabId(12));
                // Warm blush-plum tint matching the notification reference shot.
                space.gradient.stops = vec![(176, 86, 152), (146, 76, 134)];
            }
            model.sidebar_notifications.push(sample_update_notification());
            stub.replace_model(model);
        }
        "empty-essentials-promo" => {
            // No essentials yet: show Zen's "Add to Essentials" promo card above the space header.
            model.essentials.clear();
            model.tabs.retain(|tab| tab.kind != TabKind::Essential);
            model.show_essentials_promo = true;
            stub.replace_model(model);
        }
        "tab-states" => {
            // Demonstrate hovered (with close), loading (spinner), audible (speaker), and muted tabs.
            if let Some(tab) = model.tabs.iter_mut().find(|t| t.id == TabId(9)) {
                tab.is_audible = true;
                tab.is_muted = false;
                tab.title = "Focus Radio — Playing".to_string();
            }
            if let Some(tab) = model.tabs.iter_mut().find(|t| t.id == TabId(10)) {
                tab.is_audible = true;
                tab.is_muted = true;
                tab.title = "Standup Call — Muted".to_string();
            }
            if let Some(tab) = model.tabs.iter_mut().find(|t| t.id == TabId(11)) {
                tab.is_loading = true;
                tab.title = "Loading RFC Draft".to_string();
            }
            if let Some(tab) = model.tabs.iter_mut().find(|t| t.id == TabId(12)) {
                tab.favicon = None;
                tab.title = "Hovered Tab (Placeholder Icon)".to_string();
            }
            model.active_tab = Some(TabId(9));
            stub.replace_model(model);
            if let Some(state) = sidebar_state {
                let mut hovered = state.preview_hovered_tab;
                hovered.set(Some(TabId(12)));
                let mut sep = state.preview_hovered_separator;
                sep.set(true);
            }
        }
        "drag-reorder" => {
            // Show active drag preview of TabId(12) hovering above TabId(9) with insertion indicator line.
            model.active_tab = Some(TabId(12));
            stub.replace_model(model);
            if let Some(state) = sidebar_state {
                let mut drag = state.drag_state;
                drag.set(Some(SidebarDragState {
                    tab_id: TabId(12),
                    tab_title: "Zendia Documentation".to_string(),
                    tab_favicon: Some("book".to_string()),
                    pointer_x: 10.0,
                    pointer_y: 448.0,
                    target: Some(TabMoveTarget::Normal {
                        space_id: Some(SpaceId(1)),
                        index: 0,
                    }),
                    page_drop: None,
                    is_preview: true,
                }));
            }
        }
        "collapsed-sidebar" => {
            model.layout.toolbar_layout = ToolbarLayout::Collapsed;
            model.layout.sidebar_expanded = false;
            stub.replace_model(model);
        }
        _ => {
            // "full-populated": default populated sample model.
            stub.replace_model(model);
        }
    }
}

fn page_placeholder(title: &str, url: &str, space_name: &str) -> Element {
    let theme = use_theme();
    let page_bg = if theme.scheme.is_dark() {
        Color::from_rgb(20, 22, 30)
    } else {
        Color::from_rgb(245, 246, 250)
    };
    let card_bg = if theme.scheme.is_dark() {
        Color::from_rgb(29, 32, 44)
    } else {
        Color::from_rgb(255, 255, 255)
    };

    rect()
        .width(Size::fill())
        .height(Size::fill())
        .background(page_bg)
        .main_align(Alignment::center())
        .cross_align(Alignment::center())
        .child(
            rect()
                .width(Size::px(420.0))
                .padding(Gaps::new_all(24.0))
                .spacing(10.0)
                .cross_align(Alignment::center())
                .corner_radius(CornerRadius::new_all(theme.radii.card))
                .background(card_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::BorderSubtle)),
                )
                .child(Icon::new("page-portrait").size(22.0).role(ThemeRole::TextMuted))
                .child(
                    label()
                        .text(title.to_string())
                        .font_size(theme.font_sizes.xl)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(theme.color(ThemeRole::Text)),
                )
                .child(
                    label()
                        .text(url.to_string())
                        .font_size(theme.font_sizes.xs)
                        .font_family("monospace")
                        .color(theme.color(ThemeRole::TextMuted)),
                )
                .child(Pill::new(format!("Space: {space_name}")).tone(UiTone::Accent)),
        )
        .into_element()
}

impl Component for SidebarSection {
    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.state_id)
    }

    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let sidebar_state = try_use_sidebar_state();

        let mut applied_state = use_state(String::new);
        if *applied_state.peek() != self.state_id {
            applied_state.set(self.state_id.clone());
            configure_sidebar_fixture(&self.state_id, &self.gallery_core, sidebar_state);
        }

        let (active_title, active_url, space_name, essentials_count, folders_count, normal_count) = {
            let model = core.model().read();
            let active = model.active_tab();
            let space_id = model.active_space;
            let space_name = model
                .active_space()
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "Space".to_string());
            let folders_count = space_id.map(|id| model.folders_in_space(id).len()).unwrap_or(0);
            let normal_count = space_id.map(|id| model.normal_tabs_in_space(id).len()).unwrap_or(0);
            (
                active
                    .map(|tab| tab.display_title())
                    .unwrap_or_else(|| "No Active Tab".to_string()),
                active
                    .map(|tab| tab.url.clone())
                    .unwrap_or_else(|| "about:blank".to_string()),
                space_name,
                model.essential_tabs().len(),
                folders_count,
                normal_count,
            )
        };

        let mut location = use_state(|| active_url.clone());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut shown_url = use_state(|| active_url.clone());
        if *shown_url.peek() != active_url && !location_a11y_id.is_focused() {
            shown_url.set(active_url.clone());
            location.set(active_url.clone());
        }

        let next_space_core = core.clone();
        let new_folder_core = core.clone();
        let clear_core = core.clone();
        let reset_core = Arc::clone(&self.gallery_core);
        let reset_state_id = self.state_id.clone();

        let info_strip = rect()
            .width(Size::fill())
            .padding(Gaps::new(6.0, 12.0, 6.0, 12.0))
            .direction(Direction::Horizontal)
            .content(Content::wrap())
            .cross_align(Alignment::center())
            .main_align(Alignment::SpaceBetween)
            .spacing(theme.spacing.sm)
            .background(theme.color(ThemeRole::SurfaceRaised))
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
                    .spacing(theme.spacing.sm)
                    .child(Pill::new(format!("Space: {space_name}")).tone(UiTone::Accent))
                    .child(Pill::new(format!(
                        "{essentials_count} essentials · {folders_count} folders · {normal_count} tabs"
                    ))),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(theme.spacing.xs)
                    .child(KitChip::new("Switch Space").on_press(move |_| {
                        next_space_core.invoke_or_log(Command::SpaceNext);
                    }))
                    .child(KitChip::new("New Folder").on_press(move |_| {
                        new_folder_core.invoke_or_log(Command::FolderCreate {
                            name: Some("New Folder".to_string()),
                            space_id: None,
                            tab_id: None,
                        });
                    }))
                    .child(KitChip::new("Clear Tabs").on_press(move |_| {
                        clear_core.invoke_or_log(Command::SpaceClearTabs { space_id: None });
                    }))
                    .child(KitChip::new("Reset State").on_press(move |_| {
                        configure_sidebar_fixture(&reset_state_id, &reset_core, sidebar_state);
                    })),
            );

        let page = page_placeholder(&active_title, &active_url, &space_name);

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .padding(Gaps::new_all(12.0))
            .spacing(10.0)
            .child(info_strip)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .corner_radius(CornerRadius::new_all(10.0))
                    .overflow(Overflow::Clip)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.color(ThemeRole::Border)),
                    )
                    .child(WindowShell::new(location, location_a11y_id, page).interactive_window_controls(false)),
            )
    }
}
