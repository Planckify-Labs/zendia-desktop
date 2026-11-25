/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Horizontal two-finger / wheel space swipe gesture in the sidebar (`ZenSpacesSwipe.mjs` + `ZenSpaceAddSwipe.mjs`).
//!
//! Swiping horizontally over the sidebar translates the tab list toward the adjacent space and commits a
//! `Command::SpaceSwitch` once past the threshold. Swiping past the last space enters `AddSpace` mode with rubber-band
//! damping and a circular `+` progress ring badge; completing the swipe invokes `Command::SpaceCreateFormOpen`.

use crate::ui::core_view::CoreView;
use crate::ui::kit::Icon;
use crate::ui::spaces::state::{SpaceEditingState, SpaceSwipeMode, SpaceSwipeViewState};
use crate::ui::theme::{ResolvedScheme, ThemeRole, use_theme};
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

/// Processes a wheel / trackpad scroll event over the sidebar to drive horizontal space swiping.
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

    // Swiping left (negative offset_x) moves to the next space, or offers Add Space past the last space.
    // Swiping right (positive offset_x) moves to the previous space.
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
        // Apply rubber-band resistance if there is no neighbor in that direction.
        current.offset_x = if target_idx.is_some() {
            next_offset.clamp(-width * 0.85, width * 0.85)
        } else {
            rubber_band(next_offset, width * 0.22, 0.35)
        };
    }

    let commit_now = matches!(e.phase, TouchPhase::Ended | TouchPhase::Cancelled);
    swipe_state.set(current.clone());

    if commit_now {
        settle_swipe(core, editing, width);
    } else {
        let settle_core = core.clone();
        spawn(async move {
            timer(SETTLE_TIMEOUT).await;
            let latest = editing.swipe.peek().clone();
            if !latest.is_preview && latest.generation == generation && latest.mode != SpaceSwipeMode::Idle {
                settle_swipe(&settle_core, editing, width);
            }
        });
    }
}

fn settle_swipe(core: &CoreView, editing: SpaceEditingState, sidebar_width: f32) {
    let current = editing.swipe.peek().clone();
    let mut swipe_state = editing.swipe;

    match current.mode {
        SpaceSwipeMode::AddSpace => {
            let ready = current.add_progress >= 0.80;
            swipe_state.set(SpaceSwipeViewState::default());
            if ready {
                core.invoke_or_log(Command::SpaceCreateFormOpen);
            }
        }
        SpaceSwipeMode::SwitchSpace => {
            let threshold = sidebar_width.max(120.0) * SWITCH_THRESHOLD_FRACTION;
            let target = current.target_space_id.filter(|_| current.offset_x.abs() >= threshold);
            swipe_state.set(SpaceSwipeViewState::default());
            if let Some(space_id) = target {
                core.invoke_or_log(Command::SpaceSwitch { space_id });
            }
        }
        SpaceSwipeMode::Idle => {}
    }
}

/// Visual edge badge shown when swiping past the last space (`ZenSpaceAddSwipe`): radial glow + progress arc `+` ring.
#[derive(Clone, PartialEq)]
pub struct SpaceAddSwipeOverlay {
    pub progress: f32,
    pub sidebar_side: SidebarSide,
}

impl Component for SpaceAddSwipeOverlay {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let progress = self.progress.clamp(0.0, 1.25);
        if progress <= 0.01 {
            return rect().into_element();
        }

        let entry = (progress / ENTRY_PROGRESS_RATIO).clamp(0.0, 1.0);
        let ready = progress >= 1.0;
        let is_right = self.sidebar_side == SidebarSide::Right;
        let slide_offset = (1.0 - entry) * 44.0 * if is_right { -1.0 } else { 1.0 };

        let accent = theme.color(ThemeRole::Accent);
        let ring_color = if theme.scheme == ResolvedScheme::Dark {
            SkColor::from_argb(235, 255, 255, 255)
        } else {
            SkColor::from_argb(225, 24, 26, 36)
        };
        let badge_fill = SkColor::from_argb(if ready { 235 } else { 195 }, accent.r(), accent.g(), accent.b());
        let glow_color = SkColor::from_argb((72.0 * entry).round() as u8, accent.r(), accent.g(), accent.b());

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

        rect()
            .position(edge_pos)
            .width(Size::px(68.0))
            .height(Size::percent(100.0))
            .opacity(entry)
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
                            .background(Fill::Callback(ring_cb))
                            .center()
                            .child(Icon::new("plus").size(15.0).role(ThemeRole::Text)),
                    ),
            )
            .into_element()
    }
}

/// Subtle pill cue shown near the edge of the sidebar while swiping between two existing spaces.
#[derive(Clone, PartialEq)]
pub struct SpaceSwipeIndicator {
    pub target_name: String,
    pub target_icon: String,
    pub offset_x: f32,
}

impl Component for SpaceSwipeIndicator {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let progress = (self.offset_x.abs() / 54.0).clamp(0.0, 1.0);
        if progress < 0.08 {
            return rect().into_element();
        }
        let from_right = self.offset_x < 0.0;
        let bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(215, 28, 30, 38),
            ResolvedScheme::Light => Color::from_argb(230, 250, 250, 254),
        };
        let pos = if from_right {
            Position::new_absolute().right(8.0).top(108.0)
        } else {
            Position::new_absolute().left(8.0).top(108.0)
        };
        rect()
            .position(pos)
            .padding(Gaps::new(5.0, 10.0, 5.0, 10.0))
            .spacing(6.0)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(999.0))
            .background(bg)
            .opacity(progress)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .shadow(Shadow::new().y(2.0).blur(10.0).color(Color::from_argb(72, 0, 0, 0)))
            .maybe_child(
                (!self.target_icon.is_empty())
                    .then(|| Icon::new(self.target_icon.clone()).size(12.0).role(ThemeRole::Text)),
            )
            .child(
                label()
                    .text(self.target_name.clone())
                    .font_size(11.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(theme.color(ThemeRole::Text))
                    .max_lines(1),
            )
            .into_element()
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

fn paint_edge_glow(canvas: &Canvas, width: f32, height: f32, is_right: bool, color: SkColor) {
    let cx = if is_right { 0.0 } else { width };
    let cy = height * 0.5;
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Fill);
    for (radius_mul, alpha_mul) in [(1.15f32, 0.35f32), (0.82, 0.55), (0.50, 0.80)] {
        let a = ((color.a() as f32) * alpha_mul).round() as u8;
        paint.set_color(SkColor::from_argb(a, color.r(), color.g(), color.b()));
        canvas.draw_circle(SkPoint::new(cx, cy), width * radius_mul, &paint);
    }
}
