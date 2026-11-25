/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! What the page card shows: the active tab, or every pane of the active split group, with resize handles, the drop
//! zones a dragged tab can land on, Zendia's tab switch motion and Glance on top.
//!
//! Pane geometry comes from [`zendia_browser_core::split`]; this view adds gaps, rings and motion. Resizing, focusing
//! a pane and dropping a tab are commands. Only drag progress and animation stay here.

use crate::ui::chrome::menus::{open_tab_context_menu, try_use_menu_state};
use crate::ui::chrome::sidebar::{PageDrop, try_use_sidebar_state};
use crate::ui::core_view::use_core;
use crate::ui::page::glance::{GLANCE_BACKGROUND_OPACITY, GLANCE_BACKGROUND_SCALE, GlanceLayer, glance_spring};
use crate::ui::page::surface::PageSurface;
use crate::ui::theme::{ThemeRole, motion, use_theme};
use crate::ui::vim::window_mode::{WindowModeOverlay, active_pane_ring_role};
use freya::animation::*;
use freya::prelude::*;
use std::time::Instant;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{SplitDirection, SplitGroupId, TabId};
use zendia_browser_core::split::{self, DropZone, HandleAxis, PaneRect};

/// Space between split panes, as in Zen.
pub const SPLIT_GAP: f32 = 6.0;
const PANE_RADIUS: f32 = 8.0;
const ACTIVE_RING_WIDTH: f32 = 2.0;
const HANDLE_HIT: f32 = 10.0;

/// Whether panes show live Hub views or stand-in pages (the gallery has no Hub).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneMode {
    Live,
    Placeholder,
}

/// The page card's contents.
#[derive(Clone, PartialEq)]
pub struct PageArea {
    mode: PaneMode,
    glance_progress: Option<f32>,
    highlight_handle: bool,
    key: DiffKey,
}

impl KeyExt for PageArea {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl PageArea {
    pub fn new(mode: PaneMode) -> Self {
        Self {
            mode,
            glance_progress: None,
            highlight_handle: false,
            key: DiffKey::None,
        }
    }

    /// Pins Glance's open animation at `progress` (for gallery states).
    pub fn glance_progress(mut self, progress: f32) -> Self {
        self.glance_progress = Some(progress);
        self
    }

    /// Draws the first resize handle as if it were being dragged (for gallery states).
    pub fn highlight_handle(mut self, highlight: bool) -> Self {
        self.highlight_handle = highlight;
        self
    }
}

/// One pane on screen: its tab and where it sits, in fractions of the page card.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pane {
    tab_id: TabId,
    rect: PaneRect,
}

fn pane_pixels(rect: PaneRect, size: (f32, f32), in_split: bool) -> (f32, f32, f32, f32) {
    let (width, height) = size;
    if !in_split {
        return (0.0, 0.0, width, height);
    }
    let half = SPLIT_GAP / 2.0;
    let left = rect.x * width + if rect.x > 0.0 { half } else { 0.0 };
    let top = rect.y * height + if rect.y > 0.0 { half } else { 0.0 };
    let right = (rect.x + rect.width) * width - if rect.x + rect.width < 0.999 { half } else { 0.0 };
    let bottom = (rect.y + rect.height) * height - if rect.y + rect.height < 0.999 { half } else { 0.0 };
    (left, top, (right - left).max(0.0), (bottom - top).max(0.0))
}

/// The side of a pane a drop zone covers, in pixels.
fn zone_pixels(pane: (f32, f32, f32, f32), zone: DropZone) -> (f32, f32, f32, f32) {
    let (x, y, w, h) = pane;
    match zone {
        DropZone::Left => (x, y, w / 2.0, h),
        DropZone::Right => (x + w / 2.0, y, w / 2.0, h),
        DropZone::Top => (x, y, w, h / 2.0),
        DropZone::Bottom => (x, y + h / 2.0, w, h / 2.0),
    }
}

impl Component for PageArea {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let sidebar_state = try_use_sidebar_state();
        let mut area = use_state(|| (0.0f32, 0.0f32, 0.0f32, 0.0f32));
        let resize_preview = use_state(|| None::<(SplitGroupId, Vec<f32>)>);
        let mut last_key = use_state(|| None::<(bool, u64)>);
        let mut last_active = use_state(|| None::<TabId>);
        let mut last_switch = use_state(|| None::<Instant>);
        let mut slide = use_state(|| (0u64, 0.0f32));

        let (panes, group, active_tab, order, motion_enabled, glance) = {
            let model = core.model().read();
            let group = model.active_split_group().cloned();
            let panes: Vec<Pane> = match &group {
                Some(group) => {
                    let ratios = match &*resize_preview.read() {
                        Some((id, ratios)) if *id == group.id => ratios.clone(),
                        _ => group.ratios.clone(),
                    };
                    split::pane_rects(group.direction, group.tab_ids.len(), &ratios)
                        .into_iter()
                        .zip(group.tab_ids.iter().copied())
                        .map(|(rect, tab_id)| Pane { tab_id, rect })
                        .collect()
                }
                None => model
                    .active_tab
                    .map(|tab_id| Pane {
                        tab_id,
                        rect: PaneRect {
                            x: 0.0,
                            y: 0.0,
                            width: 1.0,
                            height: 1.0,
                        },
                    })
                    .into_iter()
                    .collect(),
            };
            (
                panes,
                group,
                model.active_tab,
                model.tabs_in_active_space_order(),
                model.layout.tab_switch_motion,
                model.glance.clone(),
            )
        };

        // Zendia's tab switch motion: the page slides in from below when the new tab sits below the old one in the
        // sidebar, and from above otherwise. Switching a group's panes or switching fast (key repeat) does not move.
        let shown_key = group
            .as_ref()
            .map(|g| (true, g.id.0))
            .or(active_tab.map(|t| (false, t.0)));
        if *last_key.peek() != shown_key {
            let now = Instant::now();
            let repeat = last_switch
                .peek()
                .is_some_and(|at| now.duration_since(at) < motion::TAB_REPEAT);
            let previous = *last_active.peek();
            if motion_enabled
                && !repeat
                && last_key.peek().is_some()
                && let (Some(from), Some(to)) = (previous, active_tab)
                && let (Some(from_index), Some(to_index)) = (
                    order.iter().position(|id| *id == from),
                    order.iter().position(|id| *id == to),
                )
                && from_index != to_index
            {
                let direction = if to_index > from_index { 1.0 } else { -1.0 };
                let seq = slide.peek().0 + 1;
                slide.set((seq, direction));
            }
            last_switch.set(Some(now));
            last_key.set(shown_key);
        }
        if *last_active.peek() != active_tab {
            last_active.set(active_tab);
        }

        let (slide_seq, slide_direction) = *slide.read();
        let slide_anim = use_animation_with_dependencies(&slide_seq, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(motion::TAB_SLIDE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let slide_t = slide_anim.get().value().clamp(0.0, 1.0);
        let slide_offset = slide_direction * motion::TAB_SLIDE_PX * (1.0 - slide_t);
        let fade_t = (slide_t * motion::TAB_SLIDE.as_secs_f32() / motion::TAB_FADE.as_secs_f32()).min(1.0);
        let slide_opacity = motion::TAB_FADE_FROM + (1.0 - motion::TAB_FADE_FROM) * fade_t;

        let glance_key = glance.as_ref().map(|g| g.tab_id.0);
        let glance_anim = use_animation_with_dependencies(&glance_key, move |conf, _| {
            conf.on_creation(OnCreation::Run);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(crate::ui::page::glance::GLANCE_OPEN)
                .ease(Ease::Out)
                .function(glance_spring())
        });
        let glance_t = match (&glance, self.glance_progress) {
            (None, _) => 0.0,
            (Some(_), Some(pinned)) => pinned,
            (Some(_), None) => glance_anim.get().value(),
        };
        let behind = glance_t.clamp(0.0, 1.0);
        let behind_scale = 1.0 - (1.0 - GLANCE_BACKGROUND_SCALE) * behind;
        let behind_opacity = 1.0 - (1.0 - GLANCE_BACKGROUND_OPACITY) * behind;

        let (origin_x, origin_y, width, height) = *area.read();
        let size = (width, height);
        let in_split = group.is_some();

        // While a sidebar tab is dragged over the page, work out which pane side it would land on.
        let drag_panes = panes.clone();
        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            let Some(state) = sidebar_state else {
                return;
            };
            let mut drag = state.drag_state;
            let Some(current) = drag.peek().clone() else {
                return;
            };
            if current.is_preview {
                return;
            }
            let (ox, oy, w, h) = *area.peek();
            let px = e.global_location().x as f32 - ox;
            let py = e.global_location().y as f32 - oy;
            let inside = w > 0.0 && h > 0.0 && (0.0..=w).contains(&px) && (0.0..=h).contains(&py);
            let next = inside
                .then(|| {
                    drag_panes.iter().find_map(|pane| {
                        let (x, y, pw, ph) = pane_pixels(pane.rect, (w, h), drag_panes.len() > 1);
                        let hit = px >= x && px <= x + pw && py >= y && py <= y + ph;
                        (hit && pane.tab_id != current.tab_id).then(|| PageDrop {
                            target_tab: pane.tab_id,
                            zone: split::drop_zone_at((px - x) / pw.max(1.0), (py - y) / ph.max(1.0)),
                        })
                    })
                })
                .flatten();
            if current.page_drop != next
                && let Some(current) = drag.write().as_mut()
            {
                current.page_drop = next;
            }
        };

        let mut content = rect()
            .width(Size::fill())
            .height(Size::fill())
            .offset_y(slide_offset)
            .opacity(slide_opacity * behind_opacity)
            .scale((behind_scale, behind_scale));

        for pane in &panes {
            let (x, y, w, h) = pane_pixels(pane.rect, size, in_split);
            content = content.child(SplitPaneView {
                tab_id: pane.tab_id,
                left: x,
                top: y,
                width: if width > 0.0 { w } else { 0.0 },
                height: if height > 0.0 { h } else { 0.0 },
                fill: width <= 0.0,
                active: Some(pane.tab_id) == active_tab,
                in_split,
                mode: self.mode,
            });
        }

        if let Some(group) = &group
            && width > 0.0
        {
            let ratios = match &*resize_preview.read() {
                Some((id, ratios)) if *id == group.id => ratios.clone(),
                _ => group.ratios.clone(),
            };
            for (position, handle) in split::split_handles(group.direction, group.tab_ids.len(), &ratios)
                .into_iter()
                .enumerate()
            {
                content = content.child(SplitHandleView {
                    group_id: group.id,
                    direction: group.direction,
                    count: group.tab_ids.len(),
                    ratios: ratios.clone(),
                    handle,
                    area: (origin_x, origin_y, width, height),
                    preview: resize_preview,
                    highlight: self.highlight_handle && position == 0,
                    slot: position,
                });
            }
        }

        let drop_overlay = sidebar_state
            .and_then(|state| state.drag_state.read().clone())
            .and_then(|drag| drag.page_drop)
            .and_then(|drop| {
                let pane = panes.iter().find(|pane| pane.tab_id == drop.target_tab)?;
                let (x, y, w, h) = zone_pixels(pane_pixels(pane.rect, size, in_split), drop.zone);
                let accent = theme.rgba(ThemeRole::Accent);
                Some(
                    rect()
                        .layer(Layer::Relative(8))
                        .position(Position::new_absolute().left(x + 4.0).top(y + 4.0))
                        .width(Size::px((w - 8.0).max(0.0)))
                        .height(Size::px((h - 8.0).max(0.0)))
                        .corner_radius(CornerRadius::new_all(PANE_RADIUS))
                        .background(accent.with_alpha(0.18).to_color())
                        .border(
                            Border::new()
                                .width(2.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(accent.with_alpha(0.85).to_color()),
                        )
                        .center()
                        .child(
                            rect()
                                .padding(Gaps::new(6.0, 12.0, 6.0, 12.0))
                                .corner_radius(CornerRadius::new_all(999.0))
                                .background(theme.color(ThemeRole::SurfaceRaised))
                                .direction(Direction::Horizontal)
                                .cross_align(Alignment::center())
                                .spacing(6.0)
                                .child(crate::ui::kit::Icon::new("split").size(14.0).role(ThemeRole::Accent))
                                .child(
                                    label()
                                        .text("Split View")
                                        .font_size(theme.font_sizes.sm)
                                        .color(theme.color(ThemeRole::Text)),
                                ),
                        )
                        .into_element(),
                )
            });

        let glance_layer = glance.map(|glance| {
            GlanceLayer {
                glance,
                progress: glance_t,
                area: size,
                mode: self.mode,
            }
            .into_element()
        });

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .on_sized(move |e: Event<SizedEventData>| {
                let next = (e.area.min_x(), e.area.min_y(), e.area.width(), e.area.height());
                if *area.peek() != next {
                    area.set(next);
                }
            })
            .on_global_pointer_move(on_global_pointer_move)
            .child(content)
            .maybe_child(drop_overlay)
            .child(WindowModeOverlay { area: size })
            .maybe_child(glance_layer)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// One pane: a live page or a stand-in, with the active ring and the pane handle when it is part of a split.
#[derive(Clone, PartialEq)]
struct SplitPaneView {
    tab_id: TabId,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    /// Fill the page area before its size is known.
    fill: bool,
    active: bool,
    in_split: bool,
    mode: PaneMode,
}

impl Component for SplitPaneView {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = try_use_menu_state();
        let tab_id = self.tab_id;

        let (title, url, active_ring_role) = {
            let model = core.model().read();
            let (title, url) = model
                .tab(tab_id)
                .map(|tab| (tab.display_title(), tab.url.clone()))
                .unwrap_or_default();
            (title, url, active_pane_ring_role(&model))
        };

        let page: Element = match self.mode {
            PaneMode::Live => PageSurface::for_tab(tab_id)
                .key(tab_id.0)
                .width(Size::fill())
                .height(Size::fill())
                .into_element(),
            PaneMode::Placeholder => PlaceholderPage { title, url }.into_element(),
        };

        let focus_core = core.clone();
        let active = self.active;
        let on_mouse_down = move |_: Event<MouseEventData>| {
            if !active {
                focus_core.invoke_or_log(Command::TabActivate { tab_id });
            }
        };

        let position = if self.fill {
            Position::new_absolute().left(0.0).top(0.0)
        } else {
            Position::new_absolute().left(self.left).top(self.top)
        };
        let (width, height) = if self.fill {
            (Size::fill(), Size::fill())
        } else {
            (Size::px(self.width), Size::px(self.height))
        };

        let mut pane = rect()
            .position(position)
            .width(width)
            .height(height)
            .overflow(Overflow::Clip)
            .on_mouse_down(on_mouse_down)
            .child(page);

        // In a split, the ring sits in a padding the page never covers, so focusing a pane does not resize its page.
        if self.in_split {
            let ring = if self.active {
                Border::new()
                    .width(ACTIVE_RING_WIDTH)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(active_ring_role))
            } else {
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle))
            };
            pane = pane
                .padding(Gaps::new_all(ACTIVE_RING_WIDTH))
                .corner_radius(CornerRadius::new_all(PANE_RADIUS))
                .border(ring);
        }

        let handle = (self.in_split && self.active).then(|| {
            let menu_core = core.clone();
            rect()
                .position(Position::new_absolute().left(self.width / 2.0 - 14.0).top(6.0))
                .width(Size::px(28.0))
                .height(Size::px(10.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(theme.color(ThemeRole::SurfaceRaised))
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Split pane options")
                .direction(Direction::Horizontal)
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .spacing(3.0)
                .on_mouse_down(move |e: Event<MouseEventData>| {
                    e.stop_propagation();
                    if let Some(menu_state) = &menu_state {
                        let model = menu_core.model().peek().clone();
                        let position = (e.global_location.x as f32, e.global_location.y as f32);
                        open_tab_context_menu(menu_state, &model, tab_id, position);
                    }
                })
                .children((0..3).map(|_| {
                    rect()
                        .width(Size::px(3.0))
                        .height(Size::px(3.0))
                        .corner_radius(CornerRadius::new_all(2.0))
                        .background(theme.color(ThemeRole::Text))
                        .into_element()
                }))
                .into_element()
        });

        pane.maybe_child(handle)
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.tab_id.0)
    }
}

/// A draggable boundary between panes. Dragging previews the new sizes; releasing invokes `split.resize`.
#[derive(Clone, PartialEq)]
struct SplitHandleView {
    group_id: SplitGroupId,
    direction: SplitDirection,
    count: usize,
    ratios: Vec<f32>,
    handle: split::SplitHandle,
    /// The page area in window coordinates: x, y, width, height.
    area: (f32, f32, f32, f32),
    preview: State<Option<(SplitGroupId, Vec<f32>)>>,
    highlight: bool,
    /// Position among the group's handles; two grid handles can share a ratio index.
    slot: usize,
}

impl Component for SplitHandleView {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let mut dragging = use_state(|| false);

        let (ox, oy, width, height) = self.area;
        let handle = self.handle;
        let (left, top, w, h) = match handle.axis {
            HandleAxis::Column => (
                handle.position * width - HANDLE_HIT / 2.0,
                handle.span_start * height,
                HANDLE_HIT,
                (handle.span_end - handle.span_start) * height,
            ),
            HandleAxis::Row => (
                handle.span_start * width,
                handle.position * height - HANDLE_HIT / 2.0,
                (handle.span_end - handle.span_start) * width,
                HANDLE_HIT,
            ),
        };

        let (direction, count, ratios, group_id) = (self.direction, self.count, self.ratios.clone(), self.group_id);
        let mut preview = self.preview;
        let on_global_pointer_move = {
            let ratios = ratios.clone();
            move |e: Event<PointerEventData>| {
                if !*dragging.peek() {
                    return;
                }
                let fraction = match handle.axis {
                    HandleAxis::Column => (e.global_location().x as f32 - ox) / width.max(1.0),
                    HandleAxis::Row => (e.global_location().y as f32 - oy) / height.max(1.0),
                };
                let next = split::resize_ratios(direction, count, &ratios, handle.index, fraction);
                preview.set(Some((group_id, next)));
            }
        };
        let on_global_pointer_up = move |_: Event<PointerEventData>| {
            if !*dragging.peek() {
                return;
            }
            dragging.set(false);
            let finished = preview.write().take();
            if let Some((group_id, ratios)) = finished {
                core.invoke_or_log(Command::SplitResize { group_id, ratios });
            }
        };

        let lit = *hovered.read() || *dragging.read() || self.highlight;
        let grip_color = if lit {
            theme.color(ThemeRole::Accent)
        } else {
            theme.rgba(ThemeRole::Text).with_alpha(0.35).to_color()
        };
        let (grip_w, grip_h) = match handle.axis {
            HandleAxis::Column => (3.0, 36.0),
            HandleAxis::Row => (36.0, 3.0),
        };

        rect()
            .position(Position::new_absolute().left(left).top(top))
            .width(Size::px(w.max(0.0)))
            .height(Size::px(h.max(0.0)))
            .center()
            .cursor(match handle.axis {
                HandleAxis::Column => CursorIcon::ColResize,
                HandleAxis::Row => CursorIcon::RowResize,
            })
            .a11y_role(AccessibilityRole::Splitter)
            .a11y_alt("Resize split view")
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .on_pointer_down(move |e: Event<PointerEventData>| {
                e.stop_propagation();
                dragging.set(true);
            })
            .on_global_pointer_move(on_global_pointer_move)
            .on_global_pointer_up(on_global_pointer_up)
            .child(
                rect()
                    .width(Size::px(grip_w))
                    .height(Size::px(grip_h))
                    .corner_radius(CornerRadius::new_all(2.0))
                    .background(grip_color),
            )
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.slot)
    }
}

/// A stand-in page for the gallery: a site header, a heading and a content card, in the page's own colors.
#[derive(Clone, PartialEq)]
pub struct PlaceholderPage {
    pub title: String,
    pub url: String,
}

impl Component for PlaceholderPage {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let dark = theme.scheme.is_dark();
        let (page_bg, card_bg, text, muted, line) = if dark {
            (
                Color::from_rgb(18, 20, 29),
                Color::from_rgb(28, 31, 46),
                Color::from_rgb(232, 234, 242),
                Color::from_rgb(150, 156, 178),
                Color::from_rgb(40, 44, 60),
            )
        } else {
            (
                Color::from_rgb(250, 250, 252),
                Color::from_rgb(255, 255, 255),
                Color::from_rgb(28, 30, 40),
                Color::from_rgb(98, 104, 124),
                Color::from_rgb(226, 228, 236),
            )
        };
        let title = if self.title.is_empty() {
            "New Tab".to_string()
        } else {
            self.title.clone()
        };
        let site = title.split(" — ").next().unwrap_or(&title).to_string();

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(page_bg)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(62.0))
                    .padding(Gaps::new(0.0, 40.0, 0.0, 40.0))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .main_align(Alignment::SpaceBetween)
                    .overflow(Overflow::Clip)
                    .border(Border::new().width(1.0).alignment(BorderAlignment::Inner).fill(line))
                    .child(
                        label()
                            .text(title.clone())
                            .font_size(15.0)
                            .font_weight(FontWeight::BOLD)
                            .color(text)
                            .max_lines(1),
                    )
                    .child(rect().direction(Direction::Horizontal).spacing(22.0).children(
                        ["Documentation", "Architecture", "Releases"].into_iter().map(|item| {
                            label()
                                .text(item)
                                .font_size(13.0)
                                .max_lines(1)
                                .color(muted)
                                .into_element()
                        }),
                    )),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(48.0, 40.0, 0.0, 40.0))
                    .spacing(14.0)
                    .child(
                        label()
                            .text(site)
                            .font_size(30.0)
                            .font_weight(FontWeight::BOLD)
                            .color(text)
                            .max_lines(2),
                    )
                    .child(label().text(self.url.clone()).font_size(14.0).color(muted).max_lines(1))
                    .child(
                        rect()
                            .width(Size::fill())
                            .margin(Gaps::new(12.0, 0.0, 0.0, 0.0))
                            .padding(Gaps::new_all(24.0))
                            .spacing(12.0)
                            .corner_radius(CornerRadius::new_all(10.0))
                            .background(card_bg)
                            .border(Border::new().width(1.0).alignment(BorderAlignment::Inner).fill(line))
                            .child(
                                label()
                                    .text("Core Principles")
                                    .font_size(20.0)
                                    .font_weight(FontWeight::BOLD)
                                    .color(text),
                            )
                            .child(
                                label()
                                    .text(
                                        "Every browser action is a named, typed command in a single registry. The \
                                         native UI renders state and dispatches commands.",
                                    )
                                    .font_size(14.0)
                                    .color(text),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panes_leave_a_gap_between_them() {
        let rects = split::pane_rects(SplitDirection::Horizontal, 2, &[0.5, 0.5]);
        let left = pane_pixels(rects[0], (1000.0, 800.0), true);
        let right = pane_pixels(rects[1], (1000.0, 800.0), true);
        assert_eq!(left, (0.0, 0.0, 497.0, 800.0));
        assert_eq!(right.0 - (left.0 + left.2), SPLIT_GAP);
    }

    #[test]
    fn a_single_page_fills_the_card() {
        let full = PaneRect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        };
        assert_eq!(pane_pixels(full, (900.0, 600.0), false), (0.0, 0.0, 900.0, 600.0));
    }

    #[test]
    fn drop_zones_cover_half_a_pane() {
        assert_eq!(
            zone_pixels((10.0, 0.0, 400.0, 300.0), DropZone::Right),
            (210.0, 0.0, 200.0, 300.0)
        );
        assert_eq!(
            zone_pixels((0.0, 0.0, 400.0, 300.0), DropZone::Top),
            (0.0, 0.0, 400.0, 150.0)
        );
    }
}
