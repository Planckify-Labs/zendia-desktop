/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Collapsible tab folders with Zen's 180 ms expand/collapse motion, 300 ms flap `skewX(±16deg)` icon animation
//! (`FLD-03`), custom icons & active dots (`FLD-02`), hover unload button (`FLD-04`), nested/draggable folders
//! (`FLD-05`), and the 250 px searchable collapsed-folder hover popup (`FLD-01`).

use crate::ui::chrome::layout::{ChromePalette, TAB_ROW_GAP, TAB_ROW_HEIGHT, TAB_ROW_RADIUS};
use crate::ui::chrome::menus::{AnchorRect, open_folder_context_menu, try_use_menu_state};
use crate::ui::chrome::sidebar::state::{favicon_badge_colors, try_use_sidebar_state};
use crate::ui::chrome::sidebar::tab_row::{InlineRenameField, VerticalTabRow};
use crate::ui::core_view::use_core;
use crate::ui::kit::icons::prepared_icon_bytes;
use crate::ui::kit::{Icon, KitTooltip, TextField};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, Rgba, ThemeRole, motion, use_theme};
use crate::ui::vim::explorer::{apply_explorer_background, apply_explorer_border};
use crate::ui::vim::hints::hint_badge_for_row;
use freya::animation::*;
use freya::engine::prelude as sk;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::focus::FocusTarget;
use zendia_browser_core::model::{Folder, Tab, TabId, TabMoveTarget};

/// Folder expand/collapse duration matching Zen's `ZenFolders.mjs` (`0.18s`).
pub const FOLDER_TOGGLE_DURATION: Duration = Duration::from_millis(180);
/// Horizontal indentation of tabs nested inside a folder (matching `--zen-folder-indent: 14px`).
pub const FOLDER_INDENT_PX: f32 = 14.0;
const FOLDER_CHILD_SPACING: f32 = TAB_ROW_GAP;
const FOLDER_POPUP_WIDTH: f32 = 250.0;
const FOLDER_POPUP_HOVER_DELAY: Duration = Duration::from_millis(240);
const FOLDER_POPUP_LEAVE_GRACE: Duration = Duration::from_millis(140);
const DRAG_THRESHOLD_PX: f32 = 6.0;

fn is_custom_folder_icon(icon: &str) -> bool {
    let trimmed = icon.trim();
    !trimmed.is_empty() && trimmed != "folder" && trimmed != "folder-open"
}

fn is_emoji_icon(icon: &str) -> bool {
    !icon.is_ascii()
}

/// One collapsible folder row and its animated nested tab list.
#[derive(Clone, PartialEq)]
pub struct FolderGroup {
    pub folder: Folder,
    pub tabs: Vec<Tab>,
    pub active_tab: Option<TabId>,
    pub collapsed_sidebar: bool,
    pub indent: f32,
    pub folder_index: usize,
    key: DiffKey,
}

impl KeyExt for FolderGroup {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl FolderGroup {
    pub fn new(folder: Folder, tabs: Vec<Tab>, active_tab: Option<TabId>, collapsed_sidebar: bool) -> Self {
        let key = DiffKey::from(&folder.id.0);
        Self {
            folder,
            tabs,
            active_tab,
            collapsed_sidebar,
            indent: 0.0,
            folder_index: 0,
            key,
        }
    }

    pub fn with_nesting(mut self, indent: f32, folder_index: usize) -> Self {
        self.indent = indent;
        self.folder_index = folder_index;
        self
    }
}

impl Component for FolderGroup {
    fn render(&self) -> impl IntoElement {
        let is_expanded = !self.folder.collapsed;
        let active_child = self.active_tab.and_then(|active_id| {
            self.tabs
                .iter()
                .enumerate()
                .find(|(_, tab)| tab.id == active_id)
                .map(|(idx, tab)| (idx, tab.clone()))
        });

        let expand_anim = use_animation_with_dependencies(&is_expanded, move |conf, expanded| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *expanded { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(FOLDER_TOGGLE_DURATION)
                .ease(Ease::InOut)
                .function(motion::ease_out_fn())
        });

        let factor = expand_anim.get().value().clamp(0.0, 1.0);
        let folder_id = self.folder.id;
        let active_tab = self.active_tab;
        let collapsed_sidebar = self.collapsed_sidebar;
        let child_indent = if collapsed_sidebar {
            0.0
        } else {
            self.indent + FOLDER_INDENT_PX
        };

        let full_height = if self.tabs.is_empty() {
            0.0
        } else {
            self.tabs.len() as f32 * TAB_ROW_HEIGHT
                + (self.tabs.len().saturating_sub(1)) as f32 * FOLDER_CHILD_SPACING
                + 2.0
        };

        let header = FolderHeaderRow {
            folder: self.folder.clone(),
            tabs: self.tabs.clone(),
            active_tab,
            has_active_child: active_child.is_some(),
            collapsed_sidebar,
            indent: if collapsed_sidebar { 0.0 } else { self.indent },
            folder_index: self.folder_index,
        };

        let children_section: Option<Element> = if factor > 0.01 {
            let current_height = (full_height * factor).max(0.0);
            let rows = self.tabs.iter().enumerate().map(|(idx, tab)| {
                VerticalTabRow::new(
                    tab.clone(),
                    active_tab == Some(tab.id),
                    collapsed_sidebar,
                    child_indent,
                    TabMoveTarget::Folder { folder_id, index: idx },
                )
                .into_element()
            });
            Some(
                rect()
                    .width(Size::fill())
                    .height(Size::px(current_height))
                    .overflow(Overflow::Clip)
                    .padding(Gaps::new(2.0, 0.0, 0.0, 0.0))
                    .spacing(FOLDER_CHILD_SPACING)
                    .opacity(factor)
                    .children(rows)
                    .into_element(),
            )
        } else if let Some((idx, active_tab_item)) = active_child {
            // Zen behavior: a collapsed folder that contains the active tab keeps that active tab visible below it.
            Some(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(2.0, 0.0, 0.0, 0.0))
                    .child(VerticalTabRow::new(
                        active_tab_item,
                        true,
                        collapsed_sidebar,
                        child_indent,
                        TabMoveTarget::Folder { folder_id, index: idx },
                    ))
                    .into_element(),
            )
        } else {
            None
        };

        rect().width(Size::fill()).child(header).maybe_child(children_section)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct FolderHeaderRow {
    folder: Folder,
    tabs: Vec<Tab>,
    active_tab: Option<TabId>,
    has_active_child: bool,
    collapsed_sidebar: bool,
    indent: f32,
    folder_index: usize,
}

impl Component for FolderHeaderRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let menu_state = try_use_menu_state();
        let sidebar_state = try_use_sidebar_state();
        let mut hovered = use_state(|| false);
        let mut unload_hovered = use_state(|| false);
        let mut press_origin = use_state(|| None::<(f32, f32)>);
        let mut header_anchor = use_state(|| AnchorRect {
            x: 0.0,
            y: 0.0,
            width: 180.0,
            height: TAB_ROW_HEIGHT,
        });
        let mut popup_open = use_state(|| false);
        let popup_hovered = use_state(|| false);
        let mut popup_seq = use_state(|| 0u64);
        let mut search_query = use_state(String::new);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let folder_id = self.folder.id;
        let folder_index = self.folder_index;
        let folder_collapsed = self.folder.collapsed;
        let tab_count = self.tabs.len();
        let is_renaming = sidebar_state.is_some_and(|s| *s.renaming_folder.read() == Some(folder_id));

        if (!folder_collapsed || is_renaming || tab_count == 0) && *popup_open.peek() {
            popup_open.set(false);
        }

        let is_drag_target = sidebar_state.is_some_and(|s| {
            matches!(
                s.drag_state.read().as_ref().and_then(|d| d.target),
                Some(TabMoveTarget::Folder { folder_id: target_id, .. }) if target_id == folder_id
            ) || s
                .dragging_folder
                .read()
                .as_ref()
                .is_some_and(|(fid, _, _, _, tgt)| *fid != folder_id && *tgt == Some(folder_index))
        });

        let is_being_dragged = sidebar_state.is_some_and(|s| {
            s.dragging_folder
                .read()
                .as_ref()
                .is_some_and(|(fid, _, _, _, _)| *fid == folder_id)
        });

        let toggle_core = core.clone();
        let on_press = move |_: Event<PressEventData>| {
            if is_renaming {
                return;
            }
            popup_open.set_if_modified(false);
            toggle_core.invoke_or_log(Command::FolderToggleCollapsed { folder_id });
        };

        let menu_core = core.clone();
        let on_mouse_down = move |e: Event<MouseEventData>| {
            if e.button == Some(MouseButton::Right) {
                e.stop_propagation();
                popup_open.set_if_modified(false);
                if let Some(menu_state) = &menu_state {
                    let model = menu_core.model().peek().clone();
                    let position = (e.global_location.x as f32, e.global_location.y as f32);
                    open_folder_context_menu(menu_state, &model, folder_id, position);
                }
            }
        };

        // FLD-05 / DND-01: Drag initiation on FolderHeaderRow to reorder folders.
        let drag_name = self.folder.name.clone();
        let on_pointer_down = move |e: Event<PointerEventData>| {
            if !is_renaming && e.button() == Some(MouseButton::Left) {
                press_origin.set(Some((e.global_location().x as f32, e.global_location().y as f32)));
            }
        };
        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            let Some((ox, oy)) = *press_origin.peek() else {
                return;
            };
            let px = e.global_location().x as f32;
            let py = e.global_location().y as f32;
            let dist = ((px - ox).powi(2) + (py - oy).powi(2)).sqrt();
            if let Some(state) = sidebar_state {
                let mut df = state.dragging_folder;
                let dragging_this = df.peek().as_ref().is_some_and(|(fid, _, _, _, _)| *fid == folder_id);
                if dragging_this {
                    if let Some(entry) = df.write().as_mut() {
                        entry.2 = px;
                        entry.3 = py;
                    }
                } else if dist >= DRAG_THRESHOLD_PX {
                    popup_open.set_if_modified(false);
                    df.set(Some((folder_id, drag_name.clone(), px, py, Some(folder_index))));
                }
            }
        };
        let reorder_core = core.clone();
        let on_global_pointer_up = move |_: Event<PointerEventData>| {
            if press_origin.peek().is_none() {
                return;
            }
            press_origin.set(None);
            if let Some(state) = sidebar_state {
                let finished = {
                    let mut df = state.dragging_folder;
                    if df.peek().as_ref().is_some_and(|(fid, _, _, _, _)| *fid == folder_id) {
                        df.write().take()
                    } else {
                        None
                    }
                };
                if let Some((fid, _, _, _, Some(target_idx))) = finished
                    && target_idx != folder_index
                {
                    reorder_core.invoke_or_log(Command::FolderReorder {
                        folder_id: fid,
                        target_index: target_idx,
                        parent_folder_id: None,
                    });
                }
            }
        };

        let icon_color = if self.has_active_child || !self.folder.collapsed || hovered() {
            palette.text
        } else {
            palette.text_muted
        };

        let (bg, border, hint_badge, has_loaded_child) = {
            let model = core.model().read();
            let base_bg = if is_renaming {
                palette.tab_selected_bg
            } else if hovered() || popup_open() {
                palette.tab_hover_bg
            } else {
                Color::TRANSPARENT
            };
            let base_border = if is_drag_target {
                Border::new()
                    .width(1.25)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::Accent))
            } else if focus() == Focus::Keyboard {
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::FocusRing))
            } else {
                Border::new().width(0.0).fill(Color::TRANSPARENT)
            };
            let focus_target = FocusTarget::Folder(folder_id);
            let any_loaded = self.tabs.iter().any(|t| !model.is_tab_unloaded(t.id));
            (
                apply_explorer_background(base_bg, &model, &focus_target, &theme),
                apply_explorer_border(base_border, &model, &focus_target, &theme),
                hint_badge_for_row(&model, &focus_target, TAB_ROW_HEIGHT, &theme),
                any_loaded,
            )
        };

        // FLD-02 & FLD-03: Custom emoji/SVG icon when set, else animated 300ms skewX(±16deg) folder icon with active dots.
        let folder_icon = AnimatedFolderIcon {
            custom_icon: self.folder.icon.clone(),
            is_open: !self.folder.collapsed,
            show_active_dots: self.folder.collapsed
                && (self.has_active_child || has_loaded_child && !self.tabs.is_empty()),
            color: icon_color,
            key: DiffKey::from(&("folder-icon", folder_id.0)),
        };

        let state_label = if self.folder.collapsed {
            format!("Folder {} (collapsed, {} tabs)", self.folder.name, tab_count)
        } else {
            format!("Folder {} (expanded, {} tabs)", self.folder.name, tab_count)
        };

        let on_enter_row = move |_| {
            hovered.set_if_modified(true);
            if let Some(state) = sidebar_state {
                state.set_drag_target(TabMoveTarget::Folder { folder_id, index: 0 });
                let mut df = state.dragging_folder;
                if df.peek().as_ref().is_some_and(|(fid, _, _, _, _)| *fid != folder_id)
                    && let Some(entry) = df.write().as_mut()
                {
                    entry.4 = Some(folder_index);
                }
            }
            if folder_collapsed && tab_count > 0 && !is_renaming {
                let seq = popup_seq.peek().wrapping_add(1);
                popup_seq.set(seq);
                spawn(async move {
                    timer(FOLDER_POPUP_HOVER_DELAY).await;
                    if *popup_seq.peek() == seq && *hovered.peek() {
                        search_query.set(String::new());
                        popup_open.set_if_modified(true);
                    }
                });
            }
        };

        let on_leave_row = move |_| {
            hovered.set_if_modified(false);
            let seq = popup_seq.peek().wrapping_add(1);
            popup_seq.set(seq);
            spawn(async move {
                timer(FOLDER_POPUP_LEAVE_GRACE).await;
                if *popup_seq.peek() == seq && !*hovered.peek() && !*popup_hovered.peek() {
                    popup_open.set_if_modified(false);
                }
            });
        };

        // FLD-01: 250px searchable collapsed-folder hover tabs popup (`#zen-folder-tabs-popup`).
        let tabs_popup = (popup_open() && folder_collapsed && tab_count > 0).then(|| FolderTabsPopup {
            tabs: self.tabs.clone(),
            active_tab: self.active_tab,
            anchor: *header_anchor.read(),
            query: search_query,
            popup_open,
            popup_hovered,
            popup_seq,
            header_hovered: hovered,
        });

        if self.collapsed_sidebar {
            return rect()
                .width(Size::fill())
                .center()
                .child(
                    rect()
                        .a11y_id(a11y_id)
                        .a11y_focusable(true)
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt(state_label)
                        .width(Size::px(38.0))
                        .height(Size::px(TAB_ROW_HEIGHT))
                        .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
                        .background(bg)
                        .border(border)
                        .opacity(if is_being_dragged { 0.42 } else { 1.0 })
                        .center()
                        .cursor(CursorIcon::Pointer)
                        .on_sized(move |e: Event<SizedEventData>| {
                            let rect = AnchorRect::from_area(e.area);
                            header_anchor.set_if_modified(rect);
                            if let Some(state) = sidebar_state {
                                state.set_folder_anchor(folder_id, rect);
                            }
                        })
                        .on_pointer_enter(on_enter_row)
                        .on_pointer_leave(on_leave_row)
                        .on_pointer_down(on_pointer_down)
                        .on_global_pointer_move(on_global_pointer_move)
                        .on_global_pointer_up(on_global_pointer_up)
                        .on_press(on_press)
                        .on_mouse_down(on_mouse_down)
                        .child(folder_icon)
                        .maybe_child(hint_badge),
                )
                .maybe_child(tabs_popup)
                .into_element();
        }

        // FLD-04: Hover Reset/Unload active tabs button when any child tab is loaded.
        let unload_core = core.clone();
        let unload_button = (hovered() && !is_renaming && has_loaded_child).then(|| {
            KitTooltip::new("Unload folder tabs").child(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Unload folder tabs")
                    .width(Size::px(20.0))
                    .height(Size::px(20.0))
                    .corner_radius(CornerRadius::new_all(5.0))
                    .background(if unload_hovered() {
                        palette.surface_pill
                    } else {
                        Color::TRANSPARENT
                    })
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| unload_hovered.set_if_modified(true))
                    .on_pointer_leave(move |_| unload_hovered.set_if_modified(false))
                    .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        unload_core.invoke_or_log(Command::FolderUnloadTabs { folder_id });
                    })
                    .child(
                        rect()
                            .width(Size::px(10.0))
                            .height(Size::px(1.5))
                            .corner_radius(CornerRadius::new_all(1.0))
                            .background(if unload_hovered() {
                                palette.text
                            } else {
                                palette.text_muted
                            }),
                    ),
            )
        });

        let center_el: Element = if is_renaming && let Some(state) = sidebar_state {
            InlineRenameField {
                draft: state.rename_draft,
                placeholder: "Folder name",
                on_commit: EventHandler::new({
                    let core = core.clone();
                    move |text: String| {
                        state.cancel_rename();
                        let trimmed = text.trim().to_string();
                        if !trimmed.is_empty() {
                            core.invoke_or_log(Command::FolderRename {
                                folder_id,
                                name: trimmed,
                            });
                        }
                    }
                }),
                on_cancel: EventHandler::new(move |_| {
                    state.cancel_rename();
                }),
                key: DiffKey::from(&("folder-rename", folder_id.0)),
            }
            .into_element()
        } else {
            label()
                .text(self.folder.name.clone())
                .width(Size::flex(1.0))
                .font_size(12.5)
                .font_weight(FontWeight::SEMI_BOLD)
                .color(palette.text)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .into_element()
        };

        rect()
            .width(Size::fill())
            .padding(Gaps::new(0.0, 0.0, 0.0, self.indent))
            .child(
                rect()
                    .a11y_id(a11y_id)
                    .a11y_focusable(!is_renaming)
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(state_label)
                    .width(Size::fill())
                    .height(Size::px(TAB_ROW_HEIGHT))
                    .padding(Gaps::new(0.0, 6.0, 0.0, 8.0))
                    .spacing(8.0)
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
                    .background(bg)
                    .border(border)
                    .opacity(if is_being_dragged { 0.42 } else { 1.0 })
                    .cursor(CursorIcon::Pointer)
                    .on_sized(move |e: Event<SizedEventData>| {
                        let rect = AnchorRect::from_area(e.area);
                        header_anchor.set_if_modified(rect);
                        if let Some(state) = sidebar_state {
                            state.set_folder_anchor(folder_id, rect);
                        }
                    })
                    .on_pointer_enter(on_enter_row)
                    .on_pointer_leave(on_leave_row)
                    .on_pointer_down(on_pointer_down)
                    .on_global_pointer_move(on_global_pointer_move)
                    .on_global_pointer_up(on_global_pointer_up)
                    .on_press(on_press)
                    .on_mouse_down(on_mouse_down)
                    .child(folder_icon)
                    .child(center_el)
                    .maybe_child(unload_button)
                    .maybe_child(hint_badge),
            )
            .maybe_child(tabs_popup)
            .into_element()
    }
}

/// FLD-02 & FLD-03: Animated folder icon with 300ms `skewX(±16deg)` back/front flaps, custom icon support, and
/// active-tab dots morph when collapsed.
#[derive(Clone, PartialEq)]
struct AnimatedFolderIcon {
    custom_icon: String,
    is_open: bool,
    show_active_dots: bool,
    color: Color,
    key: DiffKey,
}

impl KeyExt for AnimatedFolderIcon {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for AnimatedFolderIcon {
    fn render(&self) -> impl IntoElement {
        let open_anim = use_animation_with_dependencies(&self.is_open, move |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *open { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::FOLDER_FLAP)
                .ease(Ease::Out)
                .function(Function::Cubic)
        });

        let open_t = open_anim.get().value().clamp(0.0, 1.0);
        let color = self.color;
        let show_dots = self.show_active_dots && open_t < 0.25;

        if is_custom_folder_icon(&self.custom_icon) {
            let inner: Element = if is_emoji_icon(&self.custom_icon) {
                label()
                    .text(self.custom_icon.clone())
                    .font_size(13.5)
                    .color(color)
                    .into_element()
            } else {
                Icon::new(self.custom_icon.clone())
                    .size(14.0)
                    .role(ThemeRole::Text)
                    .into_element()
            };
            return rect()
                .width(Size::px(18.0))
                .height(Size::px(18.0))
                .center()
                .child(inner)
                .into_element();
        }

        let sk_stroke = sk::Color::from_argb(color.a(), color.r(), color.g(), color.b());
        let sk_fill = sk::Color::from_argb(
            ((color.a() as f32) * (0.08 + 0.14 * open_t)).round() as u8,
            color.r(),
            color.g(),
            color.b(),
        );

        rect()
            .width(Size::px(18.0))
            .height(Size::px(18.0))
            .interactive(Interactive::No)
            .background(Fill::Callback(RenderCallback::new(move |ctx| {
                let RenderTarget::Background { canvas, .. } = &mut ctx.target else {
                    return;
                };
                let w = ctx.size.width;
                let h = ctx.size.height;
                if w <= 1.0 || h <= 1.0 {
                    return;
                }
                let sx = w / 20.0;
                let sy = h / 20.0;
                canvas.save();
                canvas.scale((sx, sy));

                let mut stroke = sk::Paint::default();
                stroke.set_anti_alias(true);
                stroke.set_style(sk::PaintStyle::Stroke);
                stroke.set_stroke_width(1.45);
                stroke.set_stroke_cap(sk::PaintCap::Round);
                stroke.set_stroke_join(sk::PaintJoin::Round);
                stroke.set_color(sk_stroke);

                let mut fill = sk::Paint::default();
                fill.set_anti_alias(true);
                fill.set_style(sk::PaintStyle::Fill);
                fill.set_color(sk_fill);

                // Back folder plate: skewX(+16deg * open_t) matching zen-folder-icon.inc.css:13-15.
                // tan(16 deg) = 0.2867454 (precomputed constant to avoid platform libm variance).
                const TAN_16_DEG: f32 = 0.286_745_4;
                let skew_tan = TAN_16_DEG * open_t;
                canvas.save();
                canvas.translate((-1.2 * open_t, 0.8 * open_t));
                canvas.skew((skew_tan * 0.45, 0.0));
                let mut back_pb = sk::PathBuilder::new();
                back_pb.move_to(sk::Point::new(3.0, 5.2));
                back_pb.line_to(sk::Point::new(3.0, 15.2));
                back_pb.line_to(sk::Point::new(16.8, 15.2));
                back_pb.line_to(sk::Point::new(16.8, 6.8));
                back_pb.line_to(sk::Point::new(10.8, 6.8));
                back_pb.line_to(sk::Point::new(9.0, 4.6));
                back_pb.line_to(sk::Point::new(4.2, 4.6));
                back_pb.close();
                let back_path = back_pb.detach();
                canvas.draw_path(&back_path, &stroke);
                canvas.restore();

                // Front folder flap: skewX(-16deg * open_t) matching zen-folder-icon.inc.css:17-19.
                canvas.save();
                canvas.translate((2.6 * open_t, 1.2 * open_t));
                canvas.skew((-skew_tan, 0.0));
                let front_rr = sk::RRect::new_rect_xy(sk::Rect::from_xywh(3.0, 7.6, 13.8, 7.8), 1.6, 1.6);
                canvas.draw_rrect(front_rr, &fill);
                canvas.draw_rrect(front_rr, &stroke);

                // FLD-02: Active-tab indicator dots morph on collapsed folder with active/loaded tabs.
                if show_dots {
                    let mut dot_paint = sk::Paint::default();
                    dot_paint.set_anti_alias(true);
                    dot_paint.set_style(sk::PaintStyle::Fill);
                    dot_paint.set_color(sk_stroke);
                    for dx in [6.8_f32, 9.9, 13.0] {
                        canvas.draw_circle(sk::Point::new(dx, 11.5), 1.1, &dot_paint);
                    }
                }
                canvas.restore();
                canvas.restore();
            })))
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// FLD-01: 250px searchable hover popup listing all tabs inside a collapsed folder (`#zen-folder-tabs-popup`).
#[derive(Clone, PartialEq)]
struct FolderTabsPopup {
    tabs: Vec<Tab>,
    active_tab: Option<TabId>,
    anchor: AnchorRect,
    query: State<String>,
    popup_open: State<bool>,
    popup_hovered: State<bool>,
    popup_seq: State<u64>,
    header_hovered: State<bool>,
}

impl Component for FolderTabsPopup {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let search_a11y = use_hook(AccessibilityId::new_unique);

        let query_text = self.query.read().trim().to_ascii_lowercase();
        let filtered: Vec<&Tab> = self
            .tabs
            .iter()
            .filter(|tab| {
                query_text.is_empty()
                    || tab.display_title().to_ascii_lowercase().contains(&query_text)
                    || tab.url.to_ascii_lowercase().contains(&query_text)
            })
            .collect();

        let root_size = *Platform::get().current_window().root_size.read();
        let win_w = root_size.width.max(400.0);
        let win_h = root_size.height.max(300.0);
        let est_h = (42.0 + (filtered.len().max(1) as f32) * 44.0 + 12.0).min(310.0);
        // Prefer placing beside the sidebar folder row when space allows.
        let anchor_right = self.anchor.x + self.anchor.width;
        let side_x = if anchor_right + 6.0 + FOLDER_POPUP_WIDTH <= win_w - 8.0 {
            anchor_right + 6.0
        } else if self.anchor.x - 6.0 - FOLDER_POPUP_WIDTH >= 8.0 {
            self.anchor.x - 6.0 - FOLDER_POPUP_WIDTH
        } else {
            self.anchor.x.clamp(8.0, (win_w - FOLDER_POPUP_WIDTH - 8.0).max(8.0))
        };
        let side_y = self.anchor.y.clamp(8.0, (win_h - est_h - 8.0).max(8.0));

        let panel_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(26, 28, 38, 0.98).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(252, 252, 255, 0.98).to_color(),
        };

        let mut popup_open = self.popup_open;
        let mut popup_hovered = self.popup_hovered;
        let mut popup_seq = self.popup_seq;
        let header_hovered = self.header_hovered;
        let active_tab = self.active_tab;

        let rows: Vec<Element> = if filtered.is_empty() {
            vec![
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(16.0, 8.0, 16.0, 8.0))
                    .center()
                    .child(
                        label()
                            .text("No matching tabs")
                            .font_size(11.5)
                            .color(palette.text_muted),
                    )
                    .into_element(),
            ]
        } else {
            filtered
                .into_iter()
                .map(|tab| {
                    let tab_id = tab.id;
                    let is_active = active_tab == Some(tab_id);
                    let click_core = core.clone();
                    let (badge_bg, badge_fg) = favicon_badge_colors(tab.favicon.as_deref(), &theme);
                    let icon_name = tab.favicon.as_deref().unwrap_or("globe");
                    let icon_el: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(icon_name) {
                        SvgViewer::new((("zendia-icon", canonical), bytes))
                            .width(Size::px(11.0))
                            .height(Size::px(11.0))
                            .color(badge_fg)
                            .show_loader(false)
                            .into_element()
                    } else {
                        Icon::new("globe").size(11.0).role(ThemeRole::TextMuted).into_element()
                    };
                    rect()
                        .key(tab_id.0)
                        .a11y_role(AccessibilityRole::MenuItem)
                        .a11y_alt(tab.display_title())
                        .width(Size::fill())
                        .height(Size::px(40.0))
                        .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                        .spacing(8.0)
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::center())
                        .corner_radius(CornerRadius::new_all(6.0))
                        .background(if is_active {
                            palette.tab_selected_bg
                        } else {
                            Color::TRANSPARENT
                        })
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            popup_open.set(false);
                            click_core.invoke_or_log(Command::TabActivate { tab_id });
                        })
                        .child(
                            rect()
                                .width(Size::px(18.0))
                                .height(Size::px(18.0))
                                .corner_radius(CornerRadius::new_all(4.5))
                                .background(badge_bg)
                                .center()
                                .child(icon_el),
                        )
                        .child(
                            rect()
                                .width(Size::flex(1.0))
                                .spacing(1.0)
                                .child(
                                    label()
                                        .text(tab.display_title())
                                        .font_size(12.0)
                                        .font_weight(FontWeight::MEDIUM)
                                        .color(palette.text)
                                        .max_lines(1)
                                        .text_overflow(TextOverflow::Ellipsis),
                                )
                                .child(
                                    label()
                                        .text(tab.url.clone())
                                        .font_size(10.0)
                                        .color(palette.text_muted)
                                        .max_lines(1)
                                        .text_overflow(TextOverflow::Ellipsis),
                                ),
                        )
                        .into_element()
                })
                .collect()
        };

        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(side_x).top(side_y))
            .width(Size::px(FOLDER_POPUP_WIDTH))
            .corner_radius(CornerRadius::new_all(10.0))
            .background(panel_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .shadow(Shadow::new().y(6.0).blur(20.0).color(Color::from_argb(95, 0, 0, 0)))
            .overflow(Overflow::Clip)
            .on_pointer_enter(move |_| popup_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| {
                popup_hovered.set_if_modified(false);
                let seq = popup_seq.peek().wrapping_add(1);
                popup_seq.set(seq);
                spawn(async move {
                    timer(FOLDER_POPUP_LEAVE_GRACE).await;
                    if *popup_seq.peek() == seq && !*popup_hovered.peek() && !*header_hovered.peek() {
                        popup_open.set_if_modified(false);
                    }
                });
            })
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Escape)) {
                    popup_open.set_if_modified(false);
                }
            })
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(36.0))
                    .padding(Gaps::new(0.0, 8.0, 0.0, 10.0))
                    .spacing(6.0)
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .child(Icon::new("search").size(13.0).role(ThemeRole::TextMuted))
                    .child(
                        TextField::new(self.query)
                            .a11y_id(search_a11y)
                            .placeholder("Search tabs...")
                            .width(Size::flex(1.0))
                            .color(palette.text)
                            .placeholder_color(palette.text_muted),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(1.0))
                    .background(palette.divider),
            )
            .child(
                ScrollView::new().width(Size::fill()).max_height(Size::px(263.0)).child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new_all(4.0))
                        .spacing(2.0)
                        .children(rows),
                ),
            )
    }
}
