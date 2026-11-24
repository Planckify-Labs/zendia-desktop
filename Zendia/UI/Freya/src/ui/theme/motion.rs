/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Zendia's motion timings, distances, and easing functions.
//!
//! Values match the fork's floating surfaces, dock frame, tab transitions, and agent motion. All easing functions are
//! pure and compatible with Freya's [`freya::animation::Function::Fn`].

use freya::animation::{Ease, Function};
use std::time::Duration;

/// Hover color/background feedback duration (`120 ms`).
pub const HOVER: Duration = Duration::from_millis(120);
/// Floating panel / picker / popover entrance duration (`300 ms`).
pub const FLOAT_IN: Duration = Duration::from_millis(300);
/// Floating panel / picker / popover exit duration (`110 ms`).
pub const FLOAT_OUT: Duration = Duration::from_millis(110);
/// Staggered inner content entrance duration (`200 ms`).
pub const FLOAT_CONTENT_IN: Duration = Duration::from_millis(200);
/// Initial delay before floating content settles in (`40 ms`).
pub const FLOAT_CONTENT_DELAY: Duration = Duration::from_millis(40);
/// Per-row stagger interval inside pickers and menus (`12 ms`).
pub const ROW_STAGGER: Duration = Duration::from_millis(12);
/// Maximum number of staggered rows so fast keyboard picks are never slowed.
pub const ROW_STAGGER_CAP: usize = 6;
/// Picker cursor glide duration (`220 ms`).
pub const PICKER_CURSOR: Duration = Duration::from_millis(220);
/// Confirmation card rise duration (`260 ms`).
pub const CONFIRM_RISE: Duration = Duration::from_millis(260);
/// Switch thumb slide duration (`220 ms`).
pub const SWITCH_THUMB: Duration = Duration::from_millis(220);
/// Switch background color duration (`160 ms`).
pub const SWITCH_BG: Duration = Duration::from_millis(160);
/// Effort slider transition duration (`200 ms`).
pub const EFFORT_SLIDE: Duration = Duration::from_millis(200);
/// Statusline repositioning spring (`340 ms`).
pub const STATUSLINE_MOVE: Duration = Duration::from_millis(340);
/// Statusline pointer duck duration (`160 ms`).
pub const STATUSLINE_DUCK: Duration = Duration::from_millis(160);
/// Tab switch vertical slide duration (`220 ms`).
pub const TAB_SLIDE: Duration = Duration::from_millis(220);
/// Tab switch opacity fade duration (`140 ms`).
pub const TAB_FADE: Duration = Duration::from_millis(140);
/// Repeat-switch threshold below which tab motion is skipped (`90 ms`).
pub const TAB_REPEAT: Duration = Duration::from_millis(90);
/// Maximum wait for a switched tab to paint before skipping motion (`1500 ms`).
pub const TAB_PENDING: Duration = Duration::from_millis(1500);
/// Dock slide keep-hover delay (`400 ms`).
pub const DOCK_KEEP_HOVER: Duration = Duration::from_millis(400);
/// Dock handle hover activation delay (`100 ms`).
pub const DOCK_HANDLE_HOVER: Duration = Duration::from_millis(100);
/// Agent working ring active run (`3900 ms`).
pub const AGENT_RING_RUN: Duration = Duration::from_millis(3900);
/// Agent working ring rest interval (`900 ms`).
pub const AGENT_RING_REST: Duration = Duration::from_millis(900);
/// Agent rolling word per-glyph duration (`550 ms`).
pub const AGENT_ROLL: Duration = Duration::from_millis(550);
/// Agent rolling word width glide (`200 ms`).
pub const AGENT_ROLL_GLIDE: Duration = Duration::from_millis(200);
/// Agent streaming veil duration bounds (`120..=400 ms`).
pub const AGENT_VEIL_MIN: Duration = Duration::from_millis(120);
pub const AGENT_VEIL_MAX: Duration = Duration::from_millis(400);

/// Vertical slide distance for tab switching (`14 px`).
pub const TAB_SLIDE_PX: f32 = 14.0;
/// Starting opacity for tab switch fade (`0.35`).
pub const TAB_FADE_FROM: f32 = 0.35;
/// Floating entrance vertical offset (`-10 px`).
pub const FLOAT_IN_Y: f32 = -10.0;
/// Floating entrance initial scale (`0.96`).
pub const FLOAT_IN_SCALE: f32 = 0.96;
/// Floating exit vertical offset (`-4 px`).
pub const FLOAT_OUT_Y: f32 = -4.0;
/// Floating exit target scale (`0.98`).
pub const FLOAT_OUT_SCALE: f32 = 0.98;
/// Inner content entrance vertical offset (`4 px`).
pub const CONTENT_IN_Y: f32 = 4.0;

/// Sampled spring table matching Zendia's `--zendia-spring` (compact sidebar / dock / picker spring).
const SPRING_TABLE: &[(f32, f32)] = &[
    (0.00, 0.0),
    (0.04, 0.0388),
    (0.08, 0.1313),
    (0.12, 0.2498),
    (0.16, 0.3757),
    (0.20, 0.4970),
    (0.24, 0.6068),
    (0.28, 0.7018),
    (0.32, 0.7810),
    (0.36, 0.8448),
    (0.40, 0.8948),
    (0.44, 0.9327),
    (0.48, 0.9603),
    (0.52, 0.9804),
    (0.56, 0.9940),
    (0.60, 1.0025),
    (0.64, 1.0075),
    (0.68, 1.0101),
    (0.72, 1.0109),
    (0.76, 1.0104),
    (0.84, 1.0084),
    (0.95, 1.0045),
    (1.00, 1.0),
];

/// Evaluates a 1D cubic-bezier curve `cubic-bezier(x1, y1, x2, y2)` at normalized time `x` in `0.0..=1.0`.
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }

    let sample = |a: f32, b: f32, t: f32| -> f32 {
        let inv = 1.0 - t;
        3.0 * inv * inv * t * a + 3.0 * inv * t * t * b + t * t * t
    };
    let slope = |a: f32, b: f32, t: f32| -> f32 {
        let inv = 1.0 - t;
        3.0 * inv * inv * a + 6.0 * inv * t * (b - a) + 3.0 * t * t * (1.0 - b)
    };

    // Newton-Raphson with bisection fallback for steep curves.
    let mut lo = 0.0_f32;
    let mut hi = 1.0_f32;
    let mut t = x;
    for _ in 0..8 {
        let err = sample(x1, x2, t) - x;
        if err.abs() < 1e-5 {
            break;
        }
        if err > 0.0 {
            hi = t;
        } else {
            lo = t;
        }
        let d = slope(x1, x2, t);
        let next = if d.abs() > 1e-6 { t - err / d } else { 0.5 * (lo + hi) };
        t = if next > lo && next < hi { next } else { 0.5 * (lo + hi) };
    }
    sample(y1, y2, t)
}

/// Zendia's slightly underdamped spring curve (`--zendia-spring`).
pub fn zendia_spring(progress: f32, _ease: Ease) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    for window in SPRING_TABLE.windows(2) {
        let (x0, y0) = window[0];
        let (x1, y1) = window[1];
        if t <= x1 {
            let local = (t - x0) / (x1 - x0);
            return y0 + (y1 - y0) * local;
        }
    }
    1.0
}

/// Zendia's standard ease-out (`cubic-bezier(0.22, 1, 0.36, 1)`).
pub fn zendia_ease_out(progress: f32, _ease: Ease) -> f32 {
    cubic_bezier(0.22, 1.0, 0.36, 1.0, progress)
}

/// Zendia's fast exit ease-in (`cubic-bezier(0.4, 0, 1, 1)`).
pub fn zendia_ease_in(progress: f32, _ease: Ease) -> f32 {
    cubic_bezier(0.4, 0.0, 1.0, 1.0, progress)
}

/// Confirmation card rise curve (`cubic-bezier(0.16, 1, 0.3, 1)`).
pub fn zendia_ease_rise(progress: f32, _ease: Ease) -> f32 {
    cubic_bezier(0.16, 1.0, 0.3, 1.0, progress)
}

/// Switch thumb overshoot curve (`cubic-bezier(0.34, 1.4, 0.64, 1)`).
pub fn zendia_switch_spring(progress: f32, _ease: Ease) -> f32 {
    cubic_bezier(0.34, 1.4, 0.64, 1.0, progress)
}

pub const fn spring_fn() -> Function {
    Function::Fn(zendia_spring)
}

pub const fn ease_out_fn() -> Function {
    Function::Fn(zendia_ease_out)
}

pub const fn ease_in_fn() -> Function {
    Function::Fn(zendia_ease_in)
}

pub const fn ease_rise_fn() -> Function {
    Function::Fn(zendia_ease_rise)
}

pub const fn switch_spring_fn() -> Function {
    Function::Fn(zendia_switch_spring)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_matches_endpoints_and_overshoots_gently() {
        assert_eq!(zendia_spring(0.0, Ease::Out), 0.0);
        assert_eq!(zendia_spring(1.0, Ease::Out), 1.0);
        let peak = zendia_spring(0.72, Ease::Out);
        assert!((peak - 1.0109).abs() < 1e-4);
        assert!(zendia_spring(0.5, Ease::Out) > 0.96);
    }

    #[test]
    fn cubic_bezier_easings_hit_endpoints_and_expected_shapes() {
        assert_eq!(zendia_ease_out(0.0, Ease::Out), 0.0);
        assert_eq!(zendia_ease_out(1.0, Ease::Out), 1.0);
        // Ease-out moves quickly early on (> 0.8 at t=0.5).
        assert!(zendia_ease_out(0.5, Ease::Out) > 0.8);

        assert_eq!(zendia_ease_in(0.0, Ease::In), 0.0);
        assert_eq!(zendia_ease_in(1.0, Ease::In), 1.0);
        // Ease-in accelerates late (< 0.35 at t=0.5).
        assert!(zendia_ease_in(0.5, Ease::In) < 0.35);

        assert_eq!(zendia_ease_rise(0.0, Ease::Out), 0.0);
        assert_eq!(zendia_ease_rise(1.0, Ease::Out), 1.0);

        // Switch spring intentionally overshoots 1.0 mid-flight.
        assert!(zendia_switch_spring(0.6, Ease::Out) > 1.0);
        assert_eq!(zendia_switch_spring(1.0, Ease::Out), 1.0);
    }
}
