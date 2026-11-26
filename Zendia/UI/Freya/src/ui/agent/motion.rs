/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Native Skia agent motion primitives: [`WorkingRing`], [`RollingText`], and [`WorkingRow`] (`AM-01`, `AM-03`, `AM-04`).
//!
//! - [`WorkingRing`] paints the 8-dash circular ring on a 24×24 grid (`radius = 10 * unit`, `stroke = 2 * unit`),
//!   subtracts `cap_deg`, brightens stretched dashes up to 60% toward white/black by luminance (constant alpha), draws
//!   smooth Skia arcs, and sleeps during the `0.9 s` rest interval instead of redrawing while still.
//! - [`RollingText`] keeps shared prefix/suffix glyphs anchored while rolling changing middle glyphs per-glyph
//!   (staggered across 30% of `550 ms` with arriving & leaving glyphs, `±0.35 em` Y, `0.6↔1.0` scale, `±2°` rotation,
//!   and `200 ms` container width glide).
//! - [`WorkingRow`] combines a `16 px` [`WorkingRing`], muted [`RollingText`] status word (`"Starting…"` with no clock
//!   at `0 s`), and proportional tabular elapsed clock with `120 ms` / `160 ms` fade.

use crate::ui::panels::try_use_dock_state;
use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::engine::prelude::{Canvas, Paint, PaintCap, PaintStyle, PathBuilder, Rect, SkColor};
use freya::prelude::*;
use std::f32::consts::PI;
use std::time::Duration;
use zendia_browser_core::agent::{RING_REST_SECS, RING_RUN_SECS, format_duration_secs, ring_dashes, shared_ends};

const RING_CYCLE_SECS: f32 = RING_RUN_SECS + RING_REST_SECS;
const RING_FRAME_MS: u64 = 33; // ~30fps
const ROLL_STAGGER: f32 = 0.3;

/// Eight-dash animated working ring rendered natively on Skia (`AM-01`).
#[derive(Clone, PartialEq)]
pub struct WorkingRing {
    pub size: f32,
    pub color: Option<Color>,
    /// Optional deterministic phase in `[0.0, 1.0)` (overrides live timer for gallery captures).
    pub pinned_phase: Option<f32>,
    key: DiffKey,
}

impl KeyExt for WorkingRing {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl WorkingRing {
    pub fn new(size: f32) -> Self {
        Self {
            size,
            color: None,
            pinned_phase: None,
            key: DiffKey::None,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn pinned_phase(mut self, phase: Option<f32>) -> Self {
        self.pinned_phase = phase;
        self
    }
}

impl Component for WorkingRing {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let mut live_phase = use_state(|| 0.28f32);

        let effective_pinned = self
            .pinned_phase
            .or_else(|| dock_state.and_then(|ds| *ds.pinned_ring_phase.read()));

        use_side_effect(move || {
            if effective_pinned.is_some() {
                return;
            }
            let started = std::time::Instant::now();
            spawn(async move {
                loop {
                    let phase_secs = started.elapsed().as_secs_f32() % RING_CYCLE_SECS;
                    if phase_secs >= RING_RUN_SECS {
                        // AM-01: Rest is a still picture; paint rest pose once and sleep until next run.
                        live_phase.set(RING_RUN_SECS);
                        let remain = (RING_CYCLE_SECS - phase_secs).max(0.02);
                        timer(Duration::from_secs_f32(remain)).await;
                    } else {
                        live_phase.set(phase_secs);
                        timer(Duration::from_millis(RING_FRAME_MS)).await;
                    }
                }
            });
        });

        let phase_secs = effective_pinned
            .map(|p| p * RING_RUN_SECS)
            .unwrap_or_else(|| *live_phase.read());
        let color = self.color.unwrap_or_else(|| theme.color(ThemeRole::Accent));
        let sk_color = SkColor::from_argb(color.a(), color.r(), color.g(), color.b());

        let callback = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_working_ring(canvas, ctx.size.width, ctx.size.height, phase_secs, sk_color);
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

/// Luminance-based RGB brightening up to 60% toward white (light ink) or black (dark ink) with constant alpha (`AM-01`).
fn brightened_ring_color(base: SkColor, glow: f32) -> SkColor {
    let r = base.r() as f32;
    let g = base.g() as f32;
    let b = base.b() as f32;
    let lum = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
    let target = if lum > 0.5 { 255.0 } else { 0.0 };
    let t = (0.6 * glow).clamp(0.0, 0.6);
    let nr = (r + (target - r) * t).round().clamp(0.0, 255.0) as u8;
    let ng = (g + (target - g) * t).round().clamp(0.0, 255.0) as u8;
    let nb = (b + (target - b) * t).round().clamp(0.0, 255.0) as u8;
    SkColor::from_argb(base.a(), nr, ng, nb)
}

fn paint_working_ring(canvas: &Canvas, width: f32, height: f32, phase_secs: f32, color: SkColor) {
    // 24-unit grid matching ZendiaAgentMotion.sys.mjs:617-639: radius 10, constant stroke 2, capDegrees subtracted.
    let unit = width.min(height) / 24.0;
    if unit <= 0.0 {
        return;
    }
    let cx = width * 0.5;
    let cy = height * 0.5;
    let radius = 10.0 * unit;
    let cap_deg = (unit / radius) * (180.0 / PI);
    let oval = Rect::from_xywh(cx - radius, cy - radius, radius * 2.0, radius * 2.0);

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_cap(PaintCap::Round);
    paint.set_stroke_width((2.0 * unit).max(1.2));

    for dash in ring_dashes(phase_secs) {
        let half = (dash.half_deg - cap_deg).max(0.01);
        let start_deg = dash.centre_deg - half - 90.0;
        let sweep_deg = (half * 2.0).max(0.02);
        paint.set_color(brightened_ring_color(color, dash.glow));

        let mut pb = PathBuilder::new();
        pb.arc_to(oval, start_deg, sweep_deg, true);
        let path = pb.detach();
        canvas.draw_path(&path, &paint);
    }
}

/// Rolling status text (`AM-03`) keeping shared prefix/suffix characters anchored while per-glyph staggering arriving
/// and leaving middle glyphs across `motion::AGENT_ROLL` (`550 ms`) and gliding width over `motion::AGENT_ROLL_GLIDE`.
#[derive(Clone, PartialEq)]
pub struct RollingText {
    pub text: String,
    pub font_size: f32,
    pub font_weight: FontWeight,
    pub color: Option<Color>,
    key: DiffKey,
}

impl KeyExt for RollingText {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl RollingText {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            font_size: 12.5,
            font_weight: FontWeight::MEDIUM,
            color: None,
            key: DiffKey::None,
        }
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn font_weight(mut self, weight: FontWeight) -> Self {
        self.font_weight = weight;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl Component for RollingText {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut prev_text = use_state(|| self.text.clone());
        let mut curr_text = use_state(|| self.text.clone());
        let mut measured_w = use_state(|| None::<f32>);
        let mut prev_w = use_state(|| None::<f32>);

        if *curr_text.peek() != self.text {
            let old = curr_text.peek().clone();
            prev_text.set(old);
            prev_w.set(*measured_w.peek());
            curr_text.set(self.text.clone());
        }

        let total_ms = (motion::AGENT_ROLL.as_secs_f32() * (1.0 + ROLL_STAGGER)).round() as u64;
        let roll = use_animation_with_dependencies(&self.text, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(Duration::from_millis(total_ms.max(1)))
                .function(Function::Linear)
        });

        let glide = use_animation_with_dependencies(&self.text, |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(motion::AGENT_ROLL_GLIDE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let raw_t = roll.get().value().clamp(0.0, 1.0);
        let text_color = self.color.unwrap_or_else(|| theme.color(ThemeRole::TextMuted));
        let prev = prev_text.read().clone();
        let curr = curr_text.read().clone();

        if raw_t >= 0.995 || prev == curr {
            return rect()
                .on_sized(move |e: Event<SizedEventData>| {
                    let w = e.area.width();
                    if w > 0.0 && measured_w.peek().is_none_or(|old| (old - w).abs() > 0.5) {
                        measured_w.set(Some(w));
                    }
                })
                .child(
                    label()
                        .text(curr)
                        .font_size(self.font_size)
                        .font_weight(self.font_weight)
                        .color(text_color)
                        .max_lines(1),
                )
                .into_element();
        }

        let (prefix_chars, suffix_chars) = shared_ends(&prev, &curr);
        let old_chars: Vec<char> = prev.chars().collect();
        let new_chars: Vec<char> = curr.chars().collect();
        let prefix: String = new_chars.iter().take(prefix_chars).collect();
        let old_mid_end = old_chars.len().saturating_sub(suffix_chars);
        let new_mid_end = new_chars.len().saturating_sub(suffix_chars);
        let old_mid = &old_chars[prefix_chars.min(old_mid_end)..old_mid_end];
        let new_mid = &new_chars[prefix_chars.min(new_mid_end)..new_mid_end];
        let suffix: String = new_chars[new_mid_end..].iter().collect();

        let roll_dur = motion::AGENT_ROLL.as_secs_f32();
        let elapsed = raw_t * (roll_dur * (1.0 + ROLL_STAGGER));
        let shift_em = self.font_size * 0.35;

        let glyph_progress = |i: usize, count: usize| -> f32 {
            let delay = if count == 0 {
                0.0
            } else {
                (roll_dur * ROLL_STAGGER * i as f32) / count as f32
            };
            let local = ((elapsed - delay) / roll_dur).clamp(0.0, 1.0);
            motion::zendia_ease_out(local, Ease::Out)
        };

        let mut arriving_row = rect().direction(Direction::Horizontal).cross_align(Alignment::center());
        for (i, ch) in new_mid.iter().enumerate() {
            let p = glyph_progress(i, new_mid.len());
            arriving_row = arriving_row.child(
                rect()
                    .offset_y((1.0 - p) * shift_em)
                    .scale(0.6 + 0.4 * p)
                    .rotate((1.0 - p) * 2.0)
                    .opacity(p)
                    .child(
                        label()
                            .text(ch.to_string())
                            .font_size(self.font_size)
                            .font_weight(self.font_weight)
                            .color(text_color),
                    ),
            );
        }

        let leaving_overlay = (!old_mid.is_empty()).then(|| {
            let mut row = rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center());
            for (i, ch) in old_mid.iter().enumerate() {
                let p = glyph_progress(i, old_mid.len());
                row = row.child(
                    rect()
                        .offset_y(-p * shift_em)
                        .scale(1.0 - 0.4 * p)
                        .rotate(-p * 2.0)
                        .opacity((1.0 - p).max(0.0))
                        .child(
                            label()
                                .text(ch.to_string())
                                .font_size(self.font_size)
                                .font_weight(self.font_weight)
                                .color(text_color),
                        ),
                );
            }
            row
        });

        let middle_stack = rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .child(arriving_row)
            .maybe_child(leaving_overlay);

        let est_new_w = (curr.chars().count() as f32 * self.font_size * 0.56).max(8.0);
        let glide_t = glide.get().value().clamp(0.0, 1.0);
        let mut container = rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .overflow(Overflow::Clip);
        if let Some(old_w) = *prev_w.read()
            && (est_new_w - old_w).abs() > 0.5
        {
            container = container.min_width(Size::px(old_w + (est_new_w - old_w) * glide_t));
        }

        container
            .maybe_child((!prefix.is_empty()).then(|| {
                label()
                    .text(prefix)
                    .font_size(self.font_size)
                    .font_weight(self.font_weight)
                    .color(text_color)
            }))
            .child(middle_stack)
            .maybe_child((!suffix.is_empty()).then(|| {
                label()
                    .text(suffix)
                    .font_size(self.font_size)
                    .font_weight(self.font_weight)
                    .color(text_color)
            }))
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Live status row shown at the bottom of the conversation trace while the agent is working (`AM-04`).
#[derive(Clone, PartialEq)]
pub struct WorkingRow {
    pub status_word: String,
    pub elapsed_secs: u64,
    pub accent: Color,
    pub shown: bool,
    key: DiffKey,
}

impl KeyExt for WorkingRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl WorkingRow {
    pub fn new(status_word: impl Into<String>, elapsed_secs: u64, accent: Color) -> Self {
        Self {
            status_word: status_word.into(),
            elapsed_secs,
            accent,
            shown: true,
            key: DiffKey::None,
        }
    }

    pub fn shown(mut self, shown: bool) -> Self {
        self.shown = shown;
        self
    }
}

impl Component for WorkingRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let static_snapshot = dock_state.is_some_and(|ds| ds.pinned_ring_phase.read().is_some());

        // AM-04: 120ms fade-in, 160ms fade-out inside a 26px row.
        let fade = use_animation_with_dependencies(&(self.shown, static_snapshot), |conf, (shown, snap)| {
            if *snap {
                conf.on_creation(OnCreation::Finish);
            } else {
                conf.on_creation(if *shown { OnCreation::Run } else { OnCreation::Finish });
                conf.on_change(OnChange::Rerun);
            }
            if *shown {
                AnimNum::new(0.0, 1.0)
                    .duration(Duration::from_millis(120))
                    .ease(Ease::Out)
                    .function(motion::css_ease_out_fn())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(Duration::from_millis(160))
                    .ease(Ease::Out)
                    .function(motion::css_ease_out_fn())
            }
        });

        let opacity = if static_snapshot {
            if self.shown { 1.0 } else { 0.0 }
        } else {
            fade.get().value().clamp(0.0, 1.0)
        };

        // AM-04: "Starting…" has no clock until elapsed_secs > 0; proportional font in TextMuted.
        let clock = (self.elapsed_secs > 0).then(|| format_duration_secs(self.elapsed_secs));

        rect()
            .width(Size::fill())
            .height(Size::px(26.0))
            .opacity(opacity)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
            .child(WorkingRing::new(16.0).color(self.accent))
            .child(
                RollingText::new(self.status_word.clone())
                    .font_size(12.5)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
            .maybe_child(clock.map(|c| label().text(c).font_size(12.0).color(theme.color(ThemeRole::TextMuted))))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
