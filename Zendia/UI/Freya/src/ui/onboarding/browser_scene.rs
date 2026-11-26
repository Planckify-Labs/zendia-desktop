/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Native Skia 2D illustration of a Zendia browser window (`600 x 376` logical grid): vertical sidebar with pill
//! address bar, optional Essentials grid, sliding active tab pill, Explorer focus ring, rounded document viewport with
//! smooth scroll and tab-switch slide, slide-in Agent dock with real 8-dash working ring, Which-Key popup, and floating
//! inline mascot statusline.

use crate::ui::onboarding::logos::paint_brand_svg_on_canvas;
use crate::ui::onboarding::mascot::{MascotPaintOptions, paint_onboarding_mascot};
use crate::ui::onboarding::theme::{OnboardingPalette, clamp01, ring_dashes, sin_f32, sk_alpha, sk_color};
use crate::ui::theme::Rgba;
use freya::engine::prelude as sk;
use std::f32::consts::PI;
use zendia_browser_core::model::InputMode;

pub const BROWSER_W: f32 = 600.0;
pub const BROWSER_H: f32 = 376.0;
const SIDEBAR_W: f32 = 140.0;
const PAD: f32 = 10.0;
pub const VIEW_X: f32 = SIDEBAR_W + PAD;
pub const VIEW_Y: f32 = PAD;
pub const VIEW_W: f32 = BROWSER_W - VIEW_X - PAD;
pub const VIEW_H: f32 = BROWSER_H - PAD * 2.0;

#[derive(Clone, Debug)]
pub struct EssentialTileSpec {
    pub icon: String,
    pub color: Rgba,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhichKeyPage {
    Leader,
    Ai,
}

/// Complete visual parameters for one frame of [`paint_browser_2d`].
#[derive(Clone, Debug)]
pub struct BrowserSceneParams {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub vim: f32,
    pub mode: InputMode,
    pub tab_index: f32,
    pub scroll_y: f32,
    pub page_variant: usize,
    pub next_page_variant: usize,
    pub page_mix: f32,
    pub explorer: f32,
    pub explorer_row: f32,
    pub which_key: f32,
    pub which_key_page: WhichKeyPage,
    pub typed_keys: String,
    pub dock: f32,
    pub agent_name: Option<String>,
    pub agent_color: Option<Rgba>,
    pub agent_time: f32,
    pub tab_glow_color: Option<Rgba>,
    pub essentials: Vec<EssentialTileSpec>,
}

impl Default for BrowserSceneParams {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            vim: 0.0,
            mode: InputMode::Normal,
            tab_index: 0.0,
            scroll_y: 0.0,
            page_variant: 0,
            next_page_variant: 0,
            page_mix: 0.0,
            explorer: 0.0,
            explorer_row: 0.0,
            which_key: 0.0,
            which_key_page: WhichKeyPage::Leader,
            typed_keys: String::new(),
            dock: 0.0,
            agent_name: None,
            agent_color: None,
            agent_time: 0.0,
            tab_glow_color: None,
            essentials: Vec::new(),
        }
    }
}

/// Creates a Skia [`sk::Font`] in a clean sans or monospace family, falling back safely to the system default typeface.
pub fn make_font(size: f32, bold: bool, mono: bool) -> sk::Font {
    let mgr = sk::FontMgr::default();
    let style = if bold {
        sk::FontStyle::new(sk::Weight::BOLD, sk::Width::NORMAL, sk::Slant::Upright)
    } else {
        sk::FontStyle::normal()
    };
    let families: &[&str] = if mono {
        &["monospace", "DejaVu Sans Mono", "Liberation Mono", "Noto Sans Mono"]
    } else {
        &["sans-serif", "Inter", "DejaVu Sans", "Liberation Sans", "Noto Sans"]
    };
    let typeface = families
        .iter()
        .find_map(|family| mgr.match_family_style(family, style))
        .or_else(|| mgr.legacy_make_typeface(None, style));
    match typeface {
        Some(tf) => sk::Font::from_typeface(tf, size),
        None => {
            let mut f = sk::Font::default();
            f.set_size(size);
            f
        }
    }
}

/// Draws centered text at `(cx, baseline_y)` on `canvas`.
pub fn draw_text_centered(
    canvas: &sk::Canvas,
    text: &str,
    cx: f32,
    baseline_y: f32,
    font: &sk::Font,
    color: sk::Color,
) {
    let (width, _) = font.measure_str(text, None);
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    canvas.draw_str(text, sk::Point::new(cx - width * 0.5, baseline_y), font, &paint);
}

/// Draws left-aligned text at `(x, baseline_y)` on `canvas`.
pub fn draw_text_left(canvas: &sk::Canvas, text: &str, x: f32, baseline_y: f32, font: &sk::Font, color: sk::Color) {
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    canvas.draw_str(text, sk::Point::new(x, baseline_y), font, &paint);
}

pub fn draw_rrect_fill(canvas: &sk::Canvas, x: f32, y: f32, w: f32, h: f32, r: f32, color: sk::Color) {
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(sk::PaintStyle::Fill);
    paint.set_color(color);
    let rr = sk::RRect::new_rect_xy(sk::Rect::from_xywh(x, y, w, h), r, r);
    canvas.draw_rrect(rr, &paint);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_rrect_stroke(canvas: &sk::Canvas, x: f32, y: f32, w: f32, h: f32, r: f32, stroke_w: f32, color: sk::Color) {
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(sk::PaintStyle::Stroke);
    paint.set_stroke_width(stroke_w);
    paint.set_color(color);
    let rr = sk::RRect::new_rect_xy(sk::Rect::from_xywh(x, y, w, h), r, r);
    canvas.draw_rrect(rr, &paint);
}

/// Paints a dashed rounded-rectangle outline matching CSS `border: 1.5px dashed ...`.
pub fn draw_dashed_rrect_stroke(
    canvas: &sk::Canvas,
    rect: sk::Rect,
    radius: f32,
    stroke_w: f32,
    dash: &[f32],
    color: sk::Color,
) {
    let inset = stroke_w * 0.5;
    let inner = sk::Rect::from_ltrb(
        rect.left + inset,
        rect.top + inset,
        (rect.right - inset).max(rect.left + inset),
        (rect.bottom - inset).max(rect.top + inset),
    );
    let r = (radius - inset).max(0.0);
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(sk::PaintStyle::Stroke);
    paint.set_stroke_width(stroke_w);
    paint.set_stroke_cap(sk::PaintCap::Round);
    paint.set_color(color);
    if let Some(effect) = sk::PathEffect::dash(dash, 0.0) {
        paint.set_path_effect(effect);
    }
    let rr = sk::RRect::new_rect_xy(inner, r, r);
    canvas.draw_rrect(rr, &paint);
}

pub fn draw_circle_fill(canvas: &sk::Canvas, cx: f32, cy: f32, r: f32, color: sk::Color) {
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(sk::PaintStyle::Fill);
    paint.set_color(color);
    canvas.draw_circle(sk::Point::new(cx, cy), r, &paint);
}

/// Paints Zendia's real 8-dash working ring as crisp rounded Skia arcs.
pub fn paint_working_ring(canvas: &sk::Canvas, cx: f32, cy: f32, r: f32, color: Rgba, seconds: f32) {
    let dashes = ring_dashes(seconds);
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(sk::PaintStyle::Stroke);
    paint.set_stroke_cap(sk::PaintCap::Round);
    paint.set_stroke_width(1.9);

    let oval = sk::Rect::from_xywh(cx - r, cy - r, r * 2.0, r * 2.0);
    for dash in dashes {
        let start_deg = dash.centre - dash.half - 90.0;
        let sweep_deg = dash.half * 2.0;
        paint.set_color(sk_alpha(color, 0.45 + dash.glow * 0.55));
        canvas.draw_arc(oval, start_deg, sweep_deg, false, &paint);
    }
}

fn paint_document_content(
    canvas: &sk::Canvas,
    pal: &OnboardingPalette,
    variant: usize,
    scroll_y: f32,
    width: f32,
    alpha: f32,
) {
    let accent = match variant {
        0 => pal.pink,
        1 => pal.blue,
        _ => pal.teal,
    };
    let second = match variant {
        0 => pal.teal,
        1 => pal.purple,
        _ => pal.yellow,
    };

    let save = canvas.save();
    canvas.translate((0.0, -scroll_y));

    // Hero pill tag + heading block
    draw_rrect_fill(canvas, 28.0, 28.0, 76.0, 14.0, 7.0, sk_alpha(accent, 0.24 * alpha));
    draw_rrect_fill(canvas, 38.0, 32.5, 56.0, 5.0, 2.5, sk_alpha(accent, alpha));

    draw_rrect_fill(
        canvas,
        28.0,
        54.0,
        width - 140.0,
        16.0,
        6.0,
        sk_alpha(pal.text, 0.88 * alpha),
    );
    draw_rrect_fill(
        canvas,
        28.0,
        78.0,
        width - 210.0,
        13.0,
        5.0,
        sk_alpha(pal.text, 0.52 * alpha),
    );

    // Feature callout card
    draw_rrect_fill(
        canvas,
        28.0,
        108.0,
        width - 56.0,
        74.0,
        14.0,
        sk_alpha(pal.page_soft, alpha),
    );
    draw_rrect_fill(canvas, 44.0, 124.0, 38.0, 38.0, 11.0, sk_alpha(accent, 0.24 * alpha));
    draw_circle_fill(canvas, 63.0, 143.0, 9.0, sk_alpha(accent, alpha));
    draw_rrect_fill(canvas, 96.0, 128.0, 150.0, 9.0, 4.5, sk_alpha(pal.text, 0.80 * alpha));
    draw_rrect_fill(
        canvas,
        96.0,
        145.0,
        width - 180.0,
        7.0,
        3.5,
        sk_alpha(pal.muted, 0.55 * alpha),
    );
    draw_rrect_fill(
        canvas,
        96.0,
        159.0,
        width - 230.0,
        7.0,
        3.5,
        sk_alpha(pal.muted, 0.40 * alpha),
    );

    // Paragraph lines
    for (i, y) in [204.0_f32, 222.0, 240.0].into_iter().enumerate() {
        draw_rrect_fill(
            canvas,
            28.0,
            y,
            width - 56.0 - i as f32 * 42.0,
            8.0,
            4.0,
            sk_alpha(pal.muted, (0.48 - i as f32 * 0.06) * alpha),
        );
    }

    // Two side-by-side cards below the fold
    let card_w = (width - 70.0) * 0.5;
    draw_rrect_fill(canvas, 28.0, 268.0, card_w, 86.0, 13.0, sk_alpha(pal.page_soft, alpha));
    draw_rrect_fill(canvas, 42.0, 284.0, 44.0, 8.0, 4.0, sk_alpha(second, alpha));
    draw_rrect_fill(
        canvas,
        42.0,
        302.0,
        (width - 126.0) * 0.5,
        7.0,
        3.5,
        sk_alpha(pal.muted, 0.55 * alpha),
    );
    draw_rrect_fill(
        canvas,
        42.0,
        317.0,
        (width - 160.0) * 0.5,
        7.0,
        3.5,
        sk_alpha(pal.muted, 0.40 * alpha),
    );

    let right_x = 28.0 + card_w + 14.0;
    draw_rrect_fill(
        canvas,
        right_x,
        268.0,
        card_w,
        86.0,
        13.0,
        sk_alpha(pal.page_soft, alpha),
    );
    draw_rrect_fill(canvas, right_x + 14.0, 284.0, 52.0, 8.0, 4.0, sk_alpha(accent, alpha));
    draw_rrect_fill(
        canvas,
        right_x + 14.0,
        302.0,
        (width - 126.0) * 0.5,
        7.0,
        3.5,
        sk_alpha(pal.muted, 0.55 * alpha),
    );
    draw_rrect_fill(
        canvas,
        right_x + 14.0,
        317.0,
        (width - 160.0) * 0.5,
        7.0,
        3.5,
        sk_alpha(pal.muted, 0.40 * alpha),
    );

    // Lower lines visible when scrolled
    for (i, y) in [376.0_f32, 394.0, 412.0, 434.0, 452.0].into_iter().enumerate() {
        draw_rrect_fill(
            canvas,
            28.0,
            y,
            width - 56.0 - (i % 3) as f32 * 54.0,
            8.0,
            4.0,
            sk_alpha(pal.muted, 0.42 * alpha),
        );
    }

    canvas.restore_to_count(save);
}

/// Paints the 2D Zendia browser window at `params.(x, y, scale)` and invokes `viewport_overlay` inside the clipped
/// page viewport coordinate space `(0..VIEW_W, 0..VIEW_H)`.
pub fn paint_browser_2d<F>(
    canvas: &sk::Canvas,
    pal: &OnboardingPalette,
    params: &BrowserSceneParams,
    viewport_overlay: F,
) where
    F: FnOnce(&sk::Canvas),
{
    let save = canvas.save();
    canvas.translate((params.x, params.y));
    canvas.scale((params.scale, params.scale));

    let mode_pastel = OnboardingPalette::mode_pastel(params.mode);

    // Soft outer window drop shadow
    {
        let mut shadow = sk::Paint::default();
        shadow.set_anti_alias(true);
        shadow.set_color(sk::Color::from_argb(if pal.dark { 115 } else { 36 }, 0x0b, 0x0c, 0x14));
        shadow.set_mask_filter(sk::MaskFilter::blur(sk::BlurStyle::Normal, 18.0, false));
        let rr = sk::RRect::new_rect_xy(sk::Rect::from_xywh(0.0, 12.0, BROWSER_W, BROWSER_H), 20.0, 20.0);
        canvas.draw_rrect(rr, &shadow);
    }

    // Outer shell
    draw_rrect_fill(canvas, 0.0, 0.0, BROWSER_W, BROWSER_H, 20.0, sk_color(pal.window_bg));
    // Sidebar background tint
    draw_rrect_fill(
        canvas,
        1.0,
        1.0,
        SIDEBAR_W + 4.0,
        BROWSER_H - 2.0,
        19.0,
        sk_color(pal.sidebar_bg),
    );
    // Outer shell stroke
    let (shell_stroke, shell_alpha) = if params.vim > 0.3 {
        (mode_pastel, 0.55)
    } else {
        (pal.border, 0.85)
    };
    draw_rrect_stroke(
        canvas,
        0.0,
        0.0,
        BROWSER_W,
        BROWSER_H,
        20.0,
        1.6,
        sk_alpha(shell_stroke, shell_alpha),
    );

    // Window dots
    draw_circle_fill(canvas, 20.0, 21.0, 4.0, sk_alpha(pal.red, 0.72));
    draw_circle_fill(canvas, 34.0, 21.0, 4.0, sk_alpha(pal.yellow, 0.72));
    draw_circle_fill(canvas, 48.0, 21.0, 4.0, sk_alpha(pal.green, 0.72));

    // Zen pill address bar in sidebar
    draw_rrect_fill(canvas, 14.0, 38.0, SIDEBAR_W - 22.0, 24.0, 12.0, sk_color(pal.surface));
    draw_rrect_stroke(
        canvas,
        14.0,
        38.0,
        SIDEBAR_W - 22.0,
        24.0,
        12.0,
        1.0,
        sk_color(pal.border),
    );
    draw_circle_fill(canvas, 26.0, 50.0, 3.5, sk_alpha(mode_pastel, 0.85));
    draw_rrect_fill(canvas, 35.0, 47.5, 56.0, 5.0, 2.5, sk_alpha(pal.muted, 0.55));

    // Essentials tile grid (if provided)
    let essential_rows = if params.essentials.is_empty() {
        0
    } else {
        params.essentials.len().div_ceil(3)
    };
    for (idx, tile) in params.essentials.iter().enumerate() {
        let col = (idx % 3) as f32;
        let row = (idx / 3) as f32;
        let tx = 14.0 + col * 40.0;
        let ty = 74.0 + row * 38.0;
        draw_rrect_fill(canvas, tx, ty, 34.0, 32.0, 10.0, sk::Color::WHITE);
        draw_rrect_stroke(canvas, tx, ty, 34.0, 32.0, 10.0, 1.5, sk_alpha(tile.color, 0.65));
        paint_brand_svg_on_canvas(canvas, &tile.icon, tx + 9.0, ty + 8.0, 16.0);
    }

    // Sidebar tabs
    let tabs_top = 76.0 + essential_rows as f32 * 38.0 + if essential_rows > 0 { 10.0 } else { 0.0 };
    {
        let tab_save = canvas.save();
        canvas.translate((14.0, tabs_top));

        // Sliding active tab pill
        let active_y = params.tab_index * 34.0;
        draw_rrect_fill(
            canvas,
            0.0,
            active_y,
            SIDEBAR_W - 22.0,
            28.0,
            9.0,
            sk_color(pal.surface),
        );
        if let Some(glow) = params.tab_glow_color {
            draw_rrect_stroke(canvas, 0.0, active_y, SIDEBAR_W - 22.0, 28.0, 9.0, 1.8, sk_color(glow));
        } else {
            draw_rrect_stroke(
                canvas,
                0.0,
                active_y,
                SIDEBAR_W - 22.0,
                28.0,
                9.0,
                1.0,
                sk_color(pal.border),
            );
        }

        // Explorer mode blue focus ring
        if params.explorer > 0.02 {
            let ey = params.explorer_row * 34.0 - 1.0;
            let blue_pastel = OnboardingPalette::mode_pastel(InputMode::Explorer);
            draw_rrect_fill(
                canvas,
                -1.0,
                ey,
                SIDEBAR_W - 20.0,
                30.0,
                10.0,
                sk_alpha(blue_pastel, 0.18 * params.explorer),
            );
            draw_rrect_stroke(
                canvas,
                -1.0,
                ey,
                SIDEBAR_W - 20.0,
                30.0,
                10.0,
                2.0,
                sk_alpha(blue_pastel, params.explorer),
            );
        }

        // Four tab rows
        let tab_accents = [pal.pink, pal.blue, pal.teal, pal.purple];
        for (i, accent) in tab_accents.into_iter().enumerate() {
            let y = i as f32 * 34.0;
            let active = (params.tab_index - i as f32).abs() < 0.5;
            draw_circle_fill(
                canvas,
                13.0,
                y + 14.0,
                4.0,
                sk_alpha(accent, if active { 1.0 } else { 0.55 }),
            );
            let w = 52.0 + ((i * 17) % 24) as f32;
            draw_rrect_fill(
                canvas,
                24.0,
                y + 11.0,
                w,
                5.5,
                2.75,
                sk_alpha(
                    if active { pal.text } else { pal.muted },
                    if active { 0.9 } else { 0.55 },
                ),
            );
        }
        canvas.restore_to_count(tab_save);
    }

    // Main rounded page viewport
    let view_rrect = sk::RRect::new_rect_xy(sk::Rect::from_xywh(VIEW_X, VIEW_Y, VIEW_W, VIEW_H), 13.0, 13.0);
    draw_rrect_fill(canvas, VIEW_X, VIEW_Y, VIEW_W, VIEW_H, 13.0, sk_color(pal.page_bg));

    {
        let clip_save = canvas.save();
        canvas.clip_rrect(view_rrect, sk::ClipOp::Intersect, true);
        canvas.translate((VIEW_X, VIEW_Y));

        // Base document
        paint_document_content(
            canvas,
            pal,
            params.page_variant,
            params.scroll_y,
            VIEW_W,
            1.0 - params.page_mix * 0.65,
        );

        // Incoming document (tab-switch slide-up cross-fade)
        if params.page_mix > 0.01 {
            let mix_save = canvas.save();
            canvas.translate((0.0, (1.0 - params.page_mix) * 18.0));
            draw_rrect_fill(
                canvas,
                0.0,
                0.0,
                VIEW_W,
                VIEW_H,
                0.0,
                sk_alpha(pal.page_bg, params.page_mix),
            );
            paint_document_content(canvas, pal, params.next_page_variant, 0.0, VIEW_W, params.page_mix);
            canvas.restore_to_count(mix_save);
        }

        // Custom per-scene overlays inside the viewport
        viewport_overlay(canvas);

        // Slide-in Agent dock
        let dock_w = 164.0;
        if params.dock > 0.01 {
            let dock_alpha = clamp01(params.dock * 1.4);
            let dock_offset = (1.0 - clamp01(params.dock)) * (dock_w + 20.0);
            let agent_accent = params.agent_color.unwrap_or(pal.pink);
            let dock_bg = if pal.dark {
                Rgba::rgb(0x1b, 0x1d, 0x2e)
            } else {
                Rgba::rgb(0xf7, 0xf8, 0xfc)
            };
            let d_save = canvas.save();
            canvas.translate((VIEW_W - dock_w - 8.0 + dock_offset, 8.0));

            let dock_h = VIEW_H - 16.0;
            draw_rrect_fill(
                canvas,
                0.0,
                0.0,
                dock_w,
                dock_h,
                14.0,
                sk_alpha(dock_bg, 0.96 * dock_alpha),
            );
            draw_rrect_stroke(
                canvas,
                0.0,
                0.0,
                dock_w,
                dock_h,
                14.0,
                1.5,
                sk_alpha(agent_accent, 0.70 * dock_alpha),
            );

            paint_working_ring(canvas, 19.0, 22.0, 6.5, agent_accent, params.agent_time);
            let title_font = make_font(10.5, true, false);
            draw_text_left(
                canvas,
                params.agent_name.as_deref().unwrap_or("Agent"),
                33.0,
                25.5,
                &title_font,
                sk_alpha(pal.text, dock_alpha),
            );

            // Divider line
            let mut div_paint = sk::Paint::default();
            div_paint.set_stroke_width(1.0);
            div_paint.set_color(sk_alpha(pal.border, 0.6 * dock_alpha));
            canvas.draw_line(
                sk::Point::new(12.0, 38.0),
                sk::Point::new(dock_w - 12.0, 38.0),
                &div_paint,
            );

            // User prompt bubble
            draw_rrect_fill(
                canvas,
                28.0,
                48.0,
                dock_w - 40.0,
                28.0,
                9.0,
                sk_alpha(agent_accent, 0.18 * dock_alpha),
            );
            draw_rrect_fill(
                canvas,
                38.0,
                57.0,
                dock_w - 64.0,
                5.0,
                2.5,
                sk_alpha(pal.text, 0.82 * dock_alpha),
            );
            draw_rrect_fill(
                canvas,
                52.0,
                66.0,
                dock_w - 78.0,
                4.5,
                2.25,
                sk_alpha(pal.text, 0.55 * dock_alpha),
            );

            // Agent reply lines
            draw_rrect_fill(
                canvas,
                14.0,
                92.0,
                dock_w - 36.0,
                6.0,
                3.0,
                sk_alpha(pal.text, 0.72 * dock_alpha),
            );
            draw_rrect_fill(
                canvas,
                14.0,
                105.0,
                dock_w - 50.0,
                5.5,
                2.75,
                sk_alpha(pal.muted, 0.62 * dock_alpha),
            );
            draw_rrect_fill(
                canvas,
                14.0,
                118.0,
                dock_w - 42.0,
                5.5,
                2.75,
                sk_alpha(pal.muted, 0.52 * dock_alpha),
            );
            draw_rrect_fill(
                canvas,
                14.0,
                131.0,
                dock_w - 74.0,
                5.5,
                2.75,
                sk_alpha(pal.muted, 0.42 * dock_alpha),
            );

            // Action status pill
            draw_rrect_fill(
                canvas,
                14.0,
                152.0,
                dock_w - 28.0,
                24.0,
                8.0,
                sk_alpha(pal.page_soft, dock_alpha),
            );
            draw_circle_fill(canvas, 26.0, 164.0, 3.5, sk_alpha(agent_accent, dock_alpha));
            draw_rrect_fill(
                canvas,
                36.0,
                161.5,
                76.0,
                5.0,
                2.5,
                sk_alpha(pal.text, 0.70 * dock_alpha),
            );

            // Input bar at bottom of dock
            draw_rrect_fill(
                canvas,
                12.0,
                VIEW_H - 58.0,
                dock_w - 24.0,
                30.0,
                10.0,
                sk_alpha(pal.surface, dock_alpha),
            );
            draw_rrect_stroke(
                canvas,
                12.0,
                VIEW_H - 58.0,
                dock_w - 24.0,
                30.0,
                10.0,
                1.2,
                sk_alpha(agent_accent, 0.60 * dock_alpha),
            );
            draw_rrect_fill(
                canvas,
                22.0,
                VIEW_H - 46.0,
                56.0,
                5.0,
                2.5,
                sk_alpha(pal.muted, 0.50 * dock_alpha),
            );
            let caret_alpha = if sin_f32(params.agent_time * 6.0) > -0.1 {
                1.0
            } else {
                0.15
            };
            draw_rrect_fill(
                canvas,
                82.0,
                VIEW_H - 50.0,
                2.0,
                13.0,
                1.0,
                sk_alpha(agent_accent, caret_alpha * dock_alpha),
            );

            canvas.restore_to_count(d_save);
        }

        // Which-Key popup above statusline
        if params.which_key > 0.01 {
            let wk = clamp01(params.which_key);
            let rows: &[(&str, &str, Rgba)] = match params.which_key_page {
                WhichKeyPage::Leader => &[
                    ("e", "Explorer", OnboardingPalette::mode_pastel(InputMode::Explorer)),
                    ("a", "Agent", OnboardingPalette::mode_pastel(InputMode::Normal)),
                    ("f", "Find tab", OnboardingPalette::mode_pastel(InputMode::Library)),
                    ("?", "All keys", OnboardingPalette::mode_pastel(InputMode::Toolbar)),
                ],
                WhichKeyPage::Ai => &[
                    ("a", "Ask about tab", OnboardingPalette::mode_pastel(InputMode::Normal)),
                    ("s", "Switch agent", OnboardingPalette::mode_pastel(InputMode::Explorer)),
                    ("c", "New chat", OnboardingPalette::mode_pastel(InputMode::Insert)),
                ],
            };
            let wk_h = 26.0 + rows.len() as f32 * 20.0;
            let wk_save = canvas.save();
            canvas.translate((VIEW_W - 178.0, VIEW_H - 168.0 + (1.0 - wk) * 14.0));
            let pink_pastel = OnboardingPalette::mode_pastel(InputMode::Normal);
            draw_rrect_fill(canvas, 0.0, 0.0, 166.0, wk_h, 12.0, sk_alpha(pal.night, wk));
            draw_rrect_stroke(
                canvas,
                0.0,
                0.0,
                166.0,
                wk_h,
                12.0,
                1.2,
                sk_alpha(pink_pastel, 0.45 * wk),
            );

            let mono_95 = make_font(9.5, true, true);
            let mono_9 = make_font(9.0, true, true);
            let sans_95 = make_font(9.5, false, false);
            let header = if params.typed_keys.is_empty() {
                "<Space>"
            } else {
                &params.typed_keys
            };
            draw_text_left(canvas, header, 12.0, 17.0, &mono_95, sk_alpha(pink_pastel, wk));

            for (idx, &(k, label, col)) in rows.iter().enumerate() {
                let ry = 25.0 + idx as f32 * 19.5;
                draw_rrect_fill(
                    canvas,
                    12.0,
                    ry,
                    18.0,
                    14.5,
                    4.0,
                    sk_alpha(Rgba::rgb(0x2a, 0x2d, 0x45), wk),
                );
                draw_text_centered(canvas, k, 21.0, ry + 10.5, &mono_9, sk_alpha(col, wk));
                draw_text_left(canvas, label, 38.0, ry + 10.5, &sans_95, sk_alpha(pal.snow, wk));
            }
            canvas.restore_to_count(wk_save);
        }

        // Floating inline Mascot statusline (dodges open Agent dock)
        if params.vim > 0.02 {
            let dodge = -clamp01(params.dock) * (dock_w + 12.0);
            let m_save = canvas.save();
            canvas.translate((dodge, 0.0));
            if !params.typed_keys.is_empty() {
                draw_rrect_fill(
                    canvas,
                    VIEW_W - 118.0,
                    VIEW_H - 36.0,
                    60.0,
                    22.0,
                    8.0,
                    sk_color(pal.night),
                );
                let mono_95 = make_font(9.5, true, true);
                draw_text_centered(
                    canvas,
                    &params.typed_keys,
                    VIEW_W - 88.0,
                    VIEW_H - 21.5,
                    &mono_95,
                    sk_color(pal.snow),
                );
            }
            let size = 42.0 * clamp01(params.vim);
            let mx = VIEW_W - 52.0 + (42.0 - size) * 0.5;
            let my = VIEW_H - 52.0 + (42.0 - size) * 0.5;
            canvas.save();
            canvas.translate((mx, my));
            paint_onboarding_mascot(canvas, size, size, MascotPaintOptions::still(params.mode));
            canvas.restore();
            canvas.restore_to_count(m_save);
        }

        canvas.restore_to_count(clip_save);
    }

    // Subtle crisp border over viewport
    draw_rrect_stroke(
        canvas,
        VIEW_X,
        VIEW_Y,
        VIEW_W,
        VIEW_H,
        13.0,
        1.0,
        sk_alpha(pal.border, 0.45),
    );

    canvas.restore_to_count(save);
}

/// Paints subtle concentric Voronoi hairline arcs behind a chapter stage.
pub fn paint_stage_backdrop(canvas: &sk::Canvas, width: f32, height: f32, accent: Rgba) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    // Soft radial ambient glow centered slightly above middle
    let cx = width * 0.5;
    let cy = height * 0.42;
    let radius = width.max(height) * 0.62;
    let colors = [
        sk::Color4f::from(sk_alpha(accent, 0.14)),
        sk::Color4f::from(sk_alpha(accent, 0.0)),
    ];
    let positions = [0.0_f32, 1.0_f32];
    let grad = sk::gradient::Gradient::new(
        sk::gradient::Colors::new(&colors, Some(&positions), sk::TileMode::Clamp, None),
        sk::gradient::Interpolation::default(),
    );
    if let Some(shader) = sk::gradient::shaders::radial_gradient((sk::Point::new(cx, cy), radius), &grad, None) {
        let mut paint = sk::Paint::default();
        paint.set_shader(shader);
        canvas.draw_rect(sk::Rect::from_xywh(0.0, 0.0, width, height), &paint);
    }

    // Scale 720x520 ornament viewBox using slice-like scaling
    let sx = width / 720.0;
    let sy = height / 520.0;
    let s = sx.max(sy);
    let ox = (width - 720.0 * s) * 0.5;
    let oy = (height - 520.0 * s) * 0.5;

    let save = canvas.save();
    canvas.translate((ox, oy));
    canvas.scale((s, s));

    let mut stroke = sk::Paint::default();
    stroke.set_anti_alias(true);
    stroke.set_style(sk::PaintStyle::Stroke);

    stroke.set_stroke_width(1.2);
    stroke.set_color(sk_alpha(accent, 0.16));
    canvas.draw_circle(sk::Point::new(620.0, 70.0), 210.0, &stroke);

    stroke.set_stroke_width(1.0);
    stroke.set_color(sk_alpha(accent, 0.10));
    canvas.draw_circle(sk::Point::new(620.0, 70.0), 142.0, &stroke);

    stroke.set_stroke_width(1.2);
    stroke.set_color(sk_alpha(accent, 0.14));
    canvas.draw_circle(sk::Point::new(80.0, 460.0), 185.0, &stroke);

    canvas.restore_to_count(save);
}

/// Paints the ambient dual-radial background glow behind the entire onboarding surface.
pub fn paint_ambient_field(canvas: &sk::Canvas, width: f32, height: f32, mode_accent: Rgba, mode_pastel: Rgba) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let glows = [
        (width * 0.82, height * 0.24, 620.0_f32, mode_accent, 0.13_f32),
        (width * 0.16, height * 0.86, 520.0_f32, mode_pastel, 0.09_f32),
    ];
    for (gx, gy, r, col, alpha) in glows {
        let colors = [
            sk::Color4f::from(sk_alpha(col, alpha)),
            sk::Color4f::from(sk_alpha(col, 0.0)),
        ];
        let pos = [0.0_f32, 1.0_f32];
        let grad = sk::gradient::Gradient::new(
            sk::gradient::Colors::new(&colors, Some(&pos), sk::TileMode::Clamp, None),
            sk::gradient::Interpolation::default(),
        );
        if let Some(shader) = sk::gradient::shaders::radial_gradient((sk::Point::new(gx, gy), r), &grad, None) {
            let mut paint = sk::Paint::default();
            paint.set_shader(shader);
            canvas.draw_rect(sk::Rect::from_xywh(0.0, 0.0, width, height), &paint);
        }
    }
}

/// Paints the Start page concentric dashed Voronoi halo rings.
pub fn paint_emblem_rings(canvas: &sk::Canvas, width: f32, height: f32, accent: Rgba) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let scale = (width / 420.0).min(height / 420.0);
    let ox = (width - 420.0 * scale) * 0.5;
    let oy = (height - 420.0 * scale) * 0.5;

    let save = canvas.save();
    canvas.translate((ox, oy));
    canvas.scale((scale, scale));

    let mut outer = sk::Paint::default();
    outer.set_anti_alias(true);
    outer.set_style(sk::PaintStyle::Stroke);
    outer.set_stroke_width(1.2);
    outer.set_color(sk_alpha(accent, 0.34));
    outer.set_path_effect(sk::PathEffect::dash(&[6.0, 10.0], 0.0));
    canvas.draw_circle(sk::Point::new(210.0, 210.0), 176.0, &outer);

    let mut inner = sk::Paint::default();
    inner.set_anti_alias(true);
    inner.set_style(sk::PaintStyle::Stroke);
    inner.set_stroke_width(1.0);
    inner.set_color(sk_alpha(accent, 0.18));
    canvas.draw_circle(sk::Point::new(210.0, 210.0), 144.0, &inner);

    canvas.restore_to_count(save);
}

/// Helper to draw a dashed quadratic Bezier curve.
pub fn draw_dashed_quad(
    canvas: &sk::Canvas,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    stroke_w: f32,
    dash: Option<&[f32]>,
    color: sk::Color,
) {
    let mut pb = sk::PathBuilder::new();
    pb.move_to(sk::Point::new(p0.0, p0.1));
    pb.quad_to(sk::Point::new(p1.0, p1.1), sk::Point::new(p2.0, p2.1));
    let path = pb.detach();
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(sk::PaintStyle::Stroke);
    paint.set_stroke_width(stroke_w);
    paint.set_stroke_cap(sk::PaintCap::Round);
    paint.set_color(color);
    if let Some(intervals) = dash {
        paint.set_path_effect(sk::PathEffect::dash(intervals, 0.0));
    }
    canvas.draw_path(&path, &paint);
}

/// Evaluates a point on a quadratic Bezier curve at parameter `u` in `0.0..=1.0`.
pub fn quad_point(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), u: f32) -> (f32, f32) {
    let inv = 1.0 - u;
    (
        inv * inv * p0.0 + 2.0 * inv * u * p1.0 + u * u * p2.0,
        inv * inv * p0.1 + 2.0 * inv * u * p1.1 + u * u * p2.1,
    )
}

/// Unused constant anchor so PI import is always active for angle helpers.
pub const FULL_CIRCLE_RAD: f32 = 2.0 * PI;
