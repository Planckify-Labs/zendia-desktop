/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Busy indicators: a rotating [`Spinner`] and an [`IndeterminateBar`]. Both animate only while mounted, so leave them
//! out of the tree once the work is done.

use crate::ui::theme::motion;
use freya::animation::*;
use freya::prelude::*;

const SPINNER_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="5.5" stroke="currentColor" stroke-opacity="0.24" stroke-width="1.8"/><path d="M8 2.5A5.5 5.5 0 0 1 13.5 8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>"#;

/// Width of the moving part of an [`IndeterminateBar`], as a share of its track.
const BAR_SHARE: f32 = 0.42;

/// A ring with an arc that turns once every [`motion::SPINNER_TURN`].
#[derive(Clone, PartialEq)]
pub struct Spinner {
    size: f32,
    color: Color,
}

impl Spinner {
    pub fn new(size: f32, color: Color) -> Self {
        Self { size, color }
    }
}

impl Component for Spinner {
    fn render(&self) -> impl IntoElement {
        let turn = use_animation(|conf| {
            conf.on_creation(OnCreation::Run);
            conf.on_finish(OnFinish::restart());
            AnimNum::new(0.0, 360.0)
                .duration(motion::SPINNER_TURN)
                .function(Function::Linear)
        });

        rect()
            .a11y_role(AccessibilityRole::ProgressIndicator)
            .width(Size::px(self.size))
            .height(Size::px(self.size))
            .rotation(turn.get().value())
            .child(
                SvgViewer::new(("zendia-spinner", Bytes::from_static(SPINNER_SVG)))
                    .width(Size::px(self.size))
                    .height(Size::px(self.size))
                    .color(self.color)
                    .show_loader(false),
            )
    }
}

/// A segment sliding along a track, from fully left of it to past its right end, once every
/// [`motion::LOADING_BAR_PASS`].
#[derive(Clone, PartialEq)]
pub struct IndeterminateBar {
    height: f32,
    color: Color,
}

impl IndeterminateBar {
    pub fn new(height: f32, color: Color) -> Self {
        Self { height, color }
    }
}

impl Component for IndeterminateBar {
    fn render(&self) -> impl IntoElement {
        let mut track_width = use_state(|| 0.0f32);
        let pass = use_animation(|conf| {
            conf.on_creation(OnCreation::Run);
            conf.on_finish(OnFinish::restart());
            AnimNum::new(0.0, 1.0)
                .duration(motion::LOADING_BAR_PASS)
                .ease(Ease::InOut)
                .function(Function::Sine)
        });

        let segment = *track_width.read() * BAR_SHARE;
        // The segment travels from -100% to 250% of its own width.
        let offset = segment * (pass.get().value() * 3.5 - 1.0);

        rect()
            .a11y_role(AccessibilityRole::ProgressIndicator)
            .width(Size::fill())
            .height(Size::px(self.height))
            .overflow(Overflow::Clip)
            .on_sized(move |e: Event<SizedEventData>| track_width.set_if_modified(e.area.width()))
            .child(
                rect()
                    .position(Position::new_absolute().left(offset).top(0.0))
                    .width(Size::px(segment))
                    .height(Size::px(self.height))
                    .background(self.color),
            )
    }
}
