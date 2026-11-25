/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Onboarding Living Emblem mascot built on Phase 5's Voronoi mascot geometry (`ui/vim/mascot.rs`).
//!
//! Adds the onboarding-specific behaviors from `Mascot2D.jsx`: cell-by-cell jelly bloom entrance (`780 ms`), playful
//! squash-and-stretch nudge hop when choices change (`380 ms`), periodic blinking, pointer gaze tracking, and
//! interactive per-cell hit-testing so clicking or hovering any Voronoi petal morphs into that mode.

use crate::ui::onboarding::theme::{MODE_ORDER, clamp01, cos_f32, exp_f32, onboarding_spring, sin_f32, sk_color};
use crate::ui::theme::Rgba;
use crate::ui::vim::mascot::{MASCOT_MORPH_DURATION, compute_mascot_frame, mascot_mode_color, pose_index};
use crate::ui::vim::mascot_data::{DRAWINGS, LetterSpec, VIEWBOX_HEIGHT, VIEWBOX_WIDTH, VIEWBOX_X, VIEWBOX_Y};
use freya::animation::*;
use freya::engine::prelude as sk;
use freya::prelude::*;
use std::f32::consts::PI;
use std::time::Duration;
use zendia_browser_core::model::InputMode;

const BLOOM_DURATION: Duration = Duration::from_millis(780);
const BLINK_DURATION: Duration = Duration::from_millis(165);
const NUDGE_DURATION: Duration = Duration::from_millis(380);
const MASCOT_EDGE_WIDTH: f32 = 40.0;
const MASCOT_INK: Rgba = Rgba::rgb(0x1a, 0x1b, 0x26);
const RING_CELL_COLORS: [Rgba; 6] = [
    Rgba::rgb(0xfc, 0xa7, 0xea),
    Rgba::rgb(0xc3, 0xe8, 0x8d),
    Rgba::rgb(0xc0, 0x99, 0xff),
    Rgba::rgb(0x82, 0xaa, 0xff),
    Rgba::rgb(0xff, 0xc7, 0x77),
    Rgba::rgb(0x4f, 0xd6, 0xbe),
];

fn centroid(pts: &[[f32; 2]]) -> [f32; 2] {
    if pts.is_empty() {
        return [512.0, 512.0];
    }
    let inv = 1.0 / pts.len() as f32;
    let (mut sx, mut sy) = (0.0, 0.0);
    for &[x, y] in pts {
        sx += x;
        sy += y;
    }
    [sx * inv, sy * inv]
}

fn build_scaled_polygon(pts: &[[f32; 2]], scale: f32, center: [f32; 2]) -> sk::Path {
    let mut builder = sk::PathBuilder::new();
    if let Some((first, rest)) = pts.split_first() {
        let xf = |p: [f32; 2]| -> sk::Point {
            sk::Point::new(
                center[0] + (p[0] - center[0]) * scale,
                center[1] + (p[1] - center[1]) * scale,
            )
        };
        builder.move_to(xf(*first));
        for &p in rest {
            builder.line_to(xf(p));
        }
        builder.close();
    }
    builder.detach()
}

fn build_letter_path(spec: &'static LetterSpec, scale: f32) -> sk::Path {
    let mut builder = sk::PathBuilder::new();
    builder.set_fill_type(sk::PathFillType::EvenOdd);
    let rad = spec.lean_deg.to_radians();
    let cos = cos_f32(rad);
    let sin = sin_f32(rad);
    let tx = spec.x;
    let ty = spec.y;
    for poly in spec.polygons {
        if let Some((first, rest)) = poly.split_first() {
            let transform = |pt: [f32; 2]| -> sk::Point {
                let sx = pt[0] * scale;
                let sy = pt[1] * scale;
                sk::Point::new(tx + sx * cos - sy * sin, ty + sx * sin + sy * cos)
            };
            builder.move_to(transform(*first));
            for &pt in rest {
                builder.line_to(transform(pt));
            }
            builder.close();
        }
    }
    builder.detach()
}

/// Point-in-polygon hit test in the 1024 mascot coordinate space.
fn point_in_polygon(pt: [f32; 2], poly: &[[f32; 2]]) -> bool {
    let mut inside = false;
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (poly[i][0], poly[i][1]);
        let (xj, yj) = (poly[j][0], poly[j][1]);
        let intersects = ((yi > pt[1]) != (yj > pt[1]))
            && (pt[0] < (xj - xi) * (pt[1] - yi) / ((yj - yi).abs().max(1e-6) * (yj - yi).signum()) + xi);
        if intersects {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Maps a local pointer position `(lx, ly)` inside a `(width, height)` mascot box to the Voronoi mode cell under it.
pub fn hit_test_mascot_cell(lx: f32, ly: f32, width: f32, height: f32, current_mode: InputMode) -> Option<InputMode> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let pad = 16.0;
    let vb_x = VIEWBOX_X - pad;
    let vb_y = VIEWBOX_Y - pad;
    let vb_w = VIEWBOX_WIDTH + pad * 2.0;
    let vb_h = VIEWBOX_HEIGHT + pad * 2.0;
    let scale = (width / vb_w).min(height / vb_h);
    let offset_x = (width - vb_w * scale) * 0.5;
    let offset_y = (height - vb_h * scale) * 0.5;
    let mx = (lx - offset_x) / scale + vb_x;
    let my = (ly - offset_y) / scale + vb_y;

    let drawing = DRAWINGS[pose_index(current_mode)];
    for (i, cell) in drawing.cells.iter().enumerate() {
        if point_in_polygon([mx, my], cell) {
            return MODE_ORDER.get(i).copied();
        }
    }
    None
}

/// Extra painting options for [`paint_onboarding_mascot`].
#[derive(Clone, Copy, Debug)]
pub struct MascotPaintOptions {
    pub from: InputMode,
    pub to: InputMode,
    pub morph_t: f32,
    pub bloom_t: f32,
    pub blink_t: f32,
    pub nudge_t: f32,
    pub gaze: (f32, f32),
    pub hovered_cell: Option<usize>,
    pub drop_shadow: bool,
}

impl MascotPaintOptions {
    pub fn still(mode: InputMode) -> Self {
        Self {
            from: mode,
            to: mode,
            morph_t: 1.0,
            bloom_t: 1.0,
            blink_t: 0.0,
            nudge_t: 1.0,
            gaze: (0.0, 0.0),
            hovered_cell: None,
            drop_shadow: false,
        }
    }
}

/// Paints the onboarding Living Emblem mascot directly onto a Skia [`sk::Canvas`], filling `(width, height)`.
pub fn paint_onboarding_mascot(canvas: &sk::Canvas, width: f32, height: f32, opts: MascotPaintOptions) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let frame = compute_mascot_frame(opts.from, opts.to, opts.morph_t, opts.blink_t);
    let lead_index = pose_index(opts.to);
    let bloom_t = clamp01(opts.bloom_t);
    let head_center = centroid(&frame.head);
    let head_bloom = if bloom_t >= 1.0 {
        1.0
    } else {
        onboarding_spring(clamp01(bloom_t / 0.45)).max(0.0)
    };
    let letter_bloom = if bloom_t >= 1.0 {
        1.0
    } else {
        onboarding_spring(clamp01((bloom_t - 0.52) / 0.48)).max(0.0)
    };

    let cell_scales: [(f32, [f32; 2]); 6] = std::array::from_fn(|i| {
        let ring_pos = (i + 6 - lead_index) % 6;
        let delay = 0.09 + ring_pos as f32 * 0.075;
        let base_scale = if bloom_t >= 1.0 {
            1.0
        } else {
            onboarding_spring(clamp01((bloom_t - delay) / 0.5)).max(0.0)
        };
        let hover_boost = if opts.hovered_cell == Some(i) { 1.045 } else { 1.0 };
        (base_scale * hover_boost, centroid(&frame.cells[i]))
    });

    // Subtle vertical squash-and-stretch hop on nudge.
    let nudge_t = clamp01(opts.nudge_t);
    let nudge_wave = if nudge_t < 1.0 {
        exp_f32(-5.5 * nudge_t) * sin_f32(PI * 2.2 * nudge_t)
    } else {
        0.0
    };
    let nudge_sx = 1.0 - nudge_wave * 0.06;
    let nudge_sy = 1.0 + nudge_wave * 0.07;
    let nudge_ty = -nudge_wave.max(0.0) * 24.0;

    let pad = if opts.drop_shadow { 64.0 } else { 16.0 };
    let vb_x = VIEWBOX_X - pad;
    let vb_y = VIEWBOX_Y - pad;
    let vb_w = VIEWBOX_WIDTH + pad * 2.0;
    let vb_h = VIEWBOX_HEIGHT + pad * 2.0;
    let scale = (width / vb_w).min(height / vb_h);
    let offset_x = (width - vb_w * scale) * 0.5;
    let offset_y = (height - vb_h * scale) * 0.5;

    let save = canvas.save();
    canvas.translate((offset_x, offset_y));
    canvas.scale((scale, scale));
    canvas.translate((-vb_x, -vb_y));

    if nudge_wave.abs() > 0.001 {
        canvas.translate((512.0, 512.0 + nudge_ty));
        canvas.scale((nudge_sx, nudge_sy));
        canvas.translate((-512.0, -512.0));
    }

    let cell_paths: [Option<sk::Path>; 6] = std::array::from_fn(|i| {
        let (s, center) = cell_scales[i];
        (s >= 0.01).then(|| build_scaled_polygon(&frame.cells[i], s, center))
    });
    let head_path = (head_bloom >= 0.01).then(|| build_scaled_polygon(&frame.head, head_bloom, head_center));

    // Optional soft drop shadow matching StartScene2D & GoScene2D emblem badge.
    if opts.drop_shadow {
        let mut shadow_paint = sk::Paint::default();
        shadow_paint.set_anti_alias(true);
        shadow_paint.set_style(sk::PaintStyle::Fill);
        shadow_paint.set_color(sk::Color::from_argb(38, 0, 0, 0));
        shadow_paint.set_mask_filter(sk::MaskFilter::blur(sk::BlurStyle::Normal, 22.0, false));
        canvas.save();
        canvas.translate((0.0, 22.0));
        for path in cell_paths.iter().flatten() {
            canvas.draw_path(path, &shadow_paint);
        }
        canvas.restore();
    }

    // 1. Dark ink seam behind all cells and head.
    let mut edge_fill = sk::Paint::default();
    edge_fill.set_anti_alias(true);
    edge_fill.set_style(sk::PaintStyle::Fill);
    edge_fill.set_color(sk_color(MASCOT_INK));

    let mut edge_stroke = sk::Paint::default();
    edge_stroke.set_anti_alias(true);
    edge_stroke.set_style(sk::PaintStyle::Stroke);
    edge_stroke.set_stroke_width(MASCOT_EDGE_WIDTH);
    edge_stroke.set_stroke_join(sk::PaintJoin::Round);
    edge_stroke.set_stroke_cap(sk::PaintCap::Round);
    edge_stroke.set_color(sk_color(MASCOT_INK));

    for path in cell_paths.iter().flatten() {
        canvas.draw_path(path, &edge_fill);
        canvas.draw_path(path, &edge_stroke);
    }
    if let Some(ref hp) = head_path {
        canvas.draw_path(hp, &edge_fill);
        canvas.draw_path(hp, &edge_stroke);
    }

    // 2. Six Voronoi cells in luminous Tokyo Night pastels.
    let mut fill = sk::Paint::default();
    fill.set_anti_alias(true);
    fill.set_style(sk::PaintStyle::Fill);
    for (i, path) in cell_paths.iter().enumerate() {
        if let Some(p) = path {
            fill.set_color(sk_color(RING_CELL_COLORS[i]));
            canvas.draw_path(p, &fill);
        }
    }

    // 3. Head and bolt antenna + eyes with optional gaze offset.
    if let Some(ref hp) = head_path {
        let head_color = mascot_mode_color(opts.from).mix(mascot_mode_color(opts.to), frame.color_t);
        fill.set_color(sk_color(head_color));
        canvas.draw_path(hp, &fill);

        let mut eye_paint = sk::Paint::default();
        eye_paint.set_anti_alias(true);
        eye_paint.set_style(sk::PaintStyle::Stroke);
        eye_paint.set_stroke_cap(sk::PaintCap::Round);
        eye_paint.set_color(sk_color(MASCOT_INK));
        let (gx, gy) = opts.gaze;
        for &(p0, p1, stroke_w) in &frame.eyes {
            if stroke_w > 0.1 {
                eye_paint.set_stroke_width(stroke_w * head_bloom);
                let x0 = head_center[0] + (p0[0] + gx - head_center[0]) * head_bloom;
                let y0 = head_center[1] + (p0[1] + gy - head_center[1]) * head_bloom;
                let x1 = head_center[0] + (p1[0] + gx - head_center[0]) * head_bloom;
                let y1 = head_center[1] + (p1[1] + gy - head_center[1]) * head_bloom;
                canvas.draw_line(sk::Point::new(x0, y0), sk::Point::new(x1, y1), &eye_paint);
            }
        }
    }

    // 4. Carved mode letter on the leading cell.
    if letter_bloom > 0.01 {
        fill.set_color(sk_color(MASCOT_INK));
        for letter in &frame.letters {
            let path = build_letter_path(letter.spec, letter.scale * letter_bloom);
            canvas.draw_path(&path, &fill);
        }
    }

    canvas.restore_to_count(save);
}

/// Interactive Living Emblem mascot component for onboarding surfaces.
#[derive(Clone, PartialEq)]
pub struct OnboardingMascot {
    pub pose: InputMode,
    pub size: f32,
    pub still: bool,
    pub bloom: bool,
    pub blink: bool,
    pub track_pointer: bool,
    pub interactive: bool,
    pub drop_shadow: bool,
    pub nudge_seq: u64,
    pub pinned_bloom: Option<f32>,
    pub pinned_morph: Option<(InputMode, f32)>,
    pub on_select_pose: Option<EventHandler<InputMode>>,
    pub on_hover_pose: Option<EventHandler<InputMode>>,
    pub key: DiffKey,
}

impl KeyExt for OnboardingMascot {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl OnboardingMascot {
    pub fn new(pose: InputMode, size: f32) -> Self {
        Self {
            pose,
            size,
            still: false,
            bloom: false,
            blink: true,
            track_pointer: false,
            interactive: false,
            drop_shadow: false,
            nudge_seq: 0,
            pinned_bloom: None,
            pinned_morph: None,
            on_select_pose: None,
            on_hover_pose: None,
            key: DiffKey::None,
        }
    }

    pub fn still(mut self, still: bool) -> Self {
        self.still = still;
        self
    }

    pub fn bloom(mut self, bloom: bool) -> Self {
        self.bloom = bloom;
        self
    }

    pub fn blink(mut self, blink: bool) -> Self {
        self.blink = blink;
        self
    }

    pub fn track_pointer(mut self, track: bool) -> Self {
        self.track_pointer = track;
        self
    }

    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    pub fn drop_shadow(mut self, shadow: bool) -> Self {
        self.drop_shadow = shadow;
        self
    }

    pub fn nudge_seq(mut self, seq: u64) -> Self {
        self.nudge_seq = seq;
        self
    }

    pub fn pinned_morph(mut self, morph: Option<(InputMode, f32)>) -> Self {
        self.pinned_morph = morph;
        self
    }

    pub fn on_select_pose(mut self, handler: impl Into<EventHandler<InputMode>>) -> Self {
        self.on_select_pose = Some(handler.into());
        self
    }

    pub fn on_hover_pose(mut self, handler: impl Into<EventHandler<InputMode>>) -> Self {
        self.on_hover_pose = Some(handler.into());
        self
    }
}

impl Component for OnboardingMascot {
    fn render(&self) -> impl IntoElement {
        let mut last_mode = use_state(|| self.pose);
        let mut from_mode = use_state(|| self.pose);
        let mut morph_seq = use_state(|| 0u64);
        let mut blink_seq = use_state(|| 0u64);
        let mut hovered_cell = use_state(|| None::<usize>);
        let mut gaze = use_state(|| (0.0_f32, 0.0_f32));
        let mut box_area = use_state(Area::default);

        if *last_mode.peek() != self.pose {
            let prev = *last_mode.peek();
            from_mode.set(prev);
            last_mode.set(self.pose);
            let next = *morph_seq.peek() + 1;
            morph_seq.set(next);
        }

        let seq = *morph_seq.read();
        let still = self.still;
        let morph_anim = use_animation_with_dependencies(&seq, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(if still { OnChange::Finish } else { OnChange::Rerun });
            AnimNum::new(0.0, 1.0)
                .duration(MASCOT_MORPH_DURATION)
                .ease(Ease::Out)
                .function(Function::Linear)
        });

        let should_bloom = self.bloom && !self.still && self.pinned_bloom.is_none();
        let bloom_anim = use_animation(move |conf| {
            conf.on_creation(if should_bloom {
                OnCreation::Run
            } else {
                OnCreation::Finish
            });
            AnimNum::new(0.0, 1.0)
                .duration(BLOOM_DURATION)
                .ease(Ease::Out)
                .function(Function::Linear)
        });

        let nudge_seq = self.nudge_seq;
        let nudge_anim = use_animation_with_dependencies(&nudge_seq, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(if still { OnChange::Finish } else { OnChange::Rerun });
            AnimNum::new(0.0, 1.0)
                .duration(NUDGE_DURATION)
                .ease(Ease::Out)
                .function(Function::Linear)
        });

        let disable_blink = !self.blink || self.still || self.pinned_morph.is_some();
        use_hook(move || {
            if disable_blink {
                return;
            }
            spawn(async move {
                loop {
                    timer(Duration::from_millis(3600)).await;
                    let next = *blink_seq.peek() + 1;
                    blink_seq.set(next);
                }
            });
        });

        let b_seq = *blink_seq.read();
        let blink_anim = use_animation_with_dependencies(&b_seq, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(BLINK_DURATION)
                .ease(Ease::InOut)
                .function(Function::Linear)
        });

        let (morph_from, morph_t) = match self.pinned_morph {
            Some((from, t)) => (from, t),
            None if self.still => (self.pose, 1.0),
            None => (*from_mode.read(), morph_anim.get().value().clamp(0.0, 1.0)),
        };
        let bloom_t = match self.pinned_bloom {
            Some(b) => b,
            None if self.still || !self.bloom => 1.0,
            None => bloom_anim.get().value().clamp(0.0, 1.0),
        };
        let nudge_t = if self.still || nudge_seq == 0 {
            1.0
        } else {
            nudge_anim.get().value().clamp(0.0, 1.0)
        };
        let blink_t = if disable_blink || morph_t < 1.0 {
            0.0
        } else {
            let raw = blink_anim.get().value().clamp(0.0, 1.0);
            if raw <= 0.001 || raw >= 0.999 {
                0.0
            } else {
                let shut = if raw < 0.42 {
                    raw / 0.42
                } else {
                    1.0 - (raw - 0.42) / 0.58
                };
                sin_f32(clamp01(shut) * PI * 0.5)
            }
        };

        let opts = MascotPaintOptions {
            from: morph_from,
            to: self.pose,
            morph_t,
            bloom_t,
            blink_t,
            nudge_t,
            gaze: if self.track_pointer && !self.still {
                *gaze.read()
            } else {
                (0.0, 0.0)
            },
            hovered_cell: if self.interactive { *hovered_cell.read() } else { None },
            drop_shadow: self.drop_shadow,
        };

        let callback = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_onboarding_mascot(canvas, ctx.size.width, ctx.size.height, opts);
            }
        });

        let pose = self.pose;
        let interactive = self.interactive;
        let track_pointer = self.track_pointer && !self.still;
        let on_hover_pose = self.on_hover_pose.clone();
        let on_select_pose = self.on_select_pose.clone();

        let on_mouse_move = move |e: Event<MouseEventData>| {
            let area = *box_area.peek();
            let loc = e.global_location;
            let lx = loc.x as f32 - area.min_x();
            let ly = loc.y as f32 - area.min_y();
            if track_pointer && area.width() > 0.0 && area.height() > 0.0 {
                let dx = lx - area.width() * 0.5;
                let dy = ly - area.height() * 0.5;
                let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                let mag = (dist / 220.0).min(1.0);
                gaze.set_if_modified(((dx / dist) * mag * 14.0, (dy / dist) * mag * 10.0));
            }
            if interactive {
                let hit = hit_test_mascot_cell(lx, ly, area.width(), area.height(), pose);
                let hit_idx = hit.map(pose_index);
                if *hovered_cell.peek() != hit_idx {
                    hovered_cell.set(hit_idx);
                    if let (Some(mode), Some(cb)) = (hit, &on_hover_pose) {
                        cb.call(mode);
                    }
                }
            }
        };

        let on_mouse_up = move |e: Event<MouseEventData>| {
            if !interactive {
                return;
            }
            let area = *box_area.peek();
            let loc = e.global_location;
            let lx = loc.x as f32 - area.min_x();
            let ly = loc.y as f32 - area.min_y();
            if let Some(mode) = hit_test_mascot_cell(lx, ly, area.width(), area.height(), pose)
                && let Some(cb) = &on_select_pose
            {
                cb.call(mode);
            }
        };

        let on_pointer_leave = move |_: Event<PointerEventData>| {
            hovered_cell.set_if_modified(None);
            gaze.set_if_modified((0.0, 0.0));
        };

        let mut el = rect()
            .width(Size::px(self.size))
            .height(Size::px(self.size))
            .background(Fill::Callback(callback))
            .on_sized(move |e: Event<SizedEventData>| box_area.set_if_modified(e.area))
            .on_mouse_move(on_mouse_move)
            .on_mouse_up(on_mouse_up)
            .on_pointer_leave(on_pointer_leave);
        if interactive && hovered_cell.read().is_some() {
            el = el.cursor(CursorIcon::Pointer);
        }
        el
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
