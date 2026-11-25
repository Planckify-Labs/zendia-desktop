/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 5 gallery section for Vim pickers and the page region picker: Tabs picker (list with sections & cursor),
//! filtered list, Agents picker (3-column grid with dimmed uninstalled items), empty filter result, region picker
//! idle, region picker while dragging, and vim-mode gating.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::sidebar::try_use_sidebar_state;
use crate::ui::core_view::use_core;
use crate::ui::kit::{KitChip, KitSwitch, Pill};
use crate::ui::page::{PageArea, PaneMode};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ThemeRole, use_theme};
use crate::ui::vim::{RegionPickerSurface, VimPickerCard, try_use_region_picker_state, try_use_vim_picker_state};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{
    BrowserModel, Essential, PinnedTab, SIDEBAR_DEFAULT_WIDTH, SidebarSide, SpaceId, Tab, TabId, TabKind, ToolbarLayout,
};
use zendia_browser_core::picker::{PickerItem, PickerLayout, PickerRequest, build_agents_picker, build_tabs_picker};
use zendia_browser_core::region::RegionRect;
use zendia_module_api::ui::UiTone;

#[derive(Clone)]
pub struct VimPickersSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
}

impl PartialEq for VimPickersSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

fn tabs_fixture_model(vim_enabled: bool) -> BrowserModel {
    let mut model = super::gallery_sample_model();
    let space = SpaceId(1);
    model.vim_mode_enabled = vim_enabled;
    model.layout.toolbar_layout = ToolbarLayout::Single;
    model.layout.sidebar_side = SidebarSide::Left;
    model.layout.sidebar_expanded = true;
    model.layout.sidebar_width = SIDEBAR_DEFAULT_WIDTH;
    model.folders.clear();
    model.split_groups.clear();
    model.glance = None;

    let make_tab = |id: u64, kind: TabKind, space_id: Option<SpaceId>, title: &str, url: &str, icon: &str| Tab {
        id: TabId(id),
        view_id: None,
        space_id,
        folder_id: None,
        split_group_id: None,
        kind,
        title: title.to_string(),
        custom_title: None,
        url: url.to_string(),
        favicon: Some(icon.to_string()),
        is_loading: false,
        can_navigate_back: true,
        can_navigate_forward: false,
        is_audible: false,
        is_muted: false,
        closing: false,
    };

    model.tabs = vec![
        make_tab(
            1,
            TabKind::Normal,
            Some(space),
            "Zendia Documentation",
            "http://127.0.0.1/docs",
            "book",
        ),
        make_tab(
            2,
            TabKind::Normal,
            Some(space),
            "Architecture — Zendia",
            "http://127.0.0.1/architecture",
            "code",
        ),
        make_tab(
            3,
            TabKind::Normal,
            Some(space),
            "Design Tokens — Zendia",
            "http://127.0.0.1/design",
            "palette",
        ),
        make_tab(
            4,
            TabKind::Pinned,
            Some(space),
            "Releases — Zendia",
            "http://127.0.0.1/releases",
            "flame",
        ),
        make_tab(
            5,
            TabKind::Essential,
            None,
            "Inbox — Workspace",
            "http://127.0.0.1/mail",
            "mail",
        ),
    ];
    model.pinned_tabs = vec![PinnedTab {
        tab_id: TabId(4),
        space_id: space,
        order: 0,
    }];
    model.essentials = vec![Essential {
        tab_id: TabId(5),
        order: 0,
    }];
    model.active_space = Some(space);
    model.active_tab = Some(TabId(1));
    if let Some(window) = model.windows.first_mut() {
        window.active_space = Some(space);
        window.active_tab = Some(TabId(1));
    }
    if let Some(sp) = model.spaces.iter_mut().find(|s| s.id == space) {
        sp.last_active_tab = Some(TabId(1));
    }
    model
}

fn configure_fixture(state_id: &str, gallery_core: &GalleryCore) {
    let vim_enabled = state_id != "vim-disabled";
    let model = tabs_fixture_model(vim_enabled);
    gallery_core.stub().replace_model(model);
}

fn preview_picker_request(state_id: &str, model: &BrowserModel) -> Option<PickerRequest> {
    match state_id {
        "tabs-list" | "tabs-filtered" | "empty-filter" => Some(build_tabs_picker(model)),
        "agents-grid" => Some(build_agents_picker(model)),
        _ => None,
    }
}

fn preview_initial_state(state_id: &str) -> (String, usize, bool, Option<RegionRect>) {
    match state_id {
        "tabs-list" => (String::new(), 1, false, None),
        "tabs-filtered" => ("architecture".to_string(), 0, true, None),
        "agents-grid" => (String::new(), 0, false, None),
        "empty-filter" => ("zzzz".to_string(), 0, true, None),
        "region-idle" => (String::new(), 0, false, None),
        "region-dragging" => (
            String::new(),
            0,
            false,
            Some(RegionRect::new(112.0, 96.0, 396.0, 268.0)),
        ),
        _ => (String::new(), 0, false, None),
    }
}

impl Component for VimPickersSection {
    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.state_id)
    }

    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let sidebar_state = try_use_sidebar_state();
        let live_picker = try_use_vim_picker_state();
        let live_region = try_use_region_picker_state();

        let (init_query, init_cursor, init_filter, init_region) = preview_initial_state(&self.state_id);
        let mut query = use_state(|| init_query.clone());
        let mut cursor = use_state(|| init_cursor);
        let mut filter_active = use_state(|| init_filter);
        let mut drag_start = use_state(|| None::<(f32, f32)>);
        let mut selection = use_state(|| init_region);
        let mut dragging = use_state(|| false);
        let mut applied_state = use_state(String::new);

        if *applied_state.peek() != self.state_id {
            applied_state.set(self.state_id.clone());
            if let Some(sb) = sidebar_state {
                sb.reset();
            }
            if let Some(lp) = live_picker {
                lp.close();
            }
            if let Some(lr) = live_region {
                lr.stop();
            }
            configure_fixture(&self.state_id, &self.gallery_core);
            query.set(init_query);
            cursor.set(init_cursor);
            filter_active.set(init_filter);
            drag_start.set(None);
            selection.set(init_region);
            dragging.set(false);
        }

        let (model_snapshot, vim_enabled, active_url) = {
            let model = core.model().read();
            (
                model.clone(),
                model.vim_mode_enabled,
                model.active_tab().map(|tab| tab.url.clone()).unwrap_or_default(),
            )
        };

        let mut location = use_state(|| active_url.clone());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut shown_url = use_state(|| active_url.clone());
        if *shown_url.peek() != active_url && !location_a11y_id.is_focused() {
            shown_url.set(active_url.clone());
            location.set(active_url);
        }

        let is_region_preview = matches!(self.state_id.as_str(), "region-idle" | "region-dragging");
        let picker_request = vim_enabled
            .then(|| preview_picker_request(&self.state_id, &model_snapshot))
            .flatten();

        let summary_text = if !vim_enabled {
            "Vim mode OFF · surfaces gated".to_string()
        } else if is_region_preview {
            if let Some(r) = *selection.read() {
                format!("Region {:.0}×{:.0} at ({:.0}, {:.0})", r.width, r.height, r.x, r.y)
            } else {
                "Region picker · idle".to_string()
            }
        } else if let Some(req) = &picker_request {
            format!(
                "{} ({}) · {} items",
                req.title,
                req.layout.as_str(),
                req.filtered_items(&query.read()).len()
            )
        } else {
            "Vim mode ON".to_string()
        };

        let toggle_vim_core = core.clone();
        let open_tabs_core = core.clone();
        let open_agents_core = core.clone();
        let open_region_core = core.clone();
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
                    .spacing(theme.spacing.md)
                    .child(Pill::new(summary_text).tone(if vim_enabled { UiTone::Accent } else { UiTone::Warning }))
                    .child(
                        KitSwitch::new(vim_enabled)
                            .label(if vim_enabled { "Vim mode on" } else { "Vim mode off" })
                            .on_toggle(move |next| {
                                toggle_vim_core.invoke_or_log(Command::VimSetEnabled { enabled: next });
                            }),
                    ),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(theme.spacing.xs)
                    .child(KitChip::new("Open Tabs Picker").on_press(move |_| {
                        open_tabs_core.invoke_or_log(Command::PickerOpen {
                            name: Some("tabs".to_string()),
                            request_id: None,
                        });
                    }))
                    .child(KitChip::new("Open Agents Grid").on_press(move |_| {
                        open_agents_core.invoke_or_log(Command::PickerOpen {
                            name: Some("agents".to_string()),
                            request_id: None,
                        });
                    }))
                    .child(KitChip::new("Start Region Pick").on_press(move |_| {
                        open_region_core.invoke_or_log(Command::RegionPick { tab_id: None });
                    }))
                    .child(KitChip::new("Reset State").on_press(move |_| {
                        let (q, c, f, r) = preview_initial_state(&reset_state_id);
                        configure_fixture(&reset_state_id, &reset_core);
                        query.set(q);
                        cursor.set(c);
                        filter_active.set(f);
                        drag_start.set(None);
                        selection.set(r);
                        dragging.set(false);
                    })),
            );

        let region_capture_core = core.clone();
        let region_cancel_core = core.clone();
        let region_preview_layer = (vim_enabled && is_region_preview).then(move || {
            RegionPickerSurface::new(drag_start, selection, dragging)
                .capture_keys(false)
                .on_capture(move |(rect, whole_page): (RegionRect, bool)| {
                    region_capture_core.invoke_or_log(Command::RegionCapture {
                        tab_id: None,
                        rect: Some(rect),
                        whole_page,
                    });
                })
                .on_cancel(move |_| {
                    region_cancel_core.invoke_or_log(Command::RegionCancel);
                })
                .into_element()
        });

        let page_content = rect()
            .width(Size::fill())
            .height(Size::fill())
            .child(PageArea::new(PaneMode::Placeholder))
            .maybe_child(region_preview_layer);

        let choose_core = core.clone();
        let picker_overlay = picker_request.map(|request| {
            let width = match request.layout {
                PickerLayout::List => 560.0,
                PickerLayout::Grid => 500.0,
            };
            rect()
                .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                .position(Position::new_absolute().left(0.0).top(0.0))
                .width(Size::percent(100.0))
                .height(Size::percent(100.0))
                .padding(Gaps::new(112.0, 24.0, 24.0, 24.0))
                .cross_align(Alignment::center())
                .child(
                    VimPickerCard::new(request, query, cursor, filter_active)
                        .width(width)
                        .capture_keys(false)
                        .on_choose(move |item: PickerItem| {
                            if let Ok(cmd) = Command::from_name_and_args(&item.command, &item.command_args) {
                                choose_core.invoke_or_log(cmd);
                            }
                        }),
                )
        });

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
                    .child(
                        WindowShell::new(location, location_a11y_id, page_content).interactive_window_controls(false),
                    )
                    .maybe_child(picker_overlay),
            )
    }
}
