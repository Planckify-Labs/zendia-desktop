/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 5 gallery section for vim focus surfaces: explorer cursor and selection over sidebar rows, hint labels (full
//! and narrowed by a typed prefix), toolbar focus on buttons and the address bar, and window mode over split panes
//! with numbered chips and landing labels.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::sidebar::{SidebarViewState, try_use_sidebar_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::{KitChip, Pill};
use crate::ui::page::{PageArea, PaneMode};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::ContributionId;
use zendia_browser_core::focus::{
    self, FocusDirection, FocusState, FocusSurface, FocusTarget, HintSession, LandingKind, PaneDirection,
    WindowCarryState,
};
use zendia_browser_core::model::{
    BrowserModel, FolderId, InputMode, SpaceId, SplitDirection, SplitGroup, SplitGroupId, TabId, ToolbarLayout,
};
use zendia_browser_core::split;
use zendia_browser_core::stub::sample_model;
use zendia_module_api::ui::UiTone;

const DOCS: TabId = TabId(12);
const ARCH: TabId = TabId(11);
const TOKENS: TabId = TabId(10);
const RELEASES: TabId = TabId(9);
const FOLDER_UI: FolderId = FolderId(1);
const SPLIT_GROUP: SplitGroupId = SplitGroupId(1);

#[derive(Clone)]
pub struct VimFocusSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
}

impl PartialEq for VimFocusSection {
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

fn add_grid_group(model: &mut BrowserModel) {
    let tab_ids = vec![DOCS, ARCH, TOKENS];
    for tab in &mut model.tabs {
        if tab_ids.contains(&tab.id) {
            tab.split_group_id = Some(SPLIT_GROUP);
        }
    }
    model.split_groups = vec![SplitGroup {
        id: SPLIT_GROUP,
        space_id: SpaceId(1),
        direction: SplitDirection::Grid,
        ratios: split::default_ratios(SplitDirection::Grid, tab_ids.len()),
        tab_ids,
    }];
}

fn configure_fixture(state_id: &str, gallery_core: &GalleryCore, sidebar_state: Option<SidebarViewState>) {
    if let Some(state) = sidebar_state {
        state.reset();
    }
    let mut model = sample_model(true);
    model.vim_mode_enabled = true;
    model.layout.toolbar_layout = ToolbarLayout::Single;
    activate(&mut model, DOCS);

    match state_id {
        "explorer-selection" => {
            model.input_mode = InputMode::Explorer;
            model.focus = FocusState {
                surface: Some(FocusSurface::Explorer),
                cursor: Some(FocusTarget::Tab(DOCS)),
                selection: vec![
                    FocusTarget::Tab(RELEASES),
                    FocusTarget::Tab(TOKENS),
                    FocusTarget::Tab(ARCH),
                    FocusTarget::Tab(DOCS),
                ],
                visual_anchor: Some(FocusTarget::Tab(RELEASES)),
                hints: None,
                window_carry: None,
            };
        }
        "explorer-folder" => {
            model.input_mode = InputMode::Explorer;
            model.focus = FocusState {
                surface: Some(FocusSurface::Explorer),
                cursor: Some(FocusTarget::Folder(FOLDER_UI)),
                selection: Vec::new(),
                visual_anchor: None,
                hints: None,
                window_carry: None,
            };
        }
        "explorer-split-group" => {
            add_grid_group(&mut model);
            model.input_mode = InputMode::Explorer;
            model.focus = FocusState {
                surface: Some(FocusSurface::Explorer),
                cursor: Some(FocusTarget::SplitGroup(SPLIT_GROUP)),
                selection: Vec::new(),
                visual_anchor: None,
                hints: None,
                window_carry: None,
            };
        }
        "hints-sidebar" => {
            let targets = focus::explorer_targets(&model);
            model.input_mode = InputMode::Explorer;
            model.focus = FocusState {
                surface: Some(FocusSurface::Explorer),
                cursor: Some(FocusTarget::Tab(DOCS)),
                selection: Vec::new(),
                visual_anchor: None,
                hints: Some(HintSession::new(FocusSurface::Explorer, targets)),
                window_carry: None,
            };
        }
        "hints-narrowed" => {
            let targets = focus::explorer_targets(&model);
            let mut session = HintSession::with_min_len(FocusSurface::Explorer, targets, 2);
            // Narrow to the first group ("a") while keeping non-matching labels hidden.
            let _ = session.type_char('a');
            // Assign second-group prefixes to the last half so the screenshot shows both matching and hidden rows.
            for (idx, entry) in session.entries.iter_mut().enumerate() {
                if idx >= 5 {
                    let second = focus::HINT_ALPHABET.chars().nth(idx - 5).unwrap_or('a');
                    entry.label = format!("s{second}");
                }
            }
            model.input_mode = InputMode::Explorer;
            model.focus = FocusState {
                surface: Some(FocusSurface::Explorer),
                cursor: Some(FocusTarget::Tab(DOCS)),
                selection: Vec::new(),
                visual_anchor: None,
                hints: Some(session),
                window_carry: None,
            };
        }
        "toolbar-button" => {
            model.input_mode = InputMode::Toolbar;
            model.focus = FocusState {
                surface: Some(FocusSurface::Toolbar),
                cursor: Some(FocusTarget::ToolbarItem(ContributionId::new("builtin.reload"))),
                selection: Vec::new(),
                visual_anchor: None,
                hints: None,
                window_carry: None,
            };
        }
        "toolbar-urlbar" => {
            model.input_mode = InputMode::Toolbar;
            model.focus = FocusState {
                surface: Some(FocusSurface::Toolbar),
                cursor: Some(FocusTarget::ToolbarItem(ContributionId::new("builtin.url_bar"))),
                selection: Vec::new(),
                visual_anchor: None,
                hints: None,
                window_carry: None,
            };
        }
        "window-grid" => {
            add_grid_group(&mut model);
            model.input_mode = InputMode::Window;
            model.focus = FocusState {
                surface: Some(FocusSurface::Window),
                cursor: Some(FocusTarget::Pane(DOCS)),
                selection: Vec::new(),
                visual_anchor: None,
                hints: None,
                window_carry: None,
            };
        }
        "window-landing" => {
            add_grid_group(&mut model);
            let targets = focus::compute_landing_targets(&model, DOCS);
            let selected = targets
                .iter()
                .position(|target| {
                    matches!(
                        target.kind,
                        LandingKind::PaneSide {
                            target_tab,
                            zone: split::DropZone::Right
                        } if target_tab == TOKENS
                    )
                })
                .unwrap_or(0);
            model.input_mode = InputMode::Window;
            model.focus = FocusState {
                surface: Some(FocusSurface::Window),
                cursor: Some(FocusTarget::Pane(DOCS)),
                selection: Vec::new(),
                visual_anchor: None,
                hints: None,
                window_carry: Some(WindowCarryState {
                    carried_tab: DOCS,
                    targets,
                    selected,
                }),
            };
        }
        _ => {
            // explorer-tab
            model.input_mode = InputMode::Explorer;
            model.focus = FocusState {
                surface: Some(FocusSurface::Explorer),
                cursor: Some(FocusTarget::Tab(DOCS)),
                selection: Vec::new(),
                visual_anchor: None,
                hints: None,
                window_carry: None,
            };
        }
    }

    gallery_core.stub().replace_model(model);
}

impl Component for VimFocusSection {
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

        let (summary, active_url, mode_tone) = {
            let model = core.model().read();
            let mode_label = match model.input_mode {
                InputMode::Explorer => {
                    if let Some(hints) = &model.focus.hints {
                        if hints.typed.is_empty() {
                            format!("EXPLORER · {} hints", hints.entries.len())
                        } else {
                            format!(
                                "EXPLORER · hints \"{}\" ({} left)",
                                hints.typed,
                                hints.visible_entries().len()
                            )
                        }
                    } else if !model.focus.selection.is_empty() {
                        format!("EXPLORER · {} selected", model.focus.selection.len())
                    } else {
                        "EXPLORER · cursor active".to_string()
                    }
                }
                InputMode::Toolbar => "TOOLBAR · cursor active".to_string(),
                InputMode::Window => {
                    if let Some(carry) = &model.focus.window_carry {
                        let landing = carry.selected_target().map(|t| t.label.as_str()).unwrap_or("landing");
                        format!("WINDOW · carrying ({landing})")
                    } else {
                        "WINDOW · split panes".to_string()
                    }
                }
                _ => "NORMAL".to_string(),
            };
            let tone = match model.input_mode {
                InputMode::Explorer => UiTone::Accent,
                InputMode::Toolbar => UiTone::Warning,
                InputMode::Window => UiTone::Success,
                _ => UiTone::Neutral,
            };
            (
                mode_label,
                model.active_tab().map(|tab| tab.url.clone()).unwrap_or_default(),
                tone,
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

        let controls: Vec<Element> = match self.state_id.as_str() {
            "toolbar-button" | "toolbar-urlbar" => vec![
                chip(
                    "Prev",
                    Command::ToolbarFocusMove {
                        direction: FocusDirection::Previous,
                    },
                ),
                chip(
                    "Next",
                    Command::ToolbarFocusMove {
                        direction: FocusDirection::Next,
                    },
                ),
                chip("Activate", Command::ToolbarFocusActivate),
                chip("Reopen", Command::ToolbarFocusOpen),
            ],
            "window-grid" | "window-landing" => vec![
                chip("Pane 1", Command::WindowModeFocusPane { pane: 1 }),
                chip("Pane 2", Command::WindowModeFocusPane { pane: 2 }),
                chip("Pane 3", Command::WindowModeFocusPane { pane: 3 }),
                chip(
                    "Move Right",
                    Command::WindowModeMovePane {
                        direction: PaneDirection::Right,
                    },
                ),
                chip("Carry", Command::WindowModeCarryToggle),
                chip("Drop", Command::WindowModeDrop),
            ],
            _ => vec![
                chip(
                    "Up",
                    Command::ExplorerMove {
                        direction: FocusDirection::Previous,
                    },
                ),
                chip(
                    "Down",
                    Command::ExplorerMove {
                        direction: FocusDirection::Next,
                    },
                ),
                chip("Select", Command::ExplorerSelect { range: false }),
                chip("Hints", Command::ExplorerPick),
                chip("Activate", Command::ExplorerActivate),
            ],
        };

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
            .child(Pill::new(summary).tone(mode_tone))
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(theme.spacing.xs)
                    .children(controls)
                    .child(KitChip::new("Reset State").on_press(move |_| {
                        configure_fixture(&reset_state_id, &reset_core, sidebar_state);
                    })),
            );

        let page = PageArea::new(PaneMode::Placeholder);

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
