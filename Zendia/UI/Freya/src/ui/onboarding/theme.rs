/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Onboarding palette tokens (Tokyo Night Moon for dark, Day for light), mode colors, and 2D motion math matching
//! the fork's `motion.js` and `zendia-welcome.css`.

use crate::ui::theme::{ResolvedScheme, Rgba};
use freya::engine::prelude as sk;
use freya::prelude::Color;
use std::f32::consts::PI;
use zendia_browser_core::model::InputMode;

/// Resolved onboarding visual tokens for dark or light mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OnboardingPalette {
    pub dark: bool,
    pub bg: Rgba,
    pub surface: Rgba,
    pub raised: Rgba,
    pub border: Rgba,
    pub border_subtle: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub ink: Rgba,
    pub night: Rgba,
    pub snow: Rgba,
    pub stage_bg: Rgba,
    pub window_bg: Rgba,
    pub sidebar_bg: Rgba,
    pub page_bg: Rgba,
    pub page_soft: Rgba,
    pub page_line: Rgba,
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

impl OnboardingPalette {
    pub fn for_scheme(scheme: ResolvedScheme) -> Self {
        match scheme {
            ResolvedScheme::Dark => Self {
                dark: true,
                bg: Rgba::rgb(0x16, 0x17, 0x22),
                surface: Rgba::rgb(0x1e, 0x20, 0x30),
                raised: Rgba::rgb(0x27, 0x2a, 0x40),
                border: Rgba::rgb(0x3b, 0x42, 0x61),
                border_subtle: Rgba::rgb(0x2c, 0x30, 0x48),
                text: Rgba::rgb(0xc8, 0xd3, 0xf5),
                muted: Rgba::rgb(0x8f, 0x98, 0xc4),
                ink: Rgba::rgb(0x1a, 0x1b, 0x26),
                night: Rgba::rgb(0x1a, 0x1b, 0x26),
                snow: Rgba::rgb(0xc0, 0xca, 0xf5),
                stage_bg: Rgba::rgb(0x1a, 0x1c, 0x2b),
                window_bg: Rgba::rgb(0x18, 0x1a, 0x28),
                sidebar_bg: Rgba::rgb(0x1a, 0x1c, 0x2b),
                page_bg: Rgba::rgb(0x20, 0x23, 0x36),
                page_soft: Rgba::rgb(0x29, 0x2d, 0x44),
                page_line: Rgba::rgb(0x34, 0x39, 0x56),
                pink: Rgba::rgb(0xfc, 0xa7, 0xea),
                red: Rgba::rgb(0xff, 0x75, 0x7f),
                orange: Rgba::rgb(0xff, 0x96, 0x6c),
                yellow: Rgba::rgb(0xff, 0xc7, 0x77),
                green: Rgba::rgb(0xc3, 0xe8, 0x8d),
                teal: Rgba::rgb(0x4f, 0xd6, 0xbe),
                cyan: Rgba::rgb(0x86, 0xe1, 0xfc),
                blue: Rgba::rgb(0x82, 0xaa, 0xff),
                purple: Rgba::rgb(0xc0, 0x99, 0xff),
            },
            ResolvedScheme::Light => Self {
                dark: false,
                bg: Rgba::rgb(0xf5, 0xf6, 0xfa),
                surface: Rgba::rgb(0xff, 0xff, 0xff),
                raised: Rgba::rgb(0xe8, 0xeb, 0xf4),
                border: Rgba::rgb(0xcf, 0xd4, 0xe4),
                border_subtle: Rgba::rgb(0xdd, 0xe1, 0xee),
                text: Rgba::rgb(0x1f, 0x23, 0x35),
                muted: Rgba::rgb(0x52, 0x5a, 0x82),
                ink: Rgba::rgb(0x1a, 0x1b, 0x26),
                night: Rgba::rgb(0x1a, 0x1b, 0x26),
                snow: Rgba::rgb(0xc0, 0xca, 0xf5),
                stage_bg: Rgba::rgb(0xeb, 0xef, 0xf9),
                window_bg: Rgba::rgb(0xe8, 0xec, 0xf7),
                sidebar_bg: Rgba::rgb(0xdf, 0xe4, 0xf3),
                page_bg: Rgba::rgb(0xff, 0xff, 0xff),
                page_soft: Rgba::rgb(0xef, 0xf2, 0xfa),
                page_line: Rgba::rgb(0xd6, 0xdc, 0xeb),
                pink: Rgba::rgb(0xb0, 0x30, 0x7f),
                red: Rgba::rgb(0xc8, 0x17, 0x4c),
                orange: Rgba::rgb(0x9c, 0x4f, 0x00),
                yellow: Rgba::rgb(0x7a, 0x5c, 0x14),
                green: Rgba::rgb(0x47, 0x66, 0x28),
                teal: Rgba::rgb(0x0b, 0x74, 0x61),
                cyan: Rgba::rgb(0x00, 0x66, 0x8a),
                blue: Rgba::rgb(0x21, 0x60, 0xc4),
                purple: Rgba::rgb(0x73, 0x40, 0xcc),
            },
        }
    }

    /// Bright pastel Voronoi mode cell color (luminous in both light and dark themes).
    pub fn mode_pastel(mode: InputMode) -> Rgba {
        match mode {
            InputMode::Normal | InputMode::Window => Rgba::rgb(0xfc, 0xa7, 0xea),
            InputMode::Insert => Rgba::rgb(0xc3, 0xe8, 0x8d),
            InputMode::Visual => Rgba::rgb(0xc0, 0x99, 0xff),
            InputMode::Explorer => Rgba::rgb(0x82, 0xaa, 0xff),
            InputMode::Toolbar => Rgba::rgb(0xff, 0xc7, 0x77),
            InputMode::Library => Rgba::rgb(0x4f, 0xd6, 0xbe),
        }
    }

    /// High-contrast theme-aware mode accent for text, focus rings, and selected borders.
    pub fn mode_accent(&self, mode: InputMode) -> Rgba {
        match mode {
            InputMode::Normal | InputMode::Window => self.pink,
            InputMode::Insert => self.green,
            InputMode::Visual => self.purple,
            InputMode::Explorer => self.blue,
            InputMode::Toolbar => self.yellow,
            InputMode::Library => self.teal,
        }
    }

    /// Primary button background: luminous pink pastel in dark mode (matching the dark reference), night ink in light mode.
    pub fn primary_bg(&self, _mode: InputMode) -> Rgba {
        if self.dark {
            Self::mode_pastel(InputMode::Normal)
        } else {
            self.ink
        }
    }

    /// Primary button foreground text color.
    pub fn primary_fg(&self) -> Rgba {
        if self.dark {
            self.ink
        } else {
            Rgba::rgb(0xf5, 0xf6, 0xfa)
        }
    }

    /// Primary button embedded keycap pill background.
    pub fn primary_key_bg(&self) -> Color {
        if self.dark {
            Color::from_argb(36, 0x1a, 0x1b, 0x26)
        } else {
            Color::from_argb(34, 0xff, 0xff, 0xff)
        }
    }
}

/// Ordered list of the six Voronoi modes displayed as pills on the Start page.
pub const MODE_ORDER: [InputMode; 6] = [
    InputMode::Normal,
    InputMode::Insert,
    InputMode::Visual,
    InputMode::Explorer,
    InputMode::Toolbar,
    InputMode::Library,
];

/// Uppercase display name for one of the six Voronoi modes.
pub fn mode_name(mode: InputMode) -> &'static str {
    match mode {
        InputMode::Normal | InputMode::Window => "NORMAL",
        InputMode::Insert => "INSERT",
        InputMode::Visual => "VISUAL",
        InputMode::Explorer => "EXPLORER",
        InputMode::Toolbar => "TOOLBAR",
        InputMode::Library => "LIBRARY",
    }
}

/// Converts an [`Rgba`] to a Skia [`sk::Color`].
pub fn sk_color(rgba: Rgba) -> sk::Color {
    sk::Color::from_argb(rgba.a, rgba.r, rgba.g, rgba.b)
}

/// Converts an [`Rgba`] with a multiplied opacity factor in `0.0..=1.0` to a Skia [`sk::Color`].
pub fn sk_alpha(rgba: Rgba, opacity: f32) -> sk::Color {
    let a = ((rgba.a as f32) * opacity.clamp(0.0, 1.0)).round() as u8;
    sk::Color::from_argb(a, rgba.r, rgba.g, rgba.b)
}

/// Blends `fg` at `ratio` (`0.0..=1.0`) over `bg`, matching CSS `color-mix(in srgb, fg pct%, bg)`.
pub fn mix_rgba(bg: Rgba, fg: Rgba, ratio: f32) -> Rgba {
    bg.mix(fg, ratio.clamp(0.0, 1.0))
}

/* Deterministic pure-Rust trig/exp math (avoids platform-dependent libm calls). */

pub fn sin_f32(x: f32) -> f32 {
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

pub fn cos_f32(x: f32) -> f32 {
    sin_f32(x + PI * 0.5)
}

pub fn exp_f32(x: f32) -> f32 {
    let clamped = x.clamp(-40.0, 40.0);
    let u = clamped / 32.0;
    let t = 1.0 + u * (1.0 + u * (0.5 + u * (1.0 / 6.0 + u * (1.0 / 24.0 + u * (1.0 / 120.0 + u / 720.0)))));
    let t2 = t * t;
    let t4 = t2 * t2;
    let t8 = t4 * t4;
    let t16 = t8 * t8;
    t16 * t16
}

/// Natural logarithm for strictly positive finite `x`.
pub fn ln_f32(x: f32) -> f32 {
    if x <= 0.0 || !x.is_finite() {
        return -40.0;
    }
    // Extract binary exponent via IEEE-754 bits so `m` lies in `[0.7071, 1.4142]`.
    let bits = x.to_bits();
    let mut exp = ((bits >> 23) & 0xff) as i32 - 127;
    let mut m = f32::from_bits((bits & 0x007f_ffff) | (127 << 23));
    if m > std::f32::consts::SQRT_2 {
        m *= 0.5;
        exp += 1;
    }
    let y = (m - 1.0) / (m + 1.0);
    let y2 = y * y;
    let poly = 2.0 * y * (1.0 + y2 * (1.0 / 3.0 + y2 * (1.0 / 5.0 + y2 * (1.0 / 7.0 + y2 / 9.0))));
    poly + exp as f32 * std::f32::consts::LN_2
}

/// Computes `base^exp` for strictly positive `base`.
pub fn pow_pos_f32(base: f32, exp: f32) -> f32 {
    if base <= 0.0 {
        return 0.0;
    }
    exp_f32(exp * ln_f32(base))
}

/* 2D motion & easing primitives matching the fork's `motion.js`. */

pub fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - clamp01(x)).powi(3)
}

pub fn ease_in_out(x: f32) -> f32 {
    let t = clamp01(x);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp01((x - edge0) / (edge1 - edge0));
    t * t * (3.0 - 2.0 * t)
}

/// Soft jelly spring from `motion.js` that overshoots once gently and settles at `1.0`.
pub fn onboarding_spring(x: f32) -> f32 {
    let t = clamp01(x);
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    1.0 - exp_f32(-6.8 * t) * cos_f32(10.5 * t) * (1.0 - t)
}

pub fn span(t: f32, start: f32, duration: f32) -> f32 {
    clamp01((t - start) / duration)
}

/// Easing selector for piecewise-smooth timeline tracks.
#[derive(Clone, Copy)]
pub enum TrackEase {
    Out,
    InOut,
    Spring,
}

impl TrackEase {
    pub fn eval(self, x: f32) -> f32 {
        match self {
            Self::Out => ease_out(x),
            Self::InOut => ease_in_out(x),
            Self::Spring => onboarding_spring(x),
        }
    }
}

/// Evaluates a piecewise-smooth timeline track at time `t` in seconds.
pub fn eval_track(initial: f32, keys: &[(f32, f32, f32, TrackEase)], t: f32) -> f32 {
    let mut value = initial;
    for &(time, target, duration, ease) in keys {
        if t < time {
            break;
        }
        value = lerp(value, target, ease.eval((t - time) / duration));
    }
    value
}

/// One dash of Zendia's 8-dash working ring (in degrees).
#[derive(Clone, Copy, Debug)]
pub struct RingDash {
    pub centre: f32,
    pub half: f32,
    pub glow: f32,
}

const RING_RUN: f32 = 3.9;
const RING_REST: f32 = 0.9;
const RING_DASHES: usize = 8;
const RING_HALF: f32 = 10.95;

fn wrap180(degrees: f32) -> f32 {
    ((((degrees + 180.0) % 360.0) + 360.0) % 360.0) - 180.0
}

/// Computes the 8 dashes of Zendia's working ring at `seconds`.
pub fn ring_dashes(seconds: f32) -> [RingDash; RING_DASHES] {
    let period = RING_RUN + RING_REST;
    let ph = seconds.rem_euclid(period);
    let running = ph > 0.0 && ph < RING_RUN;
    let tau = if running { ph / RING_RUN } else { 0.0 };
    let lead = 720.0 * (0.5 - 0.5 * cos_f32(PI * tau));
    let speed = sin_f32(PI * tau);
    let amp = if running {
        smoothstep(0.0, 0.55, ph) * (1.0 - smoothstep(RING_RUN - 0.75, RING_RUN, ph))
    } else {
        0.0
    };
    let pull = lead - 14.0 * speed;

    std::array::from_fn(|i| {
        let centre = (360.0 / RING_DASHES as f32) * i as f32;
        let dl = wrap180(pull - centre);
        let sigma = if dl < 0.0 { 26.0 } else { 40.0 };
        let glow = amp * exp_f32((-dl * dl) / (sigma * sigma));
        let sign = if dl > 0.0 {
            1.0
        } else if dl < 0.0 {
            -1.0
        } else {
            0.0
        };
        RingDash {
            centre: centre + sign * 9.0 * glow,
            half: RING_HALF * (1.0 + 1.3 * glow),
            glow,
        }
    })
}
