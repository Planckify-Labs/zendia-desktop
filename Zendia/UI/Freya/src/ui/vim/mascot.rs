/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Zendia's animated six-cell mascot rendered natively on Skia via Freya's [`RenderCallback`].
//!
//! Between vim modes the six cells reshape on a damped jelly spring with radial squash-and-stretch, the head and bolt
//! antenna bend into the next pose, the eyes follow (and blink periodically), and the leading cell's letter springs in.

use crate::ui::theme::{Rgba, use_theme};
use crate::ui::vim::mascot_data::{DRAWINGS, LetterSpec, VIEWBOX_HEIGHT, VIEWBOX_WIDTH, VIEWBOX_X, VIEWBOX_Y};
use freya::animation::*;
use freya::engine::prelude::{
    Canvas, Paint, PaintCap, PaintJoin, PaintStyle, PathBuilder, SkColor, SkPathFillType, SkPoint,
};
use freya::prelude::*;
use std::f32::consts::PI;
use std::time::Duration;
use zendia_browser_core::model::InputMode;

/// Full-size layout width and height of the mascot in logical pixels.
pub const MASCOT_FULL_SIZE: f32 = 132.0;
/// Scale factor when the statusline rests shrunk in the corner (`zendia.statusline.show = "shrink"`).
pub const MASCOT_REST_SCALE: f32 = 0.4;
/// Duration of the jelly morph between modes.
pub const MASCOT_MORPH_DURATION: Duration = Duration::from_millis(640);
/// Ink outline width on the 1024 emblem grid (half shows outside each filled cell).
const MASCOT_EDGE_WIDTH: f32 = 40.0;
/// Spring oscillation frequency in cycles per normalized duration.
const SPRING_FREQUENCY: f32 = 2.4;
/// Exponential damping factor for the jelly spring.
const SPRING_DAMPING: f32 = 6.5;
/// Blink duration in milliseconds.
const BLINK_DURATION: Duration = Duration::from_millis(180);

/// Bright dark-variant Tokyonight mode colors (the mascot stays equally vivid in both light and dark themes).
pub fn mascot_mode_color(mode: InputMode) -> Rgba {
    match mode {
        InputMode::Normal => Rgba::rgb(0xfc, 0xa7, 0xea),
        InputMode::Insert => Rgba::rgb(0xc3, 0xe8, 0x8d),
        InputMode::Visual => Rgba::rgb(0xc0, 0x99, 0xff),
        InputMode::Explorer => Rgba::rgb(0x82, 0xaa, 0xff),
        InputMode::Toolbar => Rgba::rgb(0xff, 0xc7, 0x77),
        InputMode::Library => Rgba::rgb(0x4f, 0xd6, 0xbe),
        InputMode::Window => Rgba::rgb(0xff, 0x96, 0x6c),
    }
}

/// Six ring cell colors in order (`n`, `i`, `v`, `e`, `t`, `l`).
const RING_CELL_COLORS: [Rgba; 6] = [
    Rgba::rgb(0xfc, 0xa7, 0xea),
    Rgba::rgb(0xc3, 0xe8, 0x8d),
    Rgba::rgb(0xc0, 0x99, 0xff),
    Rgba::rgb(0x82, 0xaa, 0xff),
    Rgba::rgb(0xff, 0xc7, 0x77),
    Rgba::rgb(0x4f, 0xd6, 0xbe),
];

/// Maps any [`InputMode`] to one of the six pose drawings (`Window` wears `Normal`'s pose in orange).
pub fn pose_index(mode: InputMode) -> usize {
    match mode {
        InputMode::Normal | InputMode::Window => 0,
        InputMode::Insert => 1,
        InputMode::Visual => 2,
        InputMode::Explorer => 3,
        InputMode::Toolbar => 4,
        InputMode::Library => 5,
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Pure-Rust deterministic sine (avoids platform-dependent libm `f32::sin`).
fn sin_f32(x: f32) -> f32 {
    let two_pi = 2.0 * PI;
    let mut a = x % two_pi;
    if a > PI {
        a -= two_pi;
    } else if a < -PI {
        a += two_pi;
    }
    if a > PI * 0.5 {
        a = PI - a;
    } else if a < -PI * 0.5 {
        a = -PI - a;
    }
    let a2 = a * a;
    a * (1.0 - a2 * (1.0 / 6.0 - a2 * (1.0 / 120.0 - a2 * (1.0 / 5040.0 - a2 / 362_880.0))))
}

/// Pure-Rust deterministic cosine (avoids platform-dependent libm `f32::cos`).
fn cos_f32(x: f32) -> f32 {
    sin_f32(x + PI * 0.5)
}

/// Pure-Rust deterministic exponential for small magnitudes (avoids platform-dependent libm `f32::exp`).
fn exp_f32(x: f32) -> f32 {
    // Range-reduce by halving 4 times so |u| < 0.5, evaluate degree-6 Taylor, then square 4 times.
    let u = x / 16.0;
    let t = 1.0 + u * (1.0 + u * (0.5 + u * (1.0 / 6.0 + u * (1.0 / 24.0 + u * (1.0 / 120.0 + u / 720.0)))));
    let t2 = t * t;
    let t4 = t2 * t2;
    let t8 = t4 * t4;
    t8 * t8
}

fn hypot_f32(dx: f32, dy: f32) -> f32 {
    (dx * dx + dy * dy).sqrt()
}

fn cubic_ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Underdamped jelly spring that overshoots and settles gently.
pub fn jelly_spring(t: f32) -> f32 {
    if t <= 0.0 {
        0.0
    } else if t >= 1.0 {
        1.0
    } else {
        1.0 - exp_f32(-SPRING_DAMPING * t) * cos_f32(2.0 * PI * SPRING_FREQUENCY * t)
    }
}

/// Decaying sine wobble driving squash-and-stretch during a morph.
fn jelly_wobble(t: f32) -> f32 {
    if t <= 0.0 || t >= 1.0 {
        0.0
    } else {
        exp_f32(-5.0 * t) * sin_f32(2.0 * PI * SPRING_FREQUENCY * t)
    }
}

fn centroid(pts: &[[f32; 2]]) -> [f32; 2] {
    if pts.is_empty() {
        return [512.0, 512.0];
    }
    let inv = 1.0 / pts.len() as f32;
    let mut sx = 0.0;
    let mut sy = 0.0;
    for &[x, y] in pts {
        sx += x;
        sy += y;
    }
    [sx * inv, sy * inv]
}

fn squash_points(pts: &mut [[f32; 2]], center: [f32; 2], axis: [f32; 2], amount: f32) {
    if amount.abs() < 1e-5 {
        return;
    }
    let [cx, cy] = center;
    let [ux, uy] = axis;
    for pt in pts.iter_mut() {
        let dx = pt[0] - cx;
        let dy = pt[1] - cy;
        let along = (dx * ux + dy * uy) * (1.0 + amount);
        let across = (-dx * uy + dy * ux) * (1.0 - amount * 0.8);
        pt[0] = cx + along * ux - across * uy;
        pt[1] = cy + along * uy + across * ux;
    }
}

/// One active mode letter ready to be painted on the 1024 grid.
#[derive(Clone)]
pub struct LetterFrame {
    pub spec: &'static LetterSpec,
    pub scale: f32,
}

/// Interpolated mascot frame on the 1024 emblem grid.
#[derive(Clone)]
pub struct MascotFrame {
    pub cells: [Vec<[f32; 2]>; 6],
    pub head: Vec<[f32; 2]>,
    pub color_t: f32,
    pub eyes: [([f32; 2], [f32; 2], f32); 2],
    pub letters: Vec<LetterFrame>,
}

/// Computes the mascot's interpolated geometry between `from` and `to` at progress `t` in `0.0..=1.0`.
pub fn compute_mascot_frame(from: InputMode, to: InputMode, t: f32, blink: f32) -> MascotFrame {
    let t = t.clamp(0.0, 1.0);
    let from_idx = pose_index(from);
    let to_idx = pose_index(to);
    let still = from_idx == to_idx;
    let a = DRAWINGS[from_idx];
    let b = DRAWINGS[to_idx];
    let moved = if still { 1.0 } else { jelly_spring(t) };

    let mut head: Vec<[f32; 2]> = a
        .head
        .iter()
        .zip(b.head.iter())
        .map(|(&pa, &pb)| [lerp(pa[0], pb[0], moved), lerp(pa[1], pb[1], moved)])
        .collect();
    let head_center = centroid(&head);
    let jiggle = if still { 0.0 } else { 0.06 * jelly_wobble(t) };
    squash_points(&mut head, head_center, [0.0, 1.0], jiggle);

    let cells = std::array::from_fn(|i| {
        let dist = i.abs_diff(to_idx);
        let steps = dist.min(6 - dist) as f32;
        let local = if still {
            1.0
        } else {
            ((t - steps * 0.04) / (1.0 - 0.12)).clamp(0.0, 1.0)
        };
        let sprung = if still { 1.0 } else { jelly_spring(local) };
        let mut pts: Vec<[f32; 2]> = a.cells[i]
            .iter()
            .zip(b.cells[i].iter())
            .map(|(&pa, &pb)| [lerp(pa[0], pb[0], sprung), lerp(pa[1], pb[1], sprung)])
            .collect();
        if !still {
            let c = centroid(&pts);
            let dx = c[0] - head_center[0];
            let dy = c[1] - head_center[1];
            let len = hypot_f32(dx, dy).max(1.0);
            let amp = if i == to_idx || i == from_idx { 0.11 } else { 0.05 };
            squash_points(&mut pts, c, [dx / len, dy / len], amp * jelly_wobble(local));
        }
        pts
    });

    let blink = blink.clamp(0.0, 1.0);
    let eyes = std::array::from_fn(|i| {
        let ea = a.eyes[i];
        let eb = b.eyes[i];
        let mut p0 = [lerp(ea[0], eb[0], moved), lerp(ea[1], eb[1], moved)];
        let mut p1 = [lerp(ea[2], eb[2], moved), lerp(ea[3], eb[3], moved)];
        if blink > 0.0 {
            let mid_y = 0.5 * (p0[1] + p1[1]);
            let half = ((p1[1] - p0[1]).abs() * 0.5).max(1.0);
            p0[1] = mid_y - half * (1.0 - blink);
            p1[1] = mid_y + half * (1.0 - blink);
        }
        let mut pair = [p0, p1];
        squash_points(&mut pair, head_center, [0.0, 1.0], jiggle);
        if hypot_f32(pair[1][0] - pair[0][0], pair[1][1] - pair[0][1]) < 0.01 {
            pair[1][0] += 0.01;
        }
        let width = (lerp(ea[4], eb[4], moved.clamp(0.0, 1.0)) * (1.0 - 0.3 * blink)).max(0.0);
        (pair[0], pair[1], width)
    });

    let mut letters = Vec::with_capacity(2);
    if still {
        letters.push(LetterFrame {
            spec: &b.letter,
            scale: b.letter.size,
        });
    } else {
        let out_k = 1.0 - cubic_ease((t / 0.3).clamp(0.0, 1.0));
        if out_k > 0.001 {
            letters.push(LetterFrame {
                spec: &a.letter,
                scale: a.letter.size * out_k,
            });
        }
        let in_k = jelly_spring(((t - 0.18) / 0.82).clamp(0.0, 1.0));
        if in_k > 0.001 {
            letters.push(LetterFrame {
                spec: &b.letter,
                scale: b.letter.size * in_k,
            });
        }
    }

    let color_t = if from == to { 1.0 } else { cubic_ease(t) };
    MascotFrame {
        cells,
        head,
        color_t,
        eyes,
        letters,
    }
}

fn to_sk_color(rgba: Rgba) -> SkColor {
    SkColor::from_argb(rgba.a, rgba.r, rgba.g, rgba.b)
}

fn build_polygon_path(pts: &[[f32; 2]]) -> freya::engine::prelude::Path {
    let mut builder = PathBuilder::new();
    if let Some((first, rest)) = pts.split_first() {
        builder.move_to(SkPoint::new(first[0], first[1]));
        for p in rest {
            builder.line_to(SkPoint::new(p[0], p[1]));
        }
        builder.close();
    }
    builder.detach()
}

fn build_letter_path(letter: &LetterFrame) -> freya::engine::prelude::Path {
    let mut builder = PathBuilder::new();
    builder.set_fill_type(SkPathFillType::EvenOdd);
    let rad = letter.spec.lean_deg.to_radians();
    let cos = cos_f32(rad);
    let sin = sin_f32(rad);
    let s = letter.scale;
    let tx = letter.spec.x;
    let ty = letter.spec.y;

    for poly in letter.spec.polygons {
        if let Some((first, rest)) = poly.split_first() {
            let transform = |pt: [f32; 2]| -> SkPoint {
                let sx = pt[0] * s;
                let sy = pt[1] * s;
                SkPoint::new(tx + sx * cos - sy * sin, ty + sx * sin + sy * cos)
            };
            builder.move_to(transform(*first));
            for pt in rest {
                builder.line_to(transform(*pt));
            }
            builder.close();
        }
    }
    builder.detach()
}

/// Paints `frame` into `canvas`, uniformly scaled to fit `(width, height)`.
pub fn paint_mascot_frame(
    canvas: &Canvas,
    width: f32,
    height: f32,
    frame: &MascotFrame,
    from_color: Rgba,
    to_color: Rgba,
    ink_color: Rgba,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let scale = (width / VIEWBOX_WIDTH).min(height / VIEWBOX_HEIGHT);
    let offset_x = (width - VIEWBOX_WIDTH * scale) * 0.5;
    let offset_y = (height - VIEWBOX_HEIGHT * scale) * 0.5;

    let save = canvas.save();
    canvas.translate((offset_x, offset_y));
    canvas.scale((scale, scale));
    canvas.translate((-VIEWBOX_X, -VIEWBOX_Y));

    let cell_paths: [freya::engine::prelude::Path; 6] = std::array::from_fn(|i| build_polygon_path(&frame.cells[i]));
    let head_path = build_polygon_path(&frame.head);

    // 1. Dark ink silhouette & stroke behind all cells and the head so pastels remain crisp over light pages.
    let mut edge_fill = Paint::default();
    edge_fill.set_anti_alias(true);
    edge_fill.set_style(PaintStyle::Fill);
    edge_fill.set_color(to_sk_color(ink_color));

    let mut edge_stroke = Paint::default();
    edge_stroke.set_anti_alias(true);
    edge_stroke.set_style(PaintStyle::Stroke);
    edge_stroke.set_stroke_width(MASCOT_EDGE_WIDTH);
    edge_stroke.set_stroke_join(PaintJoin::Round);
    edge_stroke.set_stroke_cap(PaintCap::Round);
    edge_stroke.set_color(to_sk_color(ink_color));

    for path in &cell_paths {
        canvas.draw_path(path, &edge_fill);
        canvas.draw_path(path, &edge_stroke);
    }
    canvas.draw_path(&head_path, &edge_fill);
    canvas.draw_path(&head_path, &edge_stroke);

    // 2. Six mode cells in their bright pastel Tokyonight hues.
    let mut fill = Paint::default();
    fill.set_anti_alias(true);
    fill.set_style(PaintStyle::Fill);
    for (i, path) in cell_paths.iter().enumerate() {
        fill.set_color(to_sk_color(RING_CELL_COLORS[i]));
        canvas.draw_path(path, &fill);
    }

    // 3. Head and bolt antenna filled in the interpolated mode color.
    let head_color = from_color.mix(to_color, frame.color_t);
    fill.set_color(to_sk_color(head_color));
    canvas.draw_path(&head_path, &fill);

    // 4. Two rounded-cap eyes in night ink.
    let mut eye_paint = Paint::default();
    eye_paint.set_anti_alias(true);
    eye_paint.set_style(PaintStyle::Stroke);
    eye_paint.set_stroke_cap(PaintCap::Round);
    eye_paint.set_color(to_sk_color(ink_color));
    for &(p0, p1, stroke_w) in &frame.eyes {
        if stroke_w > 0.1 {
            eye_paint.set_stroke_width(stroke_w);
            canvas.draw_line(SkPoint::new(p0[0], p0[1]), SkPoint::new(p1[0], p1[1]), &eye_paint);
        }
    }

    // 5. Mode letter cut into the leading cell in night ink.
    fill.set_color(to_sk_color(ink_color));
    for letter in &frame.letters {
        let path = build_letter_path(letter);
        canvas.draw_path(&path, &fill);
    }

    canvas.restore_to_count(save);
}

/// Interactive animated Zendia mascot component with jelly morphing between vim modes and periodic blinking.
#[derive(Clone, PartialEq)]
pub struct VimMascot {
    pub mode: InputMode,
    /// Rendered diameter in logical pixels (`MASCOT_FULL_SIZE` at full size, ~`53px` when shrunk).
    pub size: f32,
    /// Optional pinned morph `(from_mode, progress)` for deterministic gallery previews.
    pub pinned_morph: Option<(InputMode, f32)>,
    /// Disables periodic blinking (used for deterministic gallery screenshots).
    pub disable_blink: bool,
    pub key: DiffKey,
}

impl KeyExt for VimMascot {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl VimMascot {
    pub fn new(mode: InputMode) -> Self {
        Self {
            mode,
            size: MASCOT_FULL_SIZE,
            pinned_morph: None,
            disable_blink: false,
            key: DiffKey::None,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn disable_blink(mut self, disable: bool) -> Self {
        self.disable_blink = disable;
        self
    }
}

impl Component for VimMascot {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut last_mode = use_state(|| self.mode);
        let mut from_mode = use_state(|| self.mode);
        let mut morph_seq = use_state(|| 0u64);
        let mut blink_seq = use_state(|| 0u64);

        if *last_mode.peek() != self.mode {
            let prev = *last_mode.peek();
            from_mode.set(prev);
            last_mode.set(self.mode);
            let next_seq = *morph_seq.peek() + 1;
            morph_seq.set(next_seq);
        }

        let seq = *morph_seq.read();
        let morph_anim = use_animation_with_dependencies(&seq, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(MASCOT_MORPH_DURATION)
                .ease(Ease::Out)
                .function(Function::Linear)
        });

        // Subtle periodic eye blink when alive in the real window.
        let disable_blink = self.disable_blink || self.pinned_morph.is_some();
        use_hook(move || {
            if disable_blink {
                return;
            }
            spawn(async move {
                loop {
                    timer(Duration::from_millis(3800)).await;
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
            None => (*from_mode.read(), morph_anim.get().value().clamp(0.0, 1.0)),
        };
        let blink_t = if disable_blink || morph_t < 1.0 {
            0.0
        } else {
            let raw = blink_anim.get().value().clamp(0.0, 1.0);
            if raw <= 0.001 || raw >= 0.999 {
                0.0
            } else {
                sin_f32(PI * raw)
            }
        };

        let frame = compute_mascot_frame(morph_from, self.mode, morph_t, blink_t);
        let from_color = mascot_mode_color(morph_from);
        let to_color = mascot_mode_color(self.mode);
        let ink_color = theme.palette.night;

        let callback = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_mascot_frame(
                    canvas,
                    ctx.size.width,
                    ctx.size.height,
                    &frame,
                    from_color,
                    to_color,
                    ink_color,
                );
            }
        });

        rect()
            .width(Size::px(self.size))
            .height(Size::px(self.size))
            .background(Fill::Callback(callback))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jelly_spring_starts_at_zero_overshoots_and_lands_at_one() {
        assert_eq!(jelly_spring(0.0), 0.0);
        assert_eq!(jelly_spring(1.0), 1.0);
        // Overshoots above 1.0 mid-flight around the first half-cycle.
        let peak = (1..100).map(|i| jelly_spring(i as f32 / 100.0)).fold(0.0_f32, f32::max);
        assert!(peak > 1.05, "expected jelly overshoot > 1.05, got {peak}");
    }

    #[test]
    fn computes_valid_frames_for_all_modes_and_transitions() {
        for mode in [
            InputMode::Normal,
            InputMode::Insert,
            InputMode::Visual,
            InputMode::Explorer,
            InputMode::Toolbar,
            InputMode::Library,
            InputMode::Window,
        ] {
            let resting = compute_mascot_frame(mode, mode, 1.0, 0.0);
            assert_eq!(resting.head.len(), 120);
            assert_eq!(resting.letters.len(), 1);
            for cell in &resting.cells {
                assert_eq!(cell.len(), 72);
            }
        }

        let mid = compute_mascot_frame(InputMode::Normal, InputMode::Insert, 0.25, 0.0);
        assert_eq!(mid.letters.len(), 2, "mid-morph blends outgoing and incoming letters");
        let blinking = compute_mascot_frame(InputMode::Normal, InputMode::Normal, 1.0, 1.0);
        assert!((blinking.eyes[0].0[1] - blinking.eyes[0].1[1]).abs() < 1e-3);
    }
}
