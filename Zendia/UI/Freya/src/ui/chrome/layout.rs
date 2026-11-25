/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Pure layout metrics, toolbar contribution placement resolution, resize math, and window backdrop gradients.

use crate::ui::theme::{ResolvedScheme, Rgba};
use freya::prelude::{Color, LinearGradient};
use std::time::Duration;
use zendia_browser_core::contributions::{ToolbarArea, ToolbarItem};
use zendia_browser_core::model::{LayoutSettings, SidebarSide, Space};
use zendia_browser_core::space_appearance::{SpaceAppearance, compute_gradient};

/// Inset separation around the rounded page card (`zen.theme.content-element-separation`).
pub const ELEMENT_SEPARATION: f32 = 8.0;
/// Full-width top toolbar height in multiple-toolbar and collapsed-sidebar layouts (`--zen-toolbar-height`).
pub const TOP_TOOLBAR_HEIGHT: f32 = 40.0;
/// Top action row height inside the single-toolbar sidebar.
pub const SIDEBAR_TOP_ROW_HEIGHT: f32 = 34.0;
/// Horizontal and bottom padding inside the expanded sidebar (`--zen-toolbox-padding`).
pub const SIDEBAR_PADDING_EXPANDED: f32 = 6.0;
/// Padding inside the collapsed icon-only sidebar.
pub const SIDEBAR_PADDING_COLLAPSED: f32 = 6.0;
/// Address bar height inside the single-toolbar sidebar.
pub const URL_BAR_HEIGHT_SINGLE: f32 = 34.0;
/// Address bar height inside the top toolbar (multiple and collapsed layouts).
pub const URL_BAR_HEIGHT_TOP: f32 = 30.0;
/// Corner radius of the inset page card.
pub const PAGE_CARD_RADIUS: f32 = 8.0;
/// Height of a vertical tab row in the sidebar.
pub const TAB_ROW_HEIGHT: f32 = 34.0;
/// Corner radius of a vertical tab row pill.
pub const TAB_ROW_RADIUS: f32 = 8.0;
/// Interactive drag-handle width between the sidebar and the page card.
pub const RESIZE_HANDLE_WIDTH: f32 = 8.0;
/// Expand/collapse animation duration for the vertical sidebar.
pub const SIDEBAR_EXPAND_DURATION: Duration = Duration::from_millis(220);
/// Cross-fade duration when switching between per-space backdrop gradients.
pub const SPACE_GRADIENT_CROSSFADE: Duration = Duration::from_millis(260);

/// Toolbar contributions partitioned by resolved [`ToolbarArea`] for the active [`LayoutSettings`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolbarSlots {
    pub top_bar_start: Vec<ToolbarItem>,
    pub top_bar_center: Vec<ToolbarItem>,
    pub top_bar_end: Vec<ToolbarItem>,
    pub sidebar_top_start: Vec<ToolbarItem>,
    pub sidebar_top_end: Vec<ToolbarItem>,
    pub sidebar_url_bar: Vec<ToolbarItem>,
    pub sidebar_bottom: Vec<ToolbarItem>,
}

impl ToolbarSlots {
    /// Partitions visible toolbar contributions into areas according to `layout` and sorts each bucket by `(order, id)`.
    pub fn resolve(items: &[ToolbarItem], layout: &LayoutSettings) -> Self {
        let mut slots = Self::default();
        for item in items.iter().filter(|item| item.visible) {
            let Some(area) = layout.resolve_toolbar_area(item.placement_rules) else {
                continue;
            };
            let bucket = match area {
                ToolbarArea::TopBarStart => &mut slots.top_bar_start,
                ToolbarArea::TopBarCenter => &mut slots.top_bar_center,
                ToolbarArea::TopBarEnd => &mut slots.top_bar_end,
                ToolbarArea::SidebarTopStart => &mut slots.sidebar_top_start,
                ToolbarArea::SidebarTopEnd => &mut slots.sidebar_top_end,
                ToolbarArea::SidebarUrlBar => &mut slots.sidebar_url_bar,
                ToolbarArea::SidebarBottom => &mut slots.sidebar_bottom,
            };
            bucket.push(item.clone());
        }

        for bucket in [
            &mut slots.top_bar_start,
            &mut slots.top_bar_center,
            &mut slots.top_bar_end,
            &mut slots.sidebar_top_start,
            &mut slots.sidebar_top_end,
            &mut slots.sidebar_url_bar,
            &mut slots.sidebar_bottom,
        ] {
            bucket.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
        }

        slots
    }

    /// Orders sidebar bottom items for the given [`SidebarSide`], mirroring when the sidebar is on the right in
    /// single-toolbar mode (matching Zen's foot row).
    pub fn sidebar_bottom_for_side(&self, side: SidebarSide) -> Vec<ToolbarItem> {
        let mut items = self.sidebar_bottom.clone();
        if side == SidebarSide::Right {
            items.reverse();
        }
        items
    }
}

/// Returns `(top, right, bottom, left)` outer margins around the rounded page card.
///
/// The edge adjacent to the sidebar gets `0.0` because the resize handle / gap occupies [`RESIZE_HANDLE_WIDTH`].
pub fn page_card_margins(layout: &LayoutSettings) -> (f32, f32, f32, f32) {
    let top = if layout.has_top_toolbar() {
        0.0
    } else {
        ELEMENT_SEPARATION
    };
    let bottom = ELEMENT_SEPARATION;
    match layout.sidebar_side {
        SidebarSide::Left => (top, ELEMENT_SEPARATION, bottom, 0.0),
        SidebarSide::Right => (top, 0.0, bottom, ELEMENT_SEPARATION),
    }
}

/// Whether window controls (min / max / close) are drawn at the trailing end of the top toolbar.
pub fn show_window_controls_in_top_bar(layout: &LayoutSettings) -> bool {
    layout.has_top_toolbar()
}

/// Whether window controls are drawn at the trailing end of the sidebar top row (single-toolbar with right sidebar).
pub fn show_window_controls_in_sidebar_top(layout: &LayoutSettings) -> bool {
    !layout.has_top_toolbar() && layout.sidebar_side == SidebarSide::Right
}

/// Computes the clamped sidebar width during a horizontal resize drag.
pub fn compute_resized_sidebar_width(start_width: f32, start_x: f32, current_x: f32, side: SidebarSide) -> f32 {
    let delta = match side {
        SidebarSide::Left => current_x - start_x,
        SidebarSide::Right => start_x - current_x,
    };
    LayoutSettings::clamp_sidebar_width(start_width + delta)
}

/// Resolved linear-gradient endpoints for a space's window backdrop, interpolated during space cross-fades.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackdropEndpoints {
    pub start: Rgba,
    pub end: Rgba,
    pub angle_deg: f32,
}

impl BackdropEndpoints {
    /// Linearly interpolates between `self` and `other` at `t` in `0.0..=1.0`.
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let clamped = t.clamp(0.0, 1.0);
        Self {
            start: self.start.mix(other.start, clamped),
            end: self.end.mix(other.end, clamped),
            angle_deg: self.angle_deg + (other.angle_deg - self.angle_deg) * clamped,
        }
    }

    pub fn to_gradient(self) -> LinearGradient {
        LinearGradient::new()
            .angle(self.angle_deg)
            .stop((Color::from(self.start), 0.0))
            .stop((Color::from(self.end), 100.0))
    }
}

/// Computes the resolved gradient endpoints for `space` in `scheme`, matching Zen's per-space backdrop tint.
pub fn window_backdrop_endpoints(space: Option<&Space>, scheme: ResolvedScheme) -> BackdropEndpoints {
    let (base_start, base_end, tint_weight, pastel_lift) = match scheme {
        ResolvedScheme::Dark => (Rgba::rgb(20, 21, 28), Rgba::rgb(16, 17, 23), 0.44, 0.0),
        ResolvedScheme::Light => (Rgba::rgb(242, 243, 248), Rgba::rgb(236, 238, 245), 0.52, 0.58),
    };

    let (start, end, angle_deg) = match space {
        Some(space) if !space.gradient.stops.is_empty() => {
            let first = space.gradient.stops[0];
            let last = *space.gradient.stops.last().unwrap_or(&first);
            let mut tint_a = Rgba::rgb(first.0, first.1, first.2);
            let mut tint_b = Rgba::rgb(last.0, last.1, last.2);
            if pastel_lift > 0.0 {
                let white = Rgba::rgb(255, 255, 255);
                tint_a = tint_a.mix(white, pastel_lift);
                tint_b = tint_b.mix(white, pastel_lift);
            }
            let weight = (tint_weight * space.gradient.opacity.clamp(0.0, 1.0)).clamp(0.0, 0.60);
            (
                base_start.mix(tint_a, weight),
                base_end.mix(tint_b, weight),
                space.gradient.angle_deg,
            )
        }
        _ => (base_start, base_end, 135.0),
    };

    BackdropEndpoints { start, end, angle_deg }
}

/// Builds the per-space backdrop gradient painted behind the sidebar, toolbar, and inset page card.
#[allow(dead_code)]
pub fn window_backdrop_gradient(space: Option<&Space>, scheme: ResolvedScheme) -> LinearGradient {
    window_backdrop_endpoints(space, scheme).to_gradient()
}

/// Computes resolved backdrop endpoints reading `appearance` when the space has custom gradient generator dots,
/// falling back to Phase 2's [`window_backdrop_endpoints`] otherwise so default spaces remain unchanged.
pub fn window_backdrop_endpoints_for_appearance(
    space: Option<&Space>,
    appearance: Option<&SpaceAppearance>,
    scheme: ResolvedScheme,
) -> BackdropEndpoints {
    if let Some(app) = appearance
        && !app.dots.is_empty()
    {
        let resolved = compute_gradient(app, scheme == ResolvedScheme::Dark);
        let first = resolved.stops.first().copied().unwrap_or((19, 19, 19));
        let last = resolved.stops.last().copied().unwrap_or(first);
        return BackdropEndpoints {
            start: Rgba::rgb(first.0, first.1, first.2),
            end: Rgba::rgb(last.0, last.1, last.2),
            angle_deg: resolved.angle_deg,
        };
    }
    window_backdrop_endpoints(space, scheme)
}

/// Neutral surface tints for chrome elements (URL bar, selected tab pill, hover fills, dividers) matching Zen's
/// dark/light look.
#[derive(Clone, Copy, Debug)]
pub struct ChromePalette {
    pub text: Color,
    pub text_muted: Color,
    pub surface_pill: Color,
    pub surface_pill_hover: Color,
    pub urlbar_bg: Color,
    pub urlbar_focus_bg: Color,
    pub urlbar_border: Color,
    pub tab_selected_bg: Color,
    pub tab_hover_bg: Color,
    pub divider: Color,
    pub page_card_border: Color,
    pub window_close_hover: Color,
}

impl ChromePalette {
    pub fn for_scheme(scheme: ResolvedScheme) -> Self {
        match scheme {
            ResolvedScheme::Dark => Self {
                text: Rgba::from_rgb_alpha(255, 255, 255, 0.88).to_color(),
                text_muted: Rgba::from_rgb_alpha(255, 255, 255, 0.56).to_color(),
                surface_pill: Rgba::from_rgb_alpha(255, 255, 255, 0.11).to_color(),
                surface_pill_hover: Rgba::from_rgb_alpha(255, 255, 255, 0.18).to_color(),
                urlbar_bg: Rgba::from_rgb_alpha(255, 255, 255, 0.11).to_color(),
                urlbar_focus_bg: Rgba::from_rgb_alpha(255, 255, 255, 0.16).to_color(),
                urlbar_border: Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
                tab_selected_bg: Rgba::from_rgb_alpha(255, 255, 255, 0.14).to_color(),
                tab_hover_bg: Rgba::from_rgb_alpha(255, 255, 255, 0.07).to_color(),
                divider: Rgba::from_rgb_alpha(255, 255, 255, 0.09).to_color(),
                page_card_border: Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
                window_close_hover: Rgba::from_rgb_alpha(232, 68, 82, 0.85).to_color(),
            },
            ResolvedScheme::Light => Self {
                text: Rgba::from_rgb_alpha(20, 22, 32, 0.88).to_color(),
                text_muted: Rgba::from_rgb_alpha(40, 44, 60, 0.58).to_color(),
                surface_pill: Rgba::from_rgb_alpha(0, 0, 0, 0.07).to_color(),
                surface_pill_hover: Rgba::from_rgb_alpha(0, 0, 0, 0.12).to_color(),
                urlbar_bg: Rgba::from_rgb_alpha(0, 0, 0, 0.075).to_color(),
                urlbar_focus_bg: Rgba::from_rgb_alpha(255, 255, 255, 0.92).to_color(),
                urlbar_border: Rgba::from_rgb_alpha(0, 0, 0, 0.06).to_color(),
                tab_selected_bg: Rgba::from_rgb_alpha(255, 255, 255, 0.90).to_color(),
                tab_hover_bg: Rgba::from_rgb_alpha(0, 0, 0, 0.055).to_color(),
                divider: Rgba::from_rgb_alpha(0, 0, 0, 0.09).to_color(),
                page_card_border: Rgba::from_rgb_alpha(0, 0, 0, 0.09).to_color(),
                window_close_hover: Rgba::from_rgb_alpha(220, 54, 68, 0.85).to_color(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zendia_browser_core::BrowserCore;
    use zendia_browser_core::model::{SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH, ToolbarLayout};
    use zendia_browser_core::stub::StubBackend;

    #[test]
    fn toolbar_slots_route_builtin_items_across_all_three_layouts() {
        let core = StubBackend::new();
        let items = core.model().contributions.toolbar_items;

        let single = LayoutSettings {
            toolbar_layout: ToolbarLayout::Single,
            sidebar_expanded: true,
            ..LayoutSettings::default()
        };
        let single_slots = ToolbarSlots::resolve(&items, &single);
        fn ids(bucket: &[ToolbarItem]) -> Vec<&str> {
            bucket.iter().map(|i| i.id.as_str()).collect()
        }

        assert!(single_slots.top_bar_start.is_empty());
        assert!(single_slots.top_bar_center.is_empty());
        assert!(single_slots.top_bar_end.is_empty());
        assert_eq!(
            ids(&single_slots.sidebar_top_start),
            vec!["builtin.app_menu", "builtin.sidebar_toggle"]
        );
        assert_eq!(
            ids(&single_slots.sidebar_top_end),
            vec!["builtin.back", "builtin.forward", "builtin.reload"]
        );
        assert_eq!(ids(&single_slots.sidebar_url_bar), vec!["builtin.url_bar"]);
        assert_eq!(
            ids(&single_slots.sidebar_bottom),
            vec![
                "builtin.library",
                "zendia.agents",
                "zendia.chat",
                "builtin.extensions",
                "builtin.new_tab"
            ]
        );
        assert_eq!(
            ids(&single_slots.sidebar_bottom_for_side(SidebarSide::Right)),
            vec![
                "builtin.new_tab",
                "builtin.extensions",
                "zendia.chat",
                "zendia.agents",
                "builtin.library"
            ]
        );

        let multiple = LayoutSettings {
            toolbar_layout: ToolbarLayout::Multiple,
            sidebar_expanded: true,
            ..LayoutSettings::default()
        };
        let multi_slots = ToolbarSlots::resolve(&items, &multiple);
        assert_eq!(
            ids(&multi_slots.top_bar_start),
            vec![
                "builtin.sidebar_toggle",
                "builtin.back",
                "builtin.forward",
                "builtin.reload"
            ]
        );
        assert_eq!(ids(&multi_slots.top_bar_center), vec!["builtin.url_bar"]);
        assert_eq!(
            ids(&multi_slots.top_bar_end),
            vec![
                "zendia.agents",
                "zendia.chat",
                "zendia.webchat",
                "builtin.extensions",
                "builtin.app_menu"
            ]
        );
        assert!(multi_slots.sidebar_top_start.is_empty());
        assert!(multi_slots.sidebar_url_bar.is_empty());
        assert_eq!(
            ids(&multi_slots.sidebar_bottom),
            vec!["builtin.library", "builtin.new_tab"]
        );

        let collapsed = LayoutSettings {
            toolbar_layout: ToolbarLayout::Collapsed,
            sidebar_expanded: false,
            ..LayoutSettings::default()
        };
        let collapsed_slots = ToolbarSlots::resolve(&items, &collapsed);
        assert_eq!(collapsed_slots.top_bar_start, multi_slots.top_bar_start);
        assert_eq!(collapsed_slots.top_bar_center, multi_slots.top_bar_center);
        assert_eq!(collapsed_slots.top_bar_end, multi_slots.top_bar_end);
        assert_eq!(collapsed_slots.sidebar_bottom, multi_slots.sidebar_bottom);
    }

    #[test]
    fn page_card_margins_and_window_controls_match_layout_and_side() {
        let single_left = LayoutSettings {
            toolbar_layout: ToolbarLayout::Single,
            sidebar_side: SidebarSide::Left,
            ..LayoutSettings::default()
        };
        assert_eq!(page_card_margins(&single_left), (8.0, 8.0, 8.0, 0.0));
        assert!(!show_window_controls_in_top_bar(&single_left));
        assert!(!show_window_controls_in_sidebar_top(&single_left));

        let single_right = LayoutSettings {
            toolbar_layout: ToolbarLayout::Single,
            sidebar_side: SidebarSide::Right,
            ..LayoutSettings::default()
        };
        assert_eq!(page_card_margins(&single_right), (8.0, 0.0, 8.0, 8.0));
        assert!(!show_window_controls_in_top_bar(&single_right));
        assert!(show_window_controls_in_sidebar_top(&single_right));

        let multiple_left = LayoutSettings {
            toolbar_layout: ToolbarLayout::Multiple,
            sidebar_side: SidebarSide::Left,
            ..LayoutSettings::default()
        };
        assert_eq!(page_card_margins(&multiple_left), (0.0, 8.0, 8.0, 0.0));
        assert!(show_window_controls_in_top_bar(&multiple_left));
        assert!(!show_window_controls_in_sidebar_top(&multiple_left));
    }

    #[test]
    fn resize_math_respects_sidebar_side_and_clamps_bounds() {
        assert_eq!(
            compute_resized_sidebar_width(200.0, 200.0, 240.0, SidebarSide::Left),
            240.0
        );
        assert_eq!(
            compute_resized_sidebar_width(200.0, 200.0, 240.0, SidebarSide::Right),
            SIDEBAR_MIN_WIDTH
        );
        assert_eq!(
            compute_resized_sidebar_width(200.0, 800.0, 740.0, SidebarSide::Right),
            260.0
        );
        assert_eq!(
            compute_resized_sidebar_width(200.0, 200.0, 1200.0, SidebarSide::Left),
            SIDEBAR_MAX_WIDTH
        );
    }
}
