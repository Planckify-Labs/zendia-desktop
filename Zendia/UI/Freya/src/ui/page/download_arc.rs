/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The download animation (Zen's `ZenDownloadAnimation`): a download icon flies in an arc from the page into the
//! library button, growing as it rises and shrinking as it lands.
//!
//! The library button records where it sits in [`DownloadArcState::anchor`]; the window starts a flight when the core
//! publishes `DownloadStarted`.

use crate::ui::kit::Icon;
use crate::ui::theme::POPUP_OVERLAY_LEVEL;
use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;

/// How long one flight takes.
pub const ARC_DURATION: Duration = Duration::from_millis(1000);
/// Size of the flying icon's circle at scale 1.
const ARC_ICON: f32 = 32.0;
const ARC_HEIGHT_RATIO: f32 = 2.0;
const MAX_ARC_HEIGHT: f32 = 1200.0;
const SCALE_END: f32 = 0.45;

/// A download icon in flight, in window coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcFlight {
    pub id: u64,
    pub start: (f32, f32),
    pub end: (f32, f32),
}

/// View state for download flights, shared by the library button and the arc layer.
#[derive(Clone, Copy, PartialEq)]
pub struct DownloadArcState {
    /// Center of the library button in window coordinates, when it is on screen.
    pub anchor: State<Option<(f32, f32)>>,
    pub flights: State<Vec<ArcFlight>>,
}

impl DownloadArcState {
    /// Starts a flight from `start` to the library button (or the bottom corner of the sidebar side when the button is
    /// hidden). The flight removes itself when it lands.
    pub fn launch(&self, start: (f32, f32), fallback_end: (f32, f32)) {
        let end = (*self.anchor.peek()).unwrap_or(fallback_end);
        let mut flights = self.flights;
        let id = flights.peek().iter().map(|flight| flight.id).max().unwrap_or(0) + 1;
        flights.write().push(ArcFlight { id, start, end });
        spawn(async move {
            timer(ARC_DURATION + Duration::from_millis(60)).await;
            flights.write().retain(|flight| flight.id != id);
        });
    }
}

pub fn use_provide_download_arc() -> DownloadArcState {
    let anchor = use_state(|| None::<(f32, f32)>);
    let flights = use_state(Vec::<ArcFlight>::new);
    use_provide_context(|| DownloadArcState { anchor, flights })
}

pub fn try_use_download_arc() -> Option<DownloadArcState> {
    try_consume_context::<DownloadArcState>()
}

/// Where the icon is at `progress` (0 to 1): position, scale and opacity, as Zen computes them.
pub fn arc_point(start: (f32, f32), end: (f32, f32), progress: f32) -> ((f32, f32), f32, f32) {
    let t = progress.clamp(0.0, 1.0);
    let eased = if t < 0.5 {
        2.0 * t * t
    } else {
        -1.0 + (4.0 - 2.0 * t) * t
    };
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let distance = (dx * dx + dy * dy).sqrt();
    let headroom = start.1.min(end.1).max(0.0);
    let height = (distance * ARC_HEIGHT_RATIO).min(MAX_ARC_HEIGHT).min(headroom * 0.9);
    let lift = height * (1.0 - (2.0 * eased - 1.0).powi(2));
    let position = (start.0 + dx * eased, start.1 + dy * eased - lift);
    let scale = if t < 0.5 {
        0.5 + (t / 0.5) * 1.3
    } else {
        1.8 - ((t - 0.5) / 0.5) * (1.8 - SCALE_END)
    };
    let opacity = if t < 0.3 {
        0.3 + (t / 0.3) * 0.6
    } else if t < 0.9 {
        0.9 + ((t - 0.3) / 0.6) * 0.1
    } else {
        1.0 - ((t - 0.9) / 0.1)
    };
    (position, scale, opacity.clamp(0.0, 1.0))
}

/// Draws every flight over the whole window.
#[derive(Clone, PartialEq)]
pub struct DownloadArcLayer {
    /// Pins every flight at this progress (for gallery states).
    pub pinned_progress: Option<f32>,
}

impl Component for DownloadArcLayer {
    fn render(&self) -> impl IntoElement {
        let state = try_use_download_arc();
        let flights = state.map(|state| state.flights.read().clone()).unwrap_or_default();
        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::px(0.0))
            .height(Size::px(0.0))
            .children(flights.into_iter().map(|flight| {
                ArcFlightView {
                    flight,
                    pinned_progress: self.pinned_progress,
                }
                .into_element()
            }))
    }
}

#[derive(Clone, PartialEq)]
struct ArcFlightView {
    flight: ArcFlight,
    pinned_progress: Option<f32>,
}

impl Component for ArcFlightView {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let flight_anim = use_animation(move |conf| {
            conf.on_creation(OnCreation::Run);
            AnimNum::new(0.0, 1.0)
                .duration(ARC_DURATION)
                .ease(Ease::InOut)
                .function(motion::ease_out_fn())
        });
        let progress = self.pinned_progress.unwrap_or_else(|| flight_anim.get().value());
        let ((x, y), scale, opacity) = arc_point(self.flight.start, self.flight.end, progress);

        rect()
            .position(Position::new_global().left(x - ARC_ICON / 2.0).top(y - ARC_ICON / 2.0))
            .width(Size::px(ARC_ICON))
            .height(Size::px(ARC_ICON))
            .corner_radius(CornerRadius::new_all(ARC_ICON))
            .scale((scale, scale))
            .opacity(opacity)
            .background(theme.color(ThemeRole::Accent))
            .shadow(Shadow::new().y(3.0).blur(10.0).color(Color::from_argb(80, 0, 0, 0)))
            .center()
            .child(Icon::new("downloads").size(16.0).role(ThemeRole::TextOnAccent))
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.flight.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arc_starts_and_lands_on_its_points() {
        let (start, _, _) = arc_point((800.0, 500.0), (30.0, 960.0), 0.0);
        assert_eq!(start, (800.0, 500.0));
        let (end, scale, opacity) = arc_point((800.0, 500.0), (30.0, 960.0), 1.0);
        assert!((end.0 - 30.0).abs() < 0.01 && (end.1 - 960.0).abs() < 0.01);
        assert!((scale - SCALE_END).abs() < 0.01);
        assert!(opacity < 0.01);
    }

    #[test]
    fn the_arc_rises_above_both_ends_midway() {
        let ((_, y), scale, _) = arc_point((800.0, 500.0), (30.0, 960.0), 0.5);
        assert!(y < 500.0, "the icon rises before it falls");
        assert!((scale - 1.8).abs() < 0.01);
    }
}
