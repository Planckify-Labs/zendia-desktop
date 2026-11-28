/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! What the page card shows: the active tab, or every pane of the active split group, with resize handles, the drop
//! zones a dragged tab can land on, Zendia's tab switch motion and Glance on top.
//!
//! Pane geometry comes from [`zendia_browser_core::split`]; this view adds gaps, rings and motion. Resizing, focusing
//! a pane and dropping a tab are commands. Only drag progress, zoom-in-view, and animation stay here.

use crate::ui::chrome::menus::{open_tab_context_menu, try_use_menu_state};
use crate::ui::chrome::sidebar::{PageDrop, try_use_sidebar_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, use_last_some};
use crate::ui::page::glance::{
    GLANCE_BACKGROUND_OPACITY, GLANCE_BACKGROUND_SCALE, GLANCE_CLOSE, GLANCE_OPEN, GlanceLayer, glance_spring,
};
use crate::ui::page::surface::PageSurface;
use crate::ui::page::surface_store::PageSurfaceContext;
use crate::ui::panels::try_use_dock_state;
use crate::ui::theme::{ThemeRole, motion, use_theme};
use crate::ui::vim::RegionPickerOverlay;
use crate::ui::vim::window_mode::{WindowModeOverlay, active_pane_ring_role};
use freya::animation::*;
use freya::prelude::*;
use std::time::{Duration, Instant};
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{SpaceId, SplitGroup, SplitGroupId, TabId};
use zendia_browser_core::split::{self, DropZone, HandleAxis, PaneRect, SplitNode};
use zendia_browser_core::split_actions::SplitAction;

/// Space between split panes, as in Zen.
pub const SPLIT_GAP: f32 = 6.0;
const PANE_RADIUS: f32 = 8.0;
const ACTIVE_RING_WIDTH: f32 = 2.0;
const HANDLE_HIT: f32 = 10.0;
const DOUBLE_CLICK_MS: Duration = Duration::from_millis(350);

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

/// Directional label shown in the accent drop-zone pill (matching Zen's split preview).
pub fn drop_zone_label(zone: DropZone) -> &'static str {
    match zone {
        DropZone::Left => "Split left",
        DropZone::Right => "Split right",
        DropZone::Top => "Split above",
        DropZone::Bottom => "Split below",
    }
}

impl Component for PageArea {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let sidebar_state = try_use_sidebar_state();
        let page_ctx = try_consume_context::<PageSurfaceContext>();
        let mut area = use_state(|| (0.0f32, 0.0f32, 0.0f32, 0.0f32));
        let resize_preview = use_state(|| None::<(SplitGroupId, SplitNode)>);
        let rearrange_drag = use_state(|| None::<TabId>);
        let mut last_key = use_state(|| None::<(bool, u64)>);
        let mut last_active = use_state(|| None::<TabId>);
        let mut last_space = use_state(|| None::<SpaceId>);
        let mut last_switch = use_state(|| None::<Instant>);
        let mut slide = use_state(|| (0u64, 0.0f32));
        // Pending live-tab switch waiting for the newly selected Hub view to paint within `motion::TAB_PENDING`.
        let mut pending_slide = use_state(|| None::<(u64, f32, Instant)>);

        let (
            panes,
            active_tree,
            group,
            active_tab,
            active_tab_url,
            active_tab_view_id,
            active_space,
            order,
            motion_enabled,
            glance,
        ) = {
            let model = core.model().read();
            let group = model.active_split_group().cloned();
            let active_tree: Option<SplitNode> = group.as_ref().and_then(|grp| match &*resize_preview.read() {
                Some((id, preview_tree)) if *id == grp.id => Some(preview_tree.clone()),
                _ => split::effective_group_tree(grp),
            });
            let panes: Vec<Pane> = match (&group, &active_tree) {
                (Some(_), Some(tree)) => split::tree_pane_rects(tree)
                    .into_iter()
                    .map(|(tab_id, rect)| Pane { tab_id, rect })
                    .collect(),
                _ => model
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
            let (active_url, active_view_id) = model
                .active_tab()
                .map(|t| (t.url.clone(), t.view_id))
                .unwrap_or_default();
            (
                panes,
                active_tree,
                group,
                model.active_tab,
                active_url,
                active_view_id,
                model.active_space,
                model.tabs_in_active_space_order(),
                model.layout.tab_switch_motion,
                model.glance.clone(),
            )
        };

        let active_zoom = group
            .as_ref()
            .and_then(|g| g.zoomed_tab.filter(|z| g.tab_ids.contains(z)));
        let is_resizing = resize_preview.read().is_some();

        let is_empty_url = |url: &str| url.is_empty() || url == "about:blank" || url == "about:newtab";

        // Zendia's tab switch motion: the page slides in from below when the new tab sits below the old one in the
        // sidebar, and from above otherwise. Suppressed on intra-split switches, key repeat (<90ms), space switches,
        // empty/newtab pages, or while Glance is open. Live tabs without a painted frame wait up to `TAB_PENDING`.
        let shown_key = group
            .as_ref()
            .map(|g| (true, g.id.0))
            .or(active_tab.map(|t| (false, t.0)));
        if *last_key.peek() != shown_key {
            let now = Instant::now();
            let repeat = last_switch
                .peek()
                .is_some_and(|at| now.duration_since(at) < motion::TAB_REPEAT);
            let same_space = *last_space.peek() == active_space;
            let previous = *last_active.peek();
            let suppress =
                !motion_enabled || repeat || !same_space || glance.is_some() || is_empty_url(&active_tab_url);
            pending_slide.set_if_modified(None);
            if !suppress
                && last_key.peek().is_some()
                && let (Some(from), Some(to)) = (previous, active_tab)
                && let (Some(from_index), Some(to_index)) = (
                    order.iter().position(|id| *id == from),
                    order.iter().position(|id| *id == to),
                )
                && from_index != to_index
            {
                let direction = if to_index > from_index { 1.0 } else { -1.0 };
                let needs_paint_wait = self.mode == PaneMode::Live
                    && active_tab_view_id.is_some_and(|vid| {
                        page_ctx
                            .as_ref()
                            .is_some_and(|ctx| ctx.surfaces().peek().get(&vid).is_none_or(|s| s.frame.is_none()))
                    });
                if needs_paint_wait && let Some(vid) = active_tab_view_id {
                    let started_at = now;
                    pending_slide.set(Some((vid, direction, started_at)));
                    spawn(async move {
                        timer(motion::TAB_PENDING).await;
                        if pending_slide
                            .peek()
                            .is_some_and(|(id, _, at)| id == vid && at == started_at)
                        {
                            pending_slide.set(None);
                        }
                    });
                } else {
                    let seq = slide.peek().0 + 1;
                    slide.set((seq, direction));
                }
            }
            last_switch.set(Some(now));
            last_key.set(shown_key);
        }
        if *last_active.peek() != active_tab {
            last_active.set(active_tab);
        }
        if *last_space.peek() != active_space {
            last_space.set(active_space);
        }

        // Release a pending live-tab slide once the newly selected view presents a frame within `motion::TAB_PENDING`.
        let current_pending = *pending_slide.read();
        if let Some((target_vid, direction, started_at)) = current_pending
            && let Some(ctx) = &page_ctx
        {
            let has_frame = ctx
                .surfaces()
                .read()
                .get(&target_vid)
                .is_some_and(|surface| surface.frame.is_some());
            if has_frame {
                pending_slide.set(None);
                if started_at.elapsed() <= motion::TAB_PENDING {
                    let seq = slide.peek().0 + 1;
                    slide.set((seq, direction));
                }
            }
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
        let fade_anim = use_animation_with_dependencies(&slide_seq, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(motion::TAB_FADE_FROM, 1.0)
                .duration(motion::TAB_FADE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let slide_t = slide_anim.get().value().clamp(0.0, 1.0);
        let slide_offset = slide_direction * motion::TAB_SLIDE_PX * (1.0 - slide_t);
        let slide_opacity = fade_anim.get().value().clamp(0.0, 1.0);

        let glance_open = glance.is_some();
        let glance_tab_key = glance.as_ref().map(|g| g.tab_id.0);
        let cached_glance = use_last_some(&glance);
        let glance_anim = use_animation_with_dependencies(&(glance_open, glance_tab_key), move |conf, (open, _)| {
            conf.on_creation(if *open { OnCreation::Run } else { OnCreation::Finish });
            conf.on_change(OnChange::Rerun);
            if *open {
                AnimNum::new(0.0, 1.0)
                    .duration(GLANCE_OPEN)
                    .ease(Ease::Out)
                    .function(glance_spring())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(GLANCE_CLOSE)
                    .ease(Ease::Out)
                    .function(motion::ease_out_fn())
            }
        });
        let glance_t = match (&glance, self.glance_progress) {
            (Some(_), Some(pinned)) => pinned,
            (Some(_), None) => glance_anim.get().value(),
            (None, _) => {
                if cached_glance.is_some() {
                    glance_anim.get().value().clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }
        };
        let show_glance_layer = glance.is_some() || (cached_glance.is_some() && glance_t > 0.01);
        let behind = if show_glance_layer {
            glance_t.clamp(0.0, 1.0)
        } else {
            0.0
        };
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

        // Finish a pane-rearrange drag by swapping the source pane with whichever pane the pointer released over.
        let swap_panes = panes.clone();
        let swap_group = group.clone();
        let swap_core = core.clone();
        let on_global_pointer_up = move |e: Event<PointerEventData>| {
            let mut drag_src = rearrange_drag;
            let Some(source_id) = drag_src.write().take() else {
                return;
            };
            let Some(grp) = &swap_group else {
                return;
            };
            let (ox, oy, w, h) = *area.peek();
            let px = e.global_location().x as f32 - ox;
            let py = e.global_location().y as f32 - oy;
            if let Some(target) = swap_panes.iter().find(|pane| {
                let (x, y, pw, ph) = pane_pixels(pane.rect, (w, h), true);
                pane.tab_id != source_id && px >= x && px <= x + pw && py >= y && py <= y + ph
            }) && let Some(mut tree) = split::effective_group_tree(grp)
                && split::swap_leaves(&mut tree, source_id, target.tab_id)
            {
                swap_core.invoke_or_log(Command::Splits(SplitAction::UpdateTree { group_id: grp.id, tree }));
            }
        };

        let mut content = rect()
            .width(Size::fill())
            .height(Size::fill())
            .offset_y(slide_offset)
            .opacity(slide_opacity * behind_opacity)
            .scale((behind_scale, behind_scale));

        for pane in &panes {
            let is_zoomed = active_zoom == Some(pane.tab_id);
            if active_zoom.is_some() && !is_zoomed {
                continue;
            }
            let (x, y, w, h) = if is_zoomed {
                (0.0, 0.0, width, height)
            } else {
                pane_pixels(pane.rect, size, in_split)
            };
            content = content.child(SplitPaneView {
                tab_id: pane.tab_id,
                group: group.clone(),
                left: x,
                top: y,
                width: if width > 0.0 { w } else { 0.0 },
                height: if height > 0.0 { h } else { 0.0 },
                fill: width <= 0.0,
                active: Some(pane.tab_id) == active_tab,
                in_split,
                is_resizing,
                zoomed: is_zoomed,
                rearrange_drag,
                mode: self.mode,
            });
        }

        if let (Some(group), Some(tree)) = (&group, &active_tree)
            && width > 0.0
            && active_zoom.is_none()
        {
            for (position, handle) in split::tree_split_handles(tree).into_iter().enumerate() {
                content = content.child(SplitHandleView {
                    group_id: group.id,
                    tree: tree.clone(),
                    handle,
                    area: (origin_x, origin_y, width, height),
                    preview: resize_preview,
                    highlight: self.highlight_handle && position == 0,
                    slot: position,
                });
            }
        }

        let drop_target = sidebar_state
            .and_then(|state| state.drag_state.read().clone())
            .and_then(|drag| drag.page_drop)
            .and_then(|drop| {
                let pane = panes.iter().find(|pane| pane.tab_id == drop.target_tab)?;
                Some((
                    zone_pixels(pane_pixels(pane.rect, size, in_split), drop.zone),
                    drop.zone,
                ))
            });
        let drop_overlay = drop_target.map(|(bounds, zone)| SplitDropZoneOverlay { bounds, zone }.into_element());

        let share_receive_overlay = zendia_browser_core::share::parse_share_url(&active_tab_url).map(|_| {
            crate::ui::share::ShareReceiveOverlay {
                preview: zendia_browser_core::share::sample_share_preview(&active_tab_url),
                host_tab_id: active_tab,
            }
            .into_element()
        });

        let glance_layer = show_glance_layer.then(|| cached_glance.clone()).flatten().map(|g| {
            GlanceLayer {
                glance: g,
                progress: glance_t,
                closing: !glance_open,
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
            .on_global_pointer_up(on_global_pointer_up)
            .child(content)
            .maybe_child(drop_overlay)
            .child(WindowModeOverlay { area: size })
            .maybe_child(share_receive_overlay)
            .maybe_child(glance_layer)
            .child(RegionPickerOverlay)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Animated drop-zone highlight and directional accent pill (`SV-03`, `SV-04`).
#[derive(Clone, PartialEq)]
struct SplitDropZoneOverlay {
    bounds: (f32, f32, f32, f32),
    zone: DropZone,
}

impl Component for SplitDropZoneOverlay {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let target = self.bounds;
        let mut from = use_state(|| target);
        let start = *from.peek();
        let rect_key = (
            target.0.to_bits(),
            target.1.to_bits(),
            target.2.to_bits(),
            target.3.to_bits(),
        );
        let rect_anim = use_animation_with_dependencies(&rect_key, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(motion::SPLIT_DROP_ZONE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let pill_anim = use_animation(move |conf| {
            conf.on_creation(OnCreation::Run);
            AnimNum::new(0.0, 1.0)
                .duration(Duration::from_millis(150))
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        if *from.peek() != target {
            from.set(target);
        }

        let t = rect_anim.get().value().clamp(0.0, 1.0);
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        let (x, y, w, h) = (
            lerp(start.0, target.0),
            lerp(start.1, target.1),
            lerp(start.2, target.2),
            lerp(start.3, target.3),
        );
        let pt = pill_anim.get().value().clamp(0.0, 1.0);
        let pill_scale = 0.92 + 0.08 * pt;
        let accent = theme.rgba(ThemeRole::Accent);

        rect()
            .layer(Layer::Relative(14))
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
                    .scale(pill_scale)
                    .opacity(pt)
                    .padding(Gaps::new(6.0, 13.0, 6.0, 12.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(theme.color(ThemeRole::Accent))
                    .shadow(Shadow::new().y(4.0).blur(14.0).color(Color::from_argb(72, 0, 0, 0)))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .child(Icon::new("split").size(14.0).role(ThemeRole::TextOnAccent))
                    .child(
                        label()
                            .text(drop_zone_label(self.zone))
                            .font_size(theme.font_sizes.sm)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(theme.color(ThemeRole::TextOnAccent)),
                    ),
            )
    }
}

/// One pane: a live page or a stand-in, with smooth layout transitions (`SV-03`), active ring, and hover-revealed
/// 3-dot nub + 150ms expandable action chip (`SV-01`, `SV-09`) lifted above live Hub `PageSurface`s.
#[derive(Clone, PartialEq)]
struct SplitPaneView {
    tab_id: TabId,
    group: Option<SplitGroup>,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    /// Fill the page area before its size is known.
    fill: bool,
    active: bool,
    in_split: bool,
    is_resizing: bool,
    zoomed: bool,
    rearrange_drag: State<Option<TabId>>,
    mode: PaneMode,
}

impl Component for SplitPaneView {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let panel_focused = try_use_dock_state().is_some_and(|ds| ds.focused_panel.read().is_some());
        let tab_id = self.tab_id;
        let mut pane_hovered = use_state(|| false);

        // Smooth 90ms inset transition on split layout switches (disabled during live resize drag for zero lag).
        let target_geom = (self.left, self.top, self.width, self.height);
        let mut prev_geom = use_state(|| target_geom);
        let start_geom = *prev_geom.peek();
        let geom_changed = !self.fill
            && !self.is_resizing
            && start_geom.2 > 0.0
            && target_geom.2 > 0.0
            && ((start_geom.0 - target_geom.0).abs() > 0.5
                || (start_geom.1 - target_geom.1).abs() > 0.5
                || (start_geom.2 - target_geom.2).abs() > 0.5
                || (start_geom.3 - target_geom.3).abs() > 0.5);
        let mut geom_seq = use_state(|| 0u64);
        let mut anim_from = use_state(|| target_geom);
        if geom_changed {
            anim_from.set(start_geom);
            let next = geom_seq.peek().wrapping_add(1);
            geom_seq.set(next);
        }
        if *prev_geom.peek() != target_geom {
            prev_geom.set(target_geom);
        }
        let current_seq = *geom_seq.read();
        let layout_anim = use_animation_with_dependencies(&current_seq, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(motion::SPLIT_LAYOUT)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let lt = if self.is_resizing || self.fill {
            1.0
        } else {
            layout_anim.get().value().clamp(0.0, 1.0)
        };
        let from_g = *anim_from.read();
        let cur_left = from_g.0 + (target_geom.0 - from_g.0) * lt;
        let cur_top = from_g.1 + (target_geom.1 - from_g.1) * lt;
        let cur_width = from_g.2 + (target_geom.2 - from_g.2) * lt;
        let cur_height = from_g.3 + (target_geom.3 - from_g.3) * lt;

        let (title, url, favicon, active_ring_role) = {
            let model = core.model().read();
            let (title, url, favicon) = model
                .tab(tab_id)
                .map(|tab| (tab.display_title(), tab.url.clone(), tab.favicon.clone()))
                .unwrap_or_default();
            (title, url, favicon, active_pane_ring_role(&model))
        };

        let page: Element = match self.mode {
            PaneMode::Live => PageSurface::for_tab(tab_id)
                .key(tab_id.0)
                .width(Size::fill())
                .height(Size::fill())
                .into_element(),
            PaneMode::Placeholder => PlaceholderPage {
                title: title.clone(),
                url,
            }
            .into_element(),
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
            Position::new_absolute().left(cur_left).top(cur_top)
        };
        let (width, height) = if self.fill {
            (Size::fill(), Size::fill())
        } else {
            (Size::px(cur_width), Size::px(cur_height))
        };

        // In a split, clip the page to the inner rounded radius inside the 2px ring padding so live web views never
        // paint over the straight edges or rounded corners of the active pane ring.
        let clipped_page: Element = if self.in_split {
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .corner_radius(CornerRadius::new_all((PANE_RADIUS - ACTIVE_RING_WIDTH).max(0.0)))
                .overflow(Overflow::Clip)
                .child(page)
                .into_element()
        } else {
            page
        };
        let is_being_rearranged = *self.rearrange_drag.read() == Some(tab_id);

        let mut pane = rect()
            .position(position)
            .width(width)
            .height(height)
            .opacity(if is_being_rearranged { 0.4 } else { 1.0 })
            .overflow(Overflow::Clip)
            .on_pointer_enter(move |_| pane_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| pane_hovered.set_if_modified(false))
            .on_mouse_down(on_mouse_down)
            .child(clipped_page);

        // In a split, the ring sits in a padding the page never covers, so focusing a pane does not resize its page.
        // DF-03 / XIX-07: Suppress the active split-pane accent ring while a side panel holds focus.
        if self.in_split {
            let ring = if self.active && !panel_focused {
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

        // Lifted above `PageSurface`'s `image(frame)` via `Layer::Relative(12)` so it stays visible over live Hub pages.
        let handle = (self.in_split && !self.is_resizing && cur_width > 60.0).then(|| {
            SplitPaneTopHandle {
                tab_id,
                title,
                favicon,
                pane_width: cur_width,
                active: self.active,
                pane_hovered: *pane_hovered.read(),
                zoomed: self.zoomed,
                rearrange_drag: self.rearrange_drag,
                can_unsplit: self.group.is_some(),
            }
            .into_element()
        });

        pane.maybe_child(handle)
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.tab_id.0)
    }
}

/// Hover-revealed 3-dot nub (`120 ms`, accent dots on active pane) and `150 ms`-delayed expandable pane action chip
/// (rearrange drag handle, favicon, title, Fill View/Zoom, Move to Own Tab, Close) (`SV-01`, `SV-09`, `WIN-11`).
#[derive(Clone, PartialEq)]
struct SplitPaneTopHandle {
    tab_id: TabId,
    title: String,
    favicon: Option<String>,
    pane_width: f32,
    active: bool,
    pane_hovered: bool,
    zoomed: bool,
    rearrange_drag: State<Option<TabId>>,
    can_unsplit: bool,
}

impl Component for SplitPaneTopHandle {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = try_use_menu_state();
        let mut handle_hovered = use_state(|| false);
        let mut chip_expanded = use_state(|| false);
        let mut chip_flashed = use_state(|| false);
        let mut last_flash_seq = use_state(|| core.model().peek().chip_flash.map(|(_, s)| s).unwrap_or(0));
        let mut hover_gen = use_state(|| 0u64);

        // `1.2 s` pane-chip flash after keyboard split mutations outside Window mode (`WIN-11`).
        let model_flash = core.model().read().chip_flash;
        if let Some((flashed_tab, seq)) = model_flash
            && seq != *last_flash_seq.peek()
        {
            last_flash_seq.set(seq);
            if flashed_tab == self.tab_id {
                chip_flashed.set_if_modified(true);
                let mut flash_state = chip_flashed;
                let observed_seq = last_flash_seq;
                spawn(async move {
                    timer(Duration::from_millis(1200)).await;
                    if *observed_seq.peek() == seq {
                        flash_state.set_if_modified(false);
                    }
                });
            } else {
                chip_flashed.set_if_modified(false);
            }
        }

        let expanded = *chip_expanded.read() || *chip_flashed.read();
        let show_nub = (self.pane_hovered || self.zoomed || self.active) && !expanded;
        let nub_anim = use_animation_with_dependencies(&show_nub, move |conf, shown| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *shown { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::SPLIT_NUB_FADE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let chip_anim = use_animation_with_dependencies(&expanded, move |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *open { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::SPLIT_NUB_FADE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let nub_t = nub_anim.get().value().clamp(0.0, 1.0);
        let chip_t = chip_anim.get().value().clamp(0.0, 1.0);
        let tab_id = self.tab_id;
        let dot_color = if self.active {
            theme.color(ThemeRole::Accent)
        } else {
            theme.rgba(ThemeRole::Text).with_alpha(0.7).to_color()
        };

        let on_zone_enter = move |_| {
            handle_hovered.set_if_modified(true);
            let next_gen = hover_gen.peek().wrapping_add(1);
            hover_gen.set(next_gen);
            spawn(async move {
                timer(motion::SPLIT_CHIP_DELAY).await;
                if *hover_gen.peek() == next_gen && *handle_hovered.peek() {
                    chip_expanded.set_if_modified(true);
                }
            });
        };
        let on_zone_leave = move |_| {
            handle_hovered.set_if_modified(false);
            let next_gen = hover_gen.peek().wrapping_add(1);
            hover_gen.set(next_gen);
            chip_expanded.set_if_modified(false);
        };

        let nub = (nub_t > 0.01 && !expanded).then(|| {
            let menu_core = core.clone();
            let nub_scale = 0.85 + 0.15 * nub_t;
            rect()
                .width(Size::px(30.0))
                .height(Size::px(11.0))
                .scale(nub_scale)
                .opacity(nub_t)
                .corner_radius(CornerRadius::new_all(6.0))
                .background(theme.rgba(ThemeRole::SurfaceRaised).with_alpha(0.92).to_color())
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::BorderSubtle)),
                )
                .shadow(Shadow::new().y(1.0).blur(4.0).color(Color::from_argb(55, 0, 0, 0)))
                .cursor(CursorIcon::Pointer)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Split pane options")
                .direction(Direction::Horizontal)
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .spacing(3.0)
                .on_mouse_down(move |e: Event<MouseEventData>| {
                    e.stop_propagation();
                    if e.button == Some(MouseButton::Right) {
                        if let Some(menu_state) = &menu_state {
                            let model = menu_core.model().peek().clone();
                            let position = (e.global_location.x as f32, e.global_location.y as f32);
                            open_tab_context_menu(menu_state, &model, tab_id, position);
                        }
                    } else {
                        chip_expanded.set(true);
                    }
                })
                .children((0..3).map(|_| {
                    rect()
                        .width(Size::px(3.2))
                        .height(Size::px(3.2))
                        .corner_radius(CornerRadius::new_all(2.0))
                        .background(dot_color)
                        .into_element()
                }))
        });

        let chip = (chip_t > 0.01 || expanded).then(|| {
            let mut drag_state = self.rearrange_drag;
            let zoom_core = core.clone();
            let detach_core = core.clone();
            let close_core = core.clone();
            let zoomed = self.zoomed;
            let chip_scale = 0.96 + 0.04 * chip_t;
            let chip_y = -4.0 * (1.0 - chip_t);
            let max_title_w = (self.pane_width - 150.0).clamp(40.0, 180.0);

            rect()
                .offset_y(chip_y)
                .scale(chip_scale)
                .opacity(chip_t)
                .padding(Gaps::new_all(3.0))
                .spacing(2.0)
                .corner_radius(CornerRadius::new_all(9.0))
                .background(theme.color(ThemeRole::SurfaceRaised))
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::Border)),
                )
                .shadow(Shadow::new().y(2.0).blur(10.0).color(Color::from_argb(65, 0, 0, 0)))
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
                .child(
                    rect()
                        .width(Size::px(20.0))
                        .height(Size::px(24.0))
                        .corner_radius(CornerRadius::new_all(6.0))
                        .center()
                        .cursor(CursorIcon::Grab)
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("Drag to rearrange pane")
                        .on_pointer_down(move |e: Event<PointerEventData>| {
                            e.stop_propagation();
                            drag_state.set(Some(tab_id));
                        })
                        .child(Icon::new("drag-indicator").size(13.0).role(ThemeRole::TextMuted)),
                )
                .child(
                    Icon::new(self.favicon.clone().unwrap_or_else(|| "globe".to_string()))
                        .size(13.5)
                        .role(ThemeRole::Text),
                )
                .child(
                    rect()
                        .max_width(Size::px(max_title_w))
                        .padding(Gaps::new(0.0, 5.0, 0.0, 3.0))
                        .child(
                            label()
                                .text(self.title.clone())
                                .font_size(12.0)
                                .color(theme.color(ThemeRole::Text))
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis),
                        ),
                )
                .child(SplitChipButton {
                    icon: if zoomed { "fullscreen-exit" } else { "fullscreen" },
                    label: if zoomed {
                        "Show all panes"
                    } else {
                        "Fill the view with this pane"
                    },
                    danger: false,
                    on_press: EventHandler::new(move |_| {
                        zoom_core.invoke_or_log(Command::Splits(SplitAction::Zoom { tab_id: Some(tab_id) }));
                    }),
                })
                .maybe_child(self.can_unsplit.then(|| SplitChipButton {
                    icon: "u-turn-to-left",
                    label: "Move to own tab",
                    danger: false,
                    on_press: EventHandler::new(move |_| {
                        detach_core.invoke_or_log(Command::SplitRemoveTab { tab_id });
                    }),
                }))
                .child(SplitChipButton {
                    icon: "close",
                    label: "Close pane",
                    danger: true,
                    on_press: EventHandler::new(move |_| {
                        close_core.invoke_or_log(Command::TabClose {
                            tab_id: Some(tab_id),
                            force: false,
                        });
                    }),
                })
        });

        let zone_w = if expanded {
            self.pane_width.clamp(120.0, 350.0)
        } else {
            120.0_f32.min(self.pane_width)
        };

        rect()
            .layer(Layer::Relative(12))
            .position(Position::new_absolute().left((self.pane_width - zone_w) * 0.5).top(4.0))
            .width(Size::px(zone_w))
            .min_height(Size::px(if expanded { 32.0 } else { 20.0 }))
            .cross_align(Alignment::center())
            .on_pointer_enter(on_zone_enter)
            .on_pointer_leave(on_zone_leave)
            .maybe_child(nub)
            .maybe_child(chip)
    }
}

#[derive(Clone, PartialEq)]
struct SplitChipButton {
    icon: &'static str,
    label: &'static str,
    danger: bool,
    on_press: EventHandler<()>,
}

impl Component for SplitChipButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let on_press = self.on_press.clone();

        let bg = if *hovered.read() {
            if self.danger {
                theme.rgba(ThemeRole::Danger).with_alpha(0.18).to_color()
            } else {
                theme.color(ThemeRole::Hover)
            }
        } else {
            Color::TRANSPARENT
        };
        let role = if *hovered.read() && self.danger {
            ThemeRole::Danger
        } else if *hovered.read() {
            ThemeRole::Text
        } else {
            ThemeRole::TextMuted
        };

        rect()
            .width(Size::px(24.0))
            .height(Size::px(24.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .center()
            .cursor(CursorIcon::Pointer)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label)
            .a11y_focusable(true)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_press.call(());
            })
            .child(Icon::new(self.icon).size(13.0).role(role))
    }
}

/// A draggable boundary between panes. Dragging previews the new sizes; releasing commits `SplitAction::UpdateTree`.
/// Double-clicking equalizes panes under that container (`SV-02`). Grabber fades in over `100 ms` on hover (`SV-06`).
#[derive(Clone, PartialEq)]
struct SplitHandleView {
    group_id: SplitGroupId,
    tree: SplitNode,
    handle: split::SplitHandle,
    /// The page area in window coordinates: x, y, width, height.
    area: (f32, f32, f32, f32),
    preview: State<Option<(SplitGroupId, SplitNode)>>,
    highlight: bool,
    /// Position among the group's handles.
    slot: usize,
}

impl Component for SplitHandleView {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let mut dragging = use_state(|| false);
        let mut last_down = use_state(|| None::<Instant>);

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

        let (tree, group_id) = (self.tree.clone(), self.group_id);
        let mut preview = self.preview;
        let on_global_pointer_move = {
            let tree = tree.clone();
            move |e: Event<PointerEventData>| {
                if !*dragging.peek() {
                    return;
                }
                let fraction = match handle.axis {
                    HandleAxis::Column => (e.global_location().x as f32 - ox) / width.max(1.0),
                    HandleAxis::Row => (e.global_location().y as f32 - oy) / height.max(1.0),
                };
                let next = split::resize_tree_handle(&tree, handle.index, fraction);
                preview.set(Some((group_id, next)));
            }
        };
        let up_core = core.clone();
        let on_global_pointer_up = move |_: Event<PointerEventData>| {
            if !*dragging.peek() {
                return;
            }
            dragging.set(false);
            let finished = preview.write().take();
            if let Some((group_id, next_tree)) = finished {
                up_core.invoke_or_log(Command::Splits(SplitAction::UpdateTree {
                    group_id,
                    tree: next_tree,
                }));
            }
        };

        let lit = *hovered.read() || *dragging.read() || self.highlight;
        let grip_anim = use_animation_with_dependencies(&lit, move |conf, shown| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *shown { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::SPLIT_HANDLE_FADE)
                .ease(Ease::InOut)
                .function(motion::ease_out_fn())
        });
        let grip_opacity = grip_anim.get().value().clamp(0.0, 1.0);
        let (grip_w, grip_h) = match handle.axis {
            HandleAxis::Column => (2.5, 50.0),
            HandleAxis::Row => (50.0, 2.5),
        };

        let dbl_core = core.clone();
        let dbl_tree = tree;
        rect()
            .layer(Layer::Relative(10))
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
                let now = Instant::now();
                let is_double = last_down
                    .peek()
                    .is_some_and(|prev| now.duration_since(prev) <= DOUBLE_CLICK_MS);
                if is_double {
                    last_down.set(None);
                    dragging.set(false);
                    preview.set(None);
                    let equalized = split::equalize_tree_at_handle(&dbl_tree, handle.index);
                    dbl_core.invoke_or_log(Command::Splits(SplitAction::UpdateTree {
                        group_id,
                        tree: equalized,
                    }));
                } else {
                    last_down.set(Some(now));
                    dragging.set(true);
                }
            })
            .on_global_pointer_move(on_global_pointer_move)
            .on_global_pointer_up(on_global_pointer_up)
            .child(
                rect()
                    .width(Size::px(grip_w))
                    .height(Size::px(grip_h))
                    .opacity(grip_opacity)
                    .corner_radius(CornerRadius::new_all(2.0))
                    .background(theme.color(ThemeRole::Accent)),
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
                    .padding(Gaps::new(0.0, 28.0, 0.0, 28.0))
                    .spacing(12.0)
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .main_align(Alignment::SpaceBetween)
                    .overflow(Overflow::Clip)
                    .border(Border::new().width(1.0).alignment(BorderAlignment::Inner).fill(line))
                    .child(
                        label()
                            .text(title.clone())
                            .width(Size::flex(1.0))
                            .font_size(15.0)
                            .font_weight(FontWeight::BOLD)
                            .color(text)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    )
                    .child(rect().direction(Direction::Horizontal).spacing(18.0).children(
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
    use zendia_browser_core::model::SplitDirection;

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

    #[test]
    fn drop_zone_labels_match_directions() {
        assert_eq!(drop_zone_label(DropZone::Left), "Split left");
        assert_eq!(drop_zone_label(DropZone::Right), "Split right");
        assert_eq!(drop_zone_label(DropZone::Top), "Split above");
        assert_eq!(drop_zone_label(DropZone::Bottom), "Split below");
    }
}
