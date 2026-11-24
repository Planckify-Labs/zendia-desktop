/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Pure-data theme palette, semantic roles, glass, radii, spacing, typography, shadows, and override resolution.
//!
//! All data and resolution logic in this module is independent of any window or GPU context so it can be unit tested
//! directly.

use freya::prelude::{Color, Shadow};
use std::collections::{BTreeMap, HashSet};
use zendia_browser_core::model::{ThemeColorOverride, ThemeRoleOverride, ThemeScheme, ThemeSettings};
use zendia_module_api::ui::{UiRole, UiTone};

/// Straight-alpha sRGB color represented as plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_rgb_alpha(r: u8, g: u8, b: u8, alpha: f32) -> Self {
        let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        Self { r, g, b, a }
    }

    /// Parses a `#rrggbb` hex string (case-insensitive).
    pub fn from_hex(hex: &str) -> Option<Self> {
        let digits = hex.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let r = u8::from_str_radix(&digits[0..2], 16).ok()?;
        let g = u8::from_str_radix(&digits[2..4], 16).ok()?;
        let b = u8::from_str_radix(&digits[4..6], 16).ok()?;
        Some(Self::rgb(r, g, b))
    }

    /// Returns a copy with alpha scaled to `alpha` (`0.0..=1.0`).
    pub fn with_alpha(self, alpha: f32) -> Self {
        let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        Self { a, ..self }
    }

    /// Linearly blends `self` toward `other` by `weight` (`0.0` = `self`, `1.0` = `other`).
    pub fn mix(self, other: Self, weight: f32) -> Self {
        let t = weight.clamp(0.0, 1.0);
        let inv = 1.0 - t;
        Self {
            r: (self.r as f32 * inv + other.r as f32 * t).round() as u8,
            g: (self.g as f32 * inv + other.g as f32 * t).round() as u8,
            b: (self.b as f32 * inv + other.b as f32 * t).round() as u8,
            a: (self.a as f32 * inv + other.a as f32 * t).round() as u8,
        }
    }

    /// Converts to Freya's [`Color`].
    pub const fn to_color(self) -> Color {
        Color::from_argb(self.a, self.r, self.g, self.b)
    }
}

impl From<Rgba> for Color {
    fn from(value: Rgba) -> Self {
        value.to_color()
    }
}

/// Concrete light or dark color scheme after resolving [`ThemeScheme::System`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResolvedScheme {
    Light,
    Dark,
}

impl ResolvedScheme {
    pub fn resolve(scheme: ThemeScheme, system_is_dark: bool) -> Self {
        match scheme {
            ThemeScheme::Light => Self::Light,
            ThemeScheme::Dark => Self::Dark,
            ThemeScheme::System => {
                if system_is_dark {
                    Self::Dark
                } else {
                    Self::Light
                }
            }
        }
    }

    pub fn is_dark(self) -> bool {
        self == Self::Dark
    }
}

/// Light and dark color pair for a single token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorPair {
    pub light: Rgba,
    pub dark: Rgba,
}

impl ColorPair {
    pub const fn new(light: Rgba, dark: Rgba) -> Self {
        Self { light, dark }
    }

    pub const fn uniform(color: Rgba) -> Self {
        Self {
            light: color,
            dark: color,
        }
    }

    pub const fn pick(self, scheme: ResolvedScheme) -> Rgba {
        match scheme {
            ResolvedScheme::Light => self.light,
            ResolvedScheme::Dark => self.dark,
        }
    }
}

/// Tokyonight palette resolved for a single scheme (Moon on dark, contrast-adjusted Day on light).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub base: Rgba,
    pub raised: Rgba,
    pub border: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub ink: Rgba,
    pub night: Rgba,
    pub snow: Rgba,
    pub pink: Rgba,
    pub red: Rgba,
    pub orange: Rgba,
    pub yellow: Rgba,
    pub green: Rgba,
    pub teal: Rgba,
    pub cyan: Rgba,
    pub blue: Rgba,
    pub purple: Rgba,
}

/// Default Tokyonight palette pairs.
pub fn default_palette_pairs() -> BTreeMap<String, ColorPair> {
    let entries: &[(&str, ColorPair)] = &[
        (
            "base",
            ColorPair::new(Rgba::rgb(0xf5, 0xf6, 0xfa), Rgba::rgb(0x1e, 0x20, 0x30)),
        ),
        (
            "raised",
            ColorPair::new(Rgba::rgb(0xe6, 0xe8, 0xf0), Rgba::rgb(0x2a, 0x2d, 0x45)),
        ),
        (
            "border",
            ColorPair::new(Rgba::rgb(0xc4, 0xc8, 0xda), Rgba::rgb(0x3b, 0x42, 0x61)),
        ),
        (
            "text",
            ColorPair::new(Rgba::rgb(0x1f, 0x23, 0x35), Rgba::rgb(0xc8, 0xd3, 0xf5)),
        ),
        (
            "muted",
            ColorPair::new(Rgba::rgb(0x54, 0x5c, 0x8a), Rgba::rgb(0x8f, 0x98, 0xc4)),
        ),
        ("ink", ColorPair::uniform(Rgba::rgb(0x1b, 0x1d, 0x2b))),
        ("night", ColorPair::uniform(Rgba::rgb(0x1a, 0x1b, 0x26))),
        ("snow", ColorPair::uniform(Rgba::rgb(0xc0, 0xca, 0xf5))),
        (
            "pink",
            ColorPair::new(Rgba::rgb(0xb0, 0x30, 0x7f), Rgba::rgb(0xfc, 0xa7, 0xea)),
        ),
        (
            "red",
            ColorPair::new(Rgba::rgb(0xc8, 0x17, 0x4c), Rgba::rgb(0xff, 0x75, 0x7f)),
        ),
        (
            "orange",
            ColorPair::new(Rgba::rgb(0x9c, 0x4f, 0x00), Rgba::rgb(0xff, 0x96, 0x6c)),
        ),
        (
            "yellow",
            ColorPair::new(Rgba::rgb(0x7a, 0x5c, 0x14), Rgba::rgb(0xff, 0xc7, 0x77)),
        ),
        (
            "green",
            ColorPair::new(Rgba::rgb(0x47, 0x66, 0x28), Rgba::rgb(0xc3, 0xe8, 0x8d)),
        ),
        (
            "teal",
            ColorPair::new(Rgba::rgb(0x0b, 0x74, 0x61), Rgba::rgb(0x4f, 0xd6, 0xbe)),
        ),
        (
            "cyan",
            ColorPair::new(Rgba::rgb(0x00, 0x66, 0x8a), Rgba::rgb(0x86, 0xe1, 0xfc)),
        ),
        (
            "blue",
            ColorPair::new(Rgba::rgb(0x21, 0x60, 0xc4), Rgba::rgb(0x82, 0xaa, 0xff)),
        ),
        (
            "purple",
            ColorPair::new(Rgba::rgb(0x73, 0x40, 0xcc), Rgba::rgb(0xc0, 0x99, 0xff)),
        ),
    ];
    entries.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
}

/// Default role specifications, matching Zendia's semantic mapping onto Tokyonight.
fn default_role_specs() -> BTreeMap<String, ThemeRoleOverride> {
    let entries: &[(&str, &str)] = &[
        ("accent", "pink"),
        ("mode-n", "pink"),
        ("mode-i", "green"),
        ("mode-v", "purple"),
        ("mode-e", "blue"),
        ("mode-t", "yellow"),
        ("mode-l", "teal"),
        ("mode-w", "orange"),
        ("statusline-background", "night"),
        ("statusline-text", "snow"),
        ("hint", "#a25772"),
        ("hint-text", "#eef5ff"),
        ("which-key-key", "orange"),
        ("which-key-group", "purple"),
        ("success", "green"),
        ("warning", "yellow"),
        ("error", "red"),
        ("danger", "error"),
        ("info", "blue"),
        ("scrollbar", "accent"),
        ("md-h1", "blue"),
        ("md-h2", "yellow"),
        ("md-h3", "green"),
        ("md-h4", "teal"),
        ("md-h5", "purple"),
        ("md-h6", "pink"),
        ("md-strong", "orange"),
        ("md-em", "text"),
        ("md-link", "blue"),
        ("md-code", "teal"),
        ("md-code-background", "raised"),
        ("md-quote", "purple"),
        ("md-bullet", "pink"),
        ("md-rule", "border"),
        ("md-table-head", "blue"),
        ("agent-surface", "base"),
        ("agent-text", "text"),
        ("agent-muted", "muted"),
        ("agent-border", "border"),
        ("agent-user", "accent"),
        ("agent-confirm", "yellow"),
        ("effort-none", "muted"),
        ("effort-minimal", "teal"),
        ("effort-low", "cyan"),
        ("effort-medium", "blue"),
        ("effort-high", "purple"),
        ("effort-xhigh", "pink"),
        ("effort-max", "orange"),
        ("effort-ultra", "red"),
    ];
    entries
        .iter()
        .map(|(k, v)| ((*k).to_string(), ThemeRoleOverride::Single((*v).to_string())))
        .collect()
}

/// Default per-agent brand colors.
pub fn default_agent_colors() -> BTreeMap<String, Rgba> {
    let entries: &[(&str, Rgba)] = &[
        ("claude", Rgba::rgb(0xd9, 0x77, 0x57)),
        ("codex", Rgba::rgb(0x10, 0xa3, 0x7f)),
        ("cursor", Rgba::rgb(0xa3, 0xa3, 0xa3)),
        ("antigravity", Rgba::rgb(0x42, 0x85, 0xf4)),
        ("opencode", Rgba::rgb(0xf5, 0xa7, 0x42)),
        ("kiro", Rgba::rgb(0x90, 0x46, 0xff)),
    ];
    entries.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
}

/// Semantic color roles passed into UI components instead of raw colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ThemeRole {
    Surface,
    SurfaceRaised,
    SurfaceGlass,
    SurfaceModelGlass,
    #[default]
    Text,
    TextMuted,
    TextOnAccent,
    Ink,
    Accent,
    Border,
    BorderSubtle,
    FocusRing,
    Selection,
    Hover,
    Danger,
    Warning,
    Success,
    Info,
    StatuslineBackground,
    StatuslineText,
    Hint,
    HintText,
    WhichKeyKey,
    WhichKeyGroup,
    Scrollbar,
    ModeNormal,
    ModeInsert,
    ModeVisual,
    ModeExplorer,
    ModeToolbar,
    ModeLibrary,
    ModeWindow,
    MdH1,
    MdH2,
    MdH3,
    MdH4,
    MdH5,
    MdH6,
    MdStrong,
    MdEm,
    MdLink,
    MdCode,
    MdCodeBackground,
    MdQuote,
    MdBullet,
    MdRule,
    MdTableHead,
    AgentConfirm,
    EffortNone,
    EffortMinimal,
    EffortLow,
    EffortMedium,
    EffortHigh,
    EffortXHigh,
    EffortMax,
    EffortUltra,
}

impl From<UiRole> for ThemeRole {
    fn from(role: UiRole) -> Self {
        match role {
            UiRole::Surface => Self::Surface,
            UiRole::SurfaceRaised => Self::SurfaceRaised,
            UiRole::Text => Self::Text,
            UiRole::TextMuted => Self::TextMuted,
            UiRole::Accent => Self::Accent,
            UiRole::Border => Self::Border,
            UiRole::Danger => Self::Danger,
            UiRole::Warning => Self::Warning,
            UiRole::Success => Self::Success,
            UiRole::Selection => Self::Selection,
            UiRole::Hint => Self::Hint,
            UiRole::WhichKeyKey => Self::WhichKeyKey,
            UiRole::WhichKeyGroup => Self::WhichKeyGroup,
        }
    }
}

impl ThemeRole {
    /// Maps a [`UiTone`] to its foreground accent role.
    pub fn from_tone(tone: UiTone) -> Self {
        match tone {
            UiTone::Neutral => Self::Text,
            UiTone::Accent => Self::Accent,
            UiTone::Success => Self::Success,
            UiTone::Warning => Self::Warning,
            UiTone::Danger => Self::Danger,
        }
    }
}

/// Resolved semantic colors for all surfaces and states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticColors {
    pub surface: Rgba,
    pub surface_raised: Rgba,
    pub surface_glass: Rgba,
    pub surface_model_glass: Rgba,
    pub text: Rgba,
    pub text_muted: Rgba,
    pub text_on_accent: Rgba,
    pub ink: Rgba,
    pub accent: Rgba,
    pub border: Rgba,
    pub border_subtle: Rgba,
    pub focus_ring: Rgba,
    pub selection: Rgba,
    pub hover: Rgba,
    pub danger: Rgba,
    pub warning: Rgba,
    pub success: Rgba,
    pub info: Rgba,
    pub statusline_background: Rgba,
    pub statusline_text: Rgba,
    pub hint: Rgba,
    pub hint_text: Rgba,
    pub which_key_key: Rgba,
    pub which_key_group: Rgba,
    pub scrollbar: Rgba,
    pub mode_n: Rgba,
    pub mode_i: Rgba,
    pub mode_v: Rgba,
    pub mode_e: Rgba,
    pub mode_t: Rgba,
    pub mode_l: Rgba,
    pub mode_w: Rgba,
    pub md_h1: Rgba,
    pub md_h2: Rgba,
    pub md_h3: Rgba,
    pub md_h4: Rgba,
    pub md_h5: Rgba,
    pub md_h6: Rgba,
    pub md_strong: Rgba,
    pub md_em: Rgba,
    pub md_link: Rgba,
    pub md_code: Rgba,
    pub md_code_background: Rgba,
    pub md_quote: Rgba,
    pub md_bullet: Rgba,
    pub md_rule: Rgba,
    pub md_table_head: Rgba,
    pub agent_confirm: Rgba,
    pub effort_none: Rgba,
    pub effort_minimal: Rgba,
    pub effort_low: Rgba,
    pub effort_medium: Rgba,
    pub effort_high: Rgba,
    pub effort_xhigh: Rgba,
    pub effort_max: Rgba,
    pub effort_ultra: Rgba,
}

/// Glass material configuration for panels, floating surfaces, and the model card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassSpec {
    /// Agent/dock glass surface opacity (`0.0..=1.0`, default `0.78`).
    pub opacity: f32,
    /// Agent/dock backdrop blur radius in pixels (default `40.0`).
    pub blur: f32,
    /// Translucent tint used by floating overlays (picker, which-key, floating address bar).
    pub float_tint: Rgba,
    /// Solid fallback color when glass transparency is disabled.
    pub float_solid: Rgba,
    /// Blur radius for floating overlays (`40.0`).
    pub float_blur: f32,
    /// Thinner tint used by surfaces floating over an already-even panel (e.g. model card).
    pub model_tint: Rgba,
    /// Blur radius for secondary overlays (`11.0`).
    pub model_blur: f32,
    /// Top specular sheen color.
    pub sheen_top: Rgba,
    /// Hairline outline separating glass from the page behind it.
    pub outline: Rgba,
    /// Dimmed text opacity over glass (`0.78`).
    pub muted_opacity: f32,
}

/// Corner radius scale in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radii {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub card: f32,
    pub dock: f32,
    pub float: f32,
    pub pill: f32,
}

impl Default for Radii {
    fn default() -> Self {
        Self {
            xs: 4.0,
            sm: 6.0,
            md: 8.0,
            lg: 11.0,
            card: 14.0,
            dock: 16.0,
            float: 18.0,
            pill: 999.0,
        }
    }
}

/// Spacing scale in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    pub xxs: f32,
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xxl: f32,
    pub panel_inset: f32,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            xxs: 2.0,
            xs: 4.0,
            sm: 6.0,
            md: 8.0,
            lg: 10.0,
            xl: 12.0,
            xxl: 16.0,
            panel_inset: 10.0,
        }
    }
}

/// Typography size scale in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontSizes {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub title: f32,
}

impl Default for FontSizes {
    fn default() -> Self {
        Self {
            xs: 11.0,
            sm: 12.0,
            md: 13.0,
            lg: 14.0,
            xl: 15.0,
            title: 18.0,
        }
    }
}

/// Pure-data description of a box shadow convertible to Freya's [`Shadow`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSpec {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Rgba,
    pub inset: bool,
}

impl ShadowSpec {
    pub fn to_shadow(self) -> Shadow {
        let shadow = Shadow::new()
            .x(self.x)
            .y(self.y)
            .blur(self.blur)
            .spread(self.spread)
            .color(self.color.to_color());
        if self.inset { shadow.inset() } else { shadow.normal() }
    }
}

/// Resolved shadow specs matching Zendia's floating glass, cards, pills, and confirmations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadows {
    pub float_highlight: ShadowSpec,
    pub float_ambient: ShadowSpec,
    pub float_drop: ShadowSpec,
    pub card: ShadowSpec,
    pub pill: ShadowSpec,
}

/// Complete theme snapshot resolved for either light or dark mode with user overrides applied.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedTheme {
    pub scheme: ResolvedScheme,
    pub palette: Palette,
    pub colors: SemanticColors,
    pub glass: GlassSpec,
    pub radii: Radii,
    pub spacing: Spacing,
    pub font_sizes: FontSizes,
    pub shadows: Shadows,
    pub agents: BTreeMap<String, Rgba>,
}

impl Default for ResolvedTheme {
    fn default() -> Self {
        Self::resolve(&ThemeSettings::default(), true)
    }
}

impl ResolvedTheme {
    /// Resolves `settings` against the current system dark-mode preference.
    pub fn resolve(settings: &ThemeSettings, system_is_dark: bool) -> Self {
        let scheme = ResolvedScheme::resolve(settings.scheme, system_is_dark);

        let mut palette_pairs = default_palette_pairs();
        for (name, override_val) in &settings.overrides.palette {
            if let Some(pair) = parse_color_override(override_val) {
                palette_pairs.insert(name.clone(), pair);
            }
        }

        let default_roles = default_role_specs();
        let mut role_specs = default_roles.clone();
        for (name, spec) in &settings.overrides.roles {
            role_specs.insert(name.clone(), spec.clone());
        }

        let mut resolved_roles = BTreeMap::new();
        for (name, spec) in &role_specs {
            let pair = resolve_role_spec(spec, &palette_pairs, &role_specs, &mut HashSet::new()).or_else(|| {
                default_roles.get(name).and_then(|fallback| {
                    resolve_role_spec(fallback, &palette_pairs, &default_roles, &mut HashSet::new())
                })
            });
            if let Some(pair) = pair {
                resolved_roles.insert(name.clone(), pair);
            }
        }

        let pick_palette = |name: &str| palette_pairs[name].pick(scheme);
        let pick_role = |name: &str, fallback_palette: &str| {
            resolved_roles
                .get(name)
                .copied()
                .unwrap_or_else(|| palette_pairs[fallback_palette])
                .pick(scheme)
        };

        let palette = Palette {
            base: pick_palette("base"),
            raised: pick_palette("raised"),
            border: pick_palette("border"),
            text: pick_palette("text"),
            muted: pick_palette("muted"),
            ink: pick_palette("ink"),
            night: pick_palette("night"),
            snow: pick_palette("snow"),
            pink: pick_palette("pink"),
            red: pick_palette("red"),
            orange: pick_palette("orange"),
            yellow: pick_palette("yellow"),
            green: pick_palette("green"),
            teal: pick_palette("teal"),
            cyan: pick_palette("cyan"),
            blue: pick_palette("blue"),
            purple: pick_palette("purple"),
        };

        let glass_opacity = settings
            .overrides
            .glass
            .opacity
            .filter(|v| !v.is_nan() && (0.0..=1.0).contains(v))
            .unwrap_or(0.78);
        let glass_blur = settings
            .overrides
            .glass
            .blur
            .filter(|v| !v.is_nan() && *v >= 0.0)
            .unwrap_or(40.0);

        let surface = pick_role("agent-surface", "base");
        let text = pick_role("agent-text", "text");
        let text_muted = pick_role("agent-muted", "muted");
        let border = pick_role("agent-border", "border");
        let accent = pick_role("accent", "pink");

        let glass = match scheme {
            ResolvedScheme::Light => GlassSpec {
                opacity: glass_opacity,
                blur: glass_blur,
                float_tint: Rgba::from_rgb_alpha(255, 255, 255, 0.55),
                float_solid: Rgba::rgb(255, 255, 255),
                float_blur: glass_blur,
                model_tint: Rgba::from_rgb_alpha(255, 255, 255, 0.36),
                model_blur: 11.0,
                sheen_top: Rgba::from_rgb_alpha(255, 255, 255, 0.24),
                outline: Rgba::from_rgb_alpha(0, 0, 0, 0.12),
                muted_opacity: 0.78,
            },
            ResolvedScheme::Dark => GlassSpec {
                opacity: glass_opacity,
                blur: glass_blur,
                float_tint: Rgba::from_rgb_alpha(22, 22, 26, 0.62),
                float_solid: Rgba::rgb(24, 24, 24),
                float_blur: glass_blur,
                model_tint: Rgba::from_rgb_alpha(22, 22, 26, 0.34),
                model_blur: 11.0,
                sheen_top: Rgba::from_rgb_alpha(255, 255, 255, 0.06),
                outline: Rgba::from_rgb_alpha(255, 255, 255, 0.12),
                muted_opacity: 0.78,
            },
        };

        let text_on_accent = match scheme {
            ResolvedScheme::Light => Rgba::rgb(255, 255, 255),
            ResolvedScheme::Dark => palette.ink,
        };
        let selection = accent.with_alpha(if scheme.is_dark() { 0.24 } else { 0.18 });
        let hover = match scheme {
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.06),
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.08),
        };
        let border_subtle = border.with_alpha(if scheme.is_dark() { 0.60 } else { 0.70 });

        let colors = SemanticColors {
            surface,
            surface_raised: palette.raised,
            surface_glass: surface.with_alpha(glass.opacity),
            surface_model_glass: glass.model_tint,
            text,
            text_muted,
            text_on_accent,
            ink: palette.ink,
            accent,
            border,
            border_subtle,
            focus_ring: accent,
            selection,
            hover,
            danger: pick_role("danger", "red"),
            warning: pick_role("warning", "yellow"),
            success: pick_role("success", "green"),
            info: pick_role("info", "blue"),
            statusline_background: pick_role("statusline-background", "night"),
            statusline_text: pick_role("statusline-text", "snow"),
            hint: pick_role("hint", "pink"),
            hint_text: pick_role("hint-text", "snow"),
            which_key_key: pick_role("which-key-key", "orange"),
            which_key_group: pick_role("which-key-group", "purple"),
            scrollbar: pick_role("scrollbar", "pink"),
            mode_n: pick_role("mode-n", "pink"),
            mode_i: pick_role("mode-i", "green"),
            mode_v: pick_role("mode-v", "purple"),
            mode_e: pick_role("mode-e", "blue"),
            mode_t: pick_role("mode-t", "yellow"),
            mode_l: pick_role("mode-l", "teal"),
            mode_w: pick_role("mode-w", "orange"),
            md_h1: pick_role("md-h1", "blue"),
            md_h2: pick_role("md-h2", "yellow"),
            md_h3: pick_role("md-h3", "green"),
            md_h4: pick_role("md-h4", "teal"),
            md_h5: pick_role("md-h5", "purple"),
            md_h6: pick_role("md-h6", "pink"),
            md_strong: pick_role("md-strong", "orange"),
            md_em: pick_role("md-em", "text"),
            md_link: pick_role("md-link", "blue"),
            md_code: pick_role("md-code", "teal"),
            md_code_background: pick_role("md-code-background", "raised"),
            md_quote: pick_role("md-quote", "purple"),
            md_bullet: pick_role("md-bullet", "pink"),
            md_rule: pick_role("md-rule", "border"),
            md_table_head: pick_role("md-table-head", "blue"),
            agent_confirm: pick_role("agent-confirm", "yellow"),
            effort_none: pick_role("effort-none", "muted"),
            effort_minimal: pick_role("effort-minimal", "teal"),
            effort_low: pick_role("effort-low", "cyan"),
            effort_medium: pick_role("effort-medium", "blue"),
            effort_high: pick_role("effort-high", "purple"),
            effort_xhigh: pick_role("effort-xhigh", "pink"),
            effort_max: pick_role("effort-max", "orange"),
            effort_ultra: pick_role("effort-ultra", "red"),
        };

        let shadows = match scheme {
            ResolvedScheme::Light => Shadows {
                float_highlight: ShadowSpec {
                    x: 0.0,
                    y: 1.0,
                    blur: 0.0,
                    spread: 0.0,
                    color: Rgba::from_rgb_alpha(255, 255, 255, 0.70),
                    inset: true,
                },
                float_ambient: ShadowSpec {
                    x: 0.0,
                    y: 2.0,
                    blur: 6.0,
                    spread: 0.0,
                    color: Rgba::from_rgb_alpha(20, 22, 40, 0.10),
                    inset: false,
                },
                float_drop: ShadowSpec {
                    x: 0.0,
                    y: 24.0,
                    blur: 64.0,
                    spread: -16.0,
                    color: Rgba::from_rgb_alpha(20, 22, 40, 0.38),
                    inset: false,
                },
                card: ShadowSpec {
                    x: 0.0,
                    y: 3.0,
                    blur: 10.0,
                    spread: 0.0,
                    color: Rgba::from_rgb_alpha(20, 22, 40, 0.14),
                    inset: false,
                },
                pill: ShadowSpec {
                    x: 0.0,
                    y: 2.0,
                    blur: 8.0,
                    spread: 0.0,
                    color: Rgba::from_rgb_alpha(20, 22, 40, 0.16),
                    inset: false,
                },
            },
            ResolvedScheme::Dark => Shadows {
                float_highlight: ShadowSpec {
                    x: 0.0,
                    y: 1.0,
                    blur: 0.0,
                    spread: 0.0,
                    color: Rgba::from_rgb_alpha(255, 255, 255, 0.14),
                    inset: true,
                },
                float_ambient: ShadowSpec {
                    x: 0.0,
                    y: 2.0,
                    blur: 6.0,
                    spread: 0.0,
                    color: Rgba::from_rgb_alpha(0, 0, 0, 0.30),
                    inset: false,
                },
                float_drop: ShadowSpec {
                    x: 0.0,
                    y: 24.0,
                    blur: 64.0,
                    spread: -16.0,
                    color: Rgba::from_rgb_alpha(0, 0, 0, 0.60),
                    inset: false,
                },
                card: ShadowSpec {
                    x: 0.0,
                    y: 3.0,
                    blur: 10.0,
                    spread: 0.0,
                    color: Rgba::from_rgb_alpha(0, 0, 0, 0.32),
                    inset: false,
                },
                pill: ShadowSpec {
                    x: 0.0,
                    y: 2.0,
                    blur: 10.0,
                    spread: 0.0,
                    color: Rgba::from_rgb_alpha(0, 0, 0, 0.40),
                    inset: false,
                },
            },
        };

        let mut agents = default_agent_colors();
        for (id, hex) in &settings.overrides.agents {
            if let Some(color) = Rgba::from_hex(hex) {
                agents.insert(id.clone(), color);
            }
        }

        Self {
            scheme,
            palette,
            colors,
            glass,
            radii: Radii::default(),
            spacing: Spacing::default(),
            font_sizes: FontSizes::default(),
            shadows,
            agents,
        }
    }

    /// Resolves a semantic role to an [`Rgba`] color.
    pub const fn rgba(&self, role: ThemeRole) -> Rgba {
        match role {
            ThemeRole::Surface => self.colors.surface,
            ThemeRole::SurfaceRaised => self.colors.surface_raised,
            ThemeRole::SurfaceGlass => self.colors.surface_glass,
            ThemeRole::SurfaceModelGlass => self.colors.surface_model_glass,
            ThemeRole::Text => self.colors.text,
            ThemeRole::TextMuted => self.colors.text_muted,
            ThemeRole::TextOnAccent => self.colors.text_on_accent,
            ThemeRole::Ink => self.colors.ink,
            ThemeRole::Accent => self.colors.accent,
            ThemeRole::Border => self.colors.border,
            ThemeRole::BorderSubtle => self.colors.border_subtle,
            ThemeRole::FocusRing => self.colors.focus_ring,
            ThemeRole::Selection => self.colors.selection,
            ThemeRole::Hover => self.colors.hover,
            ThemeRole::Danger => self.colors.danger,
            ThemeRole::Warning => self.colors.warning,
            ThemeRole::Success => self.colors.success,
            ThemeRole::Info => self.colors.info,
            ThemeRole::StatuslineBackground => self.colors.statusline_background,
            ThemeRole::StatuslineText => self.colors.statusline_text,
            ThemeRole::Hint => self.colors.hint,
            ThemeRole::HintText => self.colors.hint_text,
            ThemeRole::WhichKeyKey => self.colors.which_key_key,
            ThemeRole::WhichKeyGroup => self.colors.which_key_group,
            ThemeRole::Scrollbar => self.colors.scrollbar,
            ThemeRole::ModeNormal => self.colors.mode_n,
            ThemeRole::ModeInsert => self.colors.mode_i,
            ThemeRole::ModeVisual => self.colors.mode_v,
            ThemeRole::ModeExplorer => self.colors.mode_e,
            ThemeRole::ModeToolbar => self.colors.mode_t,
            ThemeRole::ModeLibrary => self.colors.mode_l,
            ThemeRole::ModeWindow => self.colors.mode_w,
            ThemeRole::MdH1 => self.colors.md_h1,
            ThemeRole::MdH2 => self.colors.md_h2,
            ThemeRole::MdH3 => self.colors.md_h3,
            ThemeRole::MdH4 => self.colors.md_h4,
            ThemeRole::MdH5 => self.colors.md_h5,
            ThemeRole::MdH6 => self.colors.md_h6,
            ThemeRole::MdStrong => self.colors.md_strong,
            ThemeRole::MdEm => self.colors.md_em,
            ThemeRole::MdLink => self.colors.md_link,
            ThemeRole::MdCode => self.colors.md_code,
            ThemeRole::MdCodeBackground => self.colors.md_code_background,
            ThemeRole::MdQuote => self.colors.md_quote,
            ThemeRole::MdBullet => self.colors.md_bullet,
            ThemeRole::MdRule => self.colors.md_rule,
            ThemeRole::MdTableHead => self.colors.md_table_head,
            ThemeRole::AgentConfirm => self.colors.agent_confirm,
            ThemeRole::EffortNone => self.colors.effort_none,
            ThemeRole::EffortMinimal => self.colors.effort_minimal,
            ThemeRole::EffortLow => self.colors.effort_low,
            ThemeRole::EffortMedium => self.colors.effort_medium,
            ThemeRole::EffortHigh => self.colors.effort_high,
            ThemeRole::EffortXHigh => self.colors.effort_xhigh,
            ThemeRole::EffortMax => self.colors.effort_max,
            ThemeRole::EffortUltra => self.colors.effort_ultra,
        }
    }

    /// Resolves a semantic role directly to a Freya [`Color`].
    pub const fn color(&self, role: ThemeRole) -> Color {
        self.rgba(role).to_color()
    }

    /// Returns an agent's configured brand color, falling back to Tokyonight blue.
    pub fn agent_rgba(&self, agent_id: &str) -> Rgba {
        self.agents.get(agent_id).copied().unwrap_or(self.palette.blue)
    }

    /// Returns an agent's configured brand color as a Freya [`Color`].
    pub fn agent_color(&self, agent_id: &str) -> Color {
        self.agent_rgba(agent_id).to_color()
    }
}

fn parse_color_override(value: &ThemeColorOverride) -> Option<ColorPair> {
    match value {
        ThemeColorOverride::Single(hex) => Rgba::from_hex(hex).map(ColorPair::uniform),
        ThemeColorOverride::Pair { light, dark } => Some(ColorPair::new(Rgba::from_hex(light)?, Rgba::from_hex(dark)?)),
    }
}

fn resolve_single_token(
    token: &str,
    palette: &BTreeMap<String, ColorPair>,
    roles: &BTreeMap<String, ThemeRoleOverride>,
    seen: &mut HashSet<String>,
) -> Option<ColorPair> {
    if let Some(pair) = palette.get(token) {
        return Some(*pair);
    }
    if let Some(spec) = roles.get(token)
        && seen.insert(token.to_string())
    {
        let resolved = resolve_role_spec(spec, palette, roles, seen);
        seen.remove(token);
        return resolved;
    }
    Rgba::from_hex(token).map(ColorPair::uniform)
}

fn resolve_role_spec(
    spec: &ThemeRoleOverride,
    palette: &BTreeMap<String, ColorPair>,
    roles: &BTreeMap<String, ThemeRoleOverride>,
    seen: &mut HashSet<String>,
) -> Option<ColorPair> {
    match spec {
        ThemeRoleOverride::Single(token) => resolve_single_token(token, palette, roles, seen),
        ThemeRoleOverride::Pair { light, dark } => {
            let light_pair = resolve_single_token(light, palette, roles, seen)?;
            let dark_pair = resolve_single_token(dark, palette, roles, seen)?;
            Some(ColorPair::new(light_pair.light, dark_pair.dark))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zendia_browser_core::model::{ThemeGlassOverrides, ThemeOverrides};

    #[test]
    fn hex_parsing_handles_valid_and_invalid_inputs() {
        assert_eq!(Rgba::from_hex("#fca7ea"), Some(Rgba::rgb(0xfc, 0xa7, 0xea)));
        assert_eq!(Rgba::from_hex("#1E2030"), Some(Rgba::rgb(0x1e, 0x20, 0x30)));
        assert_eq!(Rgba::from_hex("fca7ea"), None);
        assert_eq!(Rgba::from_hex("#fff"), None);
        assert_eq!(Rgba::from_hex("#gggggg"), None);
    }

    #[test]
    fn resolves_light_dark_and_system_schemes() {
        let light = ResolvedTheme::resolve(
            &ThemeSettings {
                scheme: ThemeScheme::Light,
                overrides: ThemeOverrides::default(),
            },
            true,
        );
        assert_eq!(light.scheme, ResolvedScheme::Light);
        assert_eq!(light.rgba(ThemeRole::Surface), Rgba::rgb(0xf5, 0xf6, 0xfa));
        assert_eq!(light.rgba(ThemeRole::Accent), Rgba::rgb(0xb0, 0x30, 0x7f));
        assert_eq!(light.rgba(ThemeRole::TextOnAccent), Rgba::rgb(255, 255, 255));

        let dark = ResolvedTheme::resolve(
            &ThemeSettings {
                scheme: ThemeScheme::Dark,
                overrides: ThemeOverrides::default(),
            },
            false,
        );
        assert_eq!(dark.scheme, ResolvedScheme::Dark);
        assert_eq!(dark.rgba(ThemeRole::Surface), Rgba::rgb(0x1e, 0x20, 0x30));
        assert_eq!(dark.rgba(ThemeRole::Accent), Rgba::rgb(0xfc, 0xa7, 0xea));
        assert_eq!(dark.rgba(ThemeRole::TextOnAccent), Rgba::rgb(0x1b, 0x1d, 0x2b));

        let system_dark = ResolvedTheme::resolve(&ThemeSettings::default(), true);
        let system_light = ResolvedTheme::resolve(&ThemeSettings::default(), false);
        assert_eq!(system_dark.scheme, ResolvedScheme::Dark);
        assert_eq!(system_light.scheme, ResolvedScheme::Light);
    }

    #[test]
    fn palette_and_chained_role_overrides_propagate() {
        let mut overrides = ThemeOverrides::default();
        overrides.palette.insert(
            "pink".to_string(),
            ThemeColorOverride::Pair {
                light: "#aa1155".to_string(),
                dark: "#ff79c6".to_string(),
            },
        );
        // `scrollbar` defaults to `accent` which defaults to `pink`.
        let dark = ResolvedTheme::resolve(
            &ThemeSettings {
                scheme: ThemeScheme::Dark,
                overrides: overrides.clone(),
            },
            true,
        );
        assert_eq!(dark.rgba(ThemeRole::Accent), Rgba::rgb(0xff, 0x79, 0xc6));
        assert_eq!(dark.rgba(ThemeRole::Scrollbar), Rgba::rgb(0xff, 0x79, 0xc6));

        // Direct role override to another role or literal hex.
        overrides
            .roles
            .insert("accent".to_string(), ThemeRoleOverride::Single("#00d0a0".to_string()));
        let custom = ResolvedTheme::resolve(
            &ThemeSettings {
                scheme: ThemeScheme::Light,
                overrides,
            },
            false,
        );
        assert_eq!(custom.rgba(ThemeRole::Accent), Rgba::rgb(0x00, 0xd0, 0xa0));
        assert_eq!(custom.rgba(ThemeRole::Scrollbar), Rgba::rgb(0x00, 0xd0, 0xa0));
        assert_eq!(custom.rgba(ThemeRole::FocusRing), Rgba::rgb(0x00, 0xd0, 0xa0));
    }

    #[test]
    fn cyclic_and_invalid_role_overrides_fall_back_safely() {
        let mut overrides = ThemeOverrides::default();
        overrides
            .roles
            .insert("accent".to_string(), ThemeRoleOverride::Single("scrollbar".to_string()));
        overrides
            .roles
            .insert("scrollbar".to_string(), ThemeRoleOverride::Single("accent".to_string()));
        overrides.roles.insert(
            "warning".to_string(),
            ThemeRoleOverride::Single("not-a-color".to_string()),
        );

        let resolved = ResolvedTheme::resolve(
            &ThemeSettings {
                scheme: ThemeScheme::Dark,
                overrides,
            },
            true,
        );
        // Falls back to default Tokyonight Moon values without panicking or recursing infinitely.
        assert_eq!(resolved.rgba(ThemeRole::Accent), Rgba::rgb(0xfc, 0xa7, 0xea));
        assert_eq!(resolved.rgba(ThemeRole::Warning), Rgba::rgb(0xff, 0xc7, 0x77));
    }

    #[test]
    fn glass_and_agent_overrides_apply_with_validation() {
        let mut overrides = ThemeOverrides {
            glass: ThemeGlassOverrides {
                opacity: Some(0.6),
                blur: Some(28.0),
            },
            ..ThemeOverrides::default()
        };
        overrides.agents.insert("claude".to_string(), "#e07a5f".to_string());
        overrides.agents.insert("broken".to_string(), "nope".to_string());

        let resolved = ResolvedTheme::resolve(
            &ThemeSettings {
                scheme: ThemeScheme::Dark,
                overrides,
            },
            true,
        );
        assert_eq!(resolved.glass.opacity, 0.6);
        assert_eq!(resolved.glass.blur, 28.0);
        assert_eq!(resolved.agent_rgba("claude"), Rgba::rgb(0xe0, 0x7a, 0x5f));
        assert_eq!(resolved.agent_rgba("unknown-agent"), resolved.palette.blue);
        assert_eq!(resolved.agent_rgba("broken"), resolved.palette.blue);
    }
}
