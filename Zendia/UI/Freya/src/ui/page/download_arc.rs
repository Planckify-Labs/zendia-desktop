/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The download animation (Zen's `ZenDownloadAnimation`): a download icon flies in a smooth 1000 ms arc from the
//! user's last click position into the library button, tilting along its trajectory, growing as it rises, shrinking
//! as it lands, and bouncing the library button on arrival. When the library button is hidden (e.g. compact mode),
//! plays Zen's fallback corner box animation.

use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ThemeRole, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::model::SidebarSide;

/// How long one flight takes.
pub const ARC_DURATION: Duration = Duration::from_millis(1000);
/// How long the fallback corner box animation stays visible when the library button is hidden.
const BOX_DURATION: Duration = Duration::from_millis(1200);
/// Outer diameter of the flying icon's ringed circle at scale 1 (matching Zen's 30px ring).
const ARC_ICON: f32 = 30.0;
const ARC_HEIGHT_RATIO: f32 = 2.0;
const MAX_ARC_HEIGHT: f32 = 1200.0;
const SCALE_END: f32 = 0.45;

/// A download icon in flight, in window coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcFlight {
    pub id: u64,
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub viewport_height: f32,
}

/// View state for download flights, shared by the library button and the arc layer.
#[derive(Clone, Copy, PartialEq)]
pub struct DownloadArcState {
    /// Center of the library button in window coordinates, when it is on screen.
    pub anchor: State<Option<(f32, f32)>>,
    pub flights: State<Vec<ArcFlight>>,
    /// Fallback corner box animations launched when the library button is hidden.
    pub box_flights: State<Vec<u64>>,
    /// Monotonic counter bumped whenever a flight lands on the library button (drives its landing bounce).
    pub landing_seq: State<u64>,
}

impl DownloadArcState {
    /// Starts a flight from `start` to the library button (or triggers the corner box fallback when hidden).
    /// Bumps `landing_seq` when the arc lands on the library button and cleans itself up afterwards.
    pub fn launch(&self, start: (f32, f32), fallback_end: (f32, f32)) {
        let maybe_anchor = *self.anchor.peek();
        let viewport_height = Platform::get()
            .current_window()
            .root_size
            .peek()
            .height
            .max(fallback_end.1 + 24.0)
            .max(360.0);
        let end = maybe_anchor.unwrap_or(fallback_end);
        let mut flights = self.flights;
        let mut box_flights = self.box_flights;
        let mut landing_seq = self.landing_seq;
        let id = flights
            .peek()
            .iter()
            .map(|flight| flight.id)
            .chain(box_flights.peek().iter().copied())
            .max()
            .unwrap_or(0)
            + 1;

        if maybe_anchor.is_none() {
            box_flights.write().push(id);
            spawn(async move {
                timer(BOX_DURATION + Duration::from_millis(60)).await;
                box_flights.write().retain(|box_id| *box_id != id);
            });
        }

        flights.write().push(ArcFlight {
            id,
            start,
            end,
            viewport_height,
        });
        spawn(async move {
            timer(ARC_DURATION).await;
            if maybe_anchor.is_some() {
                let next = landing_seq.peek().wrapping_add(1);
                landing_seq.set(next);
            }
            timer(Duration::from_millis(60)).await;
            flights.write().retain(|flight| flight.id != id);
        });
    }
}

pub fn use_provide_download_arc() -> DownloadArcState {
    let anchor = use_state(|| None::<(f32, f32)>);
    let flights = use_state(Vec::<ArcFlight>::new);
    let box_flights = use_state(Vec::<u64>::new);
    let landing_seq = use_state(|| 0u64);
    use_provide_context(|| DownloadArcState {
        anchor,
        flights,
        box_flights,
        landing_seq,
    })
}

pub fn try_use_download_arc() -> Option<DownloadArcState> {
    try_consume_context::<DownloadArcState>()
}

fn ease_in_out_quad(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        -1.0 + (4.0 - 2.0 * t) * t
    }
}

/// Computes `(position, scale, opacity, rotation_deg)` at linear `progress` in `0.0..=1.0`, selecting an upward or
/// downward arc based on available vertical headroom and rotating the icon gently along its flight tangent.
pub fn arc_point_in_viewport(
    start: (f32, f32),
    end: (f32, f32),
    viewport_height: f32,
    progress: f32,
) -> ((f32, f32), f32, f32, f32) {
    let t = progress.clamp(0.0, 1.0);
    let eased = ease_in_out_quad(t);
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let distance = (dx * dx + dy * dy).sqrt();

    let available_top = start.1.min(end.1).max(0.0);
    let available_bottom = (viewport_height - start.1.max(end.1)).max(0.0);
    let arc_downward = available_bottom > available_top;
    let available_space = if arc_downward { available_bottom } else { available_top };
    let arc_sign = if arc_downward { 1.0 } else { -1.0 };

    let height = (distance * ARC_HEIGHT_RATIO)
        .min(MAX_ARC_HEIGHT)
        .min(available_space * 0.85);
    let offset_y = |e: f32| arc_sign * height * (1.0 - (2.0 * e - 1.0).powi(2));
    let pos_at = |e: f32| (start.0 + dx * e, start.1 + dy * e + offset_y(e));

    let position = pos_at(eased);

    // Velocity tangent for subtle flight rotation (clamped so the download arrow stays upright and readable).
    let eps = 0.01_f32;
    let p0 = pos_at((eased - eps).max(0.0));
    let p1 = pos_at((eased + eps).min(1.0));
    let (vx, vy) = (p1.0 - p0.0, p1.1 - p0.1);
    let manhattan = vx.abs() + vy.abs();
    let rotation_deg = if manhattan > 1e-3 {
        let tilt = (vy / manhattan) * 90.0;
        (tilt * 0.30).clamp(-28.0, 28.0)
    } else {
        0.0
    };

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
    (position, scale, opacity.clamp(0.0, 1.0), rotation_deg)
}

/// Where the icon is at `progress` (0 to 1): position, scale and opacity, assuming a standard 1000 px viewport.
pub fn arc_point(start: (f32, f32), end: (f32, f32), progress: f32) -> ((f32, f32), f32, f32) {
    let (pos, scale, opacity, _) = arc_point_in_viewport(start, end, 1000.0, progress);
    (pos, scale, opacity)
}

/// Draws every flight (and any corner fallback box animations) over the whole window.
#[derive(Clone, PartialEq)]
pub struct DownloadArcLayer {
    /// Pins every flight at this progress (for gallery states).
    pub pinned_progress: Option<f32>,
}

impl Component for DownloadArcLayer {
    fn render(&self) -> impl IntoElement {
        let state = try_use_download_arc();
        let flights = state.map(|state| state.flights.read().clone()).unwrap_or_default();
        let box_flights = state.map(|state| state.box_flights.read().clone()).unwrap_or_default();
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
            .children(box_flights.into_iter().map(|id| FallbackBoxView { id }.into_element()))
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
        // Linear timeline so `arc_point_in_viewport`'s single easeInOutQuad drives the full 1000 ms flight (DA-02).
        let flight_anim = use_animation(move |conf| {
            conf.on_creation(OnCreation::Run);
            AnimNum::new(0.0, 1.0).duration(ARC_DURATION).function(Function::Linear)
        });
        let progress = self.pinned_progress.unwrap_or_else(|| flight_anim.get().value());
        let ((x, y), scale, opacity, rotation_deg) = arc_point_in_viewport(
            self.flight.start,
            self.flight.end,
            self.flight.viewport_height,
            progress,
        );

        // Ringed circle badge matching Zen's `zen-download-arc-animation.css`: outer ring + 2px padding + inner circle.
        let outer_ring_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.28).to_color();
        let inner_bg = theme.color(ThemeRole::SurfaceRaised);

        rect()
            .position(Position::new_global().left(x - ARC_ICON / 2.0).top(y - ARC_ICON / 2.0))
            .width(Size::px(ARC_ICON))
            .height(Size::px(ARC_ICON))
            .padding(Gaps::new_all(2.0))
            .corner_radius(CornerRadius::new_all(ARC_ICON))
            .scale((scale, scale))
            .rotate(rotation_deg)
            .opacity(opacity)
            .background(outer_ring_bg)
            .shadow(Shadow::new().y(3.0).blur(10.0).color(Color::from_argb(80, 0, 0, 0)))
            .center()
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .corner_radius(CornerRadius::new_all(ARC_ICON))
                    .background(inner_bg)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.rgba(ThemeRole::Accent).with_alpha(0.55).to_color()),
                    )
                    .center()
                    .child(Icon::new("downloads").size(15.0).role(ThemeRole::Accent)),
            )
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.flight.id)
    }
}

/// Fallback 42x42 rounded corner box animation when the library button is hidden (e.g. compact mode, `DA-04`).
#[derive(Clone, PartialEq)]
struct FallbackBoxView {
    id: u64,
}

impl Component for FallbackBoxView {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let sidebar_side = core.model().read().layout.sidebar_side;
        let root_size = *Platform::get().current_window().root_size.read();
        let anim = use_animation(move |conf| {
            conf.on_creation(OnCreation::Run);
            AnimNum::new(0.0, 1.0).duration(BOX_DURATION).function(Function::Linear)
        });
        let t = anim.get().value().clamp(0.0, 1.0);
        // Slide in -50 -> 34 -> 24, hold, then slide out.
        let (slide_px, scale, opacity) = if t < 0.25 {
            let p = t / 0.25;
            (-50.0 + 84.0 * p, 0.8 + 0.3 * p, p)
        } else if t < 0.40 {
            let p = (t - 0.25) / 0.15;
            (34.0 - 10.0 * p, 1.1 - 0.1 * p, 1.0)
        } else if t < 0.80 {
            (24.0, 1.0, 1.0)
        } else {
            let p = (t - 0.80) / 0.20;
            (24.0 - 74.0 * p, 1.0 - 0.2 * p, 1.0 - p)
        };
        let box_size = 42.0;
        let x = if sidebar_side == SidebarSide::Right {
            root_size.width - box_size - slide_px
        } else {
            slide_px
        };
        let y = (root_size.height - box_size - 24.0).max(24.0);

        rect()
            .position(Position::new_global().left(x).top(y))
            .width(Size::px(box_size))
            .height(Size::px(box_size))
            .scale(scale)
            .opacity(opacity.clamp(0.0, 1.0))
            .corner_radius(CornerRadius::new_all(12.0))
            .background(theme.color(ThemeRole::SurfaceRaised))
            .border(
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::Accent)),
            )
            .shadow(Shadow::new().y(4.0).blur(14.0).color(Color::from_argb(85, 0, 0, 0)))
            .center()
            .child(Icon::new("downloads").size(20.0).role(ThemeRole::Accent))
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&format!("dl-box-{}", self.id))
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

    #[test]
    fn near_top_edge_falls_back_to_downward_arc() {
        let ((_, y), _, _, rot) = arc_point_in_viewport((800.0, 40.0), (100.0, 60.0), 1000.0, 0.5);
        assert!(
            y > 60.0,
            "near the top edge the arc bows downward instead of clipping off-screen"
        );
        assert!(rot.abs() <= 28.0);
    }
}
