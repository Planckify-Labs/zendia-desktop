/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Region capture geometry and result model for Zendia's page region picker.
//!
//! Pure coordinate math and capture payloads live here so the Freya overlay, headless runners,
//! and future agent services share one contract.

use crate::model::TabId;

/// Minimum width and height (in logical viewport pixels) for a dragged region to count as a box.
pub const REGION_MIN_SIDE: f32 = 8.0;

/// Instruction banner shown while the region picker is idle over the page.
pub const REGION_PICKER_HINT: &str = "Drag over what to ask about · Enter takes the whole view · Esc cancels";

/// A rectangle in logical page coordinates (CSS pixels relative to the page viewport).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RegionRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x: if x.is_finite() { x.max(0.0) } else { 0.0 },
            y: if y.is_finite() { y.max(0.0) } else { 0.0 },
            width: if width.is_finite() { width.max(0.0) } else { 0.0 },
            height: if height.is_finite() { height.max(0.0) } else { 0.0 },
        }
    }

    /// Builds a clamped rectangle from two drag endpoints inside a viewport of `(bounds_w, bounds_h)`.
    pub fn from_drag_points(start: (f32, f32), end: (f32, f32), bounds_w: f32, bounds_h: f32) -> Self {
        let max_w = bounds_w.max(0.0);
        let max_h = bounds_h.max(0.0);
        let sx = start.0.clamp(0.0, max_w);
        let sy = start.1.clamp(0.0, max_h);
        let ex = end.0.clamp(0.0, max_w);
        let ey = end.1.clamp(0.0, max_h);
        Self {
            x: sx.min(ex),
            y: sy.min(ey),
            width: (ex - sx).abs(),
            height: (ey - sy).abs(),
        }
    }

    /// Converts a viewport-pixel rectangle into page CSS pixels using the tab's zoom factor.
    pub fn to_page_coords(self, zoom: f32) -> Self {
        let factor = if zoom.is_finite() && zoom > 0.05 { zoom } else { 1.0 };
        Self {
            x: self.x / factor,
            y: self.y / factor,
            width: self.width / factor,
            height: self.height / factor,
        }
    }

    /// Returns whether both sides meet [`REGION_MIN_SIDE`].
    pub fn is_valid_drag(self) -> bool {
        self.width >= REGION_MIN_SIDE && self.height >= REGION_MIN_SIDE
    }

    /// Builds a whole-page rectangle in page CSS pixels for a viewport of `(bounds_w, bounds_h)`.
    pub fn whole_page(bounds_w: f32, bounds_h: f32, zoom: f32) -> Self {
        Self::new(0.0, 0.0, bounds_w, bounds_h).to_page_coords(zoom)
    }
}

/// Capture result published by `region.capture` for agents and web chat callers.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionCapture {
    pub tab_id: Option<TabId>,
    /// Chosen rectangle in page CSS pixels, when known.
    pub rect: Option<RegionRect>,
    /// True when the user confirmed the whole viewport (e.g. pressing Enter without dragging a box).
    pub whole_page: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_points_normalize_and_clamp_to_viewport_bounds() {
        let rect = RegionRect::from_drag_points((320.0, 260.0), (100.0, 80.0), 800.0, 600.0);
        assert_eq!(rect, RegionRect::new(100.0, 80.0, 220.0, 180.0));
        assert!(rect.is_valid_drag());

        let clamped = RegionRect::from_drag_points((-50.0, 50.0), (950.0, 700.0), 800.0, 600.0);
        assert_eq!(clamped, RegionRect::new(0.0, 50.0, 800.0, 550.0));
    }

    #[test]
    fn tiny_drags_fail_min_side_and_zoom_scales_to_page_coords() {
        let tiny = RegionRect::from_drag_points((10.0, 10.0), (15.0, 40.0), 800.0, 600.0);
        assert!(!tiny.is_valid_drag());

        let scaled = RegionRect::new(100.0, 200.0, 400.0, 300.0).to_page_coords(2.0);
        assert_eq!(scaled, RegionRect::new(50.0, 100.0, 200.0, 150.0));

        let whole = RegionRect::whole_page(1200.0, 800.0, 1.5);
        assert_eq!(whole, RegionRect::new(0.0, 0.0, 800.0, 800.0 / 1.5));
    }
}
