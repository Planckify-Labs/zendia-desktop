/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Shared side panel dock host supporting Float, Slide, and Split layouts on either window edge.
//!
//! Implements Zendia's dock frame contract (`ZendiaDockFrame` in the legacy fork):
//! - **Float**: glass card inset `10px` from top, bottom, and its docked edge, `16px` radius, elevated shadow.
//! - **Slide**: flush to the docked edge (`0px` inset), `10px` page-facing corners, auto-hides `400ms` after pointer
//!   leave unless pinned by typing or a sub-popover, paired with a `10px` edge handle (`4px x 56px` agent-colored bar)
//!   that re-opens on `100ms` hover or click.
//! - **Split**: in-flow sibling beside the page card with an `8px` gap and `10px` radius; automatically falls back to
//!   Float (or Slide if narrow) when the stage cannot fit both the panel and the minimum page width.
//! - **Edge drag-resize**: `8px` strip on the page-facing edge with a `3px x 40px` (`72px` dragging) accent indicator,
//!   live preview, commit on release (`Command::PanelSetWidth`), and double-click reset (`Command::PanelResetWidth`).
//! - **Closed pill**: floating glass pill centered near the bottom of the stage while the active agent is working or
//!   has an unseen reply while the panel is closed.

pub mod state;

pub use state::{DictationState, DockHostState, try_use_dock_state, use_provide_dock_state};

use crate::ui::agent::{chevron_left_icon, chevron_right_icon};
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::theme::{CHROME_OVERLAY_LEVEL, ResolvedScheme, ThemeRole, motion, use_theme};
use freya::prelude::*;
use zendia_browser_core::agent::{
    AGENT_PANEL_ID, PANEL_DEFAULT_WIDTH, PANEL_MAX_WIDTH, SidePanelPrefs, format_elapsed_clock,
    resolve_panel_layout_with_min,
};
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{SidePanelId, SidePanelLayout, SidePanelSide};
use zendia_browser_core::model::ConversationStatus;

/// Inset from the stage edges in Float mode.
pub const FLOAT_INSET: f32 = 10.0;
/// Corner radius in Float mode.
pub const FLOAT_RADIUS: f32 = 16.0;
/// Corner radius in Slide and Split modes.
pub const DOCKED_RADIUS: f32 = 10.0;
/// Gap between a Split-mode side panel and the page card.
pub const SPLIT_GAP: f32 = 8.0;
/// Hit strip width of the page-facing edge resize grip.
const RESIZE_HIT_WIDTH: f32 = 8.0;
/// Width of the collapsed Slide-mode edge handle.
const SLIDE_HANDLE_WIDTH: f32 = 10.0;

/// Resolved placement of an open side panel inside [`WindowShell`].
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedDockPanel {
    pub panel_id: SidePanelId,
    pub prefs: SidePanelPrefs,
    pub effective_layout: SidePanelLayout,
    pub width: f32,
    pub split_fell_back: bool,
    pub focused: bool,
}

impl ResolvedDockPanel {
    /// Horizontal clearance reserved on the panel's edge when floating or sliding (0 for Split because Split narrows
    /// the page card in flow).
    pub fn overlay_clearance(&self) -> f32 {
        match self.effective_layout {
            SidePanelLayout::Split => 0.0,
            SidePanelLayout::Float => self.width + FLOAT_INSET,
            SidePanelLayout::Slide => self.width,
        }
    }
}

/// Computes the resolved dock placement for the currently open side panel, if any.
pub fn resolve_active_dock_panel(
    model: &zendia_browser_core::model::BrowserModel,
    dock_state: DockHostState,
    stage_width: f32,
) -> Option<ResolvedDockPanel> {
    let open_id = dock_state.open_panels.read().last().cloned()?;
    let mut prefs = model.panel_prefs(&open_id);
    let min_w = SidePanelPrefs::min_width_for(open_id.as_str());
    if let Some(preview_w) = dock_state.preview_width_for(&open_id) {
        prefs.width = preview_w.clamp(min_w, PANEL_MAX_WIDTH);
    }

    // Compute narrowest pane fraction if the active tab is inside a split view group.
    let narrowest_pane_fraction = model
        .active_tab
        .and_then(|tab_id| model.split_group_for_tab(tab_id))
        .map(|group| {
            let n = group.tab_ids.len().max(1);
            if n <= 1 {
                1.0
            } else {
                group
                    .ratios
                    .iter()
                    .copied()
                    .filter(|r| r.is_finite() && *r > 0.0)
                    .fold(1.0_f32 / n as f32, f32::min)
                    .max(0.25)
            }
        })
        .unwrap_or(1.0);

    let effective_stage = if stage_width > 200.0 { stage_width } else { 1120.0 };
    let resolved = resolve_panel_layout_with_min(prefs, effective_stage, narrowest_pane_fraction, min_w);
    let focused = dock_state.is_focused(&open_id);

    Some(ResolvedDockPanel {
        panel_id: open_id,
        prefs,
        effective_layout: resolved.effective_layout,
        width: resolved.width,
        split_fell_back: resolved.fell_back,
        focused,
    })
}

/// Resolves the brand accent color for the currently active agent (falling back to the theme's accent).
pub fn active_agent_accent(
    model: &zendia_browser_core::model::BrowserModel,
    theme: &crate::ui::theme::ResolvedTheme,
) -> Color {
    let agent_id = model
        .active_tab
        .and_then(|tid| model.active_conversation_for_tab(tid))
        .map(|conv| conv.agent_id.0.as_str())
        .or_else(|| model.default_agent().map(|a| a.id.0.as_str()));

    if let Some(id) = agent_id {
        let key = match id {
            "claude-code" | "claude" => "claude",
            "codex" => "codex",
            "cursor" => "cursor",
            "antigravity" => "antigravity",
            "opencode" => "opencode",
            "kiro" => "kiro",
            other => other,
        };
        if let Some(rgba) = theme.agents.get(key) {
            return rgba.to_color();
        }
    }
    theme.color(ThemeRole::Accent)
}

/// Segmented Float / Slide / Split layout selector + Left/Right side flip button for side panel headers.
#[derive(Clone, PartialEq)]
pub struct DockLayoutControls {
    pub panel_id: SidePanelId,
    pub prefs: SidePanelPrefs,
    pub split_fell_back: bool,
    pub accent: Color,
}

impl Component for DockLayoutControls {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut hovered_seg = use_state(|| None::<u8>);

        let group_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(20, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(12, 20, 24, 38),
        };
        let active_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.22).to_color();
        let hover_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(22, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(18, 20, 24, 38),
        };
        let border_color = theme.rgba(ThemeRole::BorderSubtle).to_color();

        let layouts = [
            (0u8, SidePanelLayout::Float, "layout-float", "Float panel"),
            (1u8, SidePanelLayout::Slide, "layout-slide", "Slide-out panel"),
            (
                2u8,
                SidePanelLayout::Split,
                "layout-split",
                if self.split_fell_back {
                    "Split beside page (window too narrow — floating)"
                } else {
                    "Split beside page"
                },
            ),
        ];

        let mut seg_row = rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(1.0)
            .padding(Gaps::new_all(2.0))
            .corner_radius(CornerRadius::new_all(7.0))
            .background(group_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            );

        for (idx, layout_kind, icon_name, tooltip) in layouts {
            let is_active = self.prefs.layout == layout_kind;
            let is_fallback_split = layout_kind == SidePanelLayout::Split && is_active && self.split_fell_back;
            let is_hovered = *hovered_seg.read() == Some(idx);
            let bg = if is_active {
                active_bg
            } else if is_hovered {
                hover_bg
            } else {
                Color::TRANSPARENT
            };
            let icon_role = if is_fallback_split {
                ThemeRole::Warning
            } else if is_active {
                ThemeRole::Accent
            } else {
                ThemeRole::TextMuted
            };
            let core_click = core.clone();
            let pid = self.panel_id.clone();

            seg_row = seg_row.child(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(tooltip)
                    .width(Size::px(22.0))
                    .height(Size::px(20.0))
                    .corner_radius(CornerRadius::new_all(5.0))
                    .background(bg)
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| hovered_seg.set_if_modified(Some(idx)))
                    .on_pointer_leave(move |_| {
                        if *hovered_seg.peek() == Some(idx) {
                            hovered_seg.set(None);
                        }
                    })
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        core_click.invoke_or_log(Command::PanelSetLayout {
                            panel_id: pid.clone(),
                            layout: layout_kind,
                        });
                    })
                    .child(Icon::new(icon_name).size(12.0).role(icon_role))
                    .maybe_child(is_fallback_split.then(|| {
                        rect()
                            .position(Position::new_absolute().top(2.0).right(2.0))
                            .width(Size::px(4.5))
                            .height(Size::px(4.5))
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(theme.color(ThemeRole::Warning))
                    })),
            );
        }

        let next_side = match self.prefs.side {
            SidePanelSide::Right => SidePanelSide::Left,
            SidePanelSide::Left => SidePanelSide::Right,
        };
        let side_tooltip = match self.prefs.side {
            SidePanelSide::Right => "Move panel to left side",
            SidePanelSide::Left => "Move panel to right side",
        };
        let side_hovered = *hovered_seg.read() == Some(3);
        let core_side = core.clone();
        let pid_side = self.panel_id.clone();
        let side_color = if side_hovered {
            theme.color(ThemeRole::Text)
        } else {
            theme.color(ThemeRole::TextMuted)
        };
        let side_icon_el = match self.prefs.side {
            SidePanelSide::Right => chevron_left_icon(12.5, side_color),
            SidePanelSide::Left => chevron_right_icon(12.5, side_color),
        };

        let side_btn = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(side_tooltip)
            .width(Size::px(24.0))
            .height(Size::px(24.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(if side_hovered { hover_bg } else { Color::TRANSPARENT })
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered_seg.set_if_modified(Some(3)))
            .on_pointer_leave(move |_| {
                if *hovered_seg.peek() == Some(3) {
                    hovered_seg.set(None);
                }
            })
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                core_side.invoke_or_log(Command::PanelSetSide {
                    panel_id: pid_side.clone(),
                    side: next_side,
                });
            })
            .child(side_icon_el);

        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(3.0)
            .child(seg_row)
            .child(side_btn)
    }
}

/// Reusable dock frame hosting a side panel in Float, Slide, or Split layout with an edge resize grip.
#[derive(Clone, PartialEq)]
pub struct DockFrame {
    pub resolved: ResolvedDockPanel,
    pub accent: Color,
    pub content: Element,
    key: DiffKey,
}

impl KeyExt for DockFrame {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl DockFrame {
    pub fn new(resolved: ResolvedDockPanel, accent: Color, content: impl IntoElement) -> Self {
        Self {
            resolved,
            accent,
            content: content.into_element(),
            key: DiffKey::None,
        }
    }
}

impl Component for DockFrame {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();

        let mut grip_hovered = use_state(|| false);
        let mut dragging = use_state(|| false);
        let mut drag_start_x = use_state(|| 0.0f32);
        let mut drag_start_width = use_state(|| self.resolved.width);
        let mut last_click_ms = use_state(|| 0u128);

        let panel_id = self.resolved.panel_id.clone();
        let side = self.resolved.prefs.side;
        let layout = self.resolved.effective_layout;
        let width = self.resolved.width;
        let focused = self.resolved.focused;

        let panel_bg = match layout {
            SidePanelLayout::Split => match theme.scheme {
                ResolvedScheme::Dark => theme.color(ThemeRole::Surface),
                ResolvedScheme::Light => theme.color(ThemeRole::SurfaceRaised),
            },
            SidePanelLayout::Float | SidePanelLayout::Slide => theme.color(ThemeRole::SurfaceGlass),
        };

        let border = if focused {
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(theme.rgba(ThemeRole::Border).with_alpha(0.85).to_color())
        } else {
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::BorderSubtle))
        };

        let radius: CornerRadius = match layout {
            SidePanelLayout::Float => CornerRadius::new_all(FLOAT_RADIUS),
            SidePanelLayout::Split => CornerRadius::new_all(DOCKED_RADIUS),
            SidePanelLayout::Slide => match side {
                SidePanelSide::Right => (DOCKED_RADIUS, 0.0, 0.0, DOCKED_RADIUS).into(),
                SidePanelSide::Left => (0.0, DOCKED_RADIUS, DOCKED_RADIUS, 0.0).into(),
            },
        };

        let shadow = match layout {
            SidePanelLayout::Split => match theme.scheme {
                ResolvedScheme::Dark => Shadow::new().y(2.0).blur(8.0).color(Color::from_argb(48, 0, 0, 0)),
                ResolvedScheme::Light => Shadow::new().y(2.0).blur(6.0).color(Color::from_argb(24, 0, 0, 0)),
            },
            SidePanelLayout::Float | SidePanelLayout::Slide => theme.shadows.float_drop.to_shadow(),
        };

        // Page-facing resize grip (left edge when docked Right, right edge when docked Left).
        let grip_pos = match side {
            SidePanelSide::Right => Position::new_absolute().left(0.0).top(0.0),
            SidePanelSide::Left => Position::new_absolute().right(0.0).top(0.0),
        };
        let is_dragging = *dragging.read();
        let show_bar = is_dragging || *grip_hovered.read();
        let bar_h = if is_dragging { 72.0 } else { 40.0 };
        let bar_color = if is_dragging {
            self.accent
        } else {
            theme.rgba(ThemeRole::Border).to_color()
        };

        let core_dbl = core.clone();
        let pid_down = panel_id.clone();
        let pid_move = panel_id.clone();
        let core_up = core.clone();
        let pid_up = panel_id.clone();

        let resize_grip = rect()
            .position(grip_pos)
            .width(Size::px(RESIZE_HIT_WIDTH))
            .height(Size::percent(100.0))
            .center()
            .cursor(CursorIcon::ColResize)
            .on_pointer_enter(move |_| grip_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| grip_hovered.set_if_modified(false))
            .on_pointer_down(move |e: Event<PointerEventData>| {
                e.stop_propagation();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let prev = *last_click_ms.peek();
                last_click_ms.set(now);
                if now.saturating_sub(prev) < 350 {
                    // Double-click resets panel width to default.
                    dragging.set(false);
                    if let Some(mut ds) = dock_state {
                        ds.resize_preview.set(None);
                    }
                    core_dbl.invoke_or_log(Command::PanelResetWidth {
                        panel_id: pid_down.clone(),
                    });
                    return;
                }
                dragging.set(true);
                drag_start_x.set(e.global_location().x as f32);
                drag_start_width.set(width);
            })
            .on_global_pointer_move(move |e: Event<PointerEventData>| {
                if !*dragging.peek() {
                    return;
                }
                let dx = e.global_location().x as f32 - *drag_start_x.peek();
                let delta = match side {
                    SidePanelSide::Right => -dx,
                    SidePanelSide::Left => dx,
                };
                let min_w = SidePanelPrefs::min_width_for(pid_move.as_str());
                let next_w = (*drag_start_width.peek() + delta).clamp(min_w, PANEL_MAX_WIDTH);
                if let Some(mut ds) = dock_state {
                    ds.resize_preview.set(Some((pid_move.clone(), next_w)));
                }
            })
            .on_global_pointer_up(move |_| {
                if !*dragging.peek() {
                    return;
                }
                dragging.set(false);
                let final_w = dock_state
                    .and_then(|ds| ds.preview_width_for(&pid_up))
                    .unwrap_or(PANEL_DEFAULT_WIDTH);
                if let Some(mut ds) = dock_state {
                    ds.resize_preview.set(None);
                }
                core_up.invoke_or_log(Command::PanelSetWidth {
                    panel_id: pid_up.clone(),
                    width: final_w,
                });
            })
            .maybe_child(show_bar.then(|| {
                rect()
                    .width(Size::px(3.0))
                    .height(Size::px(bar_h))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(bar_color)
            }));

        // Slide mode auto-hide: close 400ms after pointer leaves unless typing/sub-overlay is active.
        let pid_focus = panel_id.clone();
        let pid_leave = panel_id.clone();

        let mut card = rect()
            .width(Size::px(width))
            .height(Size::fill())
            .corner_radius(radius)
            .overflow(Overflow::Clip)
            .background(panel_bg)
            .border(border)
            .shadow(shadow)
            .on_pointer_down(move |_| {
                if let Some(mut ds) = dock_state {
                    ds.set_focus(Some(pid_focus.clone()));
                }
            })
            .on_pointer_enter(move |_| {
                if let Some(mut ds) = dock_state {
                    ds.slide_pointer_inside.set_if_modified(true);
                    let seq = *ds.slide_timer_seq.peek() + 1;
                    ds.slide_timer_seq.set(seq);
                }
            })
            .on_pointer_leave(move |_| {
                if layout != SidePanelLayout::Slide {
                    return;
                }
                if let Some(mut ds) = dock_state {
                    ds.slide_pointer_inside.set_if_modified(false);
                    if *ds.slide_keep_open.peek() || *ds.model_card_open.peek() || *ds.instructions_open.peek() {
                        return;
                    }
                    let seq = *ds.slide_timer_seq.peek() + 1;
                    ds.slide_timer_seq.set(seq);
                    let pid = pid_leave.clone();
                    spawn(async move {
                        timer(motion::DOCK_KEEP_HOVER).await;
                        if *ds.slide_timer_seq.peek() == seq
                            && !*ds.slide_pointer_inside.peek()
                            && !*ds.slide_keep_open.peek()
                            && !*ds.model_card_open.peek()
                            && !*ds.instructions_open.peek()
                        {
                            ds.close_panel(&pid);
                        }
                    });
                }
            })
            .child(self.content.clone())
            .child(resize_grip);

        if layout != SidePanelLayout::Split && theme.glass.blur > 0.0 {
            card = card.backdrop_blur(theme.glass.blur);
        }

        match layout {
            SidePanelLayout::Split => card,
            SidePanelLayout::Float => {
                let pos = match side {
                    SidePanelSide::Right => Position::new_absolute().right(FLOAT_INSET).top(FLOAT_INSET),
                    SidePanelSide::Left => Position::new_absolute().left(FLOAT_INSET).top(FLOAT_INSET),
                };
                rect()
                    .layer(Layer::OverlayLevel(CHROME_OVERLAY_LEVEL))
                    .position(pos)
                    .height(Size::percent(100.0))
                    .padding(Gaps::new(0.0, 0.0, FLOAT_INSET * 2.0, 0.0))
                    .child(card)
            }
            SidePanelLayout::Slide => {
                let pos = match side {
                    SidePanelSide::Right => Position::new_absolute().right(0.0).top(0.0),
                    SidePanelSide::Left => Position::new_absolute().left(0.0).top(0.0),
                };
                rect()
                    .layer(Layer::OverlayLevel(CHROME_OVERLAY_LEVEL))
                    .position(pos)
                    .height(Size::percent(100.0))
                    .child(card)
            }
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Default host chrome for registered side panels whose dedicated view is built in a later phase or by a plugin.
#[derive(Clone, PartialEq)]
pub struct GenericSidePanel {
    pub resolved: ResolvedDockPanel,
    pub accent: Color,
    key: DiffKey,
}

impl KeyExt for GenericSidePanel {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl GenericSidePanel {
    pub fn new(resolved: ResolvedDockPanel, accent: Color) -> Self {
        Self {
            resolved,
            accent,
            key: DiffKey::None,
        }
    }
}

impl Component for GenericSidePanel {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let panel_id = self.resolved.panel_id.clone();

        let (title, icon_name) = {
            let model = core.model().read();
            model
                .contributions
                .side_panels
                .iter()
                .find(|entry| entry.id == panel_id)
                .map(|entry| (entry.title.clone(), entry.icon.clone()))
                .unwrap_or_else(|| (panel_id.0.clone(), "layers".to_string()))
        };

        let (subtitle, phase_badge) = match panel_id.0.as_str() {
            "zendia.agents" => (
                "Live roster of every running and recent agent session across tabs.",
                "Scheduled for Phase 7 · Agents panel",
            ),
            "zendia.webchat" => (
                "Side-by-side web chat sites (ChatGPT, Claude, Gemini, Kimi) with Ask menu integration.",
                "Scheduled for Phase 8 · Web chat",
            ),
            _ => (
                "Contributed side panel hosted in Zendia's dock frame.",
                "Contributed panel",
            ),
        };

        let border_col = theme.color(ThemeRole::BorderSubtle);
        let badge_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.14).to_color();
        let badge_border = theme.rgba(ThemeRole::Accent).with_alpha(0.30).to_color();
        let icon_well_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(28, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(14, 20, 24, 38),
        };
        let core_close = core.clone();
        let pid_close = panel_id.clone();
        let core_chat = core.clone();

        let header = rect()
            .width(Size::fill())
            .height(Size::px(44.0))
            .padding(Gaps::new(0.0, 8.0, 0.0, 12.0))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .main_align(Alignment::SpaceBetween)
            .spacing(8.0)
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(7.0)
                    .child(Icon::new(icon_name.clone()).size(14.0).role(ThemeRole::Accent))
                    .child(
                        label()
                            .text(title.clone())
                            .font_size(12.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(theme.color(ThemeRole::Text))
                            .max_lines(1),
                    ),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(3.0)
                    .child(DockLayoutControls {
                        panel_id: panel_id.clone(),
                        prefs: self.resolved.prefs,
                        split_fell_back: self.resolved.split_fell_back,
                        accent: theme.color(ThemeRole::Accent),
                    })
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt(format!("Close {title}"))
                            .width(Size::px(24.0))
                            .height(Size::px(24.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_close.invoke_or_log(Command::PanelClose {
                                    panel_id: pid_close.clone(),
                                });
                            })
                            .child(Icon::new("close").size(12.0).role(ThemeRole::TextMuted)),
                    ),
            );

        let body = rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .padding(Gaps::new(24.0, 32.0, 24.0, 32.0))
            .center()
            .spacing(12.0)
            .child(
                rect()
                    .width(Size::px(48.0))
                    .height(Size::px(48.0))
                    .corner_radius(CornerRadius::new_all(14.0))
                    .background(icon_well_bg)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(border_col),
                    )
                    .center()
                    .child(Icon::new(icon_name).size(22.0).role(ThemeRole::Accent)),
            )
            .child(
                label()
                    .width(Size::fill())
                    .text(title)
                    .font_size(14.5)
                    .font_weight(FontWeight::BOLD)
                    .text_align(TextAlign::Center)
                    .color(theme.color(ThemeRole::Text)),
            )
            .child(
                rect().width(Size::fill()).max_width(Size::px(280.0)).child(
                    label()
                        .width(Size::fill())
                        .text(subtitle)
                        .font_size(12.0)
                        .line_height(1.48)
                        .text_align(TextAlign::Center)
                        .color(theme.color(ThemeRole::TextMuted)),
                ),
            )
            .child(
                rect()
                    .padding(Gaps::new(4.0, 10.0, 4.0, 10.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(badge_bg)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(badge_border),
                    )
                    .child(
                        label()
                            .text(phase_badge)
                            .font_size(11.0)
                            .font_weight(FontWeight::MEDIUM)
                            .color(theme.color(ThemeRole::Accent)),
                    ),
            )
            .child(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Open Agent Chat")
                    .margin(Gaps::new(6.0, 0.0, 0.0, 0.0))
                    .height(Size::px(30.0))
                    .padding(Gaps::new(0.0, 14.0, 0.0, 12.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(theme.color(ThemeRole::Accent))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        core_chat.invoke_or_log(Command::PanelOpen {
                            panel_id: SidePanelId::new(AGENT_PANEL_ID),
                            focus: true,
                        });
                    })
                    .child(Icon::new("chat").size(12.5).role(ThemeRole::Surface))
                    .child(
                        label()
                            .text("Open Agent Chat")
                            .font_size(11.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(theme.color(ThemeRole::Surface)),
                    ),
            );

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .child(header)
            .child(rect().width(Size::fill()).height(Size::px(1.0)).background(border_col))
            .child(body)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Floating glass pill shown centered at the bottom of the page stage when the agent panel is closed while an agent
/// is working, waiting for permission, or has an unseen reply.
#[derive(Clone, PartialEq)]
pub struct DockClosedPill;

impl Component for DockClosedPill {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let mut hovered = use_state(|| false);

        if dock_state.is_some_and(|ds| ds.is_agent_open() || ds.is_agents_open()) {
            return rect().width(Size::px(0.0)).height(Size::px(0.0));
        }

        let pill_info = {
            let model = core.model().read();
            let server_running = model.services.iter().any(|s| {
                s.service == zendia_browser_core::model::ServiceId::AgentServer
                    && matches!(s.status, zendia_browser_core::model::ServiceStatus::Running)
            });
            if !server_running {
                None
            } else {
                let conv = model
                    .active_tab
                    .and_then(|tid| model.active_conversation_for_tab(tid))
                    .or_else(|| {
                        model
                            .conversations
                            .iter()
                            .find(|c| c.status == ConversationStatus::Working || c.unseen)
                    });
                conv.and_then(|c| {
                    let agent_name = model
                        .agent(&c.agent_id)
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| "Agent".to_string());
                    let accent = active_agent_accent(&model, &theme);
                    match c.status {
                        ConversationStatus::Working => {
                            let clock = if c.elapsed_secs > 0 {
                                format!(" · {}", format_elapsed_clock(c.elapsed_secs))
                            } else {
                                String::new()
                            };
                            Some((format!("{agent_name} is working{clock}"), accent, true))
                        }
                        ConversationStatus::WaitingForUser => Some((
                            format!("{agent_name} needs permission · open"),
                            theme.color(ThemeRole::Warning),
                            true,
                        )),
                        _ if c.unseen => Some((format!("{agent_name} finished · open"), accent, false)),
                        _ => None,
                    }
                })
            }
        };

        let Some((text_label, dot_color, _is_working)) = pill_info else {
            return rect().width(Size::px(0.0)).height(Size::px(0.0));
        };

        let bg = if *hovered.read() {
            theme.color(ThemeRole::SurfaceRaised)
        } else {
            theme.color(ThemeRole::SurfaceGlass)
        };
        let core_click = core.clone();

        let mut pill = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(text_label.clone())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .height(Size::px(34.0))
            .padding(Gaps::new(0.0, 14.0, 0.0, 12.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .shadow(theme.shadows.card.to_shadow())
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                core_click.invoke_or_log(Command::PanelOpen {
                    panel_id: SidePanelId::new(AGENT_PANEL_ID),
                    focus: true,
                });
            })
            .child(
                rect()
                    .width(Size::px(8.0))
                    .height(Size::px(8.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(dot_color),
            )
            .child(
                label()
                    .text(text_label)
                    .font_size(theme.font_sizes.md)
                    .font_weight(FontWeight::MEDIUM)
                    .color(theme.color(ThemeRole::Text))
                    .max_lines(1),
            );

        if theme.glass.blur > 0.0 {
            pill = pill.backdrop_blur(theme.glass.blur);
        }

        rect()
            .layer(Layer::OverlayLevel(CHROME_OVERLAY_LEVEL))
            .position(Position::new_absolute().bottom(16.0).left(0.0))
            .width(Size::percent(100.0))
            .direction(Direction::Horizontal)
            .main_align(Alignment::center())
            .child(pill)
    }
}

/// Edge handle shown on the docked side when the agent panel is closed and configured for Slide layout.
/// Hovering for `100ms` or clicking opens the panel.
#[derive(Clone, PartialEq)]
pub struct DockSlideHandle;

impl Component for DockSlideHandle {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let mut hovered = use_state(|| false);

        if dock_state.is_some_and(|ds| ds.is_agent_open() || ds.is_agents_open()) {
            return rect().width(Size::px(0.0)).height(Size::px(0.0));
        }

        let (show_handle, side, accent) = {
            let model = core.model().read();
            let prefs = model.panel_prefs(&SidePanelId::new(AGENT_PANEL_ID));
            (
                prefs.layout == SidePanelLayout::Slide,
                prefs.side,
                active_agent_accent(&model, &theme),
            )
        };

        if !show_handle {
            return rect().width(Size::px(0.0)).height(Size::px(0.0));
        }

        let pos = match side {
            SidePanelSide::Right => Position::new_absolute().right(0.0).top(0.0),
            SidePanelSide::Left => Position::new_absolute().left(0.0).top(0.0),
        };
        let is_hovered = *hovered.read();
        let core_hover = core.clone();
        let core_click = core.clone();

        rect()
            .layer(Layer::OverlayLevel(CHROME_OVERLAY_LEVEL))
            .position(pos)
            .width(Size::px(SLIDE_HANDLE_WIDTH))
            .height(Size::percent(100.0))
            .center()
            .cursor(CursorIcon::Pointer)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Open agent panel")
            .on_pointer_enter(move |_| {
                hovered.set_if_modified(true);
                if let Some(mut ds) = dock_state {
                    let seq = *ds.slide_timer_seq.peek() + 1;
                    ds.slide_timer_seq.set(seq);
                    let core_open = core_hover.clone();
                    spawn(async move {
                        timer(motion::DOCK_HANDLE_HOVER).await;
                        if *ds.slide_timer_seq.peek() == seq && !ds.is_agent_open() {
                            core_open.invoke_or_log(Command::PanelOpen {
                                panel_id: SidePanelId::new(AGENT_PANEL_ID),
                                focus: false,
                            });
                        }
                    });
                }
            })
            .on_pointer_leave(move |_| {
                hovered.set_if_modified(false);
                if let Some(mut ds) = dock_state {
                    let seq = *ds.slide_timer_seq.peek() + 1;
                    ds.slide_timer_seq.set(seq);
                }
            })
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                core_click.invoke_or_log(Command::PanelOpen {
                    panel_id: SidePanelId::new(AGENT_PANEL_ID),
                    focus: true,
                });
            })
            .child(
                rect()
                    .width(Size::px(if is_hovered { 5.0 } else { 4.0 }))
                    .height(Size::px(if is_hovered { 72.0 } else { 56.0 }))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(accent)
                    .opacity(if is_hovered { 0.95 } else { 0.55 }),
            )
    }
}
