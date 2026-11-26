/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Horizontal two-finger / wheel space swipe gesture in the sidebar (`ZenSpacesSwipe.mjs` + `ZenSpaceAddSwipe.mjs`).
//!
//! Swiping horizontally over the sidebar translates outgoing and incoming space tab strips side-by-side (`SWP-01`)
//! and springs smoothly on settle (`SWP-03`). Swiping past the last space enters `AddSpace` mode with a smooth Skia
//! radial gradient glow and threshold `+` jump (`SWP-02`); swiping past the first space enters `OpenLibrary` mode
//! (`SWP-04`).

use crate::ui::core_view::CoreView;
use crate::ui::kit::Icon;
use crate::ui::spaces::state::{SpaceEditingState, SpaceSwipeMode, SpaceSwipeViewState};
use crate::ui::theme::{ResolvedScheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::engine::prelude as sk;
use freya::engine::prelude::{Canvas, Paint, PaintCap, PaintStyle, PathBuilder, SkColor, SkPoint};
use freya::prelude::*;
use std::f32::consts::PI;
use std::time::Duration;
use zendia_browser_core::agent::{cos_f32, sin_f32};
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::SidebarSide;

const SWITCH_THRESHOLD_FRACTION: f32 = 0.25;
const ADD_SWIPE_DISTANCE_PX: f32 = 120.0;
const ENTRY_PROGRESS_RATIO: f32 = 0.75;
const SETTLE_TIMEOUT: Duration = Duration::from_millis(190);

#[allow(clippy::disallowed_methods)]
fn exp_f32(x: f32) -> f32 {
    x.exp()
}

/// Pure rubber-band damping helper matching `ZenSpaceAddSwipe.#rubberBand`.
pub fn rubber_band(offset: f32, dimension: f32, constant: f32) -> f32 {
    if offset == 0.0 || dimension == 0.0 {
        return 0.0;
    }
    dimension * (1.0 - exp_f32(-(offset.abs() * constant) / dimension)) * offset.signum()
}

/// Sine ease-in-out curve matching `ZenSpaceAddSwipe.#easeInOut`.
pub fn ease_in_out(x: f32) -> f32 {
    let clamped = x.clamp(0.0, 1.0);
    -(cos_f32(PI * clamped) - 1.0) * 0.5
}

/// Processes a wheel / trackpad scroll event over the sidebar tab list to drive horizontal space swiping (`SWP-05`).
pub fn handle_sidebar_wheel(
    e: &Event<WheelEventData>,
    core: &CoreView,
    editing: SpaceEditingState,
    sidebar_width: f32,
) {
    if *editing.create_form_open.peek() {
        return;
    }
    let dx = e.delta_x as f32;
    let dy = e.delta_y as f32;

    // Only react when horizontal motion clearly dominates vertical scrolling.
    if dx.abs() < 0.05 || dx.abs() < dy.abs() * 0.75 {
        return;
    }

    let (spaces, active_space) = {
        let model = core.model().peek();
        (model.spaces.clone(), model.active_space)
    };
    if spaces.is_empty() {
        return;
    }
    let active_idx = active_space
        .and_then(|id| spaces.iter().position(|s| s.id == id))
        .unwrap_or(0);

    // Scale discrete X11 horizontal wheel clicks (typically ±1..±3) and continuous trackpad deltas into smooth pixels.
    let step_px = if dx.abs() <= 4.0 { dx * 28.0 } else { dx * 0.9 };
    let width = sidebar_width.max(120.0);

    let mut swipe_state = editing.swipe;
    let mut current = swipe_state.peek().clone();
    if current.is_preview {
        return;
    }

    let next_offset = current.offset_x + step_px;
    let generation = current.generation.wrapping_add(1);
    current.generation = generation;

    let at_last = active_idx + 1 >= spaces.len();
    let at_first = active_idx == 0;

    if (current.mode == SpaceSwipeMode::AddSpace || (at_last && next_offset < -2.0)) && next_offset < 0.0 {
        current.mode = SpaceSwipeMode::AddSpace;
        current.offset_x = next_offset;
        let raw_progress = (-next_offset) / ADD_SWIPE_DISTANCE_PX;
        current.add_progress = if raw_progress > 1.0 {
            1.0 + rubber_band(raw_progress - 1.0, 0.2, 0.25)
        } else {
            raw_progress.max(0.0)
        };
        current.target_space_id = None;
    } else if (current.mode == SpaceSwipeMode::OpenLibrary || (at_first && next_offset > 2.0)) && next_offset > 0.0 {
        // SWP-04: Swiping right past the first space reveals the Library edge gesture.
        current.mode = SpaceSwipeMode::OpenLibrary;
        current.offset_x = next_offset;
        let raw_progress = next_offset / ADD_SWIPE_DISTANCE_PX;
        current.add_progress = if raw_progress > 1.0 {
            1.0 + rubber_band(raw_progress - 1.0, 0.2, 0.25)
        } else {
            raw_progress.max(0.0)
        };
        current.target_space_id = None;
    } else {
        let target_idx = if next_offset < 0.0 && !at_last {
            Some(active_idx + 1)
        } else if next_offset > 0.0 && !at_first {
            Some(active_idx - 1)
        } else {
            None
        };
        current.mode = SpaceSwipeMode::SwitchSpace;
        current.add_progress = 0.0;
        current.target_space_id = target_idx.map(|idx| spaces[idx].id);
        current.offset_x = if target_idx.is_some() {
            next_offset.clamp(-width * 0.92, width * 0.92)
        } else {
            rubber_band(next_offset, width * 0.22, 0.35)
        };
    }

    let commit_now = matches!(e.phase, TouchPhase::Ended | TouchPhase::Cancelled);
    swipe_state.set(current.clone());

    if commit_now {
        settle_swipe_animated(core.clone(), editing, width, generation);
    } else {
        let settle_core = core.clone();
        spawn(async move {
            timer(SETTLE_TIMEOUT).await;
            let latest = editing.swipe.peek().clone();
            if !latest.is_preview && latest.generation == generation && latest.mode != SpaceSwipeMode::Idle {
                settle_swipe_animated(settle_core, editing, width, generation);
            }
        });
    }
}

/// SWP-03: Smooth spring settle animating `offset_x` / `add_progress` back to `0` (cancel) or `±width` (commit)
/// before invoking the target command and resetting state.
fn settle_swipe_animated(core: CoreView, editing: SpaceEditingState, sidebar_width: f32, generation: u64) {
    let start = editing.swipe.peek().clone();
    if start.is_preview || start.mode == SpaceSwipeMode::Idle {
        return;
    }
    let width = sidebar_width.max(120.0);
    let mut swipe_state = editing.swipe;

    let (target_offset, target_progress, commit_cmd) = match start.mode {
        SpaceSwipeMode::AddSpace => {
            let ready = start.add_progress >= 0.80;
            (0.0, 0.0, ready.then_some(Command::SpaceCreateFormOpen))
        }
        SpaceSwipeMode::OpenLibrary => {
            let ready = start.add_progress >= 0.80;
            (0.0, 0.0, ready.then_some(Command::LibraryToggle { section: None }))
        }
        SpaceSwipeMode::SwitchSpace => {
            let threshold = width * SWITCH_THRESHOLD_FRACTION;
            let commit_target = start.target_space_id.filter(|_| start.offset_x.abs() >= threshold);
            let end_x = if commit_target.is_some() {
                width * start.offset_x.signum()
            } else {
                0.0
            };
            (
                end_x,
                0.0,
                commit_target.map(|space_id| Command::SpaceSwitch { space_id }),
            )
        }
        SpaceSwipeMode::Idle => return,
    };

    let steps = 12usize;
    let step_ms = (motion::SWIPE_SETTLE.as_millis() as u64 / steps as u64).max(12);
    spawn(async move {
        for i in 1..=steps {
            timer(Duration::from_millis(step_ms)).await;
            let latest = swipe_state.peek().clone();
            if latest.is_preview || latest.generation != generation {
                return;
            }
            let frac = i as f32 / steps as f32;
            let eased = 1.0 - (1.0 - frac).powi(3);
            let mut updated = latest;
            updated.offset_x = start.offset_x + (target_offset - start.offset_x) * eased;
            updated.add_progress = start.add_progress + (target_progress - start.add_progress) * eased;
            swipe_state.set(updated);
        }
        if swipe_state.peek().generation == generation {
            swipe_state.set(SpaceSwipeViewState::default());
            if let Some(cmd) = commit_cmd {
                core.invoke_or_log(cmd);
            }
        }
    });
}

/// Visual edge badge shown when swiping past the last space (`ZenSpaceAddSwipe`, `SWP-02`): smooth Skia radial glow
/// + progress arc `+` ring with `1.0 -> 1.15 -> 1.0` threshold jump animation.
#[derive(Clone, PartialEq)]
pub struct SpaceAddSwipeOverlay {
    pub progress: f32,
    pub sidebar_side: SidebarSide,
}

impl Component for SpaceAddSwipeOverlay {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let progress = self.progress.clamp(0.0, 1.25);
        let ready = progress >= 1.0;

        // SWP-02: `@keyframes zen-swipe-add-icon-jump` (`1.0 -> 1.15 -> 1.0`) when crossing progress >= 1.0.
        let jump_anim = use_animation_with_dependencies(&ready, move |conf, is_ready| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *is_ready {
                AnimNum::new(1.15, 1.0)
                    .duration(motion::SWIPE_ADD_JUMP)
                    .ease(Ease::Out)
                    .function(Function::Quad)
            } else {
                AnimNum::new(1.0, 1.0).duration(Duration::from_millis(1))
            }
        });

        if progress <= 0.01 {
            return rect().into_element();
        }

        let entry = (progress / ENTRY_PROGRESS_RATIO).clamp(0.0, 1.0);
        let is_right = self.sidebar_side == SidebarSide::Right;
        let slide_offset = (1.0 - entry) * 44.0 * if is_right { -1.0 } else { 1.0 };

        let accent = theme.color(ThemeRole::Accent);
        let ring_color = if theme.scheme == ResolvedScheme::Dark {
            SkColor::from_argb(235, 255, 255, 255)
        } else {
            SkColor::from_argb(225, 24, 26, 36)
        };
        let badge_fill = SkColor::from_argb(if ready { 235 } else { 195 }, accent.r(), accent.g(), accent.b());
        let glow_color = SkColor::from_argb((125.0 * entry).round() as u8, accent.r(), accent.g(), accent.b());

        let arc_progress = ease_in_out(progress.min(1.0));
        let ring_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_add_space_ring(
                    canvas,
                    ctx.size.width,
                    ctx.size.height,
                    arc_progress,
                    ready,
                    badge_fill,
                    ring_color,
                );
            }
        });
        let glow_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_edge_glow(canvas, ctx.size.width, ctx.size.height, is_right, glow_color);
            }
        });

        let edge_pos = if is_right {
            Position::new_absolute().left(0.0).top(0.0)
        } else {
            Position::new_absolute().right(0.0).top(0.0)
        };
        let icon_scale = jump_anim.get().value();

        rect()
            .position(edge_pos)
            .width(Size::px(76.0))
            .height(Size::percent(100.0))
            .opacity(entry)
            .interactive(Interactive::No)
            .child(
                rect()
                    .position(Position::new_absolute().left(0.0).top(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::percent(100.0))
                    .background(Fill::Callback(glow_cb)),
            )
            .child(
                rect()
                    .position(Position::new_absolute().left(0.0).top(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::percent(100.0))
                    .offset_x(slide_offset)
                    .center()
                    .child(
                        rect()
                            .width(Size::px(34.0))
                            .height(Size::px(34.0))
                            .scale(icon_scale)
                            .background(Fill::Callback(ring_cb))
                            .center()
                            .child(Icon::new("plus").size(15.0).role(ThemeRole::TextOnAccent)),
                    ),
            )
            .into_element()
    }
}

/// SWP-04: Visual edge badge shown when swiping past the first space toward the Library.
#[derive(Clone, PartialEq)]
pub struct SpaceLibrarySwipeOverlay {
    pub progress: f32,
    pub sidebar_side: SidebarSide,
}

impl Component for SpaceLibrarySwipeOverlay {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let progress = self.progress.clamp(0.0, 1.25);
        let ready = progress >= 1.0;

        let jump_anim = use_animation_with_dependencies(&ready, move |conf, is_ready| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *is_ready {
                AnimNum::new(1.15, 1.0)
                    .duration(motion::SWIPE_ADD_JUMP)
                    .ease(Ease::Out)
                    .function(Function::Quad)
            } else {
                AnimNum::new(1.0, 1.0).duration(Duration::from_millis(1))
            }
        });

        if progress <= 0.01 {
            return rect().into_element();
        }

        let entry = (progress / ENTRY_PROGRESS_RATIO).clamp(0.0, 1.0);
        // Opposite edge from AddSpace.
        let glow_on_left = self.sidebar_side != SidebarSide::Right;
        let slide_offset = (1.0 - entry) * 44.0 * if glow_on_left { -1.0 } else { 1.0 };

        let accent = theme.color(ThemeRole::Accent);
        let ring_color = if theme.scheme == ResolvedScheme::Dark {
            SkColor::from_argb(235, 255, 255, 255)
        } else {
            SkColor::from_argb(225, 24, 26, 36)
        };
        let badge_fill = SkColor::from_argb(if ready { 235 } else { 195 }, accent.r(), accent.g(), accent.b());
        let glow_color = SkColor::from_argb((125.0 * entry).round() as u8, accent.r(), accent.g(), accent.b());

        let arc_progress = ease_in_out(progress.min(1.0));
        let ring_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_add_space_ring(
                    canvas,
                    ctx.size.width,
                    ctx.size.height,
                    arc_progress,
                    ready,
                    badge_fill,
                    ring_color,
                );
            }
        });
        let glow_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_edge_glow(canvas, ctx.size.width, ctx.size.height, glow_on_left, glow_color);
            }
        });

        let edge_pos = if glow_on_left {
            Position::new_absolute().left(0.0).top(0.0)
        } else {
            Position::new_absolute().right(0.0).top(0.0)
        };

        rect()
            .position(edge_pos)
            .width(Size::px(76.0))
            .height(Size::percent(100.0))
            .opacity(entry)
            .interactive(Interactive::No)
            .child(
                rect()
                    .position(Position::new_absolute().left(0.0).top(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::percent(100.0))
                    .background(Fill::Callback(glow_cb)),
            )
            .child(
                rect()
                    .position(Position::new_absolute().left(0.0).top(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::percent(100.0))
                    .offset_x(slide_offset)
                    .center()
                    .child(
                        rect()
                            .width(Size::px(34.0))
                            .height(Size::px(34.0))
                            .scale(jump_anim.get().value())
                            .background(Fill::Callback(ring_cb))
                            .center()
                            .child(Icon::new("library").size(15.0).role(ThemeRole::TextOnAccent)),
                    ),
            )
            .into_element()
    }
}

/// Kept for public API compatibility (gallery/external callers).
#[derive(Clone, PartialEq)]
pub struct SpaceSwipeIndicator {
    pub target_name: String,
    pub target_icon: String,
    pub offset_x: f32,
}

impl Component for SpaceSwipeIndicator {
    fn render(&self) -> impl IntoElement {
        rect().into_element()
    }
}

fn paint_add_space_ring(
    canvas: &Canvas,
    width: f32,
    height: f32,
    progress: f32,
    ready: bool,
    fill_color: SkColor,
    arc_color: SkColor,
) {
    let cx = width * 0.5;
    let cy = height * 0.5;
    let r = (width.min(height) * 0.5) - 1.5;

    let mut fill_paint = Paint::default();
    fill_paint.set_anti_alias(true);
    fill_paint.set_style(PaintStyle::Fill);
    fill_paint.set_color(if ready { arc_color } else { fill_color });
    canvas.draw_circle(SkPoint::new(cx, cy), r, &fill_paint);

    if !ready && progress > 0.01 {
        let mut arc_paint = Paint::default();
        arc_paint.set_anti_alias(true);
        arc_paint.set_style(PaintStyle::Stroke);
        arc_paint.set_stroke_width(2.6);
        arc_paint.set_stroke_cap(PaintCap::Round);
        arc_paint.set_color(arc_color);

        let arc_r = r - 1.5;
        let sweep = progress.clamp(0.0, 1.0) * 2.0 * PI;
        let steps = ((sweep / 0.12).ceil() as usize).max(4);
        let mut pb = PathBuilder::new();
        for i in 0..=steps {
            let a = -PI * 0.5 + sweep * (i as f32 / steps as f32);
            let pt = SkPoint::new(cx + arc_r * cos_f32(a), cy + arc_r * sin_f32(a));
            if i == 0 {
                pb.move_to(pt);
            } else {
                pb.line_to(pt);
            }
        }
        canvas.draw_path(&pb.detach(), &arc_paint);
    }
}

/// SWP-02: Smooth multi-stop Skia radial gradient glow along the swipe edge (no hard concentric bands).
fn paint_edge_glow(canvas: &Canvas, width: f32, height: f32, glow_on_left: bool, color: SkColor) {
    if width <= 1.0 || height <= 1.0 {
        return;
    }
    let cx = if glow_on_left { 0.0 } else { width };
    let cy = height * 0.5;
    let radius = width.max(height * 0.36);
    let a = color.a() as f32;
    let colors = [
        sk::Color4f::from(SkColor::from_argb(a.round() as u8, color.r(), color.g(), color.b())),
        sk::Color4f::from(SkColor::from_argb(
            (a * 0.48).round() as u8,
            color.r(),
            color.g(),
            color.b(),
        )),
        sk::Color4f::from(SkColor::from_argb(
            (a * 0.14).round() as u8,
            color.r(),
            color.g(),
            color.b(),
        )),
        sk::Color4f::from(SkColor::from_argb(0, color.r(), color.g(), color.b())),
    ];
    let positions = [0.0_f32, 0.38_f32, 0.72_f32, 1.0_f32];
    let grad = sk::gradient::Gradient::new(
        sk::gradient::Colors::new(&colors, Some(&positions), sk::TileMode::Clamp, None),
        sk::gradient::Interpolation::default(),
    );
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    if let Some(shader) = sk::gradient::shaders::radial_gradient((SkPoint::new(cx, cy), radius), &grad, None) {
        paint.set_shader(shader);
        canvas.draw_rect(sk::Rect::from_xywh(0.0, 0.0, width, height), &paint);
    }
}
