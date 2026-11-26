/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Shared popup presence motion, inner-content stagger, and window-edge positioning helpers.
//!
//! [`PopupPresence`] keeps a popup mounted while its exit animation plays:
//! - Entrance: spring curve over [`motion::FLOAT_IN`] (`300 ms`) from [`motion::FLOAT_IN_Y`] (`-10 px`) and
//!   [`motion::FLOAT_IN_SCALE`] (`0.96`).
//! - Exit: ease-in curve over [`motion::FLOAT_OUT`] (`110 ms`) to [`motion::FLOAT_OUT_Y`] (`-4 px`) and
//!   [`motion::FLOAT_OUT_SCALE`] (`0.98`).
//!
//! [`PopupContent`] adds the fork's [`motion::FLOAT_CONTENT_DELAY`] (`40 ms`) inner-content settle over
//! [`motion::FLOAT_CONTENT_IN`] (`200 ms`) from [`motion::CONTENT_IN_Y`] (`4 px`), and [`StaggeredRow`] staggers
//! individual list rows by [`motion::ROW_STAGGER`] (`12 ms`), capped at [`motion::ROW_STAGGER_CAP`] (`6`).
//! All animations stop redrawing as soon as they finish.

use crate::ui::chrome::menus::AnchorRect;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, motion};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;

/// Keeps the last `Some(value)` snapshot alive in component state so a popup driven by `Option<T>` can remain mounted
/// while [`PopupPresence`] plays its exit animation.
pub fn use_last_some<T: Clone + PartialEq + 'static>(current: &Option<T>) -> Option<T> {
    let mut cached = use_state(|| current.clone());
    if let Some(value) = current
        && cached.peek().as_ref() != Some(value)
    {
        cached.set(Some(value.clone()));
    }
    if current.is_some() {
        current.clone()
    } else {
        cached.read().clone()
    }
}

/// How [`PopupPresence`] positions its floating child within the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopupPlacement {
    /// Positioned at global `(x, y)` coordinates with an optional full-window click-away backdrop.
    Anchored { origin: (f32, f32) },
    /// Centered in the window over an optional fading scrim (modal dialogs such as Share and About).
    Centered,
    /// Rendered inline without a global overlay backdrop (for submenus or embedded cards).
    Inline,
}

/// Horizontal alignment preference when anchoring a popup to a trigger button's [`AnchorRect`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopupHorizontalAlign {
    /// Align the popup's left edge with the anchor's left edge.
    Start,
    /// Align the popup's right edge with the anchor's right edge.
    End,
    /// Align right when the anchor sits in the right half of the window (or would overflow right), otherwise left.
    #[default]
    Auto,
}

/// Clamps a popup's top-left origin so it stays fully inside `window_size` with `margin` padding.
///
/// When `desired` overflows the right or bottom edge, flips across the desired point when space permits, otherwise
/// clamps inside `[margin, window - popup - margin]`.
pub fn clamp_popup_origin(
    desired: (f32, f32),
    popup_size: (f32, f32),
    window_size: (f32, f32),
    margin: f32,
) -> (f32, f32) {
    let max_x = (window_size.0 - popup_size.0 - margin).max(margin);
    let max_y = (window_size.1 - popup_size.1 - margin).max(margin);

    let x = if desired.0 + popup_size.0 + margin > window_size.0 {
        let flipped = desired.0 - popup_size.0;
        if flipped >= margin {
            flipped
        } else {
            desired.0.clamp(margin, max_x)
        }
    } else {
        desired.0.max(margin)
    };

    let y = if desired.1 + popup_size.1 + margin > window_size.1 {
        let flipped = desired.1 - popup_size.1;
        if flipped >= margin {
            flipped
        } else {
            desired.1.clamp(margin, max_y)
        }
    } else {
        desired.1.max(margin)
    };

    (x.clamp(margin, max_x), y.clamp(margin, max_y))
}

/// Anchors a popup below or above `anchor`, aligns it horizontally according to `align`, and clamps it inside
/// `window_size` with `margin` padding.
pub fn anchor_popup_origin(
    anchor: AnchorRect,
    popup_size: (f32, f32),
    window_size: (f32, f32),
    align: PopupHorizontalAlign,
    gap: f32,
    margin: f32,
) -> (f32, f32) {
    let max_x = (window_size.0 - popup_size.0 - margin).max(margin);
    let max_y = (window_size.1 - popup_size.1 - margin).max(margin);

    let right_aligned_x = anchor.x + anchor.width - popup_size.0;
    let desired_x = match align {
        PopupHorizontalAlign::Start => anchor.x,
        PopupHorizontalAlign::End => right_aligned_x,
        PopupHorizontalAlign::Auto => {
            let anchor_center = anchor.x + anchor.width * 0.5;
            let overflows_right = anchor.x + popup_size.0 + margin > window_size.0;
            if anchor_center > window_size.0 * 0.5 || overflows_right {
                right_aligned_x
            } else {
                anchor.x
            }
        }
    };

    let below_y = anchor.y + anchor.height + gap;
    let above_y = anchor.y - gap - popup_size.1;
    let y = if below_y + popup_size.1 + margin <= window_size.1 {
        below_y
    } else if above_y >= margin {
        above_y
    } else {
        let space_below = (window_size.1 - (anchor.y + anchor.height + gap) - margin).max(0.0);
        let space_above = (anchor.y - gap - margin).max(0.0);
        if space_above > space_below {
            above_y.clamp(margin, max_y)
        } else {
            below_y.clamp(margin, max_y)
        }
    };

    (desired_x.clamp(margin, max_x), y.clamp(margin, max_y))
}

/// Returns the maximum safe popup height for a window of `window_height`, capped at `cap`.
pub fn max_popup_height(window_height: f32, margin: f32, cap: f32) -> f32 {
    (window_height - margin * 2.0).max(120.0).min(cap)
}

/// Evaluates a delayed ease-out factor in `0.0..=1.0` for a linear timeline `elapsed_ms`.
pub fn delayed_ease_out(elapsed_ms: f32, delay: Duration, duration: Duration) -> f32 {
    let delay_ms = delay.as_millis() as f32;
    let dur_ms = (duration.as_millis() as f32).max(1.0);
    let local = ((elapsed_ms - delay_ms) / dur_ms).clamp(0.0, 1.0);
    motion::zendia_ease_out(local, Ease::Out).clamp(0.0, 1.0)
}

/// Computes `(opacity, y_offset, scale)` for a [`PopupPresence`] frame.
pub fn popup_transform(open: bool, progress: f32, animate_slide: bool, animate_scale: bool) -> (f32, f32, f32) {
    let opacity = progress.clamp(0.0, 1.0);
    let y_offset = if !animate_slide {
        0.0
    } else if open {
        motion::FLOAT_IN_Y * (1.0 - progress)
    } else {
        motion::FLOAT_OUT_Y * (1.0 - progress)
    };
    let scale = if !animate_scale {
        1.0
    } else if open {
        motion::FLOAT_IN_SCALE + (1.0 - motion::FLOAT_IN_SCALE) * progress
    } else {
        motion::FLOAT_OUT_SCALE + (1.0 - motion::FLOAT_OUT_SCALE) * progress
    };
    (opacity, y_offset, scale)
}

/// Scales a [`Color`]'s alpha channel by `factor` in `0.0..=1.0` for fading backdrops.
fn scale_color_alpha(color: Color, factor: f32) -> Color {
    let alpha = ((color.a() as f32) * factor.clamp(0.0, 1.0)).round() as u8;
    Color::from_argb(alpha, color.r(), color.g(), color.b())
}

/// Presence wrapper that keeps a popup mounted while its exit animation plays and handles backdrop dismissal, Escape
/// dismissal, and overlay layering.
#[derive(Clone, PartialEq)]
pub struct PopupPresence {
    open: bool,
    open_seq: u64,
    placement: PopupPlacement,
    backdrop_color: Option<Color>,
    capture_backdrop: bool,
    layer: Option<Layer>,
    animate_scale: bool,
    animate_slide: bool,
    animate_on_mount: bool,
    on_backdrop_mouse_down: Option<EventHandler<Event<MouseEventData>>>,
    on_escape: Option<EventHandler<()>>,
    on_global_key_down: Option<EventHandler<Event<KeyboardEventData>>>,
    children: Vec<Element>,
    key: DiffKey,
}

impl KeyExt for PopupPresence {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ChildrenExt for PopupPresence {
    fn children_mut(&mut self) -> &mut Vec<Element> {
        &mut self.children
    }
}

impl PopupPresence {
    /// Creates an overlay [`PopupPresence`] anchored at global coordinates `origin`.
    pub fn anchored(open: bool, origin: (f32, f32)) -> Self {
        Self {
            open,
            open_seq: 0,
            placement: PopupPlacement::Anchored { origin },
            backdrop_color: None,
            capture_backdrop: true,
            layer: Some(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL)),
            animate_scale: true,
            animate_slide: true,
            animate_on_mount: false,
            on_backdrop_mouse_down: None,
            on_escape: None,
            on_global_key_down: None,
            children: Vec::new(),
            key: DiffKey::None,
        }
    }

    /// Creates a centered modal [`PopupPresence`] (such as Share or About dialogs).
    pub fn centered(open: bool) -> Self {
        Self {
            open,
            open_seq: 0,
            placement: PopupPlacement::Centered,
            backdrop_color: None,
            capture_backdrop: true,
            layer: Some(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL)),
            animate_scale: true,
            animate_slide: true,
            animate_on_mount: false,
            on_backdrop_mouse_down: None,
            on_escape: None,
            on_global_key_down: None,
            children: Vec::new(),
            key: DiffKey::None,
        }
    }

    /// Creates an inline [`PopupPresence`] without a full-window backdrop (for submenus or nested popovers).
    pub fn inline(open: bool) -> Self {
        Self {
            open,
            open_seq: 0,
            placement: PopupPlacement::Inline,
            backdrop_color: None,
            capture_backdrop: false,
            layer: None,
            animate_scale: true,
            animate_slide: true,
            animate_on_mount: false,
            on_backdrop_mouse_down: None,
            on_escape: None,
            on_global_key_down: None,
            children: Vec::new(),
            key: DiffKey::None,
        }
    }

    /// Bumping `seq` while `open` is true replays the entrance spring (for popups repositioned in place).
    pub fn open_seq(mut self, seq: u64) -> Self {
        self.open_seq = seq;
        self
    }

    /// Sets a backdrop tint that fades smoothly in and out with the popup's opacity.
    pub fn backdrop_color(mut self, color: Color) -> Self {
        self.backdrop_color = Some(color);
        self
    }

    /// Controls whether a full-window backdrop captures outside pointer presses while open.
    pub fn capture_backdrop(mut self, capture: bool) -> Self {
        self.capture_backdrop = capture;
        self
    }

    /// Overrides the paint layer (`POPUP_OVERLAY_LEVEL` by default for anchored/centered popups).
    pub fn layer(mut self, layer: Layer) -> Self {
        self.layer = Some(layer);
        self
    }

    /// Enables or disables scale motion (`FLOAT_IN_SCALE` / `FLOAT_OUT_SCALE`). Enabled by default.
    pub fn animate_scale(mut self, enabled: bool) -> Self {
        self.animate_scale = enabled;
        self
    }

    /// Enables or disables vertical slide motion (`FLOAT_IN_Y` / `FLOAT_OUT_Y`). Enabled by default.
    pub fn animate_slide(mut self, enabled: bool) -> Self {
        self.animate_slide = enabled;
        self
    }

    /// Plays the entrance animation immediately when first mounted open (useful for submenus mounted on demand).
    pub fn animate_on_mount(mut self, enabled: bool) -> Self {
        self.animate_on_mount = enabled;
        self
    }

    /// Invoked when the user presses a mouse button on the backdrop outside the popup while open.
    pub fn on_backdrop_mouse_down(mut self, handler: impl Into<EventHandler<Event<MouseEventData>>>) -> Self {
        self.on_backdrop_mouse_down = Some(handler.into());
        self
    }

    /// Invoked when `Escape` is pressed while open (stops propagation automatically).
    pub fn on_escape(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_escape = Some(handler.into());
        self
    }

    /// Optional global key listener active only while `open` is true (never fires during exit fade).
    pub fn on_global_key_down(mut self, handler: impl Into<EventHandler<Event<KeyboardEventData>>>) -> Self {
        self.on_global_key_down = Some(handler.into());
        self
    }
}

impl Component for PopupPresence {
    fn render(&self) -> impl IntoElement {
        let animate_on_mount = self.animate_on_mount;
        let anim = use_animation_with_dependencies(&(self.open, self.open_seq), move |conf, (open, _)| {
            conf.on_creation(if animate_on_mount && *open {
                OnCreation::Run
            } else {
                OnCreation::Finish
            });
            conf.on_change(OnChange::Rerun);
            if *open {
                AnimNum::new(0.0, 1.0)
                    .duration(motion::FLOAT_IN)
                    .ease(Ease::Out)
                    .function(motion::spring_fn())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(motion::FLOAT_OUT)
                    .ease(Ease::In)
                    .function(motion::ease_in_fn())
            }
        });

        let progress = anim.get().value().clamp(0.0, 1.05);
        let (opacity, y_offset, scale) = popup_transform(self.open, progress, self.animate_slide, self.animate_scale);

        if !self.open && opacity <= 0.01 {
            return rect().into_element();
        }

        let is_open = self.open;
        let on_escape = self.on_escape.clone();
        let on_global_key = self.on_global_key_down.clone();
        let has_key_handler = is_open && (on_escape.is_some() || on_global_key.is_some());

        let interactive_mode = if is_open { Interactive::Yes } else { Interactive::No };

        let anchored_slide = matches!(self.placement, PopupPlacement::Anchored { .. });
        let content_box = rect()
            .opacity(opacity)
            .offset_y(if anchored_slide { 0.0 } else { y_offset })
            .scale(scale)
            .interactive(interactive_mode)
            .children(self.children.clone());

        if matches!(self.placement, PopupPlacement::Inline) {
            let mut el = content_box;
            if let Some(layer) = self.layer {
                el = el.layer(layer);
            }
            if has_key_handler {
                el = el.on_global_key_down(move |e: Event<KeyboardEventData>| {
                    if !is_open {
                        return;
                    }
                    if e.key == Key::Named(NamedKey::Escape)
                        && let Some(esc) = &on_escape
                    {
                        e.stop_propagation();
                        esc.call(());
                        return;
                    }
                    if let Some(handler) = &on_global_key {
                        handler.call(e);
                    }
                });
            }
            return el.into_element();
        }

        let backdrop_bg = self
            .backdrop_color
            .map(|c| scale_color_alpha(c, opacity))
            .unwrap_or(Color::TRANSPARENT);

        let on_backdrop = self.on_backdrop_mouse_down.clone();
        let mut root = rect()
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .interactive(interactive_mode);

        if let Some(layer) = self.layer {
            root = root.layer(layer);
        }

        if has_key_handler {
            root = root.on_global_key_down(move |e: Event<KeyboardEventData>| {
                if !is_open {
                    return;
                }
                if e.key == Key::Named(NamedKey::Escape)
                    && let Some(esc) = &on_escape
                {
                    e.stop_propagation();
                    esc.call(());
                    return;
                }
                if let Some(handler) = &on_global_key {
                    handler.call(e);
                }
            });
        }

        let backdrop = (self.capture_backdrop || self.backdrop_color.is_some()).then(|| {
            let mut bg_rect = rect()
                .position(Position::new_global().left(0.0).top(0.0))
                .width(Size::window_percent(100.0))
                .height(Size::window_percent(100.0))
                .background(backdrop_bg)
                .interactive(if is_open && self.capture_backdrop {
                    Interactive::Yes
                } else {
                    Interactive::No
                });
            if is_open && self.capture_backdrop {
                bg_rect = bg_rect.on_mouse_down(move |e: Event<MouseEventData>| {
                    if let Some(handler) = &on_backdrop {
                        handler.call(e);
                    }
                });
            }
            bg_rect
        });

        match self.placement {
            PopupPlacement::Centered => root.center().maybe_child(backdrop).child(content_box).into_element(),
            PopupPlacement::Anchored { origin } => root
                .maybe_child(backdrop)
                .child(
                    rect()
                        .position(Position::new_global().left(origin.0).top(origin.1 + y_offset))
                        .interactive(interactive_mode)
                        .child(content_box),
                )
                .into_element(),
            PopupPlacement::Inline => unreachable!(),
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Inner popup content wrapper applying [`motion::FLOAT_CONTENT_DELAY`] (`40 ms`) and [`motion::FLOAT_CONTENT_IN`]
/// (`200 ms`) from [`motion::CONTENT_IN_Y`] (`4 px`), settling cleanly with zero idle redraws.
#[derive(Clone, PartialEq)]
pub struct PopupContent {
    trigger: u64,
    animate_on_mount: bool,
    children: Vec<Element>,
    key: DiffKey,
}

impl KeyExt for PopupContent {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ChildrenExt for PopupContent {
    fn children_mut(&mut self) -> &mut Vec<Element> {
        &mut self.children
    }
}

impl PopupContent {
    pub fn new(trigger: u64) -> Self {
        Self {
            trigger,
            animate_on_mount: false,
            children: Vec::new(),
            key: DiffKey::None,
        }
    }

    pub fn animate_on_mount(mut self, enabled: bool) -> Self {
        self.animate_on_mount = enabled;
        self
    }
}

impl Component for PopupContent {
    fn render(&self) -> impl IntoElement {
        let total = motion::FLOAT_CONTENT_DELAY + motion::FLOAT_CONTENT_IN;
        let total_ms = total.as_millis() as f32;
        let animate_on_mount = self.animate_on_mount;

        let anim = use_animation_with_dependencies(&self.trigger, move |conf, _| {
            conf.on_creation(if animate_on_mount {
                OnCreation::Run
            } else {
                OnCreation::Finish
            });
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, total_ms).duration(total).function(Function::Linear)
        });

        let factor = delayed_ease_out(
            anim.get().value(),
            motion::FLOAT_CONTENT_DELAY,
            motion::FLOAT_CONTENT_IN,
        );
        let y_offset = motion::CONTENT_IN_Y * (1.0 - factor);

        rect()
            .width(Size::fill())
            .opacity(factor)
            .offset_y(y_offset)
            .children(self.children.clone())
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Row wrapper that staggers entrance by [`motion::ROW_STAGGER`] (`12 ms`) per row up to [`motion::ROW_STAGGER_CAP`]
/// (`6`), rising from [`motion::CONTENT_IN_Y`] (`4 px`).
#[derive(Clone, PartialEq)]
pub struct StaggeredRow {
    index: usize,
    trigger: u64,
    animate_on_mount: bool,
    children: Vec<Element>,
    key: DiffKey,
}

impl KeyExt for StaggeredRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ChildrenExt for StaggeredRow {
    fn children_mut(&mut self) -> &mut Vec<Element> {
        &mut self.children
    }
}

impl StaggeredRow {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            trigger: 0,
            animate_on_mount: false,
            children: Vec::new(),
            key: DiffKey::None,
        }
    }

    pub fn trigger(mut self, trigger: u64) -> Self {
        self.trigger = trigger;
        self
    }

    pub fn animate_on_mount(mut self, enabled: bool) -> Self {
        self.animate_on_mount = enabled;
        self
    }
}

impl Component for StaggeredRow {
    fn render(&self) -> impl IntoElement {
        let delay = motion::row_stagger_delay(self.index);
        let total = delay + motion::FLOAT_CONTENT_IN;
        let total_ms = total.as_millis() as f32;
        let animate_on_mount = self.animate_on_mount;

        let anim = use_animation_with_dependencies(&self.trigger, move |conf, _| {
            conf.on_creation(if animate_on_mount {
                OnCreation::Run
            } else {
                OnCreation::Finish
            });
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, total_ms).duration(total).function(Function::Linear)
        });

        let factor = delayed_ease_out(anim.get().value(), delay, motion::FLOAT_CONTENT_IN);
        let y_offset = motion::CONTENT_IN_Y * (1.0 - factor);

        rect()
            .width(Size::fill())
            .opacity(factor)
            .offset_y(y_offset)
            .children(self.children.clone())
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_transform_uses_fork_float_constants() {
        let (op_start, y_start, s_start) = popup_transform(true, 0.0, true, true);
        assert_eq!(op_start, 0.0);
        assert_eq!(y_start, motion::FLOAT_IN_Y);
        assert!((s_start - motion::FLOAT_IN_SCALE).abs() < 1e-5);

        let (op_end, y_end, s_end) = popup_transform(true, 1.0, true, true);
        assert_eq!(op_end, 1.0);
        assert_eq!(y_end, 0.0);
        assert!((s_end - 1.0).abs() < 1e-5);

        let (op_out, y_out, s_out) = popup_transform(false, 0.0, true, true);
        assert_eq!(op_out, 0.0);
        assert_eq!(y_out, motion::FLOAT_OUT_Y);
        assert!((s_out - motion::FLOAT_OUT_SCALE).abs() < 1e-5);
    }

    #[test]
    fn anchor_popup_origin_right_aligns_and_flips_above() {
        let window = (1280.0, 800.0);
        // Right-side top button: right-aligns to anchor right edge.
        let right_anchor = AnchorRect {
            x: 1220.0,
            y: 8.0,
            width: 28.0,
            height: 28.0,
        };
        let (x, y) = anchor_popup_origin(
            right_anchor,
            (296.0, 320.0),
            window,
            PopupHorizontalAlign::Auto,
            6.0,
            8.0,
        );
        assert_eq!(x, 1220.0 + 28.0 - 296.0);
        assert_eq!(y, 42.0);

        // Bottom-left button: flips above anchor.
        let bottom_anchor = AnchorRect {
            x: 20.0,
            y: 760.0,
            width: 28.0,
            height: 28.0,
        };
        let (bx, by) = anchor_popup_origin(
            bottom_anchor,
            (296.0, 300.0),
            window,
            PopupHorizontalAlign::Auto,
            6.0,
            8.0,
        );
        assert_eq!(bx, 20.0);
        assert_eq!(by, 760.0 - 6.0 - 300.0);
    }

    #[test]
    fn delayed_ease_out_holds_zero_during_delay_and_reaches_one() {
        assert_eq!(
            delayed_ease_out(0.0, motion::FLOAT_CONTENT_DELAY, motion::FLOAT_CONTENT_IN),
            0.0
        );
        assert_eq!(
            delayed_ease_out(40.0, motion::FLOAT_CONTENT_DELAY, motion::FLOAT_CONTENT_IN),
            0.0
        );
        let mid = delayed_ease_out(140.0, motion::FLOAT_CONTENT_DELAY, motion::FLOAT_CONTENT_IN);
        assert!(mid > 0.7 && mid < 1.0);
        assert_eq!(
            delayed_ease_out(240.0, motion::FLOAT_CONTENT_DELAY, motion::FLOAT_CONTENT_IN),
            1.0
        );
    }

    #[test]
    fn row_stagger_delay_caps_at_row_stagger_cap() {
        assert_eq!(motion::row_stagger_delay(0), Duration::from_millis(0));
        assert_eq!(motion::row_stagger_delay(3), Duration::from_millis(36));
        assert_eq!(motion::row_stagger_delay(6), Duration::from_millis(72));
        assert_eq!(motion::row_stagger_delay(20), Duration::from_millis(72));
    }
}
