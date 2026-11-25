/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Top-level window shell: space gradient backdrop, optional top toolbar, sidebar frame, resize handle, and rounded
//! inset page card.
//!
//! In compact mode the page card fills the window and the sidebar and top toolbar hide. Moving the pointer to the
//! window edge where they live slides them out over the page with Zen's 0.25 s spring; they slide back once the pointer
//! leaves them, unless the user pinned the sidebar out with `layout.compact_reveal_sidebar`.

use crate::ui::agent::{AgentPanel, AgentsPanel};
use crate::ui::chrome::layout::{
    BackdropEndpoints, ChromePalette, ELEMENT_SEPARATION, PAGE_CARD_RADIUS, SPACE_GRADIENT_CROSSFADE,
    TOP_TOOLBAR_HEIGHT, ToolbarSlots, page_card_margins, window_backdrop_endpoints,
};
use crate::ui::chrome::sidebar::{SidebarFrame, SidebarResizeHandle, try_use_sidebar_state};
use crate::ui::chrome::toasts::ToastHost;
use crate::ui::chrome::toolbar::TopToolbar;
use crate::ui::core_view::use_core;
use crate::ui::panels::{
    DockClosedPill, DockFrame, DockSlideHandle, SPLIT_GAP, active_agent_accent, resolve_active_dock_panel,
    try_use_dock_state,
};
use crate::ui::theme::CHROME_OVERLAY_LEVEL;
use crate::ui::theme::{ResolvedScheme, motion, use_theme};
use crate::ui::vim::statusline::try_use_vim_statusline_state;
use crate::ui::vim::vim_mode_backdrop_endpoints;
use crate::ui::webchat::WebChatPanel;
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::agent::{AGENT_PANEL_ID, AGENTS_PANEL_ID};
use zendia_browser_core::contributions::{SidePanelLayout, SidePanelSide};
use zendia_browser_core::model::{InputMode, SidebarSide, SpaceId};
use zendia_browser_core::webchat::WEBCHAT_PANEL_ID;

/// How close to the window edge the pointer must come to slide hidden chrome out.
const COMPACT_EDGE: f32 = 8.0;
/// How far past the revealed chrome the pointer may stray before it slides back.
const COMPACT_SLACK: f32 = 24.0;
/// Zen's `--zen-compact-mode-time`.
const COMPACT_SLIDE: Duration = Duration::from_millis(250);
const COMPACT_RADIUS: f32 = 10.0;

/// Composes Zendia's complete browser window shell around a caller-supplied page element (real [`PageStage`] in the
/// live window, or a placeholder card in the UI gallery).
#[derive(Clone, PartialEq)]
pub struct WindowShell {
    location: State<String>,
    location_a11y_id: AccessibilityId,
    page: Element,
    interactive_window_controls: bool,
    key: DiffKey,
}

impl KeyExt for WindowShell {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl WindowShell {
    pub fn new(location: State<String>, location_a11y_id: AccessibilityId, page: impl IntoElement) -> Self {
        Self {
            location,
            location_a11y_id,
            page: page.into_element(),
            interactive_window_controls: true,
            key: DiffKey::None,
        }
    }

    /// Disables OS window minimize/maximize/close actions (used when previewing the shell inside `zendia-gallery`).
    pub fn interactive_window_controls(mut self, interactive: bool) -> Self {
        self.interactive_window_controls = interactive;
        self
    }
}

impl Component for WindowShell {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let is_resizing = use_state(|| false);
        let sidebar_state = try_use_sidebar_state();
        let dock_state = try_use_dock_state();
        let statusline_state = try_use_vim_statusline_state();
        let mut shell_area = use_state(|| (0.0f32, 0.0f32, 0.0f32, 0.0f32));
        let mut stage_width = use_state(|| 1120.0f32);
        let mut toolbar_revealed = use_state(|| false);

        let (layout, active_space, toolbar_items, vim_mode_enabled, input_mode, resolved_panel, agent_accent) = {
            let model = core.model().read();
            let resolved_panel = dock_state.and_then(|ds| resolve_active_dock_panel(&model, ds, *stage_width.read()));
            let accent = active_agent_accent(&model, &theme);
            (
                model.layout.clone(),
                model.active_space().cloned(),
                model.contributions.toolbar_items.clone(),
                model.vim_mode_enabled,
                model.input_mode,
                resolved_panel,
                accent,
            )
        };

        // Keep the vim statusline clear of any floating/sliding side panel on its resting corner edge.
        if let Some(sl) = statusline_state {
            let statusline_on_right = layout.sidebar_side == SidebarSide::Left;
            let clearance = resolved_panel
                .as_ref()
                .filter(|p| {
                    let panel_on_right = p.prefs.side == SidePanelSide::Right;
                    panel_on_right == statusline_on_right
                })
                .map(|p| p.overlay_clearance())
                .unwrap_or(0.0);
            sl.set_side_panel_clearance(clearance);
        }

        let target_endpoints = if vim_mode_enabled {
            vim_mode_backdrop_endpoints(input_mode, theme.scheme, &theme)
        } else {
            window_backdrop_endpoints(active_space.as_ref(), theme.scheme)
        };
        let gradient_key: (Option<SpaceId>, ResolvedScheme, bool, InputMode) = (
            active_space.as_ref().map(|s| s.id),
            theme.scheme,
            vim_mode_enabled,
            input_mode,
        );

        let mut from_endpoints = use_state(|| target_endpoints);
        let mut to_endpoints = use_state(|| target_endpoints);
        let mut last_gradient_key = use_state(|| gradient_key);

        if *last_gradient_key.peek() != gradient_key {
            let prev = *to_endpoints.peek();
            from_endpoints.set(prev);
            to_endpoints.set(target_endpoints);
            last_gradient_key.set(gradient_key);
        }

        let crossfade = use_animation_with_dependencies(&gradient_key, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(SPACE_GRADIENT_CROSSFADE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let compact_sidebar = layout.compact_hides_sidebar();
        let compact_toolbar = layout.compact_hides_toolbar() && layout.has_top_toolbar();
        let sidebar_pinned = sidebar_state.is_some_and(|state| *state.compact_pinned.read());
        let sidebar_out = compact_sidebar
            && sidebar_state.is_some_and(|state| *state.compact_revealed.read() || *state.compact_pinned.read());
        let toolbar_out = compact_toolbar && *toolbar_revealed.read();
        let sidebar_slide = use_animation_with_dependencies(&sidebar_out, move |conf, out| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *out { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(COMPACT_SLIDE)
                .ease(Ease::Out)
                .function(motion::spring_fn())
        });
        let toolbar_slide = use_animation_with_dependencies(&toolbar_out, move |conf, out| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *out { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(COMPACT_SLIDE)
                .ease(Ease::Out)
                .function(motion::spring_fn())
        });

        let t = crossfade.get().value().clamp(0.0, 1.0);
        let backdrop = BackdropEndpoints::lerp(&from_endpoints.read(), &to_endpoints.read(), t).to_gradient();
        let slots = ToolbarSlots::resolve(&toolbar_items, &layout);
        let (mut card_top, mut card_right, card_bottom, mut card_left) = page_card_margins(&layout);
        if compact_sidebar {
            card_left = ELEMENT_SEPARATION;
            card_right = ELEMENT_SEPARATION;
        }
        if compact_toolbar || (compact_sidebar && !layout.has_top_toolbar()) {
            card_top = ELEMENT_SEPARATION;
        }

        let make_top_bar = || TopToolbar {
            slots: slots.clone(),
            location: self.location,
            location_a11y_id: self.location_a11y_id,
            interactive_window_controls: self.interactive_window_controls,
        };
        let top_bar = (layout.has_top_toolbar() && !compact_toolbar).then(make_top_bar);
        let floating_top_bar = (compact_toolbar && toolbar_slide.get().value() > 0.001).then(|| {
            let shown = toolbar_slide.get().value();
            rect()
                .layer(Layer::OverlayLevel(CHROME_OVERLAY_LEVEL))
                .position(Position::new_absolute().left(0.0).top(0.0))
                .width(Size::percent(100.0))
                .height(Size::px(TOP_TOOLBAR_HEIGHT))
                .offset_y(-(TOP_TOOLBAR_HEIGHT + COMPACT_SLACK) * (1.0 - shown))
                .background(BackdropEndpoints::lerp(&from_endpoints.read(), &to_endpoints.read(), 1.0).to_gradient())
                .shadow(Shadow::new().y(2.0).blur(12.0).color(Color::from_argb(60, 0, 0, 0)))
                .child(make_top_bar())
        });

        let sidebar = SidebarFrame {
            slots,
            location: self.location,
            location_a11y_id: self.location_a11y_id,
            interactive_window_controls: self.interactive_window_controls,
            is_resizing,
        };

        let resize_handle = SidebarResizeHandle {
            side: layout.sidebar_side,
            expanded: layout.sidebar_expanded,
            current_width: layout.sidebar_width,
            is_resizing,
        };

        let card_shadow = match theme.scheme {
            ResolvedScheme::Dark => Shadow::new()
                .x(0.0)
                .y(3.0)
                .blur(10.0)
                .color(Color::from_argb(72, 0, 0, 0)),
            ResolvedScheme::Light => Shadow::new()
                .x(0.0)
                .y(2.0)
                .blur(8.0)
                .color(Color::from_argb(36, 0, 0, 0)),
        };

        let build_panel_content = |resolved: &crate::ui::panels::ResolvedDockPanel| -> Element {
            match resolved.panel_id.0.as_str() {
                AGENT_PANEL_ID => AgentPanel::new(resolved.clone()).into_element(),
                AGENTS_PANEL_ID => AgentsPanel::new(resolved.clone()).into_element(),
                WEBCHAT_PANEL_ID => WebChatPanel::new(resolved.clone()).into_element(),
                _ => crate::ui::panels::GenericSidePanel::new(resolved.clone(), agent_accent).into_element(),
            }
        };

        let split_panel = resolved_panel
            .as_ref()
            .filter(|p| p.effective_layout == SidePanelLayout::Split)
            .cloned();
        let overlay_panel = resolved_panel
            .as_ref()
            .filter(|p| p.effective_layout != SidePanelLayout::Split)
            .cloned();

        let split_panel_el = split_panel.as_ref().map(|p| {
            DockFrame::new(p.clone(), agent_accent, build_panel_content(p))
                .key(format!("split:{}", p.panel_id.0))
                .into_element()
        });
        let overlay_panel_el = overlay_panel.as_ref().map(|p| {
            DockFrame::new(p.clone(), agent_accent, build_panel_content(p))
                .key(format!("overlay:{}", p.panel_id.0))
                .into_element()
        });

        let page_card_inner = rect()
            .width(Size::flex(1.0))
            .height(Size::fill())
            .corner_radius(CornerRadius::new_all(PAGE_CARD_RADIUS))
            .overflow(Overflow::Clip)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(palette.page_card_border),
            )
            .shadow(card_shadow)
            .on_pointer_down(move |_| {
                if let Some(mut ds) = dock_state {
                    ds.set_focus(None);
                    ds.model_card_open.set_if_modified(false);
                }
            })
            .child(self.page.clone());

        let stage_inner = match (split_panel.as_ref().map(|p| p.prefs.side), split_panel_el) {
            (Some(SidePanelSide::Left), Some(panel_el)) => rect()
                .width(Size::fill())
                .height(Size::fill())
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .spacing(SPLIT_GAP)
                .child(panel_el)
                .child(page_card_inner),
            (Some(SidePanelSide::Right), Some(panel_el)) => rect()
                .width(Size::fill())
                .height(Size::fill())
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .spacing(SPLIT_GAP)
                .child(page_card_inner)
                .child(panel_el),
            _ => rect()
                .width(Size::fill())
                .height(Size::fill())
                .content(Content::Flex)
                .child(page_card_inner),
        };

        let page_card = rect()
            .width(Size::flex(1.0))
            .height(Size::fill())
            .padding(Gaps::new(card_top, card_right, card_bottom, card_left))
            .on_sized(move |e: Event<SizedEventData>| {
                let w = e.area.width();
                if (*stage_width.peek() - w).abs() > 1.0 {
                    stage_width.set(w);
                    if let Some(mut ds) = dock_state {
                        ds.stage_width.set(w);
                    }
                }
            })
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(stage_inner)
                    .maybe_child(overlay_panel_el)
                    .maybe_child(dock_state.is_some().then_some(DockClosedPill))
                    .maybe_child(dock_state.is_some().then_some(DockSlideHandle)),
            );

        let sidebar_side = layout.sidebar_side;
        let sidebar_width = layout.effective_sidebar_width();
        let floating_sidebar = (compact_sidebar && sidebar_slide.get().value() > 0.001).then(|| {
            let shown = sidebar_slide.get().value();
            let hidden_offset = sidebar_width + 2.0 * ELEMENT_SEPARATION + COMPACT_SLACK;
            let (position, offset) = match sidebar_side {
                SidebarSide::Left => (
                    Position::new_absolute().left(0.0).top(0.0),
                    -hidden_offset * (1.0 - shown),
                ),
                SidebarSide::Right => (
                    Position::new_absolute().right(0.0).top(0.0),
                    hidden_offset * (1.0 - shown),
                ),
            };
            rect()
                .layer(Layer::OverlayLevel(CHROME_OVERLAY_LEVEL))
                .position(position)
                .height(Size::percent(100.0))
                .padding(Gaps::new_all(ELEMENT_SEPARATION))
                .offset_x(offset)
                .child(
                    rect()
                        .height(Size::fill())
                        .corner_radius(CornerRadius::new_all(COMPACT_RADIUS))
                        .overflow(Overflow::Clip)
                        .background(backdrop.clone())
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(palette.page_card_border),
                        )
                        .shadow(Shadow::new().y(4.0).blur(18.0).color(Color::from_argb(90, 0, 0, 0)))
                        .child(sidebar.clone()),
                )
        });

        let toasts = rect()
            .layer(Layer::OverlayLevel(CHROME_OVERLAY_LEVEL))
            .position(match sidebar_side {
                SidebarSide::Left => Position::new_absolute()
                    .right(card_right + 12.0)
                    .top(card_top + 12.0 + top_bar_offset(&top_bar)),
                SidebarSide::Right => Position::new_absolute()
                    .left(card_left + 12.0)
                    .top(card_top + 12.0 + top_bar_offset(&top_bar)),
            })
            .child(ToastHost {
                align_left: sidebar_side == SidebarSide::Right,
            });

        let body = if compact_sidebar {
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .child(page_card)
        } else {
            match layout.sidebar_side {
                SidebarSide::Left => rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .child(sidebar)
                    .child(resize_handle)
                    .child(page_card),
                SidebarSide::Right => rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .child(page_card)
                    .child(resize_handle)
                    .child(sidebar),
            }
        };

        // Compact mode: the pointer reaching the sidebar's edge or the top edge slides that chrome out.
        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            if !compact_sidebar && !compact_toolbar {
                return;
            }
            let (x0, y0, width, _) = *shell_area.peek();
            let x = e.global_location().x as f32 - x0;
            let y = e.global_location().y as f32 - y0;
            if compact_sidebar && let Some(state) = sidebar_state {
                let reach = sidebar_width + 2.0 * ELEMENT_SEPARATION + COMPACT_SLACK;
                let (at_edge, away) = match sidebar_side {
                    SidebarSide::Left => (x <= COMPACT_EDGE, x > reach),
                    SidebarSide::Right => (x >= width - COMPACT_EDGE, x < width - reach),
                };
                let mut revealed = state.compact_revealed;
                if at_edge && x >= 0.0 && x <= width {
                    revealed.set_if_modified(true);
                } else if away && !sidebar_pinned {
                    revealed.set_if_modified(false);
                }
            }
            if compact_toolbar {
                if (0.0..=COMPACT_EDGE).contains(&y) {
                    toolbar_revealed.set_if_modified(true);
                } else if y > TOP_TOOLBAR_HEIGHT + COMPACT_SLACK {
                    toolbar_revealed.set_if_modified(false);
                }
            }
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .background(backdrop)
            .color(palette.text)
            .on_sized(move |e: Event<SizedEventData>| {
                let next = (e.area.min_x(), e.area.min_y(), e.area.width(), e.area.height());
                if *shell_area.peek() != next {
                    shell_area.set(next);
                }
            })
            .on_global_pointer_move(on_global_pointer_move)
            .maybe_child(top_bar)
            .child(body)
            .child(toasts)
            .maybe_child(floating_sidebar)
            .maybe_child(floating_top_bar)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Height the in-flow top toolbar takes above the page card, so toasts sit inside the card.
fn top_bar_offset(top_bar: &Option<TopToolbar>) -> f32 {
    if top_bar.is_some() { TOP_TOOLBAR_HEIGHT } else { 0.0 }
}
