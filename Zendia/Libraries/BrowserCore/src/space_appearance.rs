/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Space appearance model, Zen-compatible gradient generator math, color harmonies, preset palettes, and wave/texture
//! helpers.
//!
//! All color-space conversions (sRGB, HSL, HSB, OKLab), dot-to-color mapping, harmony placement, underlay compositing,
//! and wave slider interpolation live here as pure functions so Freya, headless runners, and unit tests share one
//! source of truth.

use crate::agent::{cos_f32, sin_f32};
use crate::commands::{ArgumentSchema, ArgumentType, Command, CommandDescriptor, CommandError};
use crate::model::{SpaceGradient, SpaceId, ThemeScheme};
use std::f32::consts::PI;

#[allow(clippy::disallowed_methods)]
mod math {
    pub fn atan2_f32(y: f32, x: f32) -> f32 {
        y.atan2(x)
    }

    pub fn cbrt_f64(x: f64) -> f64 {
        x.cbrt()
    }

    pub fn pow_f64(x: f64, exp: f64) -> f64 {
        x.powf(exp)
    }
}

fn hypot_f32(dx: f32, dy: f32) -> f32 {
    (dx * dx + dy * dy).sqrt()
}

/// Reference pad dimension matching Zen's 360x360 preset coordinate space (center at `(180, 180)`).
pub const PAD_COORD_SIZE: f32 = 360.0;
/// Maximum number of color dots placed on the gradient pad at once.
pub const MAX_GRADIENT_DOTS: usize = 3;
/// Minimum allowed value for the gradient opacity wave slider (matches Linux `min="0.25"`).
pub const MIN_GRADIENT_OPACITY: f32 = 0.25;
/// Maximum allowed value for the gradient opacity wave slider (matches `max="0.8"`).
pub const MAX_GRADIENT_OPACITY: f32 = 0.80;
/// Default gradient opacity when a new space theme is created.
pub const DEFAULT_GRADIENT_OPACITY: f32 = 0.50;
/// Default texture strength when cycling away from [`TextureKind::None`] (`10 / 16`).
pub const DEFAULT_TEXTURE_AMOUNT: f32 = 0.625;
/// Number of discrete detents around the texture rotary dial.
pub const TEXTURE_DIAL_STEPS: usize = 16;

const MIN_LAYER_ALPHA: f32 = 0.20;
const MAX_LAYER_ALPHA: f32 = 0.88;
const TEXT_OPACITY: f32 = 0.90;
const TEXT_TINT: f32 = 0.12;
const GRADIENT_STOPS_COUNT: usize = 6;
const BASE_GRADIENT_END: f32 = 0.75;

/// Built-in workspace profile (container) names offered by the space creation form.
pub const SPACE_PROFILES: &[&str] = &["Default", "Personal", "Work", "Banking", "Shopping"];

/// Color scheme override chosen inside the gradient generator (`auto`, `light`, `dark`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SpaceColorScheme {
    #[default]
    Auto,
    Light,
    Dark,
}

impl SpaceColorScheme {
    pub const ALL: [SpaceColorScheme; 3] = [Self::Auto, Self::Light, Self::Dark];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" | "system" => Some(Self::Auto),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }

    pub fn to_theme_scheme(self) -> ThemeScheme {
        match self {
            Self::Auto => ThemeScheme::System,
            Self::Light => ThemeScheme::Light,
            Self::Dark => ThemeScheme::Dark,
        }
    }

    pub fn from_theme_scheme(scheme: ThemeScheme) -> Self {
        match scheme {
            ThemeScheme::System => Self::Auto,
            ThemeScheme::Light => Self::Light,
            ThemeScheme::Dark => Self::Dark,
        }
    }
}

/// Surface grain/texture style controlled by the rotary dial in the gradient generator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextureKind {
    None,
    #[default]
    Grain,
    Sand,
    Denim,
    Tweed,
}

impl TextureKind {
    pub const ALL: [TextureKind; 5] = [Self::None, Self::Grain, Self::Sand, Self::Denim, Self::Tweed];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Grain => "grain",
            Self::Sand => "sand",
            Self::Denim => "denim",
            Self::Tweed => "tweed",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Grain => "Grain",
            Self::Sand => "Sand",
            Self::Denim => "Denim",
            Self::Tweed => "Tweed",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "grain" => Some(Self::Grain),
            "sand" => Some(Self::Sand),
            "denim" => Some(Self::Denim),
            "tweed" => Some(Self::Tweed),
            _ => None,
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|k| *k == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn previous(self) -> Self {
        let idx = Self::ALL.iter().position(|k| *k == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Harmony rule positioning secondary dots relative to the primary dot on the color wheel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColorHarmony {
    #[default]
    Floating,
    Complementary,
    SingleAnalogous,
    SplitComplementary,
    Analogous,
    Triadic,
}

impl ColorHarmony {
    /// Angular offsets in degrees from the primary dot's wheel angle for each secondary dot.
    pub fn angles(self) -> &'static [f32] {
        match self {
            Self::Floating => &[],
            Self::Complementary => &[180.0],
            Self::SingleAnalogous => &[310.0],
            Self::SplitComplementary => &[150.0, 210.0],
            Self::Analogous => &[50.0, 310.0],
            Self::Triadic => &[120.0, 240.0],
        }
    }

    /// Harmonies applicable when `dot_count` dots are on the pad.
    pub fn applicable_for_count(dot_count: usize) -> &'static [ColorHarmony] {
        match dot_count {
            2 => &[Self::Complementary, Self::SingleAnalogous],
            3 => &[Self::SplitComplementary, Self::Analogous, Self::Triadic],
            _ => &[Self::Floating],
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Floating => "floating",
            Self::Complementary => "complementary",
            Self::SingleAnalogous => "singleAnalogous",
            Self::SplitComplementary => "splitComplementary",
            Self::Analogous => "analogous",
            Self::Triadic => "triadic",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Floating => "Floating",
            Self::Complementary => "Complementary",
            Self::SingleAnalogous => "Analogous",
            Self::SplitComplementary => "Split",
            Self::Analogous => "Analogous",
            Self::Triadic => "Triadic",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "floating" | "float" | "" => Some(Self::Floating),
            "complementary" => Some(Self::Complementary),
            "singleanalogous" | "single_analogous" => Some(Self::SingleAnalogous),
            "splitcomplementary" | "split_complementary" | "split" => Some(Self::SplitComplementary),
            "analogous" => Some(Self::Analogous),
            "triadic" => Some(Self::Triadic),
            _ => None,
        }
    }

    /// Returns the next applicable harmony for `dot_count`.
    pub fn cycle_for_count(self, dot_count: usize) -> Self {
        let choices = Self::applicable_for_count(dot_count);
        let idx = choices.iter().position(|h| *h == self);
        match idx {
            Some(i) => choices[(i + 1) % choices.len()],
            None => choices[0],
        }
    }
}

/// How a dot's pad position maps to an RGB color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DotColorMode {
    /// Standard wheel mapping where radius drives lightness and saturation stays vibrant (`90..100%`).
    #[default]
    Standard,
    /// Preset color with explicit HSB brightness (`lightness` in `0..=100`).
    ExplicitLightness,
    /// Monochrome grayscale dot whose distance from center maps directly from black to white.
    ExplicitBlackWhite,
}

impl DotColorMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::ExplicitLightness => "explicit-lightness",
            Self::ExplicitBlackWhite => "explicit-black-white",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "standard" | "" => Some(Self::Standard),
            "explicit-lightness" | "explicit_lightness" => Some(Self::ExplicitLightness),
            "explicit-black-white" | "explicit_black_white" => Some(Self::ExplicitBlackWhite),
            _ => None,
        }
    }
}

/// One color dot placed on the gradient generator pad.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientDot {
    /// Stable slot index (`0` is the primary dot).
    pub id: u8,
    /// Position in reference pad coordinates (`0.0..=PAD_COORD_SIZE`).
    pub x: f32,
    pub y: f32,
    /// Computed RGB value for this dot.
    pub rgb: (u8, u8, u8),
    pub is_primary: bool,
    pub mode: DotColorMode,
    /// Explicit brightness / lightness in `0.0..=100.0`.
    pub lightness: f32,
}

/// Full appearance configuration owned by a workspace in [`crate::model::BrowserModel`].
#[derive(Clone, Debug, PartialEq)]
pub struct SpaceAppearance {
    pub dots: Vec<GradientDot>,
    pub harmony: ColorHarmony,
    pub scheme: SpaceColorScheme,
    /// Optional preset identifier `(page, index)` when created from a swatch.
    pub preset: Option<(usize, usize)>,
    /// Opacity wave slider value in `[MIN_GRADIENT_OPACITY, MAX_GRADIENT_OPACITY]`.
    pub opacity: f32,
    pub texture_kind: TextureKind,
    /// Texture strength in `[0.0, 1.0]`, quantized in 16ths around the rotary dial.
    pub texture_amount: f32,
    /// Explicit lightness carried by preset/pad edits (`0.0..=100.0`).
    pub lightness: f32,
    /// Profile / container name associated with this space (defaults to `"Default"`).
    pub profile: String,
}

impl Default for SpaceAppearance {
    fn default() -> Self {
        Self {
            dots: Vec::new(),
            harmony: ColorHarmony::Floating,
            scheme: SpaceColorScheme::Auto,
            preset: None,
            opacity: DEFAULT_GRADIENT_OPACITY,
            texture_kind: TextureKind::Grain,
            texture_amount: 0.0,
            lightness: 50.0,
            profile: "Default".to_string(),
        }
    }
}

impl SpaceAppearance {
    /// Creates an empty appearance with the default profile.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Creates an appearance from a predefined preset swatch at `(page, index)`.
    pub fn from_preset(page: usize, index: usize) -> Option<Self> {
        let preset = preset_at(page, index)?;
        let mut appearance = Self {
            harmony: preset.harmony,
            preset: Some((page, index)),
            opacity: DEFAULT_GRADIENT_OPACITY,
            lightness: preset.lightness,
            ..Self::default()
        };
        let primary = GradientDot {
            id: 0,
            x: preset.x,
            y: preset.y,
            rgb: preset.rgb,
            is_primary: true,
            mode: preset.mode,
            lightness: preset.lightness,
        };
        let mut dots = vec![primary];
        for i in 1..preset.num_dots {
            dots.push(GradientDot {
                id: i as u8,
                x: PAD_COORD_SIZE * 0.5,
                y: PAD_COORD_SIZE * 0.5,
                rgb: preset.rgb,
                is_primary: false,
                mode: preset.mode,
                lightness: preset.lightness,
            });
        }
        if preset.num_dots > 1 {
            dots = compute_harmony_dots(&dots, preset.harmony, PAD_COORD_SIZE, preset.lightness);
            if let Some(first) = dots.first_mut() {
                first.rgb = preset.rgb;
            }
            if let Some(preview) = preset.preview_colors {
                for (slot, rgb) in preview.iter().enumerate() {
                    if let Some(dot) = dots.get_mut(slot) {
                        dot.rgb = *rgb;
                    }
                }
            }
        }
        appearance.dots = dots;
        Some(appearance)
    }

    /// Adds a dot at `(x, y)` (when empty) or spawns the next harmony dot (up to [`MAX_GRADIENT_DOTS`]).
    pub fn add_dot(&mut self, at: Option<(f32, f32)>) {
        if self.dots.len() >= MAX_GRADIENT_DOTS {
            return;
        }
        self.preset = None;
        if self.dots.is_empty() {
            let (raw_x, raw_y) = at.unwrap_or((PAD_COORD_SIZE * 0.68, PAD_COORD_SIZE * 0.36));
            let (x, y) = clamp_dot_to_circle(raw_x, raw_y, PAD_COORD_SIZE);
            let (rgb, lightness) =
                color_from_pad_position(x, y, PAD_COORD_SIZE, DotColorMode::Standard, self.lightness);
            self.lightness = lightness;
            self.harmony = ColorHarmony::Floating;
            self.dots.push(GradientDot {
                id: 0,
                x,
                y,
                rgb,
                is_primary: true,
                mode: DotColorMode::Standard,
                lightness,
            });
            return;
        }

        let next_count = self.dots.len() + 1;
        let next_harmony = ColorHarmony::applicable_for_count(next_count)[0];
        self.harmony = next_harmony;
        let mode = self.dots[0].mode;
        let lightness = self.dots[0].lightness;
        self.dots.push(GradientDot {
            id: self.dots.len() as u8,
            x: PAD_COORD_SIZE * 0.5,
            y: PAD_COORD_SIZE * 0.5,
            rgb: self.dots[0].rgb,
            is_primary: false,
            mode,
            lightness,
        });
        self.dots = compute_harmony_dots(&self.dots, next_harmony, PAD_COORD_SIZE, lightness);
    }

    /// Removes the last dot (or a specific dot by `id`) and recalculates harmony positions for remaining dots.
    pub fn remove_dot(&mut self, dot_id: Option<u8>) {
        if self.dots.is_empty() {
            return;
        }
        self.preset = None;
        if let Some(id) = dot_id {
            if let Some(pos) = self.dots.iter().position(|d| d.id == id) {
                self.dots.remove(pos);
            } else {
                return;
            }
        } else {
            self.dots.pop();
        }
        for (idx, dot) in self.dots.iter_mut().enumerate() {
            dot.id = idx as u8;
            dot.is_primary = idx == 0;
        }
        if self.dots.is_empty() {
            self.harmony = ColorHarmony::Floating;
            return;
        }
        let next_harmony = match self.dots.len() {
            1 => ColorHarmony::Floating,
            2 => {
                if self.harmony == ColorHarmony::Analogous {
                    ColorHarmony::SingleAnalogous
                } else {
                    ColorHarmony::Complementary
                }
            }
            _ => ColorHarmony::applicable_for_count(self.dots.len())[0],
        };
        self.harmony = next_harmony;
        let lightness = self.dots[0].lightness;
        self.dots = compute_harmony_dots(&self.dots, next_harmony, PAD_COORD_SIZE, lightness);
    }

    /// Moves the primary dot to `(x, y)` (clamped to the color wheel circle) and updates all secondary harmony dots.
    pub fn move_primary_dot(&mut self, raw_x: f32, raw_y: f32) {
        self.preset = None;
        let (x, y) = clamp_dot_to_circle(raw_x, raw_y, PAD_COORD_SIZE);
        if self.dots.is_empty() {
            self.add_dot(Some((x, y)));
            return;
        }
        let mode = self.dots[0].mode;
        let (rgb, lightness) = color_from_pad_position(x, y, PAD_COORD_SIZE, mode, self.lightness);
        self.lightness = lightness;
        self.dots[0].x = x;
        self.dots[0].y = y;
        self.dots[0].rgb = rgb;
        self.dots[0].lightness = lightness;
        if self.dots.len() > 1 {
            let harmony = if self.harmony == ColorHarmony::Floating {
                ColorHarmony::applicable_for_count(self.dots.len())[0]
            } else {
                self.harmony
            };
            self.harmony = harmony;
            self.dots = compute_harmony_dots(&self.dots, harmony, PAD_COORD_SIZE, lightness);
        }
    }

    /// Cycles the color harmony when 2 or 3 dots are placed on the pad.
    pub fn cycle_harmony(&mut self) {
        if self.dots.len() < 2 {
            return;
        }
        self.preset = None;
        self.harmony = self.harmony.cycle_for_count(self.dots.len());
        let lightness = self.dots[0].lightness;
        self.dots = compute_harmony_dots(&self.dots, self.harmony, PAD_COORD_SIZE, lightness);
    }

    /// Sets gradient opacity clamped to `[MIN_GRADIENT_OPACITY, MAX_GRADIENT_OPACITY]`.
    pub fn set_opacity(&mut self, opacity: f32) {
        if opacity.is_finite() {
            self.opacity = opacity.clamp(MIN_GRADIENT_OPACITY, MAX_GRADIENT_OPACITY);
        }
    }

    /// Sets texture kind and quantized 16-step texture strength.
    pub fn set_texture(&mut self, kind: TextureKind, amount: f32) {
        self.texture_kind = kind;
        let clamped = if amount.is_finite() {
            amount.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let quantized = quantize_texture_amount(clamped);
        if kind == TextureKind::None {
            self.texture_amount = 0.0;
        } else {
            self.texture_amount = quantized;
        }
    }

    /// Cycles texture kind forward or backward, keeping strength non-zero when moving off `None`.
    pub fn cycle_texture(&mut self, forward: bool) {
        self.texture_kind = if forward {
            self.texture_kind.next()
        } else {
            self.texture_kind.previous()
        };
        if self.texture_kind == TextureKind::None {
            self.texture_amount = 0.0;
        } else if self.texture_amount <= 0.001 {
            self.texture_amount = DEFAULT_TEXTURE_AMOUNT;
        }
    }

    /// Projects this appearance onto the simplified [`SpaceGradient`] stored on [`crate::model::Space`].
    pub fn to_space_gradient(&self, is_dark: bool) -> SpaceGradient {
        let resolved = compute_gradient(self, is_dark);
        SpaceGradient {
            stops: resolved.stops,
            angle_deg: resolved.angle_deg,
            opacity: self.opacity,
        }
    }
}

/// Resolved gradient output computed from a [`SpaceAppearance`] and active color scheme.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedSpaceGradient {
    /// Composited sRGB stops along the base linear gradient.
    pub stops: Vec<(u8, u8, u8)>,
    /// Optional third-color vertical overlay tint when 3 dots are active.
    pub overlay_rgb: Option<(u8, u8, u8)>,
    /// Gradient angle in degrees.
    pub angle_deg: f32,
    /// Primary/dominant dot RGB before underlay compositing.
    pub dominant_rgb: Option<(u8, u8, u8)>,
    /// Whether chrome text over this gradient should prefer light-on-dark contrast.
    pub should_be_dark: bool,
    /// Effective texture kind and opacity (`0.0` disables texture overlay).
    pub texture_kind: TextureKind,
    pub texture_amount: f32,
}

/// Definition of one predefined swatch in the gradient generator's paged palette row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientPreset {
    pub x: f32,
    pub y: f32,
    pub lightness: f32,
    pub harmony: ColorHarmony,
    pub num_dots: usize,
    pub mode: DotColorMode,
    pub rgb: (u8, u8, u8),
    /// Optional 3-color preview for multi-dot analogous swatches.
    pub preview_colors: Option<[(u8, u8, u8); 3]>,
}

/// One page of predefined color swatches in the gradient generator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientPresetPage {
    pub id: &'static str,
    pub label: &'static str,
    pub presets: &'static [GradientPreset],
}

const fn single_preset(x: f32, y: f32, lightness: f32, rgb: (u8, u8, u8)) -> GradientPreset {
    GradientPreset {
        x,
        y,
        lightness,
        harmony: ColorHarmony::Floating,
        num_dots: 1,
        mode: DotColorMode::ExplicitLightness,
        rgb,
        preview_colors: None,
    }
}

const fn tri_preset(
    x: f32,
    y: f32,
    lightness: f32,
    c1: (u8, u8, u8),
    c2: (u8, u8, u8),
    c3: (u8, u8, u8),
) -> GradientPreset {
    GradientPreset {
        x,
        y,
        lightness,
        harmony: ColorHarmony::Analogous,
        num_dots: 3,
        mode: DotColorMode::ExplicitLightness,
        rgb: c1,
        preview_colors: Some([c1, c2, c3]),
    }
}

const fn bw_preset(x: f32, y: f32, rgb: (u8, u8, u8)) -> GradientPreset {
    GradientPreset {
        x,
        y,
        lightness: 50.0,
        harmony: ColorHarmony::Floating,
        num_dots: 1,
        mode: DotColorMode::ExplicitBlackWhite,
        rgb,
        preview_colors: None,
    }
}

const PAGE_VIVID_SINGLE: &[GradientPreset] = &[
    single_preset(330.0, 248.0, 95.0, (242, 234, 228)),
    single_preset(282.0, 127.0, 95.0, (242, 155, 187)),
    single_preset(255.0, 75.0, 65.0, (166, 114, 157)),
    single_preset(244.0, 165.0, 95.0, (242, 94, 107)),
    single_preset(234.0, 190.0, 100.0, (255, 133, 88)),
    single_preset(217.0, 220.0, 97.0, (248, 213, 88)),
    single_preset(135.0, 191.0, 91.0, (52, 232, 149)),
    single_preset(84.0, 145.0, 85.0, (108, 186, 217)),
    single_preset(99.0, 54.0, 54.0, (102, 103, 137)),
];

const PAGE_VIVID_ANALOGOUS: &[GradientPreset] = &[
    tri_preset(330.0, 248.0, 95.0, (242, 234, 228), (238, 242, 228), (242, 228, 234)),
    tri_preset(282.0, 127.0, 95.0, (242, 155, 187), (242, 196, 155), (224, 155, 242)),
    tri_preset(255.0, 75.0, 65.0, (166, 114, 157), (166, 114, 114), (132, 114, 166)),
    tri_preset(244.0, 165.0, 95.0, (242, 94, 107), (242, 204, 94), (242, 94, 230)),
    tri_preset(234.0, 190.0, 100.0, (255, 133, 88), (238, 255, 88), (255, 88, 182)),
    tri_preset(217.0, 220.0, 97.0, (248, 213, 88), (150, 248, 88), (248, 88, 96)),
    tri_preset(135.0, 191.0, 91.0, (52, 232, 149), (52, 165, 232), (105, 232, 52)),
    tri_preset(84.0, 145.0, 85.0, (108, 186, 217), (121, 108, 217), (108, 217, 157)),
    tri_preset(99.0, 54.0, 54.0, (102, 103, 137), (130, 102, 137), (102, 132, 137)),
];

const PAGE_PASTEL_SINGLE: &[GradientPreset] = &[
    single_preset(349.0, 219.0, 100.0, (254, 251, 250)),
    single_preset(323.0, 94.0, 100.0, (255, 233, 243)),
    single_preset(274.0, 47.0, 95.0, (241, 209, 236)),
    single_preset(319.0, 154.0, 100.0, (255, 203, 209)),
    single_preset(319.0, 219.0, 100.0, (255, 224, 211)),
    single_preset(288.0, 292.0, 100.0, (255, 249, 229)),
    single_preset(38.0, 265.0, 99.0, (222, 253, 235)),
    single_preset(17.0, 125.0, 99.0, (220, 243, 252)),
    single_preset(88.0, 36.0, 90.0, (197, 198, 229)),
];

const PAGE_PASTEL_ANALOGOUS: &[GradientPreset] = &[
    tri_preset(349.0, 219.0, 100.0, (254, 251, 250), (254, 254, 250), (254, 250, 252)),
    tri_preset(323.0, 94.0, 100.0, (255, 233, 243), (255, 241, 233), (249, 233, 255)),
    tri_preset(274.0, 47.0, 95.0, (241, 209, 236), (241, 209, 209), (219, 209, 241)),
    tri_preset(319.0, 154.0, 100.0, (255, 203, 209), (255, 240, 203), (255, 203, 252)),
    tri_preset(319.0, 219.0, 100.0, (255, 224, 211), (249, 255, 211), (255, 211, 235)),
    tri_preset(288.0, 292.0, 100.0, (255, 249, 229), (239, 255, 229), (255, 229, 231)),
    tri_preset(38.0, 265.0, 99.0, (222, 253, 235), (222, 245, 253), (235, 253, 222)),
    tri_preset(17.0, 125.0, 99.0, (220, 243, 252), (224, 220, 252), (220, 252, 234)),
    tri_preset(88.0, 36.0, 90.0, (197, 198, 229), (223, 197, 229), (197, 225, 229)),
];

const PAGE_DEEP_SINGLE: &[GradientPreset] = &[
    single_preset(188.0, 53.0, 35.0, (78, 58, 90)),
    single_preset(244.0, 111.0, 41.0, (105, 53, 88)),
    single_preset(255.0, 170.0, 56.0, (143, 64, 66)),
    single_preset(233.0, 199.0, 69.0, (177, 109, 64)),
    single_preset(232.0, 226.0, 84.0, (215, 179, 93)),
    single_preset(250.0, 299.0, 80.0, (205, 204, 164)),
    single_preset(85.0, 275.0, 66.0, (122, 169, 130)),
    single_preset(112.0, 197.0, 38.0, (34, 97, 73)),
    single_preset(117.0, 136.0, 42.0, (38, 69, 107)),
];

const PAGE_DEEP_ANALOGOUS: &[GradientPreset] = &[
    tri_preset(188.0, 53.0, 35.0, (78, 58, 90), (90, 58, 75), (58, 65, 90)),
    tri_preset(244.0, 111.0, 41.0, (105, 53, 88), (105, 61, 53), (79, 53, 105)),
    tri_preset(255.0, 170.0, 56.0, (143, 64, 66), (143, 128, 64), (143, 64, 132)),
    tri_preset(233.0, 199.0, 69.0, (177, 109, 64), (151, 177, 64), (177, 64, 113)),
    tri_preset(232.0, 226.0, 84.0, (215, 179, 93), (149, 215, 93), (215, 93, 109)),
    tri_preset(250.0, 299.0, 80.0, (205, 204, 164), (172, 205, 164), (205, 170, 164)),
    tri_preset(85.0, 275.0, 66.0, (122, 169, 130), (122, 169, 169), (153, 169, 122)),
    tri_preset(112.0, 197.0, 38.0, (34, 97, 73), (34, 68, 97), (47, 97, 34)),
    tri_preset(117.0, 136.0, 42.0, (38, 69, 107), (64, 38, 107), (38, 107, 87)),
];

const PAGE_MONOCHROME: &[GradientPreset] = &[
    bw_preset(356.0, 180.0, (242, 242, 242)),
    bw_preset(337.5, 180.0, (224, 224, 224)),
    bw_preset(315.0, 180.0, (192, 192, 192)),
    bw_preset(292.5, 180.0, (160, 160, 160)),
    bw_preset(270.0, 180.0, (128, 128, 128)),
    bw_preset(247.5, 180.0, (96, 96, 96)),
    bw_preset(225.0, 180.0, (64, 64, 64)),
    bw_preset(202.5, 180.0, (32, 32, 32)),
    bw_preset(180.0, 180.0, (0, 0, 0)),
];

/// All 7 predefined color preset pages (63 swatches) matching Zen's theme picker.
pub const GRADIENT_PRESET_PAGES: &[GradientPresetPage] = &[
    GradientPresetPage {
        id: "vivid-single",
        label: "Vivid",
        presets: PAGE_VIVID_SINGLE,
    },
    GradientPresetPage {
        id: "vivid-analogous",
        label: "Vivid Trio",
        presets: PAGE_VIVID_ANALOGOUS,
    },
    GradientPresetPage {
        id: "pastel-single",
        label: "Pastel",
        presets: PAGE_PASTEL_SINGLE,
    },
    GradientPresetPage {
        id: "pastel-analogous",
        label: "Pastel Trio",
        presets: PAGE_PASTEL_ANALOGOUS,
    },
    GradientPresetPage {
        id: "deep-single",
        label: "Deep",
        presets: PAGE_DEEP_SINGLE,
    },
    GradientPresetPage {
        id: "deep-analogous",
        label: "Deep Trio",
        presets: PAGE_DEEP_ANALOGOUS,
    },
    GradientPresetPage {
        id: "monochrome",
        label: "Monochrome",
        presets: PAGE_MONOCHROME,
    },
];

/// Looks up a predefined preset by `(page, index)`.
pub fn preset_at(page: usize, index: usize) -> Option<GradientPreset> {
    GRADIENT_PRESET_PAGES.get(page)?.presets.get(index).copied()
}

// ---------------------------------------------------------------------------
// Pure color-space & gradient math (matching ZenGradientGenerator.mjs)
// ---------------------------------------------------------------------------

/// Clamps `(x, y)` into the inscribed circle of a square pad of size `pad_size`.
pub fn clamp_dot_to_circle(x: f32, y: f32, pad_size: f32) -> (f32, f32) {
    let cx = pad_size * 0.5;
    let cy = pad_size * 0.5;
    let radius = (pad_size * 0.5).max(1.0);
    let dx = x - cx;
    let dy = y - cy;
    let dist = hypot_f32(dx, dy);
    if dist > radius {
        let scale = radius / dist;
        (cx + dx * scale, cy + dy * scale)
    } else {
        (x, y)
    }
}

/// Maps a pad coordinate `(x, y)` to an `(R, G, B)` color and updated lightness in `0..=100`.
pub fn color_from_pad_position(
    x: f32,
    y: f32,
    pad_size: f32,
    mode: DotColorMode,
    current_lightness: f32,
) -> ((u8, u8, u8), f32) {
    let cx = pad_size * 0.5;
    let cy = pad_size * 0.5;
    let radius = (pad_size * 0.5).max(1.0);
    let dx = x - cx;
    let dy = y - cy;
    let distance = (hypot_f32(dx, dy) - 0.2).max(0.0);
    let mut angle = math::atan2_f32(dy, dx).to_degrees();
    if angle < 0.0 {
        angle += 360.0;
    }
    let normalized_distance = 1.0 - (distance / radius).min(1.0);
    let hue = angle;

    match mode {
        DotColorMode::ExplicitLightness => {
            let lightness = current_lightness.clamp(0.0, 100.0);
            let rgb = hsb_to_rgb(hue, normalized_distance, lightness / 100.0);
            (rgb, lightness)
        }
        DotColorMode::ExplicitBlackWhite => {
            let lightness = ((1.0 - normalized_distance) * 100.0).round().clamp(0.0, 100.0);
            let rgb = hsl_to_rgb(hue / 360.0, 0.0, lightness / 100.0);
            (rgb, lightness)
        }
        DotColorMode::Standard => {
            let saturation = 90.0 + (1.0 - normalized_distance) * 10.0;
            let lightness = ((1.0 - normalized_distance) * 100.0).round().clamp(0.0, 100.0);
            let rgb = hsl_to_rgb(hue / 360.0, saturation / 100.0, lightness / 100.0);
            (rgb, lightness)
        }
    }
}

/// Converts an RGB color back to an initial `(x, y)` pad position.
pub fn position_from_rgb(rgb: (u8, u8, u8), pad_size: f32) -> (f32, f32) {
    let cx = pad_size * 0.5;
    let cy = pad_size * 0.5;
    let radius = pad_size * 0.5;
    let (hue_deg, sat, _) = rgb_to_hsl(rgb.0, rgb.1, rgb.2);
    let angle = hue_deg.to_radians();
    let dist = radius * sat.clamp(0.0, 1.0);
    (cx + dist * cos_f32(angle), cy + dist * sin_f32(angle))
}

/// Repositions secondary dots around the color wheel according to `harmony` relative to dot `0`.
pub fn compute_harmony_dots(
    dots: &[GradientDot],
    harmony: ColorHarmony,
    pad_size: f32,
    current_lightness: f32,
) -> Vec<GradientDot> {
    if dots.is_empty() {
        return Vec::new();
    }
    let primary = dots.iter().find(|d| d.id == 0).unwrap_or(&dots[0]);
    let angles = harmony.angles();
    if angles.is_empty() {
        return dots.to_vec();
    }

    let cx = pad_size * 0.5;
    let cy = pad_size * 0.5;
    let radius = pad_size * 0.5;
    let dx = primary.x - cx;
    let dy = primary.y - cy;
    let base_angle = (math::atan2_f32(dy, dx).to_degrees() + 360.0) % 360.0;
    let dist = hypot_f32(dx, dy).min(radius);

    let (primary_rgb, primary_lightness) =
        color_from_pad_position(primary.x, primary.y, pad_size, primary.mode, current_lightness);

    let mut out = vec![GradientDot {
        id: 0,
        x: primary.x,
        y: primary.y,
        rgb: primary_rgb,
        is_primary: true,
        mode: primary.mode,
        lightness: primary_lightness,
    }];

    for (index, offset_deg) in angles.iter().enumerate() {
        let angle_rad = ((base_angle + offset_deg) % 360.0).to_radians();
        let nx = cx + dist * cos_f32(angle_rad);
        let ny = cy + dist * sin_f32(angle_rad);
        let (rgb, lightness) = color_from_pad_position(nx, ny, pad_size, primary.mode, primary_lightness);
        out.push(GradientDot {
            id: (index + 1) as u8,
            x: nx,
            y: ny,
            rgb,
            is_primary: false,
            mode: primary.mode,
            lightness,
        });
    }

    out
}

/// Converts HSL (`h`, `s`, `l` in `0.0..=1.0`) to sRGB `(u8, u8, u8)`.
pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(1.0);
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let (r, g, b) = if s <= 0.0 {
        (l, l, l)
    } else {
        let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let p = 2.0 * l - q;
        (
            hue_to_rgb(p, q, h + 1.0 / 3.0),
            hue_to_rgb(p, q, h),
            hue_to_rgb(p, q, h - 1.0 / 3.0),
        )
    };
    (
        (r * 255.0).round().clamp(0.0, 255.0) as u8,
        (g * 255.0).round().clamp(0.0, 255.0) as u8,
        (b * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 0.5 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

/// Converts sRGB `(u8, u8, u8)` to `(hue_deg [0..360), saturation [0..1], lightness [0..1])`.
pub fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let l = (min + max) * 0.5;
    let s = if d == 0.0 {
        0.0
    } else {
        d / (1.0 - (2.0 * l - 1.0).abs())
    };
    (h * 60.0, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0))
}

/// Converts HSB/HSV (`hue_deg` in `0..360`, `saturation` & `brightness` in `0..=1`) to sRGB `(u8, u8, u8)`.
pub fn hsb_to_rgb(hue_deg: f32, saturation: f32, brightness: f32) -> (u8, u8, u8) {
    let h = hue_deg.rem_euclid(360.0);
    let s = saturation.clamp(0.0, 1.0);
    let v = brightness.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0).floor() as i32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

fn srgb_to_linear(channel: f64) -> f64 {
    let c = channel / 255.0;
    let a = c.abs();
    c.signum()
        * if a <= 0.04045 {
            a / 12.92
        } else {
            math::pow_f64((a + 0.055) / 1.055, 2.4)
        }
}

fn linear_to_srgb(c: f64) -> f64 {
    let a = c.abs();
    let gamma = c.signum()
        * if a <= 0.0031308 {
            a * 12.92
        } else {
            1.055 * math::pow_f64(a, 1.0 / 2.4) - 0.055
        };
    (gamma * 255.0).clamp(0.0, 255.0)
}

/// Converts sRGB `[r, g, b]` to perceptual OKLab `[L, a, b]`.
pub fn rgb_to_oklab(rgb: (u8, u8, u8)) -> [f64; 3] {
    let lr = srgb_to_linear(rgb.0 as f64);
    let lg = srgb_to_linear(rgb.1 as f64);
    let lb = srgb_to_linear(rgb.2 as f64);
    let l = math::cbrt_f64(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
    let m = math::cbrt_f64(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
    let s = math::cbrt_f64(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

/// Converts OKLab `[L, a, b]` back to clamped sRGB `(u8, u8, u8)`.
pub fn oklab_to_rgb(lab: [f64; 3]) -> (u8, u8, u8) {
    let l = (lab[0] + 0.3963377774 * lab[1] + 0.2158037573 * lab[2]).powi(3);
    let m = (lab[0] - 0.1055613458 * lab[1] - 0.0638541728 * lab[2]).powi(3);
    let s = (lab[0] - 0.0894841775 * lab[1] - 1.2914855480 * lab[2]).powi(3);
    let lr = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    let lg = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    let lb = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;
    (
        linear_to_srgb(lr).round() as u8,
        linear_to_srgb(lg).round() as u8,
        linear_to_srgb(lb).round() as u8,
    )
}

/// Mixes two sRGB colors linearly in perceptual OKLab space.
pub fn mix_in_oklab(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0) as f64;
    let lab1 = rgb_to_oklab(a);
    let lab2 = rgb_to_oklab(b);
    oklab_to_rgb([
        lab1[0] + (lab2[0] - lab1[0]) * t,
        lab1[1] + (lab2[1] - lab1[1]) * t,
        lab1[2] + (lab2[2] - lab1[2]) * t,
    ])
}

/// Normalizes an opacity value in `[MIN_GRADIENT_OPACITY, MAX_GRADIENT_OPACITY]` to `[0.0, 1.0]`.
pub fn layer_intensity(opacity: f32) -> f32 {
    let range = MAX_GRADIENT_OPACITY - MIN_GRADIENT_OPACITY;
    ((opacity - MIN_GRADIENT_OPACITY) / range).clamp(0.0, 1.0)
}

/// Computes the color-layer compositing alpha over the dark/light underlay (matches Zen's quadratic ramp).
pub fn layer_alpha(opacity: f32) -> f32 {
    let intensity = layer_intensity(opacity);
    MIN_LAYER_ALPHA + (MAX_LAYER_ALPHA - MIN_LAYER_ALPHA) * intensity * intensity
}

/// Composites an sRGB color over Zen's dark (`#131313`) or light (`#ffffff`) underlay.
pub fn composite_over_underlay(rgb: (u8, u8, u8), alpha: f32, is_dark: bool) -> (u8, u8, u8) {
    let underlay = if is_dark {
        (19.0, 19.0, 19.0)
    } else {
        (255.0, 255.0, 255.0)
    };
    let a = alpha.clamp(0.0, 1.0);
    (
        (rgb.0 as f32 * a + underlay.0 * (1.0 - a)).round().clamp(0.0, 255.0) as u8,
        (rgb.1 as f32 * a + underlay.1 * (1.0 - a)).round().clamp(0.0, 255.0) as u8,
        (rgb.2 as f32 * a + underlay.2 * (1.0 - a)).round().clamp(0.0, 255.0) as u8,
    )
}

/// Relative WCAG luminance of an sRGB color in `0.0..=1.0`.
pub fn relative_luminance(rgb: (u8, u8, u8)) -> f32 {
    let lin = |v: u8| -> f64 {
        let c = v as f64 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            math::pow_f64((c + 0.055) / 1.055, 2.4)
        }
    };
    (0.2126 * lin(rgb.0) + 0.7152 * lin(rgb.1) + 0.0722 * lin(rgb.2)) as f32
}

/// WCAG contrast ratio between two sRGB colors (`1.0..=21.0`).
pub fn contrast_ratio(a: (u8, u8, u8), b: (u8, u8, u8)) -> f32 {
    let l1 = relative_luminance(a);
    let l2 = relative_luminance(b);
    (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05)
}

fn blend_rgb(a: (u8, u8, u8), b: (u8, u8, u8), fraction: f32) -> (u8, u8, u8) {
    let p = fraction.clamp(0.0, 1.0);
    (
        (a.0 as f32 * p + b.0 as f32 * (1.0 - p)).round().clamp(0.0, 255.0) as u8,
        (a.1 as f32 * p + b.1 as f32 * (1.0 - p)).round().clamp(0.0, 255.0) as u8,
        (a.2 as f32 * p + b.2 as f32 * (1.0 - p)).round().clamp(0.0, 255.0) as u8,
    )
}

/// Determines whether dark-mode text/chrome contrast works better over `accent` composited at `opacity`.
pub fn should_be_dark_mode(accent: (u8, u8, u8), opacity: f32, is_dark: bool) -> bool {
    let bg = composite_over_underlay(accent, layer_alpha(opacity), is_dark);
    let light_title = blend_rgb(accent, (0, 0, 0), TEXT_TINT);
    let dark_title = blend_rgb(accent, (255, 255, 255), TEXT_TINT);
    let dark_text = blend_rgb(dark_title, bg, TEXT_OPACITY);
    let light_text = blend_rgb(light_title, bg, TEXT_OPACITY);
    contrast_ratio(bg, dark_text) > contrast_ratio(bg, light_text)
}

/// Computes the complete [`ResolvedSpaceGradient`] from `appearance` and the active window scheme.
pub fn compute_gradient(appearance: &SpaceAppearance, is_dark: bool) -> ResolvedSpaceGradient {
    let effective_dark = match appearance.scheme {
        SpaceColorScheme::Auto => is_dark,
        SpaceColorScheme::Light => false,
        SpaceColorScheme::Dark => true,
    };
    let alpha = layer_alpha(appearance.opacity);

    if appearance.dots.is_empty() {
        let base = if effective_dark { (19, 19, 19) } else { (233, 233, 233) };
        return ResolvedSpaceGradient {
            stops: vec![base, base],
            overlay_rgb: None,
            angle_deg: 135.0,
            dominant_rgb: None,
            should_be_dark: effective_dark,
            texture_kind: appearance.texture_kind,
            texture_amount: appearance.texture_amount,
        };
    }

    let mut ordered = appearance.dots.clone();
    ordered.sort_by(|a, b| b.is_primary.cmp(&a.is_primary).then_with(|| a.id.cmp(&b.id)));
    let dominant = ordered[0].rgb;
    let should_dark = should_be_dark_mode(dominant, appearance.opacity, effective_dark);

    if ordered.len() == 1 {
        let painted = composite_over_underlay(ordered[0].rgb, alpha, effective_dark);
        return ResolvedSpaceGradient {
            stops: vec![painted, painted],
            overlay_rgb: None,
            angle_deg: 135.0,
            dominant_rgb: Some(dominant),
            should_be_dark: should_dark,
            texture_kind: appearance.texture_kind,
            texture_amount: appearance.texture_amount,
        };
    }

    let c0 = ordered[0].rgb;
    let c1 = ordered[1].rgb;
    let c2 = ordered.get(2).map(|d| d.rgb);

    let mut stops = Vec::with_capacity(GRADIENT_STOPS_COUNT);
    for i in 0..GRADIENT_STOPS_COUNT {
        let pos = i as f32 / (GRADIENT_STOPS_COUNT - 1) as f32;
        let base_t = (pos / BASE_GRADIENT_END).clamp(0.0, 1.0);
        let mut mixed = mix_in_oklab(c0, c1, base_t);
        if let Some(third) = c2 {
            // Blend the 3rd dot subtly into the upper/trailing half so linear-gradient backdrops show all three hues.
            let overlay_t = (pos * 0.48).clamp(0.0, 0.55);
            mixed = mix_in_oklab(mixed, third, overlay_t);
        }
        stops.push(composite_over_underlay(mixed, alpha, effective_dark));
    }

    ResolvedSpaceGradient {
        stops,
        overlay_rgb: c2.map(|rgb| composite_over_underlay(rgb, alpha, effective_dark)),
        angle_deg: 135.0,
        dominant_rgb: Some(dominant),
        should_be_dark: should_dark,
        texture_kind: appearance.texture_kind,
        texture_amount: appearance.texture_amount,
    }
}

/// Quantizes a raw `[0.0, 1.0]` texture dial value into 16 discrete detents (`0/16 .. 15/16`).
pub fn quantize_texture_amount(raw: f32) -> f32 {
    if !raw.is_finite() {
        return 0.0;
    }
    let steps = TEXTURE_DIAL_STEPS as f32;
    let q = (raw.clamp(0.0, 1.0) * steps).round() / steps;
    if q >= 1.0 { 0.0 } else { q }
}

/// Converts a pointer angle relative to the center of the texture dial into a quantized `0.0..1.0` value.
pub fn texture_amount_from_dial_point(dx: f32, dy: f32) -> f32 {
    let mut deg = math::atan2_f32(dy, dx).to_degrees() + 90.0;
    if deg < 0.0 {
        deg += 360.0;
    }
    quantize_texture_amount(deg / 360.0)
}

/// Samples the interpolated sine-wave slider polyline for `intensity` in `0.0..=1.0` across `width x height`.
///
/// At `intensity = 0.0` the wave flattens into a straight horizontal line; at `intensity = 1.0` it reaches full
/// sine amplitude matching Zen's opacity slider wave.
pub fn wave_slider_points(intensity: f32, width: f32, height: f32, samples: usize) -> Vec<(f32, f32)> {
    let count = samples.max(2);
    let amp = intensity.clamp(0.0, 1.0) * (height * 0.36);
    let cy = height * 0.5;
    let cycles = 6.5_f32;
    (0..count)
        .map(|i| {
            let t = i as f32 / (count - 1) as f32;
            let x = t * width;
            let y = cy - sin_f32(t * cycles * 2.0 * PI) * amp;
            (x, y)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Command parsing & descriptors for spaces editing & gradient generator
// ---------------------------------------------------------------------------

fn find_arg<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn parse_optional_space_id(args: &[(String, String)], command: &str) -> Result<Option<SpaceId>, CommandError> {
    match find_arg(args, "space_id") {
        Some(raw) => raw
            .parse::<u64>()
            .map(|id| Some(SpaceId(id)))
            .map_err(|_| CommandError::InvalidArgument {
                command: command.to_string(),
                message: format!("expected integer `space_id`, got `{raw}`"),
            }),
        None => Ok(None),
    }
}

/// Parses space creation form, theme picker, and space appearance commands.
pub fn parse_command(name: &str, args: &[(String, String)]) -> Result<Option<Command>, CommandError> {
    let cmd = match name {
        "space.create_form.open" | "space.create_dialog" => Command::SpaceCreateFormOpen,
        "space.create_form.cancel" => Command::SpaceCreateFormCancel,
        "space.create_form.submit" => {
            let space_name = find_arg(args, "name")
                .ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: "missing required `name`".to_string(),
                })?
                .to_string();
            Command::SpaceCreateFormSubmit {
                name: space_name,
                icon: find_arg(args, "icon").map(str::to_string),
                profile: find_arg(args, "profile").map(str::to_string),
                appearance: None,
            }
        }
        "theme.picker.open" => Command::ThemePickerOpen {
            space_id: parse_optional_space_id(args, name)?,
        },
        "theme.picker.close" => Command::ThemePickerClose,
        "space.appearance.apply_preset" => {
            let page = find_arg(args, "page")
                .and_then(|v| v.parse::<usize>().ok())
                .ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: "missing or invalid integer `page`".to_string(),
                })?;
            let index = find_arg(args, "index")
                .and_then(|v| v.parse::<usize>().ok())
                .ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: "missing or invalid integer `index`".to_string(),
                })?;
            Command::SpaceAppearanceApplyPreset {
                space_id: parse_optional_space_id(args, name)?,
                page,
                index,
            }
        }
        "space.appearance.set_scheme" => {
            let raw = find_arg(args, "scheme").ok_or_else(|| CommandError::InvalidArgument {
                command: name.to_string(),
                message: "missing required `scheme` (auto|light|dark)".to_string(),
            })?;
            let scheme = SpaceColorScheme::parse(raw).ok_or_else(|| CommandError::InvalidArgument {
                command: name.to_string(),
                message: format!("invalid `scheme`: `{raw}`"),
            })?;
            Command::SpaceAppearanceSetScheme {
                space_id: parse_optional_space_id(args, name)?,
                scheme,
            }
        }
        "space.appearance.set_opacity" => {
            let opacity = find_arg(args, "opacity")
                .and_then(|v| v.parse::<f32>().ok())
                .ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: "missing or invalid float `opacity`".to_string(),
                })?;
            Command::SpaceAppearanceSetOpacity {
                space_id: parse_optional_space_id(args, name)?,
                opacity,
            }
        }
        "space.appearance.set_texture" => {
            let kind = find_arg(args, "kind")
                .and_then(TextureKind::parse)
                .unwrap_or(TextureKind::Grain);
            let amount = find_arg(args, "amount")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.0);
            Command::SpaceAppearanceSetTexture {
                space_id: parse_optional_space_id(args, name)?,
                kind,
                amount,
            }
        }
        "space.appearance.cycle_harmony" => Command::SpaceAppearanceCycleHarmony {
            space_id: parse_optional_space_id(args, name)?,
        },
        "space.appearance.reset" => Command::SpaceAppearanceReset {
            space_id: parse_optional_space_id(args, name)?,
        },
        _ => return Ok(None),
    };
    Ok(Some(cmd))
}

/// Command descriptors for space creation form, theme picker, and space appearance commands.
pub fn command_descriptors() -> Vec<CommandDescriptor> {
    vec![
        CommandDescriptor {
            name: "space.create_form.open".to_string(),
            title: "Create New Space".to_string(),
            description: "Open the sidebar space creation form".to_string(),
            arguments: vec![],
        },
        CommandDescriptor {
            name: "space.create_form.submit".to_string(),
            title: "Submit Space Creation Form".to_string(),
            description: "Create a new space from the creation form".to_string(),
            arguments: vec![
                ArgumentSchema::new("name", ArgumentType::String, true, "Space display name"),
                ArgumentSchema::new("icon", ArgumentType::String, false, "Optional icon name"),
                ArgumentSchema::new(
                    "profile",
                    ArgumentType::String,
                    false,
                    "Optional profile/container name",
                ),
            ],
        },
        CommandDescriptor {
            name: "space.create_form.cancel".to_string(),
            title: "Cancel Space Creation".to_string(),
            description: "Cancel and close the space creation form".to_string(),
            arguments: vec![],
        },
        CommandDescriptor {
            name: "theme.picker.open".to_string(),
            title: "Edit Space Theme".to_string(),
            description: "Open the gradient generator theme picker for a space".to_string(),
            arguments: vec![ArgumentSchema::new(
                "space_id",
                ArgumentType::SpaceId,
                false,
                "Target space identifier (defaults to active space)",
            )],
        },
        CommandDescriptor {
            name: "theme.picker.close".to_string(),
            title: "Close Space Theme Picker".to_string(),
            description: "Close the gradient generator theme picker".to_string(),
            arguments: vec![],
        },
        CommandDescriptor {
            name: "space.appearance.set".to_string(),
            title: "Set Space Appearance".to_string(),
            description: "Replace a space's gradient appearance configuration".to_string(),
            arguments: vec![ArgumentSchema::new(
                "space_id",
                ArgumentType::SpaceId,
                false,
                "Target space identifier (defaults to active space)",
            )],
        },
        CommandDescriptor {
            name: "space.appearance.apply_preset".to_string(),
            title: "Apply Space Theme Preset".to_string(),
            description: "Apply a predefined gradient swatch preset to a space".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "space_id",
                    ArgumentType::SpaceId,
                    false,
                    "Target space identifier (defaults to active space)",
                ),
                ArgumentSchema::new("page", ArgumentType::Integer, true, "Preset page index (0..7)"),
                ArgumentSchema::new(
                    "index",
                    ArgumentType::Integer,
                    true,
                    "Preset swatch index within the page (0..9)",
                ),
            ],
        },
        CommandDescriptor {
            name: "space.appearance.set_scheme".to_string(),
            title: "Set Space Color Scheme".to_string(),
            description: "Set auto, light, or dark appearance mode for a space".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "space_id",
                    ArgumentType::SpaceId,
                    false,
                    "Target space identifier (defaults to active space)",
                ),
                ArgumentSchema::new("scheme", ArgumentType::String, true, "Scheme: auto, light, or dark"),
            ],
        },
        CommandDescriptor {
            name: "space.appearance.set_opacity".to_string(),
            title: "Set Space Gradient Opacity".to_string(),
            description: "Set the gradient opacity intensity for a space".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "space_id",
                    ArgumentType::SpaceId,
                    false,
                    "Target space identifier (defaults to active space)",
                ),
                ArgumentSchema::new("opacity", ArgumentType::Float, true, "Opacity value in 0.25..=0.80"),
            ],
        },
        CommandDescriptor {
            name: "space.appearance.set_texture".to_string(),
            title: "Set Space Gradient Texture".to_string(),
            description: "Set the gradient grain texture style and intensity".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "space_id",
                    ArgumentType::SpaceId,
                    false,
                    "Target space identifier (defaults to active space)",
                ),
                ArgumentSchema::new(
                    "kind",
                    ArgumentType::String,
                    true,
                    "Texture style: none, grain, sand, denim, or tweed",
                ),
                ArgumentSchema::new("amount", ArgumentType::Float, true, "Texture intensity in 0.0..=1.0"),
            ],
        },
        CommandDescriptor {
            name: "space.appearance.cycle_harmony".to_string(),
            title: "Cycle Space Color Harmony".to_string(),
            description: "Cycle color harmony for a space's multi-dot gradient".to_string(),
            arguments: vec![ArgumentSchema::new(
                "space_id",
                ArgumentType::SpaceId,
                false,
                "Target space identifier (defaults to active space)",
            )],
        },
        CommandDescriptor {
            name: "space.appearance.reset".to_string(),
            title: "Reset Space Theme".to_string(),
            description: "Clear custom gradient dots and restore the default space theme".to_string(),
            arguments: vec![ArgumentSchema::new(
                "space_id",
                ArgumentType::SpaceId,
                false,
                "Target space identifier (defaults to active space)",
            )],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oklab_roundtrip_is_accurate_for_primaries_and_grays() {
        for rgb in [
            (0, 0, 0),
            (255, 255, 255),
            (128, 128, 128),
            (242, 94, 107),
            (52, 232, 149),
            (108, 186, 217),
        ] {
            let back = oklab_to_rgb(rgb_to_oklab(rgb));
            assert!((rgb.0 as i32 - back.0 as i32).abs() <= 1);
            assert!((rgb.1 as i32 - back.1 as i32).abs() <= 1);
            assert!((rgb.2 as i32 - back.2 as i32).abs() <= 1);
        }
    }

    #[test]
    fn clamp_dot_keeps_point_within_wheel_radius() {
        let (x, y) = clamp_dot_to_circle(500.0, 180.0, PAD_COORD_SIZE);
        assert!((x - PAD_COORD_SIZE).abs() < 0.01);
        assert!((y - 180.0).abs() < 0.01);

        let (ix, iy) = clamp_dot_to_circle(210.0, 140.0, PAD_COORD_SIZE);
        assert_eq!((ix, iy), (210.0, 140.0));
    }

    #[test]
    fn harmony_places_complementary_and_triadic_dots_at_expected_angles() {
        let mut app = SpaceAppearance::empty();
        app.add_dot(Some((270.0, 180.0)));
        assert_eq!(app.dots.len(), 1);
        assert_eq!(app.harmony, ColorHarmony::Floating);

        app.add_dot(None);
        assert_eq!(app.dots.len(), 2);
        assert_eq!(app.harmony, ColorHarmony::Complementary);
        // Primary is at (270, 180) -> +90 from center; complementary 180 deg away is (90, 180).
        assert!((app.dots[1].x - 90.0).abs() < 0.5);
        assert!((app.dots[1].y - 180.0).abs() < 0.5);

        app.add_dot(None);
        assert_eq!(app.dots.len(), 3);
        assert_eq!(app.harmony, ColorHarmony::SplitComplementary);

        app.cycle_harmony();
        assert_eq!(app.harmony, ColorHarmony::Analogous);
        app.cycle_harmony();
        assert_eq!(app.harmony, ColorHarmony::Triadic);
        assert_eq!(app.dots.len(), 3);

        app.remove_dot(None);
        assert_eq!(app.dots.len(), 2);
        app.remove_dot(None);
        assert_eq!(app.dots.len(), 1);
        app.remove_dot(None);
        assert!(app.dots.is_empty());
    }

    #[test]
    fn presets_cover_all_pages_and_produce_valid_gradients() {
        assert_eq!(GRADIENT_PRESET_PAGES.len(), 7);
        for (page_idx, page) in GRADIENT_PRESET_PAGES.iter().enumerate() {
            assert_eq!(page.presets.len(), 9);
            for swatch_idx in 0..page.presets.len() {
                let app = SpaceAppearance::from_preset(page_idx, swatch_idx).expect("preset should load");
                assert_eq!(app.dots.len(), page.presets[swatch_idx].num_dots);
                let resolved_dark = compute_gradient(&app, true);
                let resolved_light = compute_gradient(&app, false);
                assert!(resolved_dark.stops.len() >= 2);
                assert!(resolved_light.stops.len() >= 2);
                assert!(resolved_dark.dominant_rgb.is_some());
            }
        }
    }

    #[test]
    fn opacity_and_wave_interpolation_match_bounds() {
        assert!((layer_intensity(MIN_GRADIENT_OPACITY) - 0.0).abs() < 1e-5);
        assert!((layer_intensity(MAX_GRADIENT_OPACITY) - 1.0).abs() < 1e-5);
        assert!((layer_alpha(MIN_GRADIENT_OPACITY) - MIN_LAYER_ALPHA).abs() < 1e-5);
        assert!((layer_alpha(MAX_GRADIENT_OPACITY) - MAX_LAYER_ALPHA).abs() < 1e-5);

        let flat = wave_slider_points(0.0, 200.0, 40.0, 16);
        assert!(flat.iter().all(|(_, y)| (*y - 20.0).abs() < 1e-4));
        let wavy = wave_slider_points(1.0, 200.0, 40.0, 32);
        assert!(wavy.iter().any(|(_, y)| (*y - 20.0).abs() > 10.0));
    }

    #[test]
    fn texture_dial_quantizes_to_sixteen_steps() {
        assert_eq!(quantize_texture_amount(0.0), 0.0);
        assert_eq!(quantize_texture_amount(0.5), 0.5);
        assert_eq!(quantize_texture_amount(1.0), 0.0);
        assert!((texture_amount_from_dial_point(0.0, -50.0) - 0.0).abs() < 1e-5);
        assert!((texture_amount_from_dial_point(50.0, 0.0) - 0.25).abs() < 1e-5);
        assert!((texture_amount_from_dial_point(0.0, 50.0) - 0.5).abs() < 1e-5);
        assert!((texture_amount_from_dial_point(-50.0, 0.0) - 0.75).abs() < 1e-5);
    }

    #[test]
    fn contrast_decision_prefers_dark_text_on_bright_light_bg_and_light_on_dark() {
        assert!(should_be_dark_mode((24, 30, 52), 0.7, true));
        assert!(!should_be_dark_mode((248, 240, 232), 0.7, false));
    }

    #[test]
    fn stub_space_creation_and_appearance_commands_sync_model_via_events() {
        use crate::BrowserCore;
        use crate::events::{Event, ViewRequest};
        use crate::stub::StubBackend;

        let core = StubBackend::new();
        let sub = core.subscribe();
        let mut replica = core.model();

        core.invoke(Command::SpaceCreateFormOpen).unwrap();
        let mut saw_form_open = false;
        while let Ok(ev) = sub.try_recv() {
            replica.apply_event(&ev);
            if matches!(
                ev,
                Event::ViewRequested {
                    request: ViewRequest::SpaceCreateFormOpen,
                    ..
                }
            ) {
                saw_form_open = true;
            }
        }
        assert!(saw_form_open);

        let custom = SpaceAppearance::from_preset(1, 3).unwrap();
        core.invoke(Command::SpaceCreateFormSubmit {
            name: "Design".to_string(),
            icon: Some("sparkles".to_string()),
            profile: Some("Work".to_string()),
            appearance: Some(custom.clone()),
        })
        .unwrap();

        while let Ok(ev) = sub.try_recv() {
            replica.apply_event(&ev);
        }

        let created_id = {
            let created = replica
                .spaces
                .iter()
                .find(|s| s.name == "Design")
                .expect("created space should sync through events");
            assert_eq!(created.icon, "sparkles");
            assert_eq!(replica.active_space, Some(created.id));
            let synced_app = replica.space_appearance(created.id);
            assert_eq!(synced_app.profile, "Work");
            assert_eq!(synced_app.dots.len(), 3);
            created.id
        };

        core.invoke(Command::SpaceAppearanceSetOpacity {
            space_id: Some(created_id),
            opacity: 0.75,
        })
        .unwrap();
        core.invoke(Command::SpaceAppearanceSetTexture {
            space_id: Some(created_id),
            kind: TextureKind::Sand,
            amount: 0.5,
        })
        .unwrap();
        core.invoke(Command::SpaceAppearanceCycleHarmony {
            space_id: Some(created_id),
        })
        .unwrap();

        while let Ok(ev) = sub.try_recv() {
            replica.apply_event(&ev);
        }
        let updated = replica.space_appearance(created_id);
        assert!((updated.opacity - 0.75).abs() < 1e-4);
        assert_eq!(updated.texture_kind, TextureKind::Sand);
        assert!((updated.texture_amount - 0.5).abs() < 1e-4);
        assert_eq!(updated.harmony, ColorHarmony::Triadic);
    }
}
