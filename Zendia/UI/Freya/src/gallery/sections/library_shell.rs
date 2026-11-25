/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 9 gallery section: Library shell, search and filter bar, History section, keyboard layer, right-side rail,
//! and mid-open slide motion.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::menus::try_use_menu_state;
use crate::ui::core_view::use_core;
use crate::ui::kit::{KitChip, Pill};
use crate::ui::library::{LibraryViewState, try_use_library_state};
use crate::ui::page::{PageArea, PageStage, PaneMode, VimOverlayLayer};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::BrowserCore;
use zendia_browser_core::commands::Command;
use zendia_browser_core::history::{SAMPLE_NOW_UNIX, sample_library_history};
use zendia_browser_core::library::LibrarySection;
use zendia_browser_core::model::{BrowserModel, InputMode, SidebarSide, ToolbarLayout};
use zendia_module_api::ui::UiTone;

/// Gallery section showcasing the Library shell, search/filter bar, History section, and keyboard layer.
#[derive(Clone)]
pub struct LibraryShellSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
    key: DiffKey,
}

impl PartialEq for LibraryShellSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

impl KeyExt for LibraryShellSection {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl LibraryShellSection {
    pub fn new(state_id: impl Into<String>, gallery_core: Arc<GalleryCore>) -> Self {
        Self {
            state_id: state_id.into(),
            gallery_core,
            key: DiffKey::None,
        }
    }
}

fn build_fixture_model(state_id: &str) -> BrowserModel {
    let mut model = super::gallery_sample_model();
    model.layout.toolbar_layout = ToolbarLayout::Single;
    model.layout.sidebar_expanded = true;
    model.layout.compact_mode = false;
    model.layout.sidebar_side = if state_id == "right-side" {
        SidebarSide::Right
    } else {
        SidebarSide::Left
    };

    model.history = if state_id == "history-empty" {
        Vec::new()
    } else {
        sample_library_history(SAMPLE_NOW_UNIX)
    };

    if state_id == "vim-focus" {
        model.vim_mode_enabled = true;
        model.input_mode = InputMode::Library;
    } else {
        model.vim_mode_enabled = false;
        model.input_mode = InputMode::Normal;
    }

    model
}

fn apply_library_view_fixture(state_id: &str, lib: LibraryViewState) {
    lib.clear_gallery_overrides();
    lib.open(Some(LibrarySection::History));
    let mut customizing = lib.customizing;
    customizing.set_if_modified(false);

    match state_id {
        "history-search" => {
            let mut q = lib.history_query_override;
            q.set(Some("Design".to_string()));
            let mut f = lib.history_filters_open_override;
            f.set(Some(false));
        }
        "history-filters" => {
            let mut q = lib.history_query_override;
            q.set(Some(String::new()));
            let mut f = lib.history_filters_open_override;
            f.set(Some(true));
        }
        "history-empty" => {
            let mut q = lib.history_query_override;
            q.set(Some(String::new()));
            let mut f = lib.history_filters_open_override;
            f.set(Some(false));
            let mut cursor = lib.nav.cursor;
            cursor.set(None);
        }
        "vim-focus" => {
            let mut q = lib.history_query_override;
            q.set(Some(String::new()));
            let mut f = lib.history_filters_open_override;
            f.set(Some(false));
            let mut ring = lib.force_search_focus_ring;
            ring.set(true);
            let mut cursor = lib.nav.cursor;
            cursor.set(Some("history:101".to_string()));
        }
        "mid-open" => {
            let mut q = lib.history_query_override;
            q.set(Some(String::new()));
            let mut f = lib.history_filters_open_override;
            f.set(Some(false));
            let mut pinned = lib.pinned_progress;
            pinned.set(Some(0.55));
        }
        _ => {
            let mut q = lib.history_query_override;
            q.set(Some(String::new()));
            let mut f = lib.history_filters_open_override;
            f.set(Some(false));
            let mut cursor = lib.nav.cursor;
            cursor.set(Some("history:101".to_string()));
        }
    }
}

impl Component for LibraryShellSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let library_state = try_use_library_state();
        let menu_state = try_use_menu_state();
        let location = use_state(|| "http://127.0.0.1:44889/architecture".to_string());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut applied_state = use_state(String::new);

        if *applied_state.peek() != self.state_id {
            applied_state.set(self.state_id.clone());
            let mut fixture = build_fixture_model(&self.state_id);
            fixture.theme = self.gallery_core.model().theme;
            self.gallery_core.stub().replace_model(fixture);
            if let Some(lib) = library_state {
                apply_library_view_fixture(&self.state_id, lib);
            }
            if let Some(ms) = menu_state {
                ms.close_all();
            }
        }

        let is_open = library_state.is_some_and(|lib| lib.is_open());
        let active_section = library_state
            .map(|lib| *lib.section.read())
            .unwrap_or(LibrarySection::History);
        let status_label = match self.state_id.as_str() {
            "history-search" => "History · filtered by \"Design\"",
            "history-filters" => "History · filter panel open",
            "history-empty" => "History · empty state",
            "vim-focus" => "LIBRARY mode · teal cursor ring & mascot",
            "right-side" => "Right-side rail · mirrored layout",
            "mid-open" => "Open slide · 55% progress",
            _ => "History · Today (5 visits)",
        };

        let toggle_core = core.clone();
        let reset_core = Arc::clone(&self.gallery_core);
        let reset_state_id = self.state_id.clone();

        let controls = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::wrap())
            .cross_align(Alignment::center())
            .spacing(8.0)
            .child(Pill::new(status_label).tone(UiTone::Accent))
            .child(
                KitChip::new(if is_open { "Close Library" } else { "Open Library" })
                    .icon("library")
                    .selected(is_open)
                    .on_press(move |_| {
                        if let Some(lib) = library_state {
                            let mut pinned = lib.pinned_progress;
                            pinned.set_if_modified(None);
                        }
                        toggle_core.invoke_or_log(Command::LibraryToggle {
                            section: Some(active_section),
                        });
                    }),
            )
            .child(KitChip::new("Reset State").on_press(move |_| {
                let mut fixture = build_fixture_model(&reset_state_id);
                fixture.theme = reset_core.model().theme;
                reset_core.stub().replace_model(fixture);
                if let Some(lib) = library_state {
                    apply_library_view_fixture(&reset_state_id, lib);
                }
            }));

        let page_stage = PageStage::new()
            .page(PageArea::new(PaneMode::Placeholder))
            .overlay(VimOverlayLayer::new().static_preview(true));

        let shell = rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .corner_radius(CornerRadius::new_all(theme.radii.card))
            .overflow(Overflow::Clip)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .child(WindowShell::new(location, location_a11y_id, page_stage).interactive_window_controls(false));

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .padding(Gaps::new_all(12.0))
            .spacing(10.0)
            .child(controls)
            .child(shell)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
