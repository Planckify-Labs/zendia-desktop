/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 5 (run A) gallery section for the vim mode indicator, animated mascot, statusline announcements,
//! per-mode window frame tint, and which-key popups.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::sidebar::{SidebarViewState, try_use_sidebar_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::{KitChip, Pill};
use crate::ui::page::{PageArea, PageStage, PaneMode};
use crate::ui::theme::{ThemeRole, use_theme};
use crate::ui::vim::{VimOverlayLayer, VimStatuslineState, try_use_vim_statusline_state};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::commands::Command;
use zendia_browser_core::input_state::{
    StatuslineAnnouncement, StatuslineShow, mode_name, which_key_all_keymaps, which_key_for_prefix,
};
use zendia_browser_core::model::{InputMode, SpaceId, TabId};
use zendia_module_api::ui::UiTone;

const DOCS: TabId = TabId(12);

#[derive(Clone)]
pub struct VimStatuslineSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
}

impl PartialEq for VimStatuslineSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

fn configure_fixture(
    state_id: &str,
    gallery_core: &GalleryCore,
    sidebar_state: Option<SidebarViewState>,
    vim_state: Option<VimStatuslineState>,
) {
    if let Some(sidebar) = sidebar_state {
        sidebar.reset();
    }
    if let Some(state) = vim_state {
        state.close_all_keymaps();
        state.set_which_key_width(0.0);
        state.set_side_panel_clearance(0.0);
        let mut scrolling = state.scrolling;
        scrolling.set_if_modified(false);
    }

    let mut model = super::gallery_sample_model();
    model.active_tab = Some(DOCS);
    if let Some(window) = model.windows.first_mut() {
        window.active_tab = Some(DOCS);
    }
    if let Some(space) = model.spaces.iter_mut().find(|space| space.id == SpaceId(1)) {
        space.last_active_tab = Some(DOCS);
    }

    // Vim surfaces only show when vim mode is turned on.
    model.vim_mode_enabled = true;
    model.statusline.enabled = true;
    model.statusline.show = StatuslineShow::Always;
    model.input_mode = InputMode::Normal;
    model.pending_count = None;
    model.pending_keys.clear();
    model.which_key = None;
    model.statusline_announcement = None;

    match state_id {
        "normal-pending" => {
            model.input_mode = InputMode::Normal;
            model.pending_count = Some(3);
            model.pending_keys = vec!["g".to_string()];
            model.which_key =
                which_key_for_prefix(InputMode::Normal, &model.pending_keys).map(|view| view.to_which_key_state());
        }
        "insert" => {
            model.input_mode = InputMode::Insert;
        }
        "visual" => {
            model.input_mode = InputMode::Visual;
        }
        "explorer" => {
            model.input_mode = InputMode::Explorer;
        }
        "toolbar" => {
            model.input_mode = InputMode::Toolbar;
        }
        "window" => {
            model.input_mode = InputMode::Window;
        }
        "library" => {
            model.input_mode = InputMode::Library;
        }
        "shrunk" => {
            model.input_mode = InputMode::Normal;
            model.statusline.show = StatuslineShow::Shrink;
        }
        "which-key-leader" => {
            model.input_mode = InputMode::Normal;
            model.pending_keys = vec!["<Space>".to_string()];
            model.which_key =
                which_key_for_prefix(InputMode::Normal, &model.pending_keys).map(|view| view.to_which_key_state());
        }
        "which-key-subgroup" => {
            model.input_mode = InputMode::Normal;
            model.pending_keys = vec!["<Space>".to_string(), "a".to_string()];
            model.which_key =
                which_key_for_prefix(InputMode::Normal, &model.pending_keys).map(|view| view.to_which_key_state());
        }
        "which-key-all" => {
            model.input_mode = InputMode::Normal;
            model.which_key = Some(which_key_all_keymaps(InputMode::Normal).to_which_key_state());
        }
        "announcement" => {
            model.input_mode = InputMode::Normal;
            model.statusline_announcement = Some(StatuslineAnnouncement {
                id: 1,
                message: "Yanked current URL to clipboard".to_string(),
                timeout_ms: 600_000,
            });
        }
        "ducked" => {
            model.input_mode = InputMode::Normal;
            model.statusline.show = StatuslineShow::Always;
        }
        _ => {}
    }

    gallery_core.stub().replace_model(model);
}

impl Component for VimStatuslineSection {
    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.state_id)
    }

    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let sidebar_state = try_use_sidebar_state();
        let vim_state = try_use_vim_statusline_state();

        let mut applied_state = use_state(String::new);
        if *applied_state.peek() != self.state_id {
            applied_state.set(self.state_id.clone());
            configure_fixture(&self.state_id, &self.gallery_core, sidebar_state, vim_state);
        }

        let (active_url, mode, show_setting, vim_enabled) = {
            let model = core.model().read();
            (
                model.active_tab().map(|tab| tab.url.clone()).unwrap_or_default(),
                model.input_mode,
                model.statusline.show,
                model.vim_mode_enabled,
            )
        };

        let mut location = use_state(|| active_url.clone());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut shown_url = use_state(|| active_url.clone());
        if *shown_url.peek() != active_url && !location_a11y_id.is_focused() {
            shown_url.set(active_url.clone());
            location.set(active_url.clone());
        }

        let is_shrunk_state = self.state_id == "shrunk";
        let is_ducked_state = self.state_id == "ducked";
        let summary = format!(
            "Vim {} · {} · show={}",
            if vim_enabled { "ON" } else { "OFF" },
            mode_name(mode),
            show_setting.as_str()
        );

        let cycle_core = core.clone();
        let all_keys_core = core.clone();
        let announce_core = core.clone();
        let toggle_vim_core = core.clone();
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
            .child(Pill::new(summary).tone(UiTone::Accent))
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(theme.spacing.xs)
                    .child(KitChip::new("Cycle (Ctrl+Alt+M)").icon("bolt").on_press(move |_| {
                        cycle_core.invoke_or_log(Command::DebugCycleInputState);
                    }))
                    .child(KitChip::new("All Keymaps").on_press(move |_| {
                        all_keys_core.invoke_or_log(Command::WhichKeyShowAll);
                    }))
                    .child(KitChip::new("Announce").on_press(move |_| {
                        announce_core.invoke_or_log(Command::StatuslineAnnounce {
                            message: "Switched to NORMAL mode".to_string(),
                            timeout_ms: Some(3000),
                        });
                    }))
                    .child(
                        KitChip::new(if vim_enabled { "Vim: ON" } else { "Vim: OFF" })
                            .selected(vim_enabled)
                            .on_press(move |_| {
                                toggle_vim_core.invoke_or_log(Command::VimSetEnabled { enabled: !vim_enabled });
                            }),
                    )
                    .child(KitChip::new("Reset State").on_press(move |_| {
                        configure_fixture(&reset_state_id, &reset_core, sidebar_state, vim_state);
                    })),
            );

        let page_stage = PageStage::new().page(PageArea::new(PaneMode::Placeholder)).overlay(
            VimOverlayLayer::new()
                .force_shrunk(is_shrunk_state)
                .force_ducked(is_ducked_state)
                .static_preview(true),
        );

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
                    .child(WindowShell::new(location, location_a11y_id, page_stage).interactive_window_controls(false)),
            )
    }
}
