/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Native Skia agent motion primitives: [`WorkingRing`], [`RollingText`], and [`WorkingRow`].
//!
//! - [`WorkingRing`] paints the 8-dash circular ring on a 24x24 grid via Freya's [`RenderCallback`] using
//!   [`zendia_browser_core::agent::ring_dashes`] over a `3.9s` active run + `0.9s` rest cycle at `30fps` (or a
//!   deterministic `pinned_phase` in gallery fixtures).
//! - [`RollingText`] smoothly transitions status phrases using [`zendia_browser_core::agent::shared_ends`] over
//!   `motion::AGENT_ROLL` (`550ms`).
//! - [`WorkingRow`] combines a `16px` [`WorkingRing`], [`RollingText`] status word, and tabular elapsed clock.

use crate::ui::panels::try_use_dock_state;
use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::engine::prelude::{Canvas, Paint, PaintCap, PaintStyle, PathBuilder, SkColor, SkPoint};
use freya::prelude::*;
use std::f32::consts::PI;
use std::time::Duration;
use zendia_browser_core::agent::{
    RING_REST_SECS, RING_RUN_SECS, cos_f32, format_duration_secs, ring_dashes, shared_ends, sin_f32,
};

const RING_CYCLE_SECS: f32 = RING_RUN_SECS + RING_REST_SECS;
const RING_FRAME_MS: u64 = 33; // ~30fps

/// Eight-dash animated working ring rendered natively on Skia.
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
                    timer(Duration::from_millis(RING_FRAME_MS)).await;
                    let phase_secs = started.elapsed().as_secs_f32() % RING_CYCLE_SECS;
                    live_phase.set(phase_secs);
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

fn paint_working_ring(canvas: &Canvas, width: f32, height: f32, phase_secs: f32, color: SkColor) {
    let scale = width.min(height) / 24.0;
    let cx = width * 0.5;
    let cy = height * 0.5;
    let radius = 8.2 * scale;

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_cap(PaintCap::Round);

    for dash in ring_dashes(phase_secs) {
        let opacity = (0.48 + 0.52 * dash.glow).clamp(0.0, 1.0);
        let alpha = ((opacity * (color.a() as f32)).round() as u8).max(16);
        paint.set_color(SkColor::from_argb(alpha, color.r(), color.g(), color.b()));
        paint.set_stroke_width(((2.1 + 0.65 * dash.glow) * scale).max(1.35));

        let a0 = (dash.centre_deg - dash.half_deg - 90.0) * (PI / 180.0);
        let a1 = (dash.centre_deg + dash.half_deg - 90.0) * (PI / 180.0);
        let mut pb = PathBuilder::new();
        let steps = 6;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let ang = a0 + (a1 - a0) * t;
            let pt = SkPoint::new(cx + radius * cos_f32(ang), cy + radius * sin_f32(ang));
            if step == 0 {
                pb.move_to(pt);
            } else {
                pb.line_to(pt);
            }
        }
        let path = pb.detach();
        canvas.draw_path(&path, &paint);
    }
}

/// Rolling status text that keeps shared prefix/suffix characters anchored while smoothly gliding the changing middle.
#[derive(Clone, PartialEq)]
pub struct RollingText {
    pub text: String,
    pub font_size: f32,
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
            color: None,
            key: DiffKey::None,
        }
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
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

        if *curr_text.peek() != self.text {
            let old = curr_text.peek().clone();
            prev_text.set(old);
            curr_text.set(self.text.clone());
        }

        let roll = use_animation_with_dependencies(&self.text, |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(motion::AGENT_ROLL)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let t = roll.get().value().clamp(0.0, 1.0);
        let text_color = self.color.unwrap_or_else(|| theme.color(ThemeRole::TextMuted));
        let prev = prev_text.read().clone();
        let curr = curr_text.read().clone();

        if t >= 0.99 || prev == curr {
            return label()
                .text(curr)
                .font_size(self.font_size)
                .font_weight(FontWeight::MEDIUM)
                .color(text_color)
                .max_lines(1)
                .into_element();
        }

        let (prefix_chars, suffix_chars) = shared_ends(&prev, &curr);
        let chars: Vec<char> = curr.chars().collect();
        let prefix: String = chars.iter().take(prefix_chars).collect();
        let middle_end = chars.len().saturating_sub(suffix_chars);
        let middle: String = chars[prefix_chars..middle_end].iter().collect();
        let suffix: String = chars[middle_end..].iter().collect();

        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .overflow(Overflow::Clip)
            .maybe_child((!prefix.is_empty()).then(|| {
                label()
                    .text(prefix)
                    .font_size(self.font_size)
                    .font_weight(FontWeight::MEDIUM)
                    .color(text_color)
            }))
            .maybe_child((!middle.is_empty()).then(|| {
                rect().offset_y((1.0 - t) * 7.0).opacity(t).child(
                    label()
                        .text(middle)
                        .font_size(self.font_size)
                        .font_weight(FontWeight::MEDIUM)
                        .color(text_color),
                )
            }))
            .maybe_child((!suffix.is_empty()).then(|| {
                label()
                    .text(suffix)
                    .font_size(self.font_size)
                    .font_weight(FontWeight::MEDIUM)
                    .color(text_color)
            }))
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Live status row shown at the bottom of the conversation trace while the agent is working.
#[derive(Clone, PartialEq)]
pub struct WorkingRow {
    pub status_word: String,
    pub elapsed_secs: u64,
    pub accent: Color,
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
            key: DiffKey::None,
        }
    }
}

impl Component for WorkingRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let clock = format_duration_secs(self.elapsed_secs);

        rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .padding(Gaps::new(6.0, 4.0, 6.0, 4.0))
            .child(WorkingRing::new(16.0).color(self.accent))
            .child(RollingText::new(self.status_word.clone()).color(theme.color(ThemeRole::Text)))
            .child(
                label()
                    .text(clock)
                    .font_size(11.5)
                    .font_family("monospace")
                    .color(theme.color(ThemeRole::TextMuted)),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
