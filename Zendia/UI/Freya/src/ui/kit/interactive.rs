/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Shared hover and press motion feedback helpers for buttons, toolbar items, and clickable rows.
//!
//! Uses the fork's timings ([`motion::HOVER`] = `120 ms`, [`motion::PRESS_DURATION`] = `100 ms`,
//! [`motion::PRESS_SCALE`] = `0.98`) with `OnCreation::Finish` and `OnChange::Rerun`, so controls never redraw at rest.

use crate::ui::theme::motion;
use freya::animation::*;
use freya::prelude::*;

/// Linearly blends `from` toward `to` by `t` in `0.0..=1.0`, avoiding dark fringes when fading to/from transparent.
pub fn mix_color(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.001 {
        return from;
    }
    if t >= 0.999 {
        return to;
    }
    let a = (from.a() as f32 + (to.a() as f32 - from.a() as f32) * t).round() as u8;
    if from.a() == 0 {
        return Color::from_argb(a, to.r(), to.g(), to.b());
    }
    if to.a() == 0 {
        return Color::from_argb(a, from.r(), from.g(), from.b());
    }
    let r = (from.r() as f32 + (to.r() as f32 - from.r() as f32) * t).round() as u8;
    let g = (from.g() as f32 + (to.g() as f32 - from.g() as f32) * t).round() as u8;
    let b = (from.b() as f32 + (to.b() as f32 - from.b() as f32) * t).round() as u8;
    Color::from_argb(a, r, g, b)
}

/// Computes the press scale for a normalized press progress `t` in `0.0..=1.0`.
pub fn press_scale_for(t: f32) -> f32 {
    1.0 - (1.0 - motion::PRESS_SCALE) * t.clamp(0.0, 1.0)
}

/// Animated hover/press state returned by [`use_interactive_feedback`].
#[derive(Clone, Copy)]
pub struct InteractiveFeedback {
    pub hovered: State<bool>,
    pub pressed: State<bool>,
    hover_t: f32,
    press_t: f32,
    enabled: bool,
}

impl InteractiveFeedback {
    /// Whether the pointer is currently hovering the control (and it is enabled).
    pub fn is_hovered(&self) -> bool {
        self.enabled && *self.hovered.read()
    }

    /// Whether the pointer is currently pressing the control (and it is enabled).
    pub fn is_pressed(&self) -> bool {
        self.enabled && *self.pressed.read()
    }

    /// Animated hover progress in `0.0..=1.0` over [`motion::HOVER`].
    pub fn hover_progress(&self) -> f32 {
        self.hover_t
    }

    /// Animated press progress in `0.0..=1.0` over [`motion::PRESS_DURATION`].
    pub fn press_progress(&self) -> f32 {
        self.press_t
    }

    /// Active scale factor (`1.0` at rest -> [`motion::PRESS_SCALE`] while pressed).
    pub fn scale(&self) -> f32 {
        press_scale_for(self.press_t)
    }

    /// Blends `rest -> hover -> pressed` smoothly using the animated hover and press factors.
    pub fn background(&self, rest: Color, hover: Color, pressed: Color) -> Color {
        let hovered_bg = mix_color(rest, hover, self.hover_t);
        mix_color(hovered_bg, pressed, self.press_t)
    }

    /// Returns a pointer-enter handler that sets hover state.
    pub fn on_pointer_enter(&self) -> impl FnMut(Event<PointerEventData>) + 'static {
        let mut hovered = self.hovered;
        move |_| hovered.set_if_modified(true)
    }

    /// Returns a pointer-leave handler that clears both hover and pressed state.
    pub fn on_pointer_leave(&self) -> impl FnMut(Event<PointerEventData>) + 'static {
        let mut hovered = self.hovered;
        let mut pressed = self.pressed;
        move |_| {
            hovered.set_if_modified(false);
            pressed.set_if_modified(false);
        }
    }

    /// Returns a pointer-down handler that sets pressed state while enabled.
    pub fn on_pointer_down(&self) -> impl FnMut(Event<PointerEventData>) + 'static {
        let mut pressed = self.pressed;
        let enabled = self.enabled;
        move |_| {
            if enabled {
                pressed.set_if_modified(true);
            }
        }
    }

    /// Returns a pointer-up handler that clears pressed state.
    pub fn on_pointer_up(&self) -> impl FnMut(Event<PointerEventData>) + 'static {
        let mut pressed = self.pressed;
        move |_| pressed.set_if_modified(false)
    }
}

/// Hook providing animated hover (`120 ms`) and press (`100 ms`, `scale 0.98`) feedback for any interactive control.
///
/// Must be called unconditionally at the top of `Component::render`.
pub fn use_interactive_feedback(enabled: bool) -> InteractiveFeedback {
    let mut hovered = use_state(|| false);
    let mut pressed = use_state(|| false);

    if !enabled {
        if *hovered.peek() {
            hovered.set(false);
        }
        if *pressed.peek() {
            pressed.set(false);
        }
    }

    let is_hovered = enabled && *hovered.read();
    let is_pressed = enabled && *pressed.read();

    let hover_anim = use_animation_with_dependencies(&is_hovered, move |conf, active| {
        conf.on_creation(OnCreation::Finish);
        conf.on_change(OnChange::Rerun);
        let (from, to) = if *active { (0.0, 1.0) } else { (1.0, 0.0) };
        AnimNum::new(from, to)
            .duration(motion::HOVER)
            .ease(Ease::Out)
            .function(motion::ease_out_fn())
    });

    let press_anim = use_animation_with_dependencies(&is_pressed, move |conf, active| {
        conf.on_creation(OnCreation::Finish);
        conf.on_change(OnChange::Rerun);
        let (from, to) = if *active { (0.0, 1.0) } else { (1.0, 0.0) };
        AnimNum::new(from, to)
            .duration(motion::PRESS_DURATION)
            .ease(Ease::Out)
            .function(motion::ease_out_fn())
    });

    InteractiveFeedback {
        hovered,
        pressed,
        hover_t: hover_anim.get().value().clamp(0.0, 1.0),
        press_t: press_anim.get().value().clamp(0.0, 1.0),
        enabled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_color_handles_transparent_endpoints_without_darkening() {
        let target = Color::from_argb(100, 200, 210, 220);
        let mid = mix_color(Color::TRANSPARENT, target, 0.5);
        assert_eq!(mid.r(), 200);
        assert_eq!(mid.g(), 210);
        assert_eq!(mid.b(), 220);
        assert_eq!(mid.a(), 50);

        assert_eq!(mix_color(Color::TRANSPARENT, target, 0.0), Color::TRANSPARENT);
        assert_eq!(mix_color(Color::TRANSPARENT, target, 1.0), target);
    }

    #[test]
    fn press_scale_reaches_fork_active_scale() {
        assert_eq!(press_scale_for(0.0), 1.0);
        assert!((press_scale_for(1.0) - motion::PRESS_SCALE).abs() < 1e-5);
        assert!((press_scale_for(0.5) - 0.99).abs() < 1e-5);
    }
}
