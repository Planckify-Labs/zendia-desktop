/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The library's Spaces section: side-by-side workspace cards with inline rename, icon picker, theme trigger,
//! scrollable tab/folder lists, cross-card tab drag-and-drop, horizontal card reordering, `...` menu, and `+` button.

use crate::ui::chrome::layout::{ChromePalette, TAB_ROW_HEIGHT, TAB_ROW_RADIUS};
use crate::ui::chrome::menus::{OpenContextMenu, open_tab_context_menu, try_use_menu_state};
use crate::ui::chrome::sidebar::state::favicon_badge_colors;
use crate::ui::chrome::sidebar::tab_row::DropIndicatorBar;
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::kit::icons::prepared_icon_bytes;
use crate::ui::library::nav::{LibraryCursorRing, LibraryNavLayout, LibraryNavRow, use_library_nav};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use std::collections::BTreeMap;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::library::LibrarySection;
use zendia_browser_core::model::{Folder, FolderId, Space, SpaceId, Tab, TabId, TabKind, TabMoveTarget};
use zendia_browser_core::space_routing::{SELECTABLE_SPACE_ICONS, build_library_space_menu};
use zendia_keymap::KeyTable;

/// Width of this section's body next to the rail.
pub const WIDTH: f32 = 620.0;

const CARD_WIDTH: f32 = 242.0;
const CARD_GAP: f32 = 24.0;
const CARD_RADIUS: f32 = 14.0;
const DRAG_THRESHOLD_PX: f32 = 6.0;

/// Active tab drag-and-drop across or within space cards in the library.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryTabDrag {
    pub tab_id: TabId,
    pub tab_title: String,
    pub tab_favicon: Option<String>,
    pub source_space: Option<SpaceId>,
    pub pointer_x: f32,
    pub pointer_y: f32,
    /// Destination space and move target currently hovered.
    pub target_space: Option<SpaceId>,
    pub target: Option<TabMoveTarget>,
    /// When true, rendered as a static gallery preview at fixed stage coordinates.
    pub is_preview: bool,
}

/// Active horizontal space-card reorder drag initiated from a card's bottom move handle.
#[derive(Clone, Debug, PartialEq)]
pub struct LibrarySpaceReorderDrag {
    pub space_id: SpaceId,
    pub start_index: usize,
    pub target_index: usize,
    pub start_x: f32,
    pub current_x: f32,
}

/// View-local state for the library's Spaces section (inline rename, icon picker, drag previews, scroll request).
#[derive(Clone, Copy, PartialEq)]
pub struct LibrarySpacesViewState {
    pub renaming_space: State<Option<SpaceId>>,
    pub rename_draft: State<String>,
    pub icon_picker_space: State<Option<SpaceId>>,
    pub icon_picker_anchor: State<(f32, f32)>,
    pub tab_drag: State<Option<LibraryTabDrag>>,
    pub space_drag: State<Option<LibrarySpaceReorderDrag>>,
    pub scroll_to_end_request: State<u64>,
    pub collapsed_folders: State<BTreeMap<FolderId, bool>>,
    /// Window-coordinate bounds of each visible space card, used for reliable cross-card drop hit testing.
    pub card_areas: State<BTreeMap<SpaceId, (f32, f32, f32, f32)>>,
}

impl LibrarySpacesViewState {
    /// Starts inline renaming for `space_id` prefilled with `current_name`.
    pub fn start_rename(&self, space_id: SpaceId, current_name: impl Into<String>) {
        let mut picker = self.icon_picker_space;
        picker.set_if_modified(None);
        let mut draft = self.rename_draft;
        draft.set(current_name.into());
        let mut renaming = self.renaming_space;
        renaming.set(Some(space_id));
    }

    /// Cancels inline renaming without committing changes.
    pub fn cancel_rename(&self) {
        let mut renaming = self.renaming_space;
        renaming.set_if_modified(None);
    }

    /// Opens the icon picker popover for `space_id` near `anchor`.
    pub fn open_icon_picker(&self, space_id: SpaceId, anchor: Option<(f32, f32)>) {
        self.cancel_rename();
        if let Some(pos) = anchor {
            let mut anchor_state = self.icon_picker_anchor;
            anchor_state.set(pos);
        }
        let mut picker = self.icon_picker_space;
        picker.set(Some(space_id));
    }

    /// Closes the icon picker popover.
    pub fn close_icon_picker(&self) {
        let mut picker = self.icon_picker_space;
        picker.set_if_modified(None);
    }

    /// Returns true when inline text editing is active in the Spaces section.
    #[allow(dead_code)]
    pub fn is_renaming(&self) -> bool {
        self.renaming_space.read().is_some()
    }

    /// Clears transient view states (used when resetting gallery fixtures).
    pub fn reset(&self) {
        let mut renaming = self.renaming_space;
        renaming.set_if_modified(None);
        let mut draft = self.rename_draft;
        draft.set(String::new());
        let mut picker = self.icon_picker_space;
        picker.set_if_modified(None);
        let mut tab_drag = self.tab_drag;
        tab_drag.set(None);
        let mut space_drag = self.space_drag;
        space_drag.set(None);
    }

    /// Updates the hovered drop target during an active live tab drag.
    pub fn set_tab_drop_target(&self, space_id: SpaceId, target: TabMoveTarget) {
        let mut drag = self.tab_drag;
        let should_update = drag
            .peek()
            .as_ref()
            .is_some_and(|d| !d.is_preview && (d.target_space != Some(space_id) || d.target != Some(target)));
        if should_update && let Some(current) = drag.write().as_mut() {
            current.target_space = Some(space_id);
            current.target = Some(target);
        }
    }

    /// Requests the horizontal strip to scroll toward its end (used by the scrolled gallery fixture).
    pub fn request_scroll_to_end(&self) {
        let mut req = self.scroll_to_end_request;
        let next = *req.peek() + 1;
        req.set(next);
    }
}

/// Installs [`LibrarySpacesViewState`] into Freya context for the current window.
pub fn use_provide_spaces_view_state() -> LibrarySpacesViewState {
    let renaming_space = use_state(|| None);
    let rename_draft = use_state(String::new);
    let icon_picker_space = use_state(|| None);
    let icon_picker_anchor = use_state(|| (180.0f32, 110.0f32));
    let tab_drag = use_state(|| None);
    let space_drag = use_state(|| None);
    let scroll_to_end_request = use_state(|| 0u64);
    let collapsed_folders = use_state(BTreeMap::new);
    let card_areas = use_state(BTreeMap::new);
    use_provide_context(|| LibrarySpacesViewState {
        renaming_space,
        rename_draft,
        icon_picker_space,
        icon_picker_anchor,
        tab_drag,
        space_drag,
        scroll_to_end_request,
        collapsed_folders,
        card_areas,
    })
}

/// Consumes [`LibrarySpacesViewState`] if installed in this window.
pub fn try_use_spaces_view_state() -> Option<LibrarySpacesViewState> {
    try_consume_context::<LibrarySpacesViewState>()
}

#[derive(Clone, PartialEq)]
pub struct SpacesSection;

impl Component for SpacesSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let provided_state = try_use_spaces_view_state();
        let fallback_renaming = use_state(|| None);
        let fallback_draft = use_state(String::new);
        let fallback_picker = use_state(|| None);
        let fallback_anchor = use_state(|| (180.0f32, 110.0f32));
        let fallback_tab_drag = use_state(|| None);
        let fallback_space_drag = use_state(|| None);
        let fallback_scroll_req = use_state(|| 0u64);
        let fallback_collapsed = use_state(BTreeMap::new);
        let fallback_card_areas = use_state(BTreeMap::new);
        let spaces_state = provided_state.unwrap_or(LibrarySpacesViewState {
            renaming_space: fallback_renaming,
            rename_draft: fallback_draft,
            icon_picker_space: fallback_picker,
            icon_picker_anchor: fallback_anchor,
            tab_drag: fallback_tab_drag,
            space_drag: fallback_space_drag,
            scroll_to_end_request: fallback_scroll_req,
            collapsed_folders: fallback_collapsed,
            card_areas: fallback_card_areas,
        });

        let scroll_controller = use_scroll_controller(ScrollConfig::default);
        let mut last_scroll_req = use_state(|| 0u64);
        let mut container_area = use_state(|| (0.0f32, 0.0f32, WIDTH, 640.0f32));

        let req = *spaces_state.scroll_to_end_request.read();
        if req != *last_scroll_req.peek() {
            last_scroll_req.set(req);
            let mut controller = scroll_controller;
            spawn(async move {
                timer(Duration::from_millis(80)).await;
                controller.scroll_to(ScrollPosition::End, Direction::Horizontal);
            });
        }

        let (spaces, active_space, tabs, pinned_tabs, folders) = {
            let model = core.model().read();
            (
                model.spaces.clone(),
                model.active_space,
                model.tabs.clone(),
                model.pinned_tabs.clone(),
                model.folders.clone(),
            )
        };

        // Register rows in drawn order for the keyboard layer (one column per card + the `+` button).
        let mut nav_rows = Vec::new();
        for space in &spaces {
            nav_rows.push(LibraryNavRow::new(
                format!("space:{}", space.id.0),
                Some(Command::SpaceSwitch { space_id: space.id }),
            ));
            for tab in tabs_for_space_card(&tabs, &pinned_tabs, &folders, space.id) {
                nav_rows.push(LibraryNavRow::new(
                    format!("tab:{}", tab.id.0),
                    Some(Command::SpaceOpenTab { tab_id: tab.id }),
                ));
            }
        }
        nav_rows.push(LibraryNavRow::new(
            "space:add",
            Some(Command::SpaceCreate { name: None, icon: None }),
        ));
        let cursor_key = use_library_nav(LibrarySection::Spaces, LibraryNavLayout::Columns, nav_rows);

        let active_tab_drag = spaces_state.tab_drag.read().clone();
        let active_space_drag = spaces_state.space_drag.read().clone();
        let active_icon_picker = *spaces_state.icon_picker_space.read();
        let space_count = spaces.len();
        let container_origin = {
            let (ox, oy, _, _) = *container_area.read();
            (ox, oy)
        };

        let cards_row = rect()
            .height(Size::fill())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Start)
            .padding(Gaps::new(36.0, 24.0, 36.0, 24.0))
            .spacing(CARD_GAP)
            .children(spaces.iter().enumerate().map(|(index, space)| {
                let space_pinned = pinned_tabs_for_space(&tabs, &pinned_tabs, space.id);
                let space_folders: Vec<Folder> = folders.iter().filter(|f| f.space_id == space.id).cloned().collect();
                let space_normal: Vec<Tab> = tabs
                    .iter()
                    .filter(|t| t.kind == TabKind::Normal && t.space_id == Some(space.id) && t.folder_id.is_none())
                    .cloned()
                    .collect();
                let folder_tabs_map: Vec<(Folder, Vec<Tab>)> = space_folders
                    .into_iter()
                    .map(|folder| {
                        let children: Vec<Tab> = folder
                            .tab_ids
                            .iter()
                            .filter_map(|id| tabs.iter().find(|t| t.id == *id).cloned())
                            .collect();
                        (folder, children)
                    })
                    .collect();

                SpaceCard {
                    space: space.clone(),
                    index,
                    total_spaces: space_count,
                    is_active: active_space == Some(space.id),
                    pinned_tabs: space_pinned,
                    folders: folder_tabs_map,
                    normal_tabs: space_normal,
                    cursor_key: cursor_key.clone(),
                    spaces_state,
                    key: DiffKey::from(&space.id.0),
                }
                .into_element()
            }))
            .child(AddSpaceButton {
                focused: cursor_key.as_deref() == Some("space:add"),
            });

        let drag_ghost = active_tab_drag.map(|drag| LibraryTabDragGhost { drag, container_origin });

        let icon_picker = active_icon_picker.map(|space_id| {
            let current_icon = spaces
                .iter()
                .find(|s| s.id == space_id)
                .map(|s| s.icon.clone())
                .unwrap_or_default();
            let anchor = *spaces_state.icon_picker_anchor.read();
            SpaceIconPickerPopover {
                space_id,
                current_icon,
                anchor,
                spaces_state,
            }
        });

        let reorder_banner = active_space_drag.as_ref().and_then(|drag| {
            let name = spaces.iter().find(|s| s.id == drag.space_id)?.name.clone();
            let bg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(20, 22, 30, 0.92).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(250, 251, 255, 0.96).to_color(),
            };
            Some(
                rect()
                    .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                    .position(Position::new_absolute().top(10.0).left(24.0))
                    .padding(Gaps::new(5.0, 12.0, 5.0, 12.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(bg)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.color(ThemeRole::Accent)),
                    )
                    .child(
                        label()
                            .text(format!(
                                "Moving \"{name}\" → position {} of {space_count}",
                                drag.target_index + 1
                            ))
                            .font_size(12.0)
                            .font_weight(FontWeight::MEDIUM)
                            .color(theme.color(ThemeRole::Text)),
                    ),
            )
        });

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .overflow(Overflow::Clip)
            .on_sized(move |e: Event<SizedEventData>| {
                container_area.set_if_modified((e.area.min_x(), e.area.min_y(), e.area.width(), e.area.height()));
            })
            .child(
                ScrollView::new_controlled(scroll_controller)
                    .width(Size::fill())
                    .height(Size::fill())
                    .direction(Direction::Horizontal)
                    .show_scrollbar(true)
                    .child(cards_row),
            )
            .maybe_child(reorder_banner)
            .maybe_child(drag_ghost)
            .maybe_child(icon_picker)
    }
}

fn pinned_tabs_for_space(
    tabs: &[Tab],
    pinned_tabs: &[zendia_browser_core::model::PinnedTab],
    space_id: SpaceId,
) -> Vec<Tab> {
    let mut entries: Vec<_> = pinned_tabs.iter().filter(|p| p.space_id == space_id).collect();
    entries.sort_by_key(|p| p.order);
    entries
        .into_iter()
        .filter_map(|p| tabs.iter().find(|t| t.id == p.tab_id).cloned())
        .collect()
}

fn tabs_for_space_card(
    tabs: &[Tab],
    pinned_tabs: &[zendia_browser_core::model::PinnedTab],
    folders: &[Folder],
    space_id: SpaceId,
) -> Vec<Tab> {
    let mut out = pinned_tabs_for_space(tabs, pinned_tabs, space_id);
    for folder in folders.iter().filter(|f| f.space_id == space_id) {
        for tab_id in &folder.tab_ids {
            if let Some(tab) = tabs.iter().find(|t| t.id == *tab_id) {
                out.push(tab.clone());
            }
        }
    }
    for tab in tabs
        .iter()
        .filter(|t| t.kind == TabKind::Normal && t.space_id == Some(space_id) && t.folder_id.is_none())
    {
        out.push(tab.clone());
    }
    out
}

/// Computes the dark or light card surface color for a space, matching Zen's library space cards.
fn space_card_background(space: &Space, scheme: ResolvedScheme) -> Color {
    let (base, tint_weight) = match scheme {
        ResolvedScheme::Dark => (Rgba::rgb(20, 20, 22), 0.08),
        ResolvedScheme::Light => (Rgba::rgb(232, 232, 235), 0.05),
    };
    if let Some(&(r, g, b)) = space.gradient.stops.first() {
        base.mix(Rgba::rgb(r, g, b), tint_weight).to_color()
    } else {
        base.to_color()
    }
}

#[derive(Clone, PartialEq)]
struct SpaceCard {
    space: Space,
    index: usize,
    total_spaces: usize,
    is_active: bool,
    pinned_tabs: Vec<Tab>,
    folders: Vec<(Folder, Vec<Tab>)>,
    normal_tabs: Vec<Tab>,
    cursor_key: Option<String>,
    spaces_state: LibrarySpacesViewState,
    key: DiffKey,
}

impl KeyExt for SpaceCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SpaceCard {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut body_hovered = use_state(|| false);

        let space_id = self.space.id;
        let spaces_state = self.spaces_state;
        let normal_count = self.normal_tabs.len();
        let tab_drag = spaces_state.tab_drag.read().clone();
        let space_drag = spaces_state.space_drag.read().clone();

        let is_dragging_this_card = space_drag.as_ref().is_some_and(|d| d.space_id == space_id);
        let is_drop_end_target = tab_drag.as_ref().is_some_and(|d| {
            d.target_space == Some(space_id)
                && matches!(
                    d.target,
                    Some(TabMoveTarget::Normal {
                        space_id: Some(sid),
                        index,
                    }) if sid == space_id && index >= normal_count
                )
        });
        let is_hovered_drop_card = tab_drag.as_ref().is_some_and(|d| d.target_space == Some(space_id));

        let card_bg = space_card_background(&self.space, theme.scheme);
        let card_border = if is_dragging_this_card || is_hovered_drop_card {
            theme.color(ThemeRole::Accent)
        } else {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.06).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.06).to_color(),
            }
        };
        let card_shadow = if is_dragging_this_card {
            Shadow::new()
                .x(0.0)
                .y(10.0)
                .blur(20.0)
                .color(Color::from_argb(95, 0, 0, 0))
        } else {
            match theme.scheme {
                ResolvedScheme::Dark => Shadow::new()
                    .x(0.0)
                    .y(2.0)
                    .blur(12.0)
                    .color(Color::from_argb(65, 0, 0, 0)),
                ResolvedScheme::Light => Shadow::new()
                    .x(0.0)
                    .y(2.0)
                    .blur(12.0)
                    .color(Color::from_argb(28, 0, 0, 0)),
            }
        };

        let shift_x = match &space_drag {
            Some(drag) if drag.space_id == space_id => (drag.current_x - drag.start_x).clamp(-320.0, 320.0),
            Some(drag) if drag.start_index < self.index && self.index <= drag.target_index => -(CARD_WIDTH + CARD_GAP),
            Some(drag) if drag.target_index <= self.index && self.index < drag.start_index => CARD_WIDTH + CARD_GAP,
            _ => 0.0,
        };

        let has_pinned = !self.pinned_tabs.is_empty();
        let has_below_pinned = !self.folders.is_empty() || !self.normal_tabs.is_empty();
        let is_empty = self.pinned_tabs.is_empty() && self.folders.is_empty() && self.normal_tabs.is_empty();
        let header_focused = self.cursor_key.as_deref() == Some(&format!("space:{}", space_id.0));

        let on_body_enter = move |_| {
            body_hovered.set_if_modified(true);
            spaces_state.set_tab_drop_target(
                space_id,
                TabMoveTarget::Normal {
                    space_id: Some(space_id),
                    index: normal_count,
                },
            );
        };

        rect()
            .width(Size::px(CARD_WIDTH))
            .height(Size::fill())
            .content(Content::Flex)
            .offset_x(shift_x)
            .on_sized(move |e: Event<SizedEventData>| {
                let area = (e.area.min_x(), e.area.min_y(), e.area.width(), e.area.height());
                let mut areas = spaces_state.card_areas;
                if areas.peek().get(&space_id) != Some(&area) {
                    areas.write().insert(space_id, area);
                }
            })
            .corner_radius(CornerRadius::new_all(CARD_RADIUS))
            .background(card_bg)
            .border(
                Border::new()
                    .width(if is_dragging_this_card || is_hovered_drop_card {
                        1.5
                    } else {
                        1.0
                    })
                    .alignment(BorderAlignment::Inner)
                    .fill(card_border),
            )
            .shadow(card_shadow)
            .overflow(Overflow::Clip)
            .child(SpaceCardHeader {
                space: self.space.clone(),
                focused: header_focused,
                spaces_state,
                key: DiffKey::from(&space_id.0),
            })
            .child(
                // Thin horizontal divider directly below the card header.
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(4.0, 14.0, 6.0, 14.0))
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(1.0))
                            .background(palette.divider),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .on_pointer_enter(on_body_enter)
                    .on_pointer_leave(move |_| body_hovered.set_if_modified(false))
                    .child(
                        ScrollView::new()
                            .width(Size::fill())
                            .height(Size::fill())
                            .show_scrollbar(false)
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .padding(Gaps::new(2.0, 8.0, 8.0, 8.0))
                                    .spacing(2.0)
                                    .children(self.pinned_tabs.iter().enumerate().map(|(idx, tab)| {
                                        SpaceTabRow {
                                            tab: tab.clone(),
                                            space_id,
                                            indent: 0.0,
                                            drop_target: TabMoveTarget::Pinned {
                                                space_id: Some(space_id),
                                                index: idx,
                                            },
                                            focused: self.cursor_key.as_deref() == Some(&format!("tab:{}", tab.id.0)),
                                            spaces_state,
                                            key: DiffKey::from(&tab.id.0),
                                        }
                                        .into_element()
                                    }))
                                    .maybe_child((has_pinned && has_below_pinned).then(|| {
                                        rect().width(Size::fill()).padding(Gaps::new(4.0, 6.0, 4.0, 6.0)).child(
                                            rect()
                                                .width(Size::fill())
                                                .height(Size::px(1.0))
                                                .background(palette.divider),
                                        )
                                    }))
                                    .children(self.folders.iter().map(|(folder, child_tabs)| {
                                        SpaceFolderGroup {
                                            folder: folder.clone(),
                                            tabs: child_tabs.clone(),
                                            space_id,
                                            cursor_key: self.cursor_key.clone(),
                                            spaces_state,
                                            key: DiffKey::from(&folder.id.0),
                                        }
                                        .into_element()
                                    }))
                                    .children(self.normal_tabs.iter().enumerate().map(|(idx, tab)| {
                                        SpaceTabRow {
                                            tab: tab.clone(),
                                            space_id,
                                            indent: 0.0,
                                            drop_target: TabMoveTarget::Normal {
                                                space_id: Some(space_id),
                                                index: idx,
                                            },
                                            focused: self.cursor_key.as_deref() == Some(&format!("tab:{}", tab.id.0)),
                                            spaces_state,
                                            key: DiffKey::from(&tab.id.0),
                                        }
                                        .into_element()
                                    }))
                                    .maybe_child(is_drop_end_target.then_some(DropIndicatorBar { indent: 0.0 }))
                                    .maybe_child((is_empty && !is_drop_end_target).then(|| {
                                        rect()
                                            .width(Size::fill())
                                            .height(Size::px(64.0))
                                            .padding(Gaps::new_all(8.0))
                                            .on_pointer_enter(move |_| {
                                                spaces_state.set_tab_drop_target(
                                                    space_id,
                                                    TabMoveTarget::Normal {
                                                        space_id: Some(space_id),
                                                        index: 0,
                                                    },
                                                );
                                            })
                                    })),
                            ),
                    ),
            )
            .child(SpaceCardFooter {
                space_id,
                space_name: self.space.name.clone(),
                index: self.index,
                total_spaces: self.total_spaces,
                spaces_state,
                key: DiffKey::from(&space_id.0),
            })
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SpaceCardHeader {
    space: Space,
    focused: bool,
    spaces_state: LibrarySpacesViewState,
    key: DiffKey,
}

impl KeyExt for SpaceCardHeader {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SpaceCardHeader {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut header_hovered = use_state(|| false);
        let mut icon_hovered = use_state(|| false);
        let mut theme_hovered = use_state(|| false);
        let mut icon_area = use_state(|| (180.0f32, 100.0f32));

        let space_id = self.space.id;
        let space_name = self.space.name.clone();
        let spaces_state = self.spaces_state;
        let is_renaming = *spaces_state.renaming_space.read() == Some(space_id);
        let has_icon = !self.space.icon.is_empty();

        let icon_core = core.clone();
        let on_icon_press = move |e: Event<PressEventData>| {
            e.stop_propagation();
            let pos = *icon_area.peek();
            spaces_state.open_icon_picker(space_id, Some(pos));
            icon_core.invoke_or_log(Command::SpaceStartIconPicker { space_id });
        };

        let rename_core = core.clone();
        let rename_name = space_name.clone();
        let on_name_press = move |e: Event<PressEventData>| {
            e.stop_propagation();
            spaces_state.start_rename(space_id, rename_name.clone());
            rename_core.invoke_or_log(Command::SpaceStartRename { space_id });
        };

        let theme_core = core.clone();
        let on_theme_press = move |e: Event<PressEventData>| {
            e.stop_propagation();
            theme_core.invoke_or_log(Command::SpaceOpenThemePicker { space_id });
        };

        let icon_bg = if icon_hovered() {
            palette.surface_pill_hover
        } else {
            Color::TRANSPARENT
        };
        let dashed_border_color = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.40).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.35).to_color(),
        };

        let show_icon_slot = has_icon || (header_hovered() && !is_renaming);
        let icon_button = show_icon_slot.then(|| {
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(format!("Change icon for {space_name}"))
                .width(Size::px(22.0))
                .height(Size::px(22.0))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(icon_bg)
                .maybe(!has_icon, |r| {
                    r.border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(dashed_border_color),
                    )
                })
                .center()
                .cursor(CursorIcon::Pointer)
                .on_sized(move |e: Event<SizedEventData>| {
                    icon_area.set_if_modified((e.area.min_x(), e.area.max_y() + 6.0));
                })
                .on_pointer_enter(move |_| icon_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| icon_hovered.set_if_modified(false))
                .on_press(on_icon_press)
                .maybe_child(has_icon.then(|| Icon::new(self.space.icon.clone()).size(15.0).role(ThemeRole::TextMuted)))
        });

        let name_el: Element = if is_renaming {
            let commit_core = core.clone();
            SpaceInlineRenameField {
                draft: spaces_state.rename_draft,
                on_commit: EventHandler::new(move |new_name: String| {
                    spaces_state.cancel_rename();
                    let trimmed = new_name.trim().to_string();
                    if !trimmed.is_empty() {
                        commit_core.invoke_or_log(Command::SpaceRename {
                            space_id,
                            name: trimmed,
                        });
                    }
                }),
                on_cancel: EventHandler::new(move |()| {
                    spaces_state.cancel_rename();
                }),
                key: DiffKey::from(&space_id.0),
            }
            .into_element()
        } else {
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(format!("Rename space {space_name}"))
                .width(Size::flex(1.0))
                .height(Size::px(26.0))
                .cross_align(Alignment::center())
                .overflow(Overflow::Clip)
                .cursor(CursorIcon::Text)
                .on_press(on_name_press)
                .child(
                    label()
                        .text(space_name.clone())
                        .width(Size::fill())
                        .font_size(13.0)
                        .font_weight(FontWeight::MEDIUM)
                        .color(palette.text)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
                .into_element()
        };

        let theme_button = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Change theme for {space_name}"))
            .width(Size::px(24.0))
            .height(Size::px(24.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(if theme_hovered() {
                palette.surface_pill_hover
            } else {
                Color::TRANSPARENT
            })
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| theme_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| theme_hovered.set_if_modified(false))
            .on_press(on_theme_press)
            .child(Icon::new("paintbrush-fill").size(14.0).role(if theme_hovered() {
                ThemeRole::Text
            } else {
                ThemeRole::TextMuted
            }));

        // When renaming, draw the crisp teal ring around the header box as in the reference shot.
        let header_border = if is_renaming {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::ModeLibrary))
        } else {
            Border::new().width(0.0).fill(Color::TRANSPARENT)
        };
        let header_bg = if !is_renaming && header_hovered() {
            palette.tab_hover_bg
        } else {
            Color::TRANSPARENT
        };

        let inner = rect()
            .width(Size::fill())
            .height(Size::px(32.0))
            .padding(Gaps::new(0.0, 6.0, 0.0, 6.0))
            .spacing(6.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(if is_renaming { 4.0 } else { 8.0 }))
            .background(header_bg)
            .border(header_border)
            .on_pointer_enter(move |_| header_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| header_hovered.set_if_modified(false))
            .maybe_child(icon_button)
            .child(name_el)
            .child(theme_button);

        rect()
            .width(Size::fill())
            .padding(Gaps::new(14.0, 10.0, 2.0, 10.0))
            .child(LibraryCursorRing::new(self.focused, inner).corner_radius(8.0))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SpaceInlineRenameField {
    draft: State<String>,
    on_commit: EventHandler<String>,
    on_cancel: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for SpaceInlineRenameField {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SpaceInlineRenameField {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let a11y_id = use_hook(AccessibilityId::new_unique);

        let draft = self.draft;
        let on_commit_submit = self.on_commit.clone();
        let on_commit_key = self.on_commit.clone();
        let on_cancel_key = self.on_cancel.clone();

        rect()
            .width(Size::flex(1.0))
            .height(Size::px(24.0))
            .cross_align(Alignment::center())
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .on_press(|e: Event<PressEventData>| e.stop_propagation())
            .child(
                Input::new(draft)
                    .a11y_id(a11y_id)
                    .auto_focus(true)
                    .flat()
                    .compact()
                    .placeholder("Space name")
                    .width(Size::fill())
                    .background(Color::TRANSPARENT)
                    .focus_background(Color::TRANSPARENT)
                    .border_fill(Color::TRANSPARENT)
                    .focus_border_fill(Color::TRANSPARENT)
                    .color(palette.text)
                    .placeholder_color(palette.text_muted)
                    .padding(Gaps::new(0.0, 2.0, 0.0, 2.0))
                    .corner_radius(CornerRadius::new_all(0.0))
                    .on_pre_key_down(move |e: Event<KeyboardEventData>| {
                        e.stop_propagation();
                        match &e.key {
                            Key::Named(NamedKey::Escape) => {
                                e.prevent_default();
                                on_cancel_key.call(());
                                false
                            }
                            Key::Named(NamedKey::Enter) => {
                                e.prevent_default();
                                on_commit_key.call(draft.peek().clone());
                                false
                            }
                            _ => true,
                        }
                    })
                    .on_submit(move |text: String| {
                        on_commit_submit.call(text);
                    }),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SpaceFolderGroup {
    folder: Folder,
    tabs: Vec<Tab>,
    space_id: SpaceId,
    cursor_key: Option<String>,
    spaces_state: LibrarySpacesViewState,
    key: DiffKey,
}

impl KeyExt for SpaceFolderGroup {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SpaceFolderGroup {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);

        let folder_id = self.folder.id;
        let space_id = self.space_id;
        let spaces_state = self.spaces_state;
        let collapsed = spaces_state
            .collapsed_folders
            .read()
            .get(&folder_id)
            .copied()
            .unwrap_or(self.folder.collapsed);

        let on_toggle = move |e: Event<PressEventData>| {
            e.stop_propagation();
            let mut map = spaces_state.collapsed_folders;
            map.write().insert(folder_id, !collapsed);
        };

        let on_enter = move |_| {
            hovered.set_if_modified(true);
            spaces_state.set_tab_drop_target(space_id, TabMoveTarget::Folder { folder_id, index: 0 });
        };

        let is_folder_drop_target = spaces_state.tab_drag.read().as_ref().is_some_and(
            |d| matches!(d.target, Some(TabMoveTarget::Folder { folder_id: fid, .. }) if fid == folder_id),
        );

        let header_bg = if is_folder_drop_target {
            theme.color(ThemeRole::Accent).with_a(45)
        } else if hovered() {
            palette.tab_hover_bg
        } else {
            Color::TRANSPARENT
        };

        rect()
            .width(Size::fill())
            .spacing(2.0)
            .child(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(format!("Folder {}", self.folder.name))
                    .width(Size::fill())
                    .height(Size::px(TAB_ROW_HEIGHT))
                    .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                    .spacing(8.0)
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
                    .background(header_bg)
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(on_enter)
                    .on_pointer_leave(move |_| hovered.set_if_modified(false))
                    .on_press(on_toggle)
                    .child(Icon::new("folder").size(15.0).role(ThemeRole::TextMuted))
                    .child(
                        label()
                            .text(self.folder.name.clone())
                            .width(Size::flex(1.0))
                            .font_size(12.5)
                            .font_weight(FontWeight::MEDIUM)
                            .color(palette.text)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    )
                    .child(
                        Icon::new(if collapsed { "arrow-right" } else { "arrow-down" })
                            .size(11.0)
                            .role(ThemeRole::TextMuted),
                    ),
            )
            .maybe_child((!collapsed).then(|| {
                rect()
                    .width(Size::fill())
                    .spacing(2.0)
                    .children(self.tabs.iter().enumerate().map(|(idx, tab)| {
                        SpaceTabRow {
                            tab: tab.clone(),
                            space_id,
                            indent: 14.0,
                            drop_target: TabMoveTarget::Folder { folder_id, index: idx },
                            focused: self.cursor_key.as_deref() == Some(&format!("tab:{}", tab.id.0)),
                            spaces_state,
                            key: DiffKey::from(&tab.id.0),
                        }
                        .into_element()
                    }))
            }))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SpaceTabRow {
    tab: Tab,
    space_id: SpaceId,
    indent: f32,
    drop_target: TabMoveTarget,
    focused: bool,
    spaces_state: LibrarySpacesViewState,
    key: DiffKey,
}

impl KeyExt for SpaceTabRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SpaceTabRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let menu_state = try_use_menu_state();
        let mut hovered = use_state(|| false);
        let mut close_hovered = use_state(|| false);
        let mut press_origin = use_state(|| None::<(f32, f32)>);

        let tab_id = self.tab.id;
        let space_id = self.space_id;
        let drop_target = self.drop_target;
        let spaces_state = self.spaces_state;
        let title = self.tab.display_title();

        let (is_being_dragged, is_drop_before) = match spaces_state.tab_drag.read().as_ref() {
            Some(drag) => (
                drag.tab_id == tab_id,
                drag.tab_id != tab_id && drag.target == Some(drop_target),
            ),
            None => (false, false),
        };

        let open_core = core.clone();
        let on_press = move |_: Event<PressEventData>| {
            if spaces_state.tab_drag.peek().is_some() {
                return;
            }
            open_core.invoke_or_log(Command::SpaceOpenTab { tab_id });
        };

        let menu_core = core.clone();
        let on_mouse_down = move |e: Event<MouseEventData>| {
            if e.button == Some(MouseButton::Right) {
                e.stop_propagation();
                if let Some(menu_state) = &menu_state {
                    let model = menu_core.model().peek().clone();
                    let position = (e.global_location.x as f32, e.global_location.y as f32);
                    open_tab_context_menu(menu_state, &model, tab_id, position);
                }
            }
        };

        let on_pointer_down = move |e: Event<PointerEventData>| {
            if e.button() == Some(MouseButton::Left) {
                press_origin.set(Some((e.global_location().x as f32, e.global_location().y as f32)));
            }
        };

        let drag_title = title.clone();
        let drag_favicon = self.tab.favicon.clone();
        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            let Some((ox, oy)) = *press_origin.peek() else {
                return;
            };
            let px = e.global_location().x as f32;
            let py = e.global_location().y as f32;
            let dist = ((px - ox).powi(2) + (py - oy).powi(2)).sqrt();
            let mut drag = spaces_state.tab_drag;
            let already_dragging_this = drag
                .peek()
                .as_ref()
                .is_some_and(|d| d.tab_id == tab_id && !d.is_preview);
            if already_dragging_this {
                let hit_space = spaces_state
                    .card_areas
                    .peek()
                    .iter()
                    .find(|(_, (x, y, w, h))| px >= *x && px <= *x + *w && py >= *y && py <= *y + *h)
                    .map(|(sid, _)| *sid);
                if let Some(current) = drag.write().as_mut() {
                    current.pointer_x = px;
                    current.pointer_y = py;
                    if let Some(dest_sid) = hit_space
                        && current.target_space != Some(dest_sid)
                    {
                        current.target_space = Some(dest_sid);
                        current.target = Some(TabMoveTarget::Normal {
                            space_id: Some(dest_sid),
                            index: usize::MAX,
                        });
                    }
                }
            } else if dist >= DRAG_THRESHOLD_PX {
                drag.set(Some(LibraryTabDrag {
                    tab_id,
                    tab_title: drag_title.clone(),
                    tab_favicon: drag_favicon.clone(),
                    source_space: Some(space_id),
                    pointer_x: px,
                    pointer_y: py,
                    target_space: Some(space_id),
                    target: Some(drop_target),
                    is_preview: false,
                }));
            }
        };

        let drop_core = core.clone();
        let on_global_pointer_up = move |_: Event<PointerEventData>| {
            if press_origin.peek().is_none() {
                return;
            }
            press_origin.set(None);
            let finished = {
                let mut drag = spaces_state.tab_drag;
                if drag
                    .peek()
                    .as_ref()
                    .is_some_and(|d| d.tab_id == tab_id && !d.is_preview)
                {
                    drag.write().take()
                } else {
                    None
                }
            };
            if let Some(drag) = finished {
                if let Some(target) = drag.target {
                    drop_core.invoke_or_log(Command::TabMove { tab_id, target });
                } else if let Some(dest_space) = drag.target_space
                    && Some(dest_space) != drag.source_space
                {
                    drop_core.invoke_or_log(Command::TabMoveToSpace {
                        tab_id: Some(tab_id),
                        space_id: dest_space,
                    });
                }
            }
        };

        let close_core = core.clone();
        let close_button = hovered().then(|| {
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(format!("Close tab {title}"))
                .width(Size::px(20.0))
                .height(Size::px(20.0))
                .corner_radius(CornerRadius::new_all(5.0))
                .background(if close_hovered() {
                    palette.surface_pill_hover
                } else {
                    Color::TRANSPARENT
                })
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| close_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| close_hovered.set_if_modified(false))
                .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    close_core.invoke_or_log(Command::TabClose {
                        tab_id: Some(tab_id),
                        force: false,
                    });
                })
                .child(Icon::new("close").size(11.0).role(ThemeRole::TextMuted))
        });

        let (badge_bg, badge_fg) = favicon_badge_colors(self.tab.favicon.as_deref(), &theme);
        let icon_name = self.tab.favicon.as_deref().unwrap_or("globe");
        let svg_el: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(icon_name) {
            SvgViewer::new((("zendia-icon", canonical), bytes))
                .width(Size::px(11.5))
                .height(Size::px(11.5))
                .color(badge_fg)
                .show_loader(false)
                .into_element()
        } else {
            Icon::new("globe").size(11.5).role(ThemeRole::TextMuted).into_element()
        };
        let favicon = rect()
            .width(Size::px(18.0))
            .height(Size::px(18.0))
            .corner_radius(CornerRadius::new_all(4.5))
            .background(badge_bg)
            .center()
            .child(svg_el);

        let bg = if hovered() {
            palette.tab_hover_bg
        } else {
            Color::TRANSPARENT
        };

        let row = rect()
            .a11y_role(AccessibilityRole::Tab)
            .a11y_alt(title.clone())
            .width(Size::fill())
            .height(Size::px(TAB_ROW_HEIGHT))
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .spacing(10.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
            .background(bg)
            .opacity(if is_being_dragged { 0.38 } else { 1.0 })
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| {
                hovered.set_if_modified(true);
                spaces_state.set_tab_drop_target(space_id, drop_target);
            })
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_pointer_down(on_pointer_down)
            .on_global_pointer_move(on_global_pointer_move)
            .on_global_pointer_up(on_global_pointer_up)
            .on_press(on_press)
            .on_mouse_down(on_mouse_down)
            .child(favicon)
            .child(
                label()
                    .text(title)
                    .width(Size::flex(1.0))
                    .font_size(12.5)
                    .font_weight(FontWeight::NORMAL)
                    .color(palette.text)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(close_button);

        rect()
            .width(Size::fill())
            .padding(Gaps::new(0.0, 0.0, 0.0, self.indent))
            .maybe_child(is_drop_before.then_some(DropIndicatorBar { indent: 0.0 }))
            .child(LibraryCursorRing::new(self.focused, row).corner_radius(TAB_ROW_RADIUS))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SpaceCardFooter {
    space_id: SpaceId,
    space_name: String,
    index: usize,
    total_spaces: usize,
    spaces_state: LibrarySpacesViewState,
    key: DiffKey,
}

impl KeyExt for SpaceCardFooter {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SpaceCardFooter {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let menu_state = try_use_menu_state();
        let mut handle_hovered = use_state(|| false);
        let mut more_hovered = use_state(|| false);
        let mut more_area = use_state(|| (240.0f32, 600.0f32));

        let space_id = self.space_id;
        let start_index = self.index;
        let total_spaces = self.total_spaces;
        let spaces_state = self.spaces_state;

        let on_handle_down = move |e: Event<PointerEventData>| {
            if e.button() != Some(MouseButton::Left) || total_spaces < 2 {
                return;
            }
            e.stop_propagation();
            let x = e.global_location().x as f32;
            let mut drag = spaces_state.space_drag;
            drag.set(Some(LibrarySpaceReorderDrag {
                space_id,
                start_index,
                target_index: start_index,
                start_x: x,
                current_x: x,
            }));
        };

        let on_global_move = move |e: Event<PointerEventData>| {
            let is_mine = spaces_state
                .space_drag
                .peek()
                .as_ref()
                .is_some_and(|d| d.space_id == space_id);
            if !is_mine {
                return;
            }
            let x = e.global_location().x as f32;
            let mut drag = spaces_state.space_drag;
            if let Some(current) = drag.write().as_mut() {
                current.current_x = x;
                let step = CARD_WIDTH + CARD_GAP;
                let delta = ((x - current.start_x) / step).round() as isize;
                let target =
                    (current.start_index as isize + delta).clamp(0, total_spaces.saturating_sub(1) as isize) as usize;
                current.target_index = target;
            }
        };

        let reorder_core = core.clone();
        let on_global_up = move |_: Event<PointerEventData>| {
            let finished = {
                let mut drag = spaces_state.space_drag;
                if drag.peek().as_ref().is_some_and(|d| d.space_id == space_id) {
                    drag.write().take()
                } else {
                    None
                }
            };
            if let Some(drag) = finished
                && drag.target_index != drag.start_index
            {
                reorder_core.invoke_or_log(Command::SpaceReorder {
                    space_id,
                    index: drag.target_index,
                });
            }
        };

        let menu_core = core.clone();
        let on_more_press = move |e: Event<PressEventData>| {
            e.stop_propagation();
            if let Some(ms) = &menu_state {
                let model = menu_core.model().peek().clone();
                let key_table = KeyTable::standard();
                let items = build_library_space_menu(&model, space_id, Some(&key_table));
                let pos = *more_area.peek();
                ms.open_context_menu(OpenContextMenu {
                    position: pos,
                    items,
                    tab_id: None,
                    open_submenu: None,
                });
            }
        };

        let dot_color = if more_hovered() {
            palette.text
        } else {
            palette.text_muted
        };

        rect()
            .width(Size::fill())
            .padding(Gaps::new(8.0, 14.0, 12.0, 14.0))
            .direction(Direction::Horizontal)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .child(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(format!("Drag to reorder {}", self.space_name))
                    .width(Size::px(26.0))
                    .height(Size::px(26.0))
                    .corner_radius(CornerRadius::new_all(6.0))
                    .background(if handle_hovered() {
                        palette.surface_pill_hover
                    } else {
                        Color::TRANSPARENT
                    })
                    .center()
                    .cursor(CursorIcon::Grab)
                    .on_pointer_enter(move |_| handle_hovered.set_if_modified(true))
                    .on_pointer_leave(move |_| handle_hovered.set_if_modified(false))
                    .on_pointer_down(on_handle_down)
                    .on_global_pointer_move(on_global_move)
                    .on_global_pointer_up(on_global_up)
                    .child(Icon::new("move-cursor").size(14.0).role(if handle_hovered() {
                        ThemeRole::Text
                    } else {
                        ThemeRole::TextMuted
                    })),
            )
            .child(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(format!("More options for {}", self.space_name))
                    .width(Size::px(26.0))
                    .height(Size::px(26.0))
                    .corner_radius(CornerRadius::new_all(6.0))
                    .background(if more_hovered() {
                        palette.surface_pill_hover
                    } else {
                        Color::TRANSPARENT
                    })
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::Center)
                    .cross_align(Alignment::Center)
                    .spacing(2.5)
                    .cursor(CursorIcon::Pointer)
                    .on_sized(move |e: Event<SizedEventData>| {
                        more_area.set_if_modified((e.area.min_x(), e.area.min_y()));
                    })
                    .on_pointer_enter(move |_| more_hovered.set_if_modified(true))
                    .on_pointer_leave(move |_| more_hovered.set_if_modified(false))
                    .on_press(on_more_press)
                    .child(
                        rect()
                            .width(Size::px(3.0))
                            .height(Size::px(3.0))
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(dot_color),
                    )
                    .child(
                        rect()
                            .width(Size::px(3.0))
                            .height(Size::px(3.0))
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(dot_color),
                    )
                    .child(
                        rect()
                            .width(Size::px(3.0))
                            .height(Size::px(3.0))
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(dot_color),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct AddSpaceButton {
    focused: bool,
}

impl Component for AddSpaceButton {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut hovered = use_state(|| false);

        let bg = match theme.scheme {
            ResolvedScheme::Dark => {
                if hovered() {
                    Rgba::from_rgb_alpha(60, 56, 78, 0.92).to_color()
                } else {
                    Rgba::from_rgb_alpha(44, 42, 58, 0.85).to_color()
                }
            }
            ResolvedScheme::Light => {
                if hovered() {
                    theme.palette.purple.with_alpha(0.95).to_color()
                } else {
                    theme.palette.purple.with_alpha(0.78).to_color()
                }
            }
        };

        let ring_color = if hovered() {
            theme.color(ThemeRole::Accent)
        } else {
            Color::TRANSPARENT
        };

        let badge = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Create space")
            .width(Size::px(32.0))
            .height(Size::px(32.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(bg)
            .border(
                Border::new()
                    .width(if hovered() { 1.5 } else { 0.0 })
                    .alignment(BorderAlignment::Inner)
                    .fill(ring_color),
            )
            .shadow(
                Shadow::new()
                    .x(0.0)
                    .y(2.0)
                    .blur(8.0)
                    .color(Color::from_argb(45, 0, 0, 0)),
            )
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| {
                core.invoke_or_log(Command::SpaceCreate { name: None, icon: None });
            })
            .child(Icon::new("plus").size(14.0).role(match theme.scheme {
                ResolvedScheme::Dark => ThemeRole::Text,
                ResolvedScheme::Light => ThemeRole::Surface,
            }));

        rect()
            .width(Size::px(44.0))
            .height(Size::fill())
            .main_align(Alignment::Center)
            .cross_align(Alignment::Center)
            .child(LibraryCursorRing::new(self.focused, badge).corner_radius(999.0))
    }
}

#[derive(Clone, PartialEq)]
struct LibraryTabDragGhost {
    drag: LibraryTabDrag,
    container_origin: (f32, f32),
}

impl Component for LibraryTabDragGhost {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let (badge_bg, badge_fg) = favicon_badge_colors(self.drag.tab_favicon.as_deref(), &theme);
        let icon_name = self.drag.tab_favicon.as_deref().unwrap_or("globe");
        let svg_el: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(icon_name) {
            SvgViewer::new((("zendia-icon", canonical), bytes))
                .width(Size::px(11.5))
                .height(Size::px(11.5))
                .color(badge_fg)
                .show_loader(false)
                .into_element()
        } else {
            Icon::new("globe").size(11.5).role(ThemeRole::Text).into_element()
        };

        let bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(32, 36, 52, 0.94).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(255, 255, 255, 0.96).to_color(),
        };

        let pill = rect()
            .width(Size::px(CARD_WIDTH - 24.0))
            .height(Size::px(TAB_ROW_HEIGHT))
            .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
            .spacing(10.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
            .background(bg)
            .border(
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::Accent)),
            )
            .shadow(
                Shadow::new()
                    .x(0.0)
                    .y(8.0)
                    .blur(18.0)
                    .color(Color::from_argb(90, 0, 0, 0)),
            )
            .child(
                rect()
                    .width(Size::px(18.0))
                    .height(Size::px(18.0))
                    .corner_radius(CornerRadius::new_all(4.5))
                    .background(badge_bg)
                    .center()
                    .child(svg_el),
            )
            .child(
                label()
                    .text(self.drag.tab_title.clone())
                    .width(Size::flex(1.0))
                    .font_size(12.5)
                    .font_weight(FontWeight::MEDIUM)
                    .color(palette.text)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            );

        if self.drag.is_preview {
            rect()
                .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                .position(
                    Position::new_absolute()
                        .left(self.drag.pointer_x)
                        .top(self.drag.pointer_y),
                )
                .child(pill)
        } else {
            let local_x = (self.drag.pointer_x - self.container_origin.0 + 14.0).max(8.0);
            let local_y = (self.drag.pointer_y - self.container_origin.1 + 14.0).max(8.0);
            rect()
                .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                .position(Position::new_absolute().left(local_x).top(local_y))
                .child(pill)
        }
    }
}

#[derive(Clone, PartialEq)]
struct SpaceIconPickerPopover {
    space_id: SpaceId,
    current_icon: String,
    anchor: (f32, f32),
    spaces_state: LibrarySpacesViewState,
}

impl Component for SpaceIconPickerPopover {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let spaces_state = self.spaces_state;
        let space_id = self.space_id;

        let panel_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(26, 28, 38, 0.96).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(252, 252, 255, 0.98).to_color(),
        };

        let clear_core = core.clone();
        let clear_button = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Clear space icon")
            .padding(Gaps::new(4.0, 10.0, 4.0, 10.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(palette.surface_pill)
            .cursor(CursorIcon::Pointer)
            .on_press(move |_| {
                clear_core.invoke_or_log(Command::SpaceSetIcon {
                    space_id,
                    icon: String::new(),
                });
                spaces_state.close_icon_picker();
            })
            .child(
                label()
                    .text("None")
                    .font_size(11.5)
                    .font_weight(FontWeight::MEDIUM)
                    .color(palette.text_muted),
            );

        let mut rows: Vec<Element> = Vec::new();
        for chunk in SELECTABLE_SPACE_ICONS.chunks(6) {
            rows.push(
                rect()
                    .direction(Direction::Horizontal)
                    .spacing(4.0)
                    .children(chunk.iter().map(|icon_name| {
                        SpaceIconTile {
                            space_id,
                            icon_name: (*icon_name).to_string(),
                            selected: self.current_icon == *icon_name,
                            spaces_state,
                            key: DiffKey::from(icon_name),
                        }
                        .into_element()
                    }))
                    .into_element(),
            );
        }

        let card = rect()
            .position(Position::new_global().left(self.anchor.0).top(self.anchor.1))
            .width(Size::px(228.0))
            .padding(Gaps::new_all(12.0))
            .spacing(10.0)
            .corner_radius(CornerRadius::new_all(12.0))
            .background(panel_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .shadow(
                Shadow::new()
                    .x(0.0)
                    .y(10.0)
                    .blur(24.0)
                    .color(Color::from_argb(95, 0, 0, 0)),
            )
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::SpaceBetween)
                    .cross_align(Alignment::Center)
                    .child(
                        label()
                            .text("Choose Icon")
                            .font_size(12.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(palette.text),
                    )
                    .child(clear_button),
            )
            .child(rect().spacing(4.0).children(rows));

        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Escape)) {
                    spaces_state.close_icon_picker();
                }
            })
            .child(
                rect()
                    .position(Position::new_global().left(0.0).top(0.0))
                    .width(Size::window_percent(100.0))
                    .height(Size::window_percent(100.0))
                    .on_mouse_down(move |_| {
                        spaces_state.close_icon_picker();
                    }),
            )
            .child(card)
    }
}

#[derive(Clone, PartialEq)]
struct SpaceIconTile {
    space_id: SpaceId,
    icon_name: String,
    selected: bool,
    spaces_state: LibrarySpacesViewState,
    key: DiffKey,
}

impl KeyExt for SpaceIconTile {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SpaceIconTile {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);

        let space_id = self.space_id;
        let icon_name = self.icon_name.clone();
        let spaces_state = self.spaces_state;

        let bg = if self.selected {
            theme.color(ThemeRole::Accent).with_a(48)
        } else if hovered() {
            palette.surface_pill_hover
        } else {
            Color::TRANSPARENT
        };

        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Icon {}", self.icon_name))
            .width(Size::px(30.0))
            .height(Size::px(30.0))
            .corner_radius(CornerRadius::new_all(7.0))
            .background(bg)
            .maybe(self.selected, |r| {
                r.border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::Accent)),
                )
            })
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| {
                core.invoke_or_log(Command::SpaceSetIcon {
                    space_id,
                    icon: icon_name.clone(),
                });
                spaces_state.close_icon_picker();
            })
            .child(
                Icon::new(self.icon_name.clone())
                    .size(16.0)
                    .role(if self.selected || hovered() {
                        ThemeRole::Text
                    } else {
                        ThemeRole::TextMuted
                    }),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
