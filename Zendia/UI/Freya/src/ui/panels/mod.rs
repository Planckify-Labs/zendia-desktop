/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Shared side panel dock host supporting Float, Slide, and Split layouts on either window edge.
//!
//! Implements Zendia's dock frame contract (`ZendiaDockFrame` in the legacy fork):
//! - **Float**: glass card inset `10px` from top, bottom, and its docked edge, `16px` radius, elevated shadow.
//! - **Slide**: identical geometry and look to Float (`10px` inset, `16px` radius, same shadow/border/focus ring),
//!   differing only in behaviour: auto-hides `400ms` after pointer leave (keeping open while resting in the gap
//!   between the panel and the window edge, or while pinned by typing, pending confirmation, dictation, resizing, or
//!   a sub-popover), paired with a `10px` edge handle (`4px x 56px` growing to `4px x 88px` on hover/unseen) per closed
//!   slide panel that re-opens on `100ms` hover or click.
//! - **Split**: in-flow glass sibling beside the page card with an `8px` gap and `10px` radius; automatically falls
//!   back to Float (or Slide) when the stage cannot fit both the panel and the minimum page width.
//! - **Motion**: `360ms` spring entrance and `180ms` ease-in exit (`DF-01`), `150ms` inner accent focus ring (`DF-02`),
//!   and `150ms` accent resize grip growth (`DF-07`).
//! - **Edge drag-resize**: `8px` strip on the page-facing edge with a `3px x 40px -> 72px` accent indicator,
//!   clamped to `resolved.max_width`, disabled when `resize_disabled`, live preview, commit on release only when width
//!   actually changed (`XPN-03`), and double-click reset (`Command::PanelResetWidth`).
//! - **Closed pill**: animated floating glass pill centered near the bottom of the stage without blocking bottom-edge
//!   page clicks (`DF-14`, `XIX-06`).

pub mod state;

pub use state::{DictationState, DockHostState, try_use_dock_state, use_provide_dock_state};

use crate::ui::core_view::use_core;
use crate::ui::kit::{DeclaredUi, Icon, KitTooltip, focus_ring_border, use_interactive_feedback};
use crate::ui::page::PageSurfaceContext;
use crate::ui::theme::{CHROME_OVERLAY_LEVEL, ResolvedScheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::agent::{
    AGENT_PANEL_ID, AGENTS_PANEL_ID, PANEL_MAX_WIDTH, SidePanelPrefs, format_elapsed_clock,
    resolve_panel_layout_with_min, resolve_stacked_panels,
};
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{ContributionSource, SidePanelId, SidePanelLayout, SidePanelSide};
use zendia_browser_core::model::{BrowserModel, ConversationStatus, ServiceId, ServiceStatus, StepKind};
use zendia_browser_core::modules::{PluginId, PluginStatus, service_down_summary, service_status_in};
use zendia_browser_core::webchat::WEBCHAT_PANEL_ID;

/// Inset from the stage edges in Float and Slide modes.
pub const FLOAT_INSET: f32 = 10.0;
/// Corner radius in Float and Slide modes.
pub const FLOAT_RADIUS: f32 = 16.0;
/// Corner radius in Split mode.
pub const DOCKED_RADIUS: f32 = 10.0;
/// Gap between a Split-mode side panel and the page card.
pub const SPLIT_GAP: f32 = 8.0;
/// Extra window-edge separation outside the stage card (`ELEMENT_SEPARATION`), kept inside a Slide panel's hover zone
/// when docked at offset 0 so resting in the gap between the panel and the window edge never triggers auto-hide.
const SLIDE_WINDOW_EDGE_GAP: f32 = 8.0;
/// Hit strip width of the page-facing edge resize grip.
const RESIZE_HIT_WIDTH: f32 = 8.0;
/// Width of the collapsed Slide-mode edge handle.
const SLIDE_HANDLE_WIDTH: f32 = 10.0;
/// Relative paint layer for the page-facing resize grip so it sits above deep nested web views (`PageSurface`).
const GRIP_RELATIVE_LAYER: i16 = 32;
/// Relative paint layer for the panel's subtle 1px border outline so web pages never paint over it.
const BORDER_RELATIVE_LAYER: i16 = 35;
/// Relative paint layer for the panel's 2px inner accent focus ring so it paints cleanly over web pages and corners.
const FOCUS_RING_RELATIVE_LAYER: i16 = 36;
/// Overlay paint level for docked floating/sliding panels, handles, and pills: above page (`0`), below compact
/// floating chrome (`CHROME_OVERLAY_LEVEL = 2`) and popups (`POPUP_OVERLAY_LEVEL = 3`) (`XIX-02`).
pub const DOCK_OVERLAY_LEVEL: u8 = CHROME_OVERLAY_LEVEL.saturating_sub(1);

/// Resolved placement of an open (or exiting) side panel inside [`WindowShell`].
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedDockPanel {
    pub panel_id: SidePanelId,
    pub prefs: SidePanelPrefs,
    pub effective_layout: SidePanelLayout,
    pub width: f32,
    /// Dynamic upper bound for edge drag resizing (`splitMax` in Split, `stageWidth - 20` in Float/Slide) (`DF-08`).
    pub max_width: f32,
    /// True when Split mode cannot widen beyond `min_width`, disabling the resize grip (`DF-08`).
    pub resize_disabled: bool,
    pub split_fell_back: bool,
    pub focused: bool,
    /// True while playing the `180ms` close animation before unmounting (`DF-01`).
    pub closing: bool,
    /// How far a floating or sliding panel sits from its side's edge, past the panels already docked there.
    pub offset: f32,
}

impl ResolvedDockPanel {
    /// Horizontal clearance reserved on the panel's edge when floating or sliding (0 for Split or closing panels).
    pub fn overlay_clearance(&self) -> f32 {
        if self.closing {
            return 0.0;
        }
        match self.effective_layout {
            SidePanelLayout::Split => 0.0,
            SidePanelLayout::Float | SidePanelLayout::Slide => self.offset + self.width + FLOAT_INSET,
        }
    }
}

/// Resolves where every open (and closing-animation) side panel sits, in open order. Any number can be open at once,
/// each with its own layout and side, as each of the fork's dock frames is independent.
pub fn resolve_open_dock_panels(
    model: &BrowserModel,
    dock_state: DockHostState,
    stage_width: f32,
) -> Vec<ResolvedDockPanel> {
    let visible = dock_state.visible_panels();
    if visible.is_empty() {
        return Vec::new();
    }

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

    let inputs: Vec<(SidePanelPrefs, f32)> = visible
        .iter()
        .map(|(id, _)| {
            let mut prefs = model.panel_prefs(id);
            let min_w = SidePanelPrefs::min_width_for(id.as_str());
            if let Some(preview_w) = dock_state.preview_width_for(id) {
                prefs.width = preview_w.max(min_w);
            }
            (prefs, min_w)
        })
        .collect();
    let effective_stage = if stage_width > 200.0 { stage_width } else { 1120.0 };
    let stacked = resolve_stacked_panels(&inputs, effective_stage, narrowest_pane_fraction);

    visible
        .into_iter()
        .zip(inputs)
        .zip(stacked)
        .map(|(((panel_id, closing), (prefs, min_w)), placed)| {
            let max_width = if placed.layout.max_width.is_finite() {
                placed.layout.max_width.max(min_w)
            } else {
                PANEL_MAX_WIDTH
            };
            ResolvedDockPanel {
                focused: !closing && dock_state.is_focused(&panel_id),
                closing,
                panel_id,
                prefs,
                effective_layout: placed.layout.effective_layout,
                width: placed.layout.width,
                max_width,
                resize_disabled: placed.layout.resize_disabled,
                split_fell_back: placed.layout.fell_back,
                offset: placed.offset,
            }
        })
        .collect()
}

/// Resolves the brand accent color for the currently active agent (falling back to the theme's accent).
pub fn active_agent_accent(model: &BrowserModel, theme: &crate::ui::theme::ResolvedTheme) -> Color {
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

/// Returns whether pointer `(px, py)` (in window coordinates) lies inside a Slide-mode panel's keep-open zone:
/// the panel card itself plus the inset gap between the panel and its docked window/stage edge (and the vertical
/// float insets), so resting in the gap never counts as leaving.
pub fn slide_keep_open_contains(
    card_area: (f32, f32, f32, f32),
    side: SidePanelSide,
    offset: f32,
    px: f32,
    py: f32,
) -> bool {
    let (cx, cy, cw, ch) = card_area;
    if cw <= 1.0 || ch <= 1.0 {
        return false;
    }
    let side_gap = if offset <= 0.5 {
        FLOAT_INSET + SLIDE_WINDOW_EDGE_GAP
    } else {
        FLOAT_INSET
    };
    let in_x = match side {
        SidePanelSide::Right => px >= cx && px <= cx + cw + side_gap,
        SidePanelSide::Left => px >= cx - side_gap && px <= cx + cw,
    };
    let in_y = py >= cy - FLOAT_INSET && py <= cy + ch + FLOAT_INSET + SLIDE_WINDOW_EDGE_GAP;
    in_x && in_y
}

/// Returns whether a slide-mode panel must stay open despite the pointer leaving (`DF-13`).
fn slide_panel_keep_open(model: &BrowserModel, ds: DockHostState, panel_id: &SidePanelId) -> bool {
    if ds.is_slide_resizing(panel_id) {
        return true;
    }
    match panel_id.0.as_str() {
        AGENT_PANEL_ID => {
            let pending_confirm = model
                .active_tab
                .and_then(|tid| model.active_conversation_for_tab(tid))
                .is_some_and(|conv| {
                    conv.steps
                        .iter()
                        .any(|s| matches!(s.kind, StepKind::Confirmation { resolved: None, .. }))
                });
            *ds.slide_keep_open.peek()
                || *ds.model_card_open.peek()
                || *ds.instructions_open.peek()
                || *ds.dictation.peek() != DictationState::Idle
                || pending_confirm
        }
        WEBCHAT_PANEL_ID => *ds.webchat_accounts_open.peek() || *ds.webchat_add_site_open.peek(),
        _ => false,
    }
}

/// Focusable `24×24px` icon button used inside [`DockLayoutControls`] so each button carries its own `:focus-visible`
/// `2px` accent ring (`DF-10`).
#[derive(Clone, PartialEq)]
struct DockControlButton {
    icon_name: &'static str,
    icon_role: ThemeRole,
    a11y_alt: String,
    bg: Color,
    hover_bg: Color,
    on_activate: Callback<(), ()>,
    key: DiffKey,
}

impl KeyExt for DockControlButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for DockControlButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let feedback = use_interactive_feedback(true);
        let a11y_id = use_hook(AccessibilityId::new_unique);
        let focus = use_focus(a11y_id);

        let rest_bg = self.bg;
        let hover_bg = if self.bg != Color::TRANSPARENT {
            self.bg
        } else {
            self.hover_bg
        };
        let press_bg = theme.rgba(ThemeRole::Text).with_alpha(0.16).to_color();
        let bg = feedback.background(rest_bg, hover_bg, press_bg);
        let border = focus_ring_border(focus() == Focus::Keyboard, &theme, Border::new());
        let on_press_cb = self.on_activate.clone();
        let on_key_cb = self.on_activate.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.a11y_alt.clone())
            .a11y_focusable(true)
            .width(Size::px(24.0))
            .height(Size::px(24.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .border(border)
            .scale(feedback.scale())
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_press_cb.call(());
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Enter)) || e.key == Key::Character(" ".into()) {
                    e.stop_propagation();
                    on_key_cb.call(());
                }
            })
            .child(Icon::new(self.icon_name).size(14.0).role(self.icon_role))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Segmented Float / Slide / Split layout selector + Left/Right side flip button for side panel headers (`DF-10`, `XPN-07`).
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

        let group_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(20, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(12, 20, 24, 38),
        };
        let active_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.22).to_color();
        let warning_bg = theme.rgba(ThemeRole::Warning).with_alpha(0.22).to_color();
        let hover_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(22, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(18, 20, 24, 38),
        };

        let split_tooltip = if self.split_fell_back {
            if self.prefs.split_fallback == SidePanelLayout::Slide {
                "Side by side with the tab: no room beside this tab, so it slides for now"
            } else {
                "Side by side with the tab: no room beside this tab, so it floats for now"
            }
        } else {
            "Side by side with the tab"
        };

        let layouts = [
            (
                "float",
                SidePanelLayout::Float,
                "layout-float",
                "Floating over the page",
            ),
            (
                "slide",
                SidePanelLayout::Slide,
                "layout-slide",
                "Slide in from the side, hide when the mouse leaves",
            ),
            ("split", SidePanelLayout::Split, "layout-split", split_tooltip),
        ];

        // DF-10: Borderless 8px-radius group with 2px padding/gap and 24x24px buttons (14px icons).
        let mut seg_row = rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(2.0)
            .padding(Gaps::new_all(2.0))
            .margin(Gaps::new(0.0, 2.0, 0.0, 6.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(group_bg);

        for (key_name, layout_kind, icon_name, tooltip) in layouts {
            let is_active = self.prefs.layout == layout_kind;
            let is_fallback_split = layout_kind == SidePanelLayout::Split && is_active && self.split_fell_back;
            let bg = if is_fallback_split {
                warning_bg
            } else if is_active {
                active_bg
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
                KitTooltip::new(tooltip)
                    .key(format!("{}:{key_name}", self.panel_id.0))
                    .child(DockControlButton {
                        icon_name,
                        icon_role,
                        a11y_alt: tooltip.to_string(),
                        bg,
                        hover_bg,
                        on_activate: Callback::new(move |_| {
                            core_click.invoke_or_log(Command::PanelSetLayout {
                                panel_id: pid.clone(),
                                layout: layout_kind,
                            });
                        }),
                        key: DiffKey::None,
                    }),
            );
        }

        let next_side = match self.prefs.side {
            SidePanelSide::Right => SidePanelSide::Left,
            SidePanelSide::Left => SidePanelSide::Right,
        };
        let (side_icon_name, side_tooltip) = match self.prefs.side {
            SidePanelSide::Right => ("arrow-left", "Move to the left"),
            SidePanelSide::Left => ("arrow-right", "Move to the right"),
        };
        let core_side = core.clone();
        let pid_side = self.panel_id.clone();

        let side_btn = KitTooltip::new(side_tooltip)
            .key(format!("{}:side", self.panel_id.0))
            .child(DockControlButton {
                icon_name: side_icon_name,
                icon_role: ThemeRole::TextMuted,
                a11y_alt: side_tooltip.to_string(),
                bg: Color::TRANSPARENT,
                hover_bg,
                on_activate: Callback::new(move |_| {
                    core_side.invoke_or_log(Command::PanelSetSide {
                        panel_id: pid_side.clone(),
                        side: next_side,
                    });
                }),
                key: DiffKey::None,
            });

        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(2.0)
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
        let page_ctx = try_consume_context::<PageSurfaceContext>();

        let mut grip_hovered = use_state(|| false);
        let mut dragging = use_state(|| false);
        let mut drag_start_x = use_state(|| 0.0f32);
        let mut drag_start_width = use_state(|| self.resolved.width);
        let mut last_click_ms = use_state(|| 0u128);
        let mut card_area = use_state(|| (0.0f32, 0.0f32, 0.0f32, 0.0f32));
        let mut slide_hide_armed = use_state(|| false);

        let panel_id = self.resolved.panel_id.clone();
        let side = self.resolved.prefs.side;
        let layout = self.resolved.effective_layout;
        let width = self.resolved.width;
        let max_width = self.resolved.max_width;
        let resize_disabled = self.resolved.resize_disabled;
        let focused = self.resolved.focused;
        let open_target = !self.resolved.closing;
        let static_snapshot = dock_state.is_some_and(|ds| ds.pinned_ring_phase.read().is_some());

        // DF-01: 360ms spring entrance and 180ms ease-in exit.
        let presence_anim = use_animation_with_dependencies(&open_target, move |conf, is_open| {
            if static_snapshot {
                conf.on_creation(OnCreation::Finish);
            } else {
                conf.on_creation(OnCreation::Run);
            }
            conf.on_change(OnChange::Rerun);
            if *is_open {
                AnimNum::new(0.0, 1.0)
                    .duration(if static_snapshot {
                        Duration::ZERO
                    } else {
                        motion::DOCK_OPEN
                    })
                    .ease(Ease::Out)
                    .function(motion::spring_fn())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(if static_snapshot {
                        Duration::ZERO
                    } else {
                        motion::DOCK_CLOSE
                    })
                    .ease(Ease::In)
                    .function(motion::ease_in_fn())
            }
        });

        // DF-02: 2px inner accent ring fading over 150ms ease-out when focused.
        let focus_ring_anim = use_animation_with_dependencies(&focused, move |conf, is_focused| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *is_focused { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::DOCK_UI_TRANSITION)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let is_dragging = *dragging.read();
        let grip_active = !resize_disabled && (is_dragging || *grip_hovered.read());
        // DF-07: Resize indicator bar grows smoothly (150ms ease-out) from 40px/0.0 to 72px/1.0 in ThemeRole::Accent.
        let grip_anim = use_animation_with_dependencies(&grip_active, move |conf, active| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *active { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::DOCK_UI_TRANSITION)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let presence_val = presence_anim.get().value();
        let presence_opacity = presence_val.clamp(0.0, 1.0);
        let focus_ring_t = focus_ring_anim.get().value().clamp(0.0, 1.0);
        let grip_t = grip_anim.get().value().clamp(0.0, 1.0);

        // DF-06: Use SurfaceGlass + backdrop blur in Split layout too (matching Float & Slide).
        let panel_bg = theme.color(ThemeRole::SurfaceGlass);

        let border = Border::new()
            .width(1.0)
            .alignment(BorderAlignment::Inner)
            .fill(theme.color(ThemeRole::BorderSubtle));

        // Float and Slide share identical corner radius (user decision); Split uses the docked radius.
        let radius: CornerRadius = match layout {
            SidePanelLayout::Float | SidePanelLayout::Slide => CornerRadius::new_all(FLOAT_RADIUS),
            SidePanelLayout::Split => CornerRadius::new_all(DOCKED_RADIUS),
        };

        let shadow = match layout {
            SidePanelLayout::Split => match theme.scheme {
                ResolvedScheme::Dark => Shadow::new().y(3.0).blur(8.0).color(Color::from_argb(61, 0, 0, 0)),
                ResolvedScheme::Light => Shadow::new().y(3.0).blur(8.0).color(Color::from_argb(46, 0, 0, 0)),
            },
            SidePanelLayout::Float | SidePanelLayout::Slide => theme.shadows.float_drop.to_shadow(),
        };

        // Page-facing resize grip (left edge when docked Right, right edge when docked Left).
        let grip_pos = match side {
            SidePanelSide::Right => Position::new_absolute().left(0.0).top(0.0),
            SidePanelSide::Left => Position::new_absolute().right(0.0).top(0.0),
        };
        let bar_h = 40.0 + 32.0 * grip_t;

        let core_dbl = core.clone();
        let pid_down = panel_id.clone();
        let pid_move = panel_id.clone();
        let core_up = core.clone();
        let pid_up = panel_id.clone();
        let core_up_check = core.clone();

        // Layer above nested PageSurface frames and use a 1/255 alpha fill so Freya's hit-tester treats the grip
        // strip as opaque and never passes hover/drag pointer events through to the website behind it.
        let resize_grip = rect()
            .layer(Layer::Relative(GRIP_RELATIVE_LAYER))
            .position(grip_pos)
            .width(Size::px(RESIZE_HIT_WIDTH))
            .height(Size::percent(100.0))
            .background(Color::from_argb(1, 0, 0, 0))
            .center()
            .a11y_role(AccessibilityRole::Splitter)
            .a11y_alt(if resize_disabled {
                "No room to widen it beside this tab: float it to make it wider"
            } else {
                "Resize side panel (double-click to reset)"
            })
            .cursor(if resize_disabled {
                CursorIcon::Default
            } else {
                CursorIcon::ColResize
            })
            .on_pointer_enter(move |_| grip_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| grip_hovered.set_if_modified(false))
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_mouse_move(|e: Event<MouseEventData>| e.stop_propagation())
            .on_mouse_up(|e: Event<MouseEventData>| e.stop_propagation())
            .on_pointer_down(move |e: Event<PointerEventData>| {
                if resize_disabled || e.button() != Some(MouseButton::Left) {
                    return;
                }
                e.stop_propagation();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let prev = *last_click_ms.peek();
                if now.saturating_sub(prev) < 350 {
                    // Double-click resets panel width to default.
                    last_click_ms.set(0);
                    dragging.set(false);
                    if let Some(mut ds) = dock_state {
                        ds.resize_preview.set(None);
                        ds.set_slide_resizing(&pid_down, false);
                    }
                    core_dbl.invoke_or_log(Command::PanelResetWidth {
                        panel_id: pid_down.clone(),
                    });
                    return;
                }
                last_click_ms.set(now);
                dragging.set(true);
                drag_start_x.set(e.global_location().x as f32);
                drag_start_width.set(width);
                if let Some(mut ds) = dock_state {
                    ds.set_slide_resizing(&pid_down, true);
                    ds.bump_slide_timer_seq(&pid_down);
                }
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
                let upper_w = max_width.max(min_w);
                let next_w = (*drag_start_width.peek() + delta).clamp(min_w, upper_w);
                if let Some(mut ds) = dock_state {
                    ds.resize_preview.set(Some((pid_move.clone(), next_w)));
                }
            })
            .on_global_pointer_up(move |_| {
                if !*dragging.peek() {
                    return;
                }
                dragging.set(false);
                let maybe_final_w = dock_state.and_then(|ds| ds.preview_width_for(&pid_up));
                let start_w = *drag_start_width.peek();
                if let Some(mut ds) = dock_state {
                    ds.resize_preview.set(None);
                    ds.set_slide_resizing(&pid_up, false);
                }
                // XPN-03: Only commit width when the pointer actually moved during drag.
                if let Some(final_w) = maybe_final_w
                    && (final_w - start_w).abs() >= 0.5
                {
                    core_up.invoke_or_log(Command::PanelSetWidth {
                        panel_id: pid_up.clone(),
                        width: final_w,
                    });
                }
                // DF-08: Resume Slide auto-hide if pointer already left the panel while resizing.
                if layout == SidePanelLayout::Slide
                    && let Some(mut ds) = dock_state
                    && !ds.is_slide_pointer_inside(&pid_up)
                {
                    let keep = {
                        let model = core_up_check.model().peek();
                        slide_panel_keep_open(&model, ds, &pid_up)
                    };
                    if !keep {
                        let seq = ds.bump_slide_timer_seq(&pid_up);
                        let pid = pid_up.clone();
                        let core_timer = core_up_check.clone();
                        spawn(async move {
                            timer(motion::DOCK_KEEP_HOVER).await;
                            let still_keep = {
                                let model = core_timer.model().peek();
                                slide_panel_keep_open(&model, ds, &pid)
                            };
                            if ds.slide_timer_seq_for(&pid) == seq && !ds.is_slide_pointer_inside(&pid) && !still_keep {
                                ds.close_panel(&pid);
                            }
                        });
                    }
                }
            })
            .maybe_child((grip_t > 0.01).then(|| {
                rect()
                    .width(Size::px(3.0))
                    .height(Size::px(bar_h))
                    .corner_radius(CornerRadius::new_all(2.0))
                    .background(theme.color(ThemeRole::Accent))
                    .opacity(grip_t)
            }));

        // Subtle 1px panel border drawn above nested PageSurface frames so web chat sites never erase the panel outline.
        let border_overlay = rect()
            .layer(Layer::Relative(BORDER_RELATIVE_LAYER))
            .position(Position::new_absolute().left(0.0).top(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .corner_radius(radius)
            .interactive(Interactive::No)
            .border(border);

        // DF-02: 2px inner accent ring drawn above all content (including nested PageSurface frames), non-interactive.
        let focus_ring_overlay = (focus_ring_t > 0.01).then(|| {
            rect()
                .layer(Layer::Relative(FOCUS_RING_RELATIVE_LAYER))
                .position(Position::new_absolute().left(0.0).top(0.0))
                .width(Size::percent(100.0))
                .height(Size::percent(100.0))
                .corner_radius(radius)
                .interactive(Interactive::No)
                .border(
                    Border::new()
                        .width(2.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.rgba(ThemeRole::Accent).with_alpha(focus_ring_t).to_color()),
                )
        });

        let panel_offset = self.resolved.offset;
        let pid_focus = panel_id.clone();
        let pid_enter = panel_id.clone();
        let pid_leave = panel_id.clone();
        let pid_global_move = panel_id.clone();
        let core_leave = core.clone();
        let core_global_move = core.clone();
        let pid_esc = panel_id.clone();
        let core_esc = core.clone();
        let page_ctx_esc = page_ctx.clone();

        let mut card = rect()
            .width(Size::px(width))
            .height(Size::fill())
            .corner_radius(radius)
            .overflow(Overflow::Clip)
            .background(panel_bg)
            .shadow(shadow)
            .on_sized(move |e: Event<SizedEventData>| {
                let next = (e.area.min_x(), e.area.min_y(), e.area.width(), e.area.height());
                if *card_area.peek() != next {
                    card_area.set(next);
                }
            })
            .on_pointer_down(move |_| {
                if open_target && let Some(mut ds) = dock_state {
                    ds.set_focus(Some(pid_focus.clone()));
                }
            })
            .on_pointer_enter(move |_| {
                slide_hide_armed.set_if_modified(false);
                if let Some(mut ds) = dock_state {
                    ds.set_slide_pointer_inside(&pid_enter, true);
                    ds.bump_slide_timer_seq(&pid_enter);
                }
            })
            .on_global_pointer_move(move |e: Event<PointerEventData>| {
                if layout != SidePanelLayout::Slide || !open_target {
                    return;
                }
                let area = *card_area.peek();
                if area.2 <= 1.0 || area.3 <= 1.0 {
                    return;
                }
                let px = e.global_location().x as f32;
                let py = e.global_location().y as f32;
                let inside = slide_keep_open_contains(area, side, panel_offset, px, py);
                let Some(mut ds) = dock_state else {
                    return;
                };
                if inside {
                    slide_hide_armed.set_if_modified(false);
                    ds.set_slide_pointer_inside(&pid_global_move, true);
                    ds.bump_slide_timer_seq(&pid_global_move);
                } else if !*slide_hide_armed.peek() {
                    ds.set_slide_pointer_inside(&pid_global_move, false);
                    let keep = {
                        let model = core_global_move.model().peek();
                        slide_panel_keep_open(&model, ds, &pid_global_move)
                    };
                    if !keep {
                        slide_hide_armed.set(true);
                        let seq = ds.bump_slide_timer_seq(&pid_global_move);
                        let pid = pid_global_move.clone();
                        let core_timer = core_global_move.clone();
                        spawn(async move {
                            timer(motion::DOCK_KEEP_HOVER).await;
                            let still_keep = {
                                let model = core_timer.model().peek();
                                slide_panel_keep_open(&model, ds, &pid)
                            };
                            if ds.slide_timer_seq_for(&pid) == seq && !ds.is_slide_pointer_inside(&pid) && !still_keep {
                                ds.close_panel(&pid);
                            }
                        });
                    }
                }
            })
            .on_pointer_leave(move |e: Event<PointerEventData>| {
                if layout != SidePanelLayout::Slide {
                    return;
                }
                let px = e.global_location().x as f32;
                let py = e.global_location().y as f32;
                if slide_keep_open_contains(*card_area.peek(), side, panel_offset, px, py) {
                    return;
                }
                if let Some(mut ds) = dock_state {
                    ds.set_slide_pointer_inside(&pid_leave, false);
                    let keep = {
                        let model = core_leave.model().peek();
                        slide_panel_keep_open(&model, ds, &pid_leave)
                    };
                    if keep {
                        return;
                    }
                    slide_hide_armed.set(true);
                    let seq = ds.bump_slide_timer_seq(&pid_leave);
                    let pid = pid_leave.clone();
                    let core_timer = core_leave.clone();
                    spawn(async move {
                        timer(motion::DOCK_KEEP_HOVER).await;
                        let still_keep = {
                            let model = core_timer.model().peek();
                            slide_panel_keep_open(&model, ds, &pid)
                        };
                        if ds.slide_timer_seq_for(&pid) == seq && !ds.is_slide_pointer_inside(&pid) && !still_keep {
                            ds.close_panel(&pid);
                        }
                    });
                }
            })
            // XPN-01 & XIX-03: Central Escape handling for every focused side panel.
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if !open_target || e.key != Key::Named(NamedKey::Escape) {
                    return;
                }
                let Some(mut ds) = dock_state else {
                    return;
                };
                if ds.focused_panel.peek().as_ref() != Some(&pid_esc) {
                    return;
                }
                e.stop_propagation();
                if pid_esc.0 == AGENT_PANEL_ID {
                    if *ds.model_card_open.peek() {
                        ds.model_card_open.set(false);
                        return;
                    }
                    if *ds.instructions_open.peek() {
                        ds.instructions_open.set(false);
                        return;
                    }
                } else if pid_esc.0 == WEBCHAT_PANEL_ID {
                    if *ds.webchat_accounts_open.peek() {
                        ds.webchat_accounts_open.set(false);
                        return;
                    }
                    if *ds.webchat_add_site_open.peek() {
                        ds.webchat_add_site_open.set(false);
                        return;
                    }
                }
                if layout == SidePanelLayout::Split {
                    ds.set_focus(None);
                    if let Some(ctx) = &page_ctx_esc {
                        ctx.request_focus_active_surface();
                    }
                } else {
                    core_esc.invoke_or_log(Command::PanelClose {
                        panel_id: pid_esc.clone(),
                    });
                    if let Some(ctx) = &page_ctx_esc {
                        ctx.request_focus_active_surface();
                    }
                }
            })
            .child(self.content.clone())
            .child(resize_grip)
            .child(border_overlay)
            .maybe_child(focus_ring_overlay);

        if theme.glass.blur > 0.0 {
            card = card.backdrop_blur(theme.glass.blur);
        }

        let side_sign = match side {
            SidePanelSide::Right => 1.0_f32,
            SidePanelSide::Left => -1.0_f32,
        };

        match layout {
            SidePanelLayout::Split => {
                let slot_w = (width * presence_opacity).max(0.0);
                let slide_x = side_sign * (width + SPLIT_GAP) * (1.0 - presence_val);
                if (presence_val - 1.0).abs() < 0.002 && open_target {
                    card
                } else {
                    rect()
                        .width(Size::px(slot_w))
                        .height(Size::fill())
                        .overflow(Overflow::Clip)
                        .opacity(presence_opacity)
                        .child(
                            rect()
                                .width(Size::px(width))
                                .height(Size::fill())
                                .offset_x(slide_x)
                                .child(card),
                        )
                }
            }
            SidePanelLayout::Float | SidePanelLayout::Slide => {
                let inset = FLOAT_INSET + self.resolved.offset;
                let pos = match side {
                    SidePanelSide::Right => Position::new_absolute().right(inset).top(FLOAT_INSET),
                    SidePanelSide::Left => Position::new_absolute().left(inset).top(FLOAT_INSET),
                };
                // Include the inset so Float and Slide panels leave the window fully on close/hide.
                let slide_x = side_sign * (width + inset + 24.0) * (1.0 - presence_val);
                rect()
                    .layer(Layer::OverlayLevel(DOCK_OVERLAY_LEVEL))
                    .position(pos)
                    .height(Size::percent(100.0))
                    .padding(Gaps::new(0.0, 0.0, FLOAT_INSET * 2.0, 0.0))
                    .offset_x(slide_x)
                    .opacity(presence_opacity)
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

        let (title, icon_name, plugin_state) = {
            let model = core.model().read();
            let entry = model
                .contributions
                .side_panels
                .iter()
                .find(|entry| entry.id == panel_id);
            let title = entry.map(|e| e.title.clone()).unwrap_or_else(|| panel_id.0.clone());
            let icon = entry.map(|e| e.icon.clone()).unwrap_or_else(|| "layers".to_string());
            let plugin_info = entry.and_then(|e| match &e.source {
                ContributionSource::Plugin { plugin_id } => {
                    let host_status = service_status_in(&model.services, ServiceId::PluginHost);
                    let plugin = model.plugins.iter().find(|p| p.id.0 == *plugin_id).cloned();
                    Some((
                        PluginId::new(plugin_id.clone()),
                        host_status,
                        model.plugins_enabled,
                        plugin,
                    ))
                }
                _ => None,
            });
            (title, icon, plugin_info)
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

        let body: Element = if let Some((plugin_id, host_status, master_enabled, maybe_plugin)) = plugin_state {
            let host_down = !matches!(host_status, ServiceStatus::Running);
            let plugin_stopped_reason = if !master_enabled {
                Some("All plugins are disabled in Settings.".to_string())
            } else if host_down {
                match &host_status {
                    ServiceStatus::Stopped { reason } => Some(
                        reason
                            .clone()
                            .unwrap_or_else(|| service_down_summary(ServiceId::PluginHost).to_string()),
                    ),
                    ServiceStatus::Restarting | ServiceStatus::Starting => {
                        Some("PluginHost is restarting…".to_string())
                    }
                    ServiceStatus::Running => None,
                }
            } else if let Some(ref p) = maybe_plugin {
                if !p.enabled {
                    Some(format!("{} is disabled.", p.name))
                } else {
                    match &p.status {
                        PluginStatus::Running => None,
                        PluginStatus::Disabled => Some(format!("{} is disabled.", p.name)),
                        PluginStatus::Restarting => Some(format!("Restarting {}…", p.name)),
                        PluginStatus::Stopped { reason } => Some(
                            reason
                                .clone()
                                .unwrap_or_else(|| "This plugin sandbox stopped unexpectedly.".to_string()),
                        ),
                    }
                }
            } else {
                Some("Plugin is not installed.".to_string())
            };

            if let Some(reason) = plugin_stopped_reason {
                let is_restarting = matches!(host_status, ServiceStatus::Restarting | ServiceStatus::Starting)
                    || maybe_plugin
                        .as_ref()
                        .is_some_and(|p| matches!(p.status, PluginStatus::Restarting));
                let role = if is_restarting {
                    ThemeRole::Warning
                } else {
                    ThemeRole::Danger
                };
                let core_restart = core.clone();
                let pid_restart = plugin_id.clone();
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .padding(Gaps::new(24.0, 28.0, 24.0, 28.0))
                    .center()
                    .spacing(12.0)
                    .child(
                        rect()
                            .width(Size::px(48.0))
                            .height(Size::px(48.0))
                            .corner_radius(CornerRadius::new_all(14.0))
                            .background(theme.rgba(role).with_alpha(0.15).to_color())
                            .border(
                                Border::new()
                                    .width(1.0)
                                    .alignment(BorderAlignment::Inner)
                                    .fill(theme.rgba(role).with_alpha(0.40).to_color()),
                            )
                            .center()
                            .child(Icon::new("warning").size(22.0).role(role)),
                    )
                    .child(
                        label()
                            .width(Size::fill())
                            .text(if is_restarting {
                                format!("{title} restarting")
                            } else {
                                format!("{title} stopped")
                            })
                            .font_size(14.5)
                            .font_weight(FontWeight::BOLD)
                            .text_align(TextAlign::Center)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .child(
                        rect().width(Size::fill()).max_width(Size::px(280.0)).child(
                            label()
                                .width(Size::fill())
                                .text(reason)
                                .font_size(12.0)
                                .line_height(1.48)
                                .text_align(TextAlign::Center)
                                .color(theme.color(ThemeRole::TextMuted)),
                        ),
                    )
                    .child(PluginRestartButton {
                        on_restart: EventHandler::new(move |_| {
                            core_restart.invoke_or_log(Command::PluginRestart {
                                plugin_id: pid_restart.clone(),
                            });
                        }),
                    })
                    .into_element()
            } else if let Some(ui_node) = maybe_plugin.and_then(|p| p.panel_ui) {
                ScrollView::new()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .child(DeclaredUi::new(ui_node))
                    .into_element()
            } else {
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .center()
                    .child(
                        label()
                            .text(format!("{title} is running"))
                            .font_size(13.0)
                            .color(theme.color(ThemeRole::TextMuted)),
                    )
                    .into_element()
            }
        } else {
            rect()
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
                )
                .into_element()
        };

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

/// Stepped `2s` ticking indicator dot for [`DockClosedPill`], mounted only while an agent is actively working so idle
/// pills incur zero redraws (`DF-14`).
#[derive(Clone, PartialEq)]
struct DockPillTickDot {
    color: Color,
    static_snapshot: bool,
}

impl Component for DockPillTickDot {
    fn render(&self) -> impl IntoElement {
        let static_snapshot = self.static_snapshot;
        let cycle = use_animation(move |conf| {
            if static_snapshot {
                conf.on_creation(OnCreation::Finish);
            } else {
                conf.on_creation(OnCreation::Run);
                conf.on_finish(OnFinish::restart());
            }
            AnimNum::new(0.0, 8.0)
                .duration(Duration::from_millis(2000))
                .function(Function::Linear)
        });

        let opacity = if static_snapshot {
            1.0
        } else {
            let step = (cycle.get().value().floor() as u32) % 8;
            match step {
                3 | 4 => 0.35,
                2 | 5 => 0.68,
                _ => 1.0,
            }
        };

        rect()
            .width(Size::px(8.0))
            .height(Size::px(8.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(self.color)
            .opacity(opacity)
    }
}

#[derive(Clone, PartialEq)]
struct ClosedPillSnapshot {
    text: String,
    dot_color: Color,
    is_working: bool,
    target_panel: SidePanelId,
}

/// Floating glass pill shown centered at the bottom of the page stage when the Agent panel is closed while working or
/// unseen, OR when the Agents panel is closed while another tab's agent needs permission (`DF-14`, `XIX-06`).
#[derive(Clone, PartialEq)]
pub struct DockClosedPill;

impl Component for DockClosedPill {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let mut hovered = use_state(|| false);
        let mut last_pill = use_state(|| None::<ClosedPillSnapshot>);
        let mut pill_width = use_state(|| 190.0f32);

        let agent_open = dock_state.is_some_and(|ds| ds.is_agent_open());
        let agents_open = dock_state.is_some_and(|ds| ds.is_agents_open());
        let static_snapshot = dock_state.is_some_and(|ds| ds.pinned_ring_phase.read().is_some());
        let stage_w = dock_state.map(|ds| *ds.stage_width.read()).unwrap_or(1120.0);

        let active_pill = {
            let model = core.model().read();
            let server_running = model
                .services
                .iter()
                .any(|s| s.service == ServiceId::AgentServer && matches!(s.status, ServiceStatus::Running));
            if !server_running {
                None
            } else {
                // Separate Agent pill (active tab) vs Agents waiting pill (other tabs) per DF-14.
                let active_tab_pill = if !agent_open {
                    model
                        .active_tab
                        .and_then(|tid| model.active_conversation_for_tab(tid))
                        .and_then(|c| {
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
                                    Some(ClosedPillSnapshot {
                                        text: format!("{agent_name} is working{clock}"),
                                        dot_color: accent,
                                        is_working: true,
                                        target_panel: SidePanelId::new(AGENT_PANEL_ID),
                                    })
                                }
                                ConversationStatus::WaitingForUser => Some(ClosedPillSnapshot {
                                    text: format!("{agent_name} needs permission · open"),
                                    dot_color: theme.color(ThemeRole::Warning),
                                    is_working: true,
                                    target_panel: SidePanelId::new(AGENT_PANEL_ID),
                                }),
                                _ if c.unseen => Some(ClosedPillSnapshot {
                                    text: format!("{agent_name} finished · open"),
                                    dot_color: accent,
                                    is_working: false,
                                    target_panel: SidePanelId::new(AGENT_PANEL_ID),
                                }),
                                _ => None,
                            }
                        })
                } else {
                    None
                };

                active_tab_pill.or_else(|| {
                    if agents_open {
                        return None;
                    }
                    let waiting_count = model
                        .conversations
                        .iter()
                        .filter(|c| c.status == ConversationStatus::WaitingForUser)
                        .count();
                    if waiting_count > 0 {
                        let text = if waiting_count == 1 {
                            "1 agent needs you · open".to_string()
                        } else {
                            format!("{waiting_count} agents need you · open")
                        };
                        Some(ClosedPillSnapshot {
                            text,
                            dot_color: theme.color(ThemeRole::Warning),
                            is_working: true,
                            target_panel: SidePanelId::new(AGENTS_PANEL_ID),
                        })
                    } else if !agent_open {
                        model
                            .conversations
                            .iter()
                            .find(|c| c.status == ConversationStatus::Working || c.unseen)
                            .map(|c| {
                                let agent_name = model
                                    .agent(&c.agent_id)
                                    .map(|a| a.name.clone())
                                    .unwrap_or_else(|| "Agent".to_string());
                                let accent = active_agent_accent(&model, &theme);
                                if c.status == ConversationStatus::Working {
                                    let clock = if c.elapsed_secs > 0 {
                                        format!(" · {}", format_elapsed_clock(c.elapsed_secs))
                                    } else {
                                        String::new()
                                    };
                                    ClosedPillSnapshot {
                                        text: format!("{agent_name} is working{clock}"),
                                        dot_color: accent,
                                        is_working: true,
                                        target_panel: SidePanelId::new(AGENT_PANEL_ID),
                                    }
                                } else {
                                    ClosedPillSnapshot {
                                        text: format!("{agent_name} finished · open"),
                                        dot_color: accent,
                                        is_working: false,
                                        target_panel: SidePanelId::new(AGENT_PANEL_ID),
                                    }
                                }
                            })
                    } else {
                        None
                    }
                })
            }
        };

        if let Some(ref info) = active_pill
            && last_pill.peek().as_ref() != Some(info)
        {
            last_pill.set(Some(info.clone()));
        }

        let visible = active_pill.is_some();
        // DF-14: 300ms spring entrance (translateY 10->0, scale 0.94->1), 110ms ease-in exit.
        let pill_anim = use_animation_with_dependencies(&visible, move |conf, shown| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *shown {
                AnimNum::new(0.0, 1.0)
                    .duration(if static_snapshot {
                        Duration::ZERO
                    } else {
                        motion::DOCK_PILL_IN
                    })
                    .ease(Ease::Out)
                    .function(motion::spring_fn())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(if static_snapshot {
                        Duration::ZERO
                    } else {
                        motion::DOCK_PILL_OUT
                    })
                    .ease(Ease::In)
                    .function(motion::ease_in_fn())
            }
        });

        let anim_v = pill_anim.get().value();
        let opacity = anim_v.clamp(0.0, 1.0);
        if opacity <= 0.005 {
            return rect().width(Size::px(0.0)).height(Size::px(0.0));
        }

        let Some(info) = active_pill.or_else(|| last_pill.read().clone()) else {
            return rect().width(Size::px(0.0)).height(Size::px(0.0));
        };

        let bg = if *hovered.read() {
            theme.color(ThemeRole::SurfaceRaised)
        } else {
            theme.color(ThemeRole::SurfaceGlass)
        };
        let core_click = core.clone();
        let target_panel = info.target_panel.clone();
        let offset_y = if visible {
            10.0 * (1.0 - anim_v)
        } else {
            6.0 * (1.0 - opacity)
        };
        let scale = if visible {
            0.94 + 0.06 * anim_v
        } else {
            0.97 + 0.03 * opacity
        };

        let dot: Element = if info.is_working {
            DockPillTickDot {
                color: info.dot_color,
                static_snapshot,
            }
            .into_element()
        } else {
            rect()
                .width(Size::px(8.0))
                .height(Size::px(8.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(info.dot_color)
                .into_element()
        };

        let measured_w = *pill_width.read();
        let pill_left = ((stage_w - measured_w) * 0.5).max(8.0);

        // XIX-06: Position the pill directly without a full-width wrapper so bottom-edge page clicks pass through
        // untouched while keeping the pill itself 100% clickable (Freya's `Interactive::No` cascades to children).
        let mut pill = rect()
            .layer(Layer::OverlayLevel(DOCK_OVERLAY_LEVEL))
            .position(Position::new_absolute().bottom(16.0).left(pill_left))
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(info.text.clone())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .padding(Gaps::new(6.0, 14.0, 6.0, 12.0))
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
            .offset_y(offset_y)
            .opacity(opacity)
            .on_sized(move |e: Event<SizedEventData>| {
                let w = e.area.width();
                if w > 10.0 && (*pill_width.peek() - w).abs() > 1.0 {
                    pill_width.set(w);
                }
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_pointer_down(move |e: Event<PointerEventData>| {
                if e.button() == Some(MouseButton::Left) {
                    e.stop_propagation();
                    core_click.invoke_or_log(Command::PanelOpen {
                        panel_id: target_panel.clone(),
                        focus: true,
                    });
                }
            })
            .child(dot)
            .child(
                label()
                    .text(info.text)
                    .font_size(12.0)
                    .font_weight(FontWeight::MEDIUM)
                    .color(theme.color(ThemeRole::Text))
                    .max_lines(1),
            );

        if (scale - 1.0).abs() > 0.002 {
            pill = pill.scale((scale, scale));
        }

        if theme.glass.blur > 0.0 {
            pill = pill.backdrop_blur(theme.glass.blur);
        }

        pill
    }
}

/// Pulsing slide handle indicator bar, mounted only while a panel's agent is actively working (`DF-12`).
#[derive(Clone, PartialEq)]
struct SlideHandlePulseBar {
    height: f32,
    base_opacity: f32,
    accent: Color,
    static_snapshot: bool,
}

impl Component for SlideHandlePulseBar {
    fn render(&self) -> impl IntoElement {
        let static_snapshot = self.static_snapshot;
        let cycle = use_animation(move |conf| {
            if static_snapshot {
                conf.on_creation(OnCreation::Finish);
            } else {
                conf.on_creation(OnCreation::Run);
                conf.on_finish(OnFinish::restart());
            }
            AnimNum::new(0.0, 1.0)
                .duration(Duration::from_millis(1200))
                .function(Function::Linear)
        });

        let pulse = if static_snapshot {
            1.0
        } else {
            // 1.2s smooth ease-in-out pulse dipping to 0.35 opacity (`zendia.css:3912-3920`).
            let p = cycle.get().value().clamp(0.0, 1.0);
            let tri = (2.0 * p - 1.0).abs();
            let wave = tri * tri * (3.0 - 2.0 * tri);
            0.35 + 0.65 * wave
        };

        rect()
            .width(Size::px(4.0))
            .height(Size::px(self.height))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(self.accent)
            .opacity((self.base_opacity * pulse).clamp(0.2, 1.0))
    }
}

#[derive(Clone, PartialEq)]
struct SlideHandleEntry {
    panel_id: SidePanelId,
    title: String,
    side: SidePanelSide,
    accent: Color,
    upper_half: bool,
    lower_half: bool,
    unseen: bool,
    running: bool,
}

/// Single Slide-mode edge handle for one closed panel (`DF-11`, `DF-12`).
#[derive(Clone, PartialEq)]
struct DockSlideHandleItem {
    entry: SlideHandleEntry,
    key: DiffKey,
}

impl KeyExt for DockSlideHandleItem {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for DockSlideHandleItem {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let dock_state = try_use_dock_state();
        let mut hovered = use_state(|| false);

        let expanded = *hovered.read() || self.entry.unseen;
        let static_snapshot = dock_state.is_some_and(|ds| ds.pinned_ring_phase.read().is_some());

        // DF-12: 4x56px (0.6) -> 4x88px (1.0) over 150ms ease-out on hover or unseen.
        let expand_anim = use_animation_with_dependencies(&expanded, move |conf, is_exp| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *is_exp { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::DOCK_UI_TRANSITION)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let t = expand_anim.get().value().clamp(0.0, 1.0);
        let bar_h = 56.0 + 32.0 * t;
        let bar_opacity = 0.6 + 0.4 * t;

        let pos = match (self.entry.side, self.entry.lower_half) {
            (SidePanelSide::Right, false) => Position::new_absolute().right(0.0).top(0.0),
            (SidePanelSide::Left, false) => Position::new_absolute().left(0.0).top(0.0),
            (SidePanelSide::Right, true) => Position::new_absolute().right(0.0).bottom(0.0),
            (SidePanelSide::Left, true) => Position::new_absolute().left(0.0).bottom(0.0),
        };
        let height = if self.entry.lower_half || self.entry.upper_half {
            Size::percent(50.0)
        } else {
            Size::percent(100.0)
        };

        let pid_enter = self.entry.panel_id.clone();
        let pid_leave = self.entry.panel_id.clone();
        let pid_click = self.entry.panel_id.clone();
        let core_hover = core.clone();
        let core_click = core.clone();

        let bar: Element = if self.entry.running {
            SlideHandlePulseBar {
                height: bar_h,
                base_opacity: bar_opacity,
                accent: self.entry.accent,
                static_snapshot,
            }
            .into_element()
        } else {
            rect()
                .width(Size::px(4.0))
                .height(Size::px(bar_h))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(self.entry.accent)
                .opacity(bar_opacity)
                .into_element()
        };

        rect()
            .layer(Layer::OverlayLevel(DOCK_OVERLAY_LEVEL))
            .position(pos)
            .width(Size::px(SLIDE_HANDLE_WIDTH))
            .height(height)
            .center()
            .cursor(CursorIcon::Pointer)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Open {}", self.entry.title))
            .on_pointer_enter(move |_| {
                hovered.set_if_modified(true);
                if let Some(mut ds) = dock_state {
                    let seq = ds.bump_slide_timer_seq(&pid_enter);
                    let core_open = core_hover.clone();
                    let pid = pid_enter.clone();
                    spawn(async move {
                        timer(motion::DOCK_HANDLE_HOVER).await;
                        if ds.slide_timer_seq_for(&pid) == seq && !ds.is_open(&pid) {
                            core_open.invoke_or_log(Command::PanelOpen {
                                panel_id: pid,
                                focus: false,
                            });
                        }
                    });
                }
            })
            .on_pointer_leave(move |_| {
                hovered.set_if_modified(false);
                if let Some(mut ds) = dock_state {
                    ds.bump_slide_timer_seq(&pid_leave);
                }
            })
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                core_click.invoke_or_log(Command::PanelOpen {
                    panel_id: pid_click.clone(),
                    focus: true,
                });
            })
            .child(bar)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Builds edge handle elements for EVERY closed side panel whose effective layout is `Slide` (including Split falling
/// back to Slide): Agent, Agents, Web Chat, and contributed plugin panels (`DF-11`, `DF-12`, `XPN-07`).
///
/// Returned as direct stage children rather than inside a full-stage wrapper so each `10px` strip receives pointer
/// hover/click events without blocking page clicks anywhere else on the stage.
pub fn build_dock_slide_handles(
    model: &BrowserModel,
    ds: DockHostState,
    theme: &crate::ui::theme::ResolvedTheme,
    stage_width: f32,
) -> Vec<Element> {
    let effective_stage = if stage_width > 200.0 { stage_width } else { 1120.0 };
    let agent_accent = active_agent_accent(model, theme);
    let theme_accent = theme.color(ThemeRole::Accent);

    let mut candidate_ids: Vec<(SidePanelId, String)> = vec![
        (SidePanelId::new(AGENT_PANEL_ID), "Agent panel".to_string()),
        (SidePanelId::new(AGENTS_PANEL_ID), "Agents panel".to_string()),
        (SidePanelId::new(WEBCHAT_PANEL_ID), "Web chat panel".to_string()),
    ];
    for contrib in &model.contributions.side_panels {
        if !candidate_ids.iter().any(|(id, _)| id == &contrib.id) {
            candidate_ids.push((contrib.id.clone(), contrib.title.clone()));
        }
    }

    let mut raw: Vec<SlideHandleEntry> = Vec::new();
    for (panel_id, title) in candidate_ids {
        if ds.is_open(&panel_id) {
            continue;
        }
        let prefs = model.panel_prefs(&panel_id);
        let min_w = SidePanelPrefs::min_width_for(panel_id.as_str());
        let resolved = resolve_panel_layout_with_min(prefs, effective_stage, 1.0, min_w);
        if resolved.effective_layout != SidePanelLayout::Slide {
            continue;
        }

        let (accent, unseen, running) = match panel_id.0.as_str() {
            AGENT_PANEL_ID => {
                let conv = model.active_tab.and_then(|tid| model.active_conversation_for_tab(tid));
                (
                    agent_accent,
                    conv.is_some_and(|c| c.unseen),
                    conv.is_some_and(|c| {
                        matches!(
                            c.status,
                            ConversationStatus::Working | ConversationStatus::WaitingForUser
                        )
                    }),
                )
            }
            AGENTS_PANEL_ID => {
                let waiting = model
                    .conversations
                    .iter()
                    .any(|c| c.status == ConversationStatus::WaitingForUser);
                let working = model
                    .conversations
                    .iter()
                    .any(|c| c.status == ConversationStatus::Working);
                (
                    if waiting {
                        theme.color(ThemeRole::Warning)
                    } else {
                        theme_accent
                    },
                    waiting,
                    working || waiting,
                )
            }
            WEBCHAT_PANEL_ID => (theme_accent, false, false),
            _ => (theme_accent, false, false),
        };

        raw.push(SlideHandleEntry {
            panel_id,
            title,
            side: prefs.side,
            accent,
            upper_half: false,
            lower_half: false,
            unseen,
            running,
        });
    }

    // DF-11: If Web Chat shares a side with another closed Slide handle (e.g. Agent), partition that edge into upper
    // and lower halves so both handles remain independently hoverable and clickable without overlapping.
    let sides_with_non_webchat: Vec<SidePanelSide> = raw
        .iter()
        .filter(|e| e.panel_id.0 != WEBCHAT_PANEL_ID)
        .map(|e| e.side)
        .collect();
    let sides_with_webchat: Vec<SidePanelSide> = raw
        .iter()
        .filter(|e| e.panel_id.0 == WEBCHAT_PANEL_ID)
        .map(|e| e.side)
        .collect();
    for entry in &mut raw {
        if entry.panel_id.0 == WEBCHAT_PANEL_ID && sides_with_non_webchat.contains(&entry.side) {
            entry.lower_half = true;
        } else if entry.panel_id.0 != WEBCHAT_PANEL_ID && sides_with_webchat.contains(&entry.side) {
            entry.upper_half = true;
        }
    }

    raw.into_iter()
        .map(|entry| {
            let key = format!("slide-handle:{}", entry.panel_id.0);
            DockSlideHandleItem {
                entry,
                key: DiffKey::None,
            }
            .key(key)
            .into_element()
        })
        .collect()
}

#[derive(Clone, PartialEq)]
struct PluginRestartButton {
    on_restart: EventHandler<()>,
}

impl Component for PluginRestartButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let feedback = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_restart = self.on_restart.clone();

        let accent = theme.rgba(ThemeRole::Accent);
        let bg = feedback.background(
            accent.to_color(),
            accent.with_alpha(0.88).to_color(),
            accent.with_alpha(0.76).to_color(),
        );
        let border = focus_ring_border(focus() == Focus::Keyboard, &theme, Border::new());

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Restart Plugin")
            .margin(Gaps::new(6.0, 0.0, 0.0, 0.0))
            .height(Size::px(30.0))
            .padding(Gaps::new(0.0, 14.0, 0.0, 12.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(bg)
            .border(border)
            .scale(feedback.scale())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(6.0)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_restart.call(());
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if e.key == Key::Named(NamedKey::Enter) || matches!(&e.key, Key::Character(c) if c == " ") {
                    e.stop_propagation();
                }
            })
            .child(Icon::new("reload").size(12.5).role(ThemeRole::TextOnAccent))
            .child(
                label()
                    .text("Restart Plugin")
                    .font_size(11.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(theme.color(ThemeRole::TextOnAccent)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slide_keep_open_includes_panel_and_edge_gap_only() {
        let right_card = (1100.0, 18.0, 480.0, 964.0);
        // Inside card
        assert!(slide_keep_open_contains(
            right_card,
            SidePanelSide::Right,
            0.0,
            1300.0,
            500.0
        ));
        // Inside gap between right edge of panel (1580) and window edge (1598)
        assert!(slide_keep_open_contains(
            right_card,
            SidePanelSide::Right,
            0.0,
            1585.0,
            500.0
        ));
        assert!(slide_keep_open_contains(
            right_card,
            SidePanelSide::Right,
            0.0,
            1598.0,
            500.0
        ));
        // Moved onto page to the left of the panel
        assert!(!slide_keep_open_contains(
            right_card,
            SidePanelSide::Right,
            0.0,
            1095.0,
            500.0
        ));
        // Moved above stage into top bar
        assert!(!slide_keep_open_contains(
            right_card,
            SidePanelSide::Right,
            0.0,
            1300.0,
            4.0
        ));

        let left_card = (18.0, 18.0, 340.0, 964.0);
        // Inside gap between window edge (0) and left edge of panel (18)
        assert!(slide_keep_open_contains(
            left_card,
            SidePanelSide::Left,
            0.0,
            4.0,
            500.0
        ));
        // Moved onto page to the right of the panel (358)
        assert!(!slide_keep_open_contains(
            left_card,
            SidePanelSide::Left,
            0.0,
            365.0,
            500.0
        ));
    }

    #[test]
    fn overlay_clearance_matches_for_float_and_slide() {
        let base = ResolvedDockPanel {
            panel_id: SidePanelId::new(AGENT_PANEL_ID),
            prefs: SidePanelPrefs::default(),
            effective_layout: SidePanelLayout::Float,
            width: 420.0,
            max_width: 1100.0,
            resize_disabled: false,
            split_fell_back: false,
            focused: true,
            closing: false,
            offset: 20.0,
        };
        let slide = ResolvedDockPanel {
            effective_layout: SidePanelLayout::Slide,
            ..base.clone()
        };
        assert_eq!(base.overlay_clearance(), slide.overlay_clearance());
        assert_eq!(slide.overlay_clearance(), 20.0 + 420.0 + FLOAT_INSET);
    }
}
