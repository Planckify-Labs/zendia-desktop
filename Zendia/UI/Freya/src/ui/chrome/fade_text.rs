/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Single-line text with a true alpha right-edge fade mask when overflowing its container.
//!
//! Used by the location bar (`TB-05`) and vertical tab rows (`TAB-02`) so long text fades cleanly over any
//! translucent or gradient chrome surface without showing ellipsis or a mismatched overlay rectangle.

use freya::engine::prelude as sk;
use freya::prelude::*;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq)]
enum WeightBucket {
    Normal,
    Medium,
    SemiBold,
    Bold,
}

fn bucket_weight(w: FontWeight) -> WeightBucket {
    if w >= FontWeight::BOLD {
        WeightBucket::Bold
    } else if w >= FontWeight::SEMI_BOLD {
        WeightBucket::SemiBold
    } else if w >= FontWeight::MEDIUM {
        WeightBucket::Medium
    } else {
        WeightBucket::Normal
    }
}

fn cached_typeface(bucket: WeightBucket) -> Option<sk::Typeface> {
    static NORMAL: OnceLock<Option<sk::Typeface>> = OnceLock::new();
    static MEDIUM: OnceLock<Option<sk::Typeface>> = OnceLock::new();
    static SEMIBOLD: OnceLock<Option<sk::Typeface>> = OnceLock::new();
    static BOLD: OnceLock<Option<sk::Typeface>> = OnceLock::new();

    let (slot, sk_weight) = match bucket {
        WeightBucket::Normal => (&NORMAL, sk::Weight::NORMAL),
        WeightBucket::Medium => (&MEDIUM, sk::Weight::MEDIUM),
        WeightBucket::SemiBold => (&SEMIBOLD, sk::Weight::SEMI_BOLD),
        WeightBucket::Bold => (&BOLD, sk::Weight::BOLD),
    };

    slot.get_or_init(|| {
        let mgr = sk::FontMgr::default();
        let style = sk::FontStyle::new(sk_weight, sk::Width::NORMAL, sk::Slant::Upright);
        let families = ["Inter", "sans-serif", "DejaVu Sans", "Liberation Sans", "Noto Sans"];
        families
            .iter()
            .find_map(|family| mgr.match_family_style(family, style))
            .or_else(|| mgr.legacy_make_typeface(None, style))
    })
    .clone()
}

fn make_font(size: f32, weight: FontWeight) -> sk::Font {
    let mut font = match cached_typeface(bucket_weight(weight)) {
        Some(tf) => sk::Font::from_typeface(tf, size),
        None => {
            let mut f = sk::Font::default();
            f.set_size(size);
            f
        }
    };
    font.set_subpixel(true);
    font
}

fn to_sk_color(color: Color, alpha_scale: f32) -> sk::Color {
    let a = ((color.a() as f32) * alpha_scale.clamp(0.0, 1.0)).round() as u8;
    sk::Color::from_argb(a, color.r(), color.g(), color.b())
}

/// Renders single-line left-aligned text that applies a smooth right-edge alpha fade whenever the string is wider
/// than the available box.
#[derive(Clone, PartialEq)]
pub struct FadeText {
    pub text: String,
    pub font_size: f32,
    pub font_weight: FontWeight,
    pub color: Color,
    pub fade_px: f32,
    pub y_offset: f32,
}

impl FadeText {
    pub fn new(text: impl Into<String>, font_size: f32, font_weight: FontWeight, color: Color) -> Self {
        Self {
            text: text.into(),
            font_size,
            font_weight,
            color,
            fade_px: 20.0,
            y_offset: 0.0,
        }
    }

    pub fn fade_px(mut self, px: f32) -> Self {
        self.fade_px = px;
        self
    }

    pub fn y_offset(mut self, offset: f32) -> Self {
        self.y_offset = offset;
        self
    }
}

impl Component for FadeText {
    fn render(&self) -> impl IntoElement {
        let text = self.text.clone();
        let font_size = self.font_size;
        let font_weight = self.font_weight;
        let color = self.color;
        let fade_px = self.fade_px;
        let y_offset = self.y_offset;

        rect()
            .width(Size::flex(1.0))
            .height(Size::fill())
            .overflow(Overflow::Clip)
            .background(Fill::Callback(RenderCallback::new(move |ctx| {
                let RenderTarget::Background { canvas, .. } = &mut ctx.target else {
                    return;
                };
                let w = ctx.size.width;
                let h = ctx.size.height;
                if w <= 1.0 || h <= 1.0 || text.is_empty() {
                    return;
                }

                let left = 0.0_f32;
                let top = 0.0_f32;
                let font = make_font(font_size, font_weight);
                let (text_w, _) = font.measure_str(&text, None);
                let (_, metrics) = font.metrics();
                let baseline_y = top + h * 0.5 - (metrics.ascent + metrics.descent) * 0.5 + y_offset;

                let mut paint = sk::Paint::default();
                paint.set_anti_alias(true);
                let base_color = to_sk_color(color, 1.0);

                if text_w > w - 1.0 {
                    let fade_width = fade_px.min(w * 0.45).max(6.0);
                    let solid_stop = ((w - fade_width) / w).clamp(0.0, 0.96);
                    let colors = [
                        sk::Color4f::from(base_color),
                        sk::Color4f::from(base_color),
                        sk::Color4f::from(to_sk_color(color, 0.0)),
                    ];
                    let positions = [0.0_f32, solid_stop, 1.0_f32];
                    let grad = sk::gradient::Gradient::new(
                        sk::gradient::Colors::new(&colors, Some(&positions), sk::TileMode::Clamp, None),
                        sk::gradient::Interpolation::default(),
                    );
                    if let Some(shader) = sk::gradient::shaders::linear_gradient(
                        (sk::Point::new(left, top), sk::Point::new(left + w, top)),
                        &grad,
                        None,
                    ) {
                        paint.set_shader(shader);
                    } else {
                        paint.set_color(base_color);
                    }
                } else {
                    paint.set_color(base_color);
                }

                canvas.save();
                canvas.clip_rect(sk::Rect::from_xywh(left, top, w, h), None, false);
                canvas.draw_str(&text, sk::Point::new(left, baseline_y), &font, &paint);
                canvas.restore();
            })))
    }
}
