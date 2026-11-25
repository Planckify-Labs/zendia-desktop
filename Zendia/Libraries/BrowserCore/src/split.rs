/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Split view geometry and Glance state.
//!
//! A split group shows 2 to 4 tabs in the page card at once. Its panes are laid out from the group's direction and
//! ratios, both owned by the core, so every view (Freya, Headless) places the panes the same way. Rectangles here are
//! fractions of the page card; each view adds its own gaps.

use crate::model::{SplitDirection, TabId};

/// Most panes one split group may hold, as in Zen.
pub const MAX_SPLIT_PANES: usize = 4;

/// Smallest fraction a pane may shrink to when resized.
pub const MIN_PANE_FRACTION: f32 = 0.15;

/// A pane's place in the page card, as fractions of its width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Which way a resize handle moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleAxis {
    /// The handle is a vertical bar between panes side by side; it moves along x.
    Column,
    /// The handle is a horizontal bar between stacked panes; it moves along y.
    Row,
}

/// A draggable boundary between panes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitHandle {
    /// Index of the ratio this handle changes; pass it back to [`resize_ratios`].
    pub index: usize,
    pub axis: HandleAxis,
    /// Position along the axis, as a fraction of the page card.
    pub position: f32,
    /// Where the handle starts and ends across the other axis, as fractions.
    pub span_start: f32,
    pub span_end: f32,
}

/// The ratios a new group of `count` panes starts with.
///
/// Side-by-side and stacked groups keep one share per pane. A grid keeps two: the left column's width and the row
/// boundary inside the columns.
pub fn default_ratios(direction: SplitDirection, count: usize) -> Vec<f32> {
    match direction {
        SplitDirection::Grid if count >= 3 => vec![0.5, 0.5],
        _ => vec![1.0 / count.max(1) as f32; count.max(1)],
    }
}

/// Brings stored ratios back into shape for `count` panes, falling back to the defaults when they do not fit.
pub fn normalized_ratios(direction: SplitDirection, count: usize, ratios: &[f32]) -> Vec<f32> {
    let expected = default_ratios(direction, count);
    if ratios.len() != expected.len() || ratios.iter().any(|ratio| !ratio.is_finite() || *ratio <= 0.0) {
        return expected;
    }
    if matches!(direction, SplitDirection::Grid) && count >= 3 {
        return ratios
            .iter()
            .map(|ratio| ratio.clamp(MIN_PANE_FRACTION, 1.0 - MIN_PANE_FRACTION))
            .collect();
    }
    let total: f32 = ratios.iter().sum();
    ratios.iter().map(|ratio| ratio / total).collect()
}

/// Lays out `count` panes of a group.
///
/// `Horizontal` places panes side by side, `Vertical` stacks them. A three-pane grid puts the first two panes in the
/// left column and the third in the right column at full height, as Zen does; a four-pane grid is two by two. A grid of
/// two panes is laid out side by side.
pub fn pane_rects(direction: SplitDirection, count: usize, ratios: &[f32]) -> Vec<PaneRect> {
    if count == 0 {
        return Vec::new();
    }
    let ratios = normalized_ratios(direction, count, ratios);
    match direction {
        SplitDirection::Grid if count >= 3 => {
            let column = ratios[0];
            let row = ratios[1];
            let mut rects = vec![
                PaneRect {
                    x: 0.0,
                    y: 0.0,
                    width: column,
                    height: row,
                },
                PaneRect {
                    x: 0.0,
                    y: row,
                    width: column,
                    height: 1.0 - row,
                },
            ];
            if count == 3 {
                rects.push(PaneRect {
                    x: column,
                    y: 0.0,
                    width: 1.0 - column,
                    height: 1.0,
                });
            } else {
                rects.push(PaneRect {
                    x: column,
                    y: 0.0,
                    width: 1.0 - column,
                    height: row,
                });
                rects.push(PaneRect {
                    x: column,
                    y: row,
                    width: 1.0 - column,
                    height: 1.0 - row,
                });
            }
            rects
        }
        SplitDirection::Vertical => {
            let mut offset = 0.0;
            ratios
                .iter()
                .map(|share| {
                    let rect = PaneRect {
                        x: 0.0,
                        y: offset,
                        width: 1.0,
                        height: *share,
                    };
                    offset += share;
                    rect
                })
                .collect()
        }
        SplitDirection::Horizontal | SplitDirection::Grid => {
            let mut offset = 0.0;
            ratios
                .iter()
                .map(|share| {
                    let rect = PaneRect {
                        x: offset,
                        y: 0.0,
                        width: *share,
                        height: 1.0,
                    };
                    offset += share;
                    rect
                })
                .collect()
        }
    }
}

/// The resize handles of a group, one per boundary between panes.
pub fn split_handles(direction: SplitDirection, count: usize, ratios: &[f32]) -> Vec<SplitHandle> {
    if count < 2 {
        return Vec::new();
    }
    let ratios = normalized_ratios(direction, count, ratios);
    match direction {
        SplitDirection::Grid if count >= 3 => {
            let column = ratios[0];
            let row = ratios[1];
            let mut handles = vec![
                SplitHandle {
                    index: 0,
                    axis: HandleAxis::Column,
                    position: column,
                    span_start: 0.0,
                    span_end: 1.0,
                },
                SplitHandle {
                    index: 1,
                    axis: HandleAxis::Row,
                    position: row,
                    span_start: 0.0,
                    span_end: column,
                },
            ];
            if count == 4 {
                handles.push(SplitHandle {
                    index: 1,
                    axis: HandleAxis::Row,
                    position: row,
                    span_start: column,
                    span_end: 1.0,
                });
            }
            handles
        }
        _ => {
            let axis = if matches!(direction, SplitDirection::Vertical) {
                HandleAxis::Row
            } else {
                HandleAxis::Column
            };
            let mut offset = 0.0;
            ratios[..ratios.len() - 1]
                .iter()
                .enumerate()
                .map(|(index, share)| {
                    offset += share;
                    SplitHandle {
                        index,
                        axis,
                        position: offset,
                        span_start: 0.0,
                        span_end: 1.0,
                    }
                })
                .collect()
        }
    }
}

/// Moves one handle to `position` (a fraction of the page card) and returns the new ratios.
///
/// For side-by-side and stacked groups only the two panes beside the handle change size. No pane shrinks below
/// [`MIN_PANE_FRACTION`].
pub fn resize_ratios(direction: SplitDirection, count: usize, ratios: &[f32], index: usize, position: f32) -> Vec<f32> {
    let mut ratios = normalized_ratios(direction, count, ratios);
    if !position.is_finite() {
        return ratios;
    }
    if matches!(direction, SplitDirection::Grid) && count >= 3 {
        if let Some(ratio) = ratios.get_mut(index) {
            *ratio = position.clamp(MIN_PANE_FRACTION, 1.0 - MIN_PANE_FRACTION);
        }
        return ratios;
    }
    if index + 1 >= ratios.len() {
        return ratios;
    }
    let before: f32 = ratios[..index].iter().sum();
    let pair = ratios[index] + ratios[index + 1];
    let first = (position - before).clamp(
        MIN_PANE_FRACTION.min(pair / 2.0),
        pair - MIN_PANE_FRACTION.min(pair / 2.0),
    );
    ratios[index] = first;
    ratios[index + 1] = pair - first;
    ratios
}

/// Which side of a pane a dragged tab is dropped on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DropZone {
    Left,
    Right,
    Top,
    Bottom,
}

impl DropZone {
    /// The split direction a drop on this side creates.
    pub fn direction(self) -> SplitDirection {
        match self {
            Self::Left | Self::Right => SplitDirection::Horizontal,
            Self::Top | Self::Bottom => SplitDirection::Vertical,
        }
    }

    /// Whether the dropped tab goes before the existing pane.
    pub fn places_first(self) -> bool {
        matches!(self, Self::Left | Self::Top)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "top" => Some(Self::Top),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }
}

/// Picks the drop zone for a point inside the page card, given as fractions.
///
/// The card is cut along its diagonals, so the nearest edge wins.
pub fn drop_zone_at(x: f32, y: f32) -> DropZone {
    let x = x.clamp(0.0, 1.0);
    let y = y.clamp(0.0, 1.0);
    let to_left = x;
    let to_right = 1.0 - x;
    let to_top = y;
    let to_bottom = 1.0 - y;
    let nearest = to_left.min(to_right).min(to_top).min(to_bottom);
    if nearest == to_left {
        DropZone::Left
    } else if nearest == to_right {
        DropZone::Right
    } else if nearest == to_top {
        DropZone::Top
    } else {
        DropZone::Bottom
    }
}

pub fn direction_as_str(direction: SplitDirection) -> &'static str {
    match direction {
        SplitDirection::Horizontal => "horizontal",
        SplitDirection::Vertical => "vertical",
        SplitDirection::Grid => "grid",
    }
}

pub fn parse_direction(value: &str) -> Option<SplitDirection> {
    match value {
        "horizontal" | "vsep" | "columns" => Some(SplitDirection::Horizontal),
        "vertical" | "hsep" | "rows" => Some(SplitDirection::Vertical),
        "grid" => Some(SplitDirection::Grid),
        _ => None,
    }
}

/// A page previewed in a card over the current page.
///
/// The Glance page is a real tab with its own Hub view. It is left out of the sidebar's lists and drawn next to its
/// parent's row instead, until it is closed, expanded into a normal tab, or split with its parent.
#[derive(Clone, Debug, PartialEq)]
pub struct Glance {
    pub tab_id: TabId,
    pub parent_tab_id: Option<TabId>,
    /// Where the open animation starts, as fractions of the page card (the link that was clicked).
    pub origin: Option<(f32, f32)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn side_by_side_panes_share_the_width() {
        let rects = pane_rects(SplitDirection::Horizontal, 2, &[0.5, 0.5]);
        assert_eq!(rects.len(), 2);
        assert!(approx(rects[0].width, 0.5) && approx(rects[1].x, 0.5) && approx(rects[1].height, 1.0));
    }

    #[test]
    fn stacked_panes_share_the_height() {
        let rects = pane_rects(SplitDirection::Vertical, 3, &[]);
        assert_eq!(rects.len(), 3);
        assert!(approx(rects[2].y, 2.0 / 3.0) && approx(rects[2].width, 1.0));
    }

    #[test]
    fn three_pane_grid_has_a_full_height_right_column() {
        let rects = pane_rects(SplitDirection::Grid, 3, &[0.5, 0.5]);
        assert_eq!(rects.len(), 3);
        assert!(approx(rects[0].height, 0.5) && approx(rects[1].y, 0.5));
        assert!(approx(rects[2].x, 0.5) && approx(rects[2].height, 1.0));
    }

    #[test]
    fn four_pane_grid_is_two_by_two() {
        let rects = pane_rects(SplitDirection::Grid, 4, &[0.4, 0.6]);
        assert!(approx(rects[3].x, 0.4) && approx(rects[3].y, 0.6) && approx(rects[3].width, 0.6));
        assert_eq!(split_handles(SplitDirection::Grid, 4, &[0.4, 0.6]).len(), 3);
    }

    #[test]
    fn bad_ratios_fall_back_to_equal_shares() {
        assert_eq!(normalized_ratios(SplitDirection::Horizontal, 2, &[1.0]), vec![0.5, 0.5]);
        assert_eq!(
            normalized_ratios(SplitDirection::Horizontal, 2, &[f32::NAN, 1.0]),
            vec![0.5, 0.5]
        );
        let scaled = normalized_ratios(SplitDirection::Horizontal, 2, &[2.0, 6.0]);
        assert!(approx(scaled[0], 0.25) && approx(scaled[1], 0.75));
    }

    #[test]
    fn resizing_moves_only_the_neighbouring_panes() {
        let ratios = resize_ratios(SplitDirection::Horizontal, 3, &[], 1, 0.8);
        assert!(approx(ratios[0], 1.0 / 3.0));
        assert!(approx(ratios[1], 0.8 - 1.0 / 3.0));
        assert!(approx(ratios.iter().sum::<f32>(), 1.0));
    }

    #[test]
    fn resizing_keeps_a_minimum_pane_size() {
        let ratios = resize_ratios(SplitDirection::Horizontal, 2, &[0.5, 0.5], 0, 0.99);
        assert!(approx(ratios[1], MIN_PANE_FRACTION));
        let grid = resize_ratios(SplitDirection::Grid, 3, &[0.5, 0.5], 0, 0.01);
        assert!(approx(grid[0], MIN_PANE_FRACTION));
    }

    #[test]
    fn handles_sit_between_panes() {
        let handles = split_handles(SplitDirection::Horizontal, 2, &[0.3, 0.7]);
        assert_eq!(handles.len(), 1);
        assert!(approx(handles[0].position, 0.3));
        assert_eq!(handles[0].axis, HandleAxis::Column);
        let grid = split_handles(SplitDirection::Grid, 3, &[0.5, 0.5]);
        assert_eq!(grid[1].axis, HandleAxis::Row);
        assert!(approx(grid[1].span_end, 0.5));
    }

    #[test]
    fn drop_zone_is_the_nearest_edge() {
        assert_eq!(drop_zone_at(0.05, 0.5), DropZone::Left);
        assert_eq!(drop_zone_at(0.95, 0.4), DropZone::Right);
        assert_eq!(drop_zone_at(0.5, 0.1), DropZone::Top);
        assert_eq!(drop_zone_at(0.5, 0.9), DropZone::Bottom);
        assert_eq!(DropZone::Top.direction(), SplitDirection::Vertical);
    }

    #[test]
    fn directions_parse_zen_names() {
        assert_eq!(parse_direction("vsep"), Some(SplitDirection::Horizontal));
        assert_eq!(parse_direction("hsep"), Some(SplitDirection::Vertical));
        assert_eq!(parse_direction("grid"), Some(SplitDirection::Grid));
        assert_eq!(parse_direction("diagonal"), None);
    }
}
