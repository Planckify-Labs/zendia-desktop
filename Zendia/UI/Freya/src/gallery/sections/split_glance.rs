/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 4 gallery section for split view and Glance: two panes side by side and stacked, a three-pane grid, a resize
//! in progress, the drop zones of a dragged tab, a split group row in the sidebar, and Glance open and opening.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::sidebar::{PageDrop, SidebarDragState, SidebarViewState, try_use_sidebar_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::{KitChip, Pill};
use crate::ui::page::{PageArea, PaneMode};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{
    BrowserModel, SpaceId, SplitDirection, SplitGroup, SplitGroupId, TabId, TabMoveTarget,
};
use zendia_browser_core::split::{self, DropZone, Glance};
use zendia_module_api::ui::UiTone;

const DOCS: TabId = TabId(12);
const ARCH: TabId = TabId(11);
const TOKENS: TabId = TabId(10);
const RELEASES: TabId = TabId(9);
const GLANCE_TAB: TabId = TabId(40);

#[derive(Clone)]
pub struct SplitGlanceSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
}

impl PartialEq for SplitGlanceSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

fn activate(model: &mut BrowserModel, tab_id: TabId) {
    model.active_tab = Some(tab_id);
    if let Some(window) = model.windows.first_mut() {
        window.active_tab = Some(tab_id);
    }
    if let Some(space) = model.spaces.iter_mut().find(|space| space.id == SpaceId(1)) {
        space.last_active_tab = Some(tab_id);
    }
}

fn add_group(model: &mut BrowserModel, tab_ids: Vec<TabId>, direction: SplitDirection, ratios: Option<Vec<f32>>) {
    let id = SplitGroupId(1);
    for tab in &mut model.tabs {
        if tab_ids.contains(&tab.id) {
            tab.split_group_id = Some(id);
        }
    }
    model.split_groups = vec![SplitGroup {
        id,
        space_id: SpaceId(1),
        direction,
        ratios: ratios.unwrap_or_else(|| split::default_ratios(direction, tab_ids.len())),
        tab_ids,
        tree: None,
        zoomed_tab: None,
    }];
}

fn add_glance(model: &mut BrowserModel) {
    if let Some(mut tab) = model.tab(ARCH).cloned() {
        tab.id = GLANCE_TAB;
        tab.title = "Architecture — Zendia".to_string();
        tab.url = "https://zendia.dev/architecture".to_string();
        model.tabs.push(tab);
    }
    model.glance = Some(Glance {
        tab_id: GLANCE_TAB,
        parent_tab_id: Some(DOCS),
        origin: Some((0.3, 0.6)),
    });
}

fn configure_fixture(state_id: &str, gallery_core: &GalleryCore, sidebar_state: Option<SidebarViewState>) {
    if let Some(state) = sidebar_state {
        state.reset();
    }
    let mut model = super::gallery_sample_model();
    activate(&mut model, DOCS);
    match state_id {
        "stacked" => add_group(&mut model, vec![DOCS, ARCH], SplitDirection::Vertical, None),
        "grid" => add_group(&mut model, vec![DOCS, ARCH, TOKENS], SplitDirection::Grid, None),
        "resize" => add_group(
            &mut model,
            vec![DOCS, ARCH],
            SplitDirection::Horizontal,
            Some(vec![0.62, 0.38]),
        ),
        "group-inactive" => {
            add_group(&mut model, vec![DOCS, ARCH], SplitDirection::Horizontal, None);
            activate(&mut model, RELEASES);
        }
        "drop-zones" | "glance-open" | "glance-opening" => {
            if state_id != "drop-zones" {
                add_glance(&mut model);
            }
        }
        _ => add_group(&mut model, vec![DOCS, ARCH], SplitDirection::Horizontal, None),
    }
    gallery_core.stub().replace_model(model);

    if state_id == "drop-zones"
        && let Some(state) = sidebar_state
    {
        let mut drag = state.drag_state;
        drag.set(Some(SidebarDragState {
            tab_id: RELEASES,
            tab_title: "Releases — Zendia".to_string(),
            tab_favicon: Some("flame".to_string()),
            pointer_x: 1180.0,
            pointer_y: 520.0,
            target: Some(TabMoveTarget::Normal {
                space_id: Some(SpaceId(1)),
                index: 0,
            }),
            page_drop: Some(PageDrop {
                target_tab: DOCS,
                zone: DropZone::Right,
            }),
            is_preview: true,
        }));
    }
}

impl Component for SplitGlanceSection {
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
            configure_fixture(&self.state_id, &self.gallery_core, sidebar_state);
        }

        let (summary, active_url) = {
            let model = core.model().read();
            let summary = match (model.active_split_group(), &model.glance) {
                (_, Some(_)) => "Glance open".to_string(),
                (Some(group), None) => format!(
                    "{} panes · {}",
                    group.tab_ids.len(),
                    split::direction_as_str(group.direction)
                ),
                (None, None) => "Single page".to_string(),
            };
            (
                summary,
                model.active_tab().map(|tab| tab.url.clone()).unwrap_or_default(),
            )
        };

        let mut location = use_state(|| active_url.clone());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut shown_url = use_state(|| active_url.clone());
        if *shown_url.peek() != active_url && !location_a11y_id.is_focused() {
            shown_url.set(active_url.clone());
            location.set(active_url.clone());
        }

        let chip = |label: &'static str, command: Command| {
            let core = core.clone();
            KitChip::new(label)
                .on_press(move |_| {
                    core.invoke_or_log(command.clone());
                })
                .into_element()
        };
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
                    .child(chip(
                        "Side by Side",
                        Command::SplitSetLayout {
                            group_id: None,
                            direction: SplitDirection::Horizontal,
                        },
                    ))
                    .child(chip(
                        "Stacked",
                        Command::SplitSetLayout {
                            group_id: None,
                            direction: SplitDirection::Vertical,
                        },
                    ))
                    .child(chip(
                        "Grid",
                        Command::SplitSetLayout {
                            group_id: None,
                            direction: SplitDirection::Grid,
                        },
                    ))
                    .child(chip("Unsplit", Command::SplitUnsplit { group_id: None }))
                    .child(chip(
                        "Open Glance",
                        Command::GlanceOpen {
                            url: "https://zendia.dev/releases".to_string(),
                            parent_tab_id: None,
                            origin: Some((0.5, 0.3)),
                        },
                    ))
                    .child(chip("Close Glance", Command::GlanceClose))
                    .child(KitChip::new("Reset State").on_press(move |_| {
                        configure_fixture(&reset_state_id, &reset_core, sidebar_state);
                    })),
            );

        let mut page = PageArea::new(PaneMode::Placeholder).highlight_handle(self.state_id == "resize");
        if self.state_id == "glance-opening" {
            page = page.glance_progress(0.55);
        } else if self.state_id == "glance-open" {
            page = page.glance_progress(1.0);
        }

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
