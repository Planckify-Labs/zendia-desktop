/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Top-level window shell: space gradient backdrop, optional top toolbar, sidebar frame, resize handle, and rounded
//! inset page card.
//!
//! In compact mode the page card fills the window and the sidebar and top toolbar hide. Moving the pointer to the
//! window edge where they live slides them out over the page with Zen's 0.25 s spring; they slide back after Zen's
//! keep-hover delay (`150 ms` sidebar, `1000 ms` outside-window leave fallback) with a non-overshooting `150 ms` hide
//! transition, unless pinned out (`layout.compact_reveal_sidebar`), flashing on tab switch (`800 ms`), or kept open by
//! an active popup, rename, drag, or focused location bar.

use crate::ui::agent::{AgentPanel, AgentsPanel};
use crate::ui::chrome::address_bar::try_use_address_bar_state;
use crate::ui::chrome::layout::{
    BackdropEndpoints, COMPACT_TOP_BAR_HEIGHT, ChromePalette, ELEMENT_SEPARATION, PAGE_CARD_RADIUS,
    RESIZE_HANDLE_WIDTH, SPACE_GRADIENT_CROSSFADE, TOP_TOOLBAR_HEIGHT, ToolbarSlots, page_card_margins,
    window_backdrop_endpoints_for_appearance, window_backdrop_extra_stops,
};
use crate::ui::chrome::menus::try_use_menu_state;
use crate::ui::chrome::sidebar::{SidebarFrame, SidebarResizeHandle, try_use_sidebar_state};
use crate::ui::chrome::toasts::ToastHost;
use crate::ui::chrome::toolbar::{CompactTopBar, TopToolbar, WindowControls};
use crate::ui::core_view::use_core;
use crate::ui::library::{LIBRARY_SLIDE_DURATION, Library, try_use_library_state};
use crate::ui::panels::{
    DockClosedPill, DockFrame, SPLIT_GAP, active_agent_accent, build_dock_slide_handles, resolve_open_dock_panels,
    try_use_dock_state,
};
use crate::ui::spaces::{SpaceSwipeMode, SpaceTextureOverlay, try_use_space_editing_state};
use crate::ui::theme::{CHROME_OVERLAY_LEVEL, ResolvedScheme, Rgba, ThemeRole, motion, use_theme};
use crate::ui::vim::statusline::try_use_vim_statusline_state;
use crate::ui::webchat::WebChatPanel;
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::agent::{AGENT_PANEL_ID, AGENTS_PANEL_ID};
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{SidePanelLayout, SidePanelSide};
use zendia_browser_core::model::{ConversationStatus, InputMode, SidebarSide, SpaceId, TabId};
use zendia_browser_core::shell_layout::effective_sidebar_width_for_window;
use zendia_browser_core::webchat::WEBCHAT_PANEL_ID;

/// How close to the window edge the pointer must come to slide hidden chrome out.
const COMPACT_EDGE: f32 = 8.0;
/// How far past the revealed chrome the pointer may stray before it slides back.
const COMPACT_SLACK: f32 = 24.0;
/// Corner radius of the floating compact sidebar (`--zen-native-inner-radius`).
const COMPACT_RADIUS: f32 = 8.0;
/// Backdrop blur for the acrylic floating compact sidebar (`zen-compact-mode.css:38`).
const COMPACT_SIDEBAR_BLUR: f32 = 42.0;
/// Opacity of the vim mode tint on the floating compact sidebar, as the acrylic surface's `0.80`, a little more so the
/// mode's color holds over a busy page.
const COMPACT_TINT_ALPHA: f32 = 0.88;
/// Hover delay before revealing hidden top-edge window controls in Single + Left mode (`vertical-tabs-topbar.inc.css:19`).
const TOP_WINDOW_CONTROLS_DELAY: Duration = Duration::from_millis(200);
/// Overlay level for compact floating chrome: sits above dock panels (`DOCK_OVERLAY_LEVEL = 1`) and below popups
/// (`POPUP_OVERLAY_LEVEL = 3`) (`XIX-02`).
const COMPACT_CHROME_OVERLAY_LEVEL: u8 = CHROME_OVERLAY_LEVEL;

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
        let menu_state = try_use_menu_state();
        let address_bar_state = try_use_address_bar_state();
        let space_editing = try_use_space_editing_state();
        let dock_state = try_use_dock_state();
        let library_state = try_use_library_state();
        let library_open = library_state.is_some_and(|state| state.is_open());
        let library_pinned_progress = library_state.and_then(|state| *state.pinned_progress.read());
        let library_slide = use_animation_with_dependencies(&library_open, move |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *open { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(LIBRARY_SLIDE_DURATION)
                .ease(Ease::Out)
                .function(motion::spring_fn())
        });
        let library_progress = library_pinned_progress
            .unwrap_or_else(|| library_slide.get().value().clamp(0.0, 1.02))
            .clamp(0.0, 1.02);
        let statusline_state = try_use_vim_statusline_state();
        let mut shell_area = use_state(|| (0.0f32, 0.0f32, 0.0f32, 0.0f32));
        let mut stage_width = use_state(|| 1120.0f32);
        let mut toolbar_revealed = use_state(|| false);
        let mut toolbar_hide_seq = use_state(|| 0u64);
        let mut last_compact_mode = use_state(|| false);
        let mut last_pointer_in_sidebar = use_state(|| false);
        let mut last_active_tab = use_state(|| None::<TabId>);
        let mut top_controls_revealed = use_state(|| false);
        let mut top_controls_seq = use_state(|| 0u64);

        let library_inserting = crate::ui::vim::statusline::current_library_inserting();
        let (
            layout,
            settings,
            active_tab_id,
            active_in_split,
            glance_open,
            active_space,
            active_appearance,
            swipe_target_space,
            swipe_target_appearance,
            toolbar_items,
            vim_mode_enabled,
            raw_input_mode,
            toolbar_from_sidebar,
            input_mode,
            resolved_panels,
            slide_handle_els,
            agent_accent,
            agent_driving_active,
        ) = {
            let model = core.model().read();
            let cur_stage_w = *stage_width.read();
            let resolved_panels = dock_state
                .map(|ds| resolve_open_dock_panels(&model, ds, cur_stage_w))
                .unwrap_or_default();
            let slide_handle_els = dock_state
                .map(|ds| build_dock_slide_handles(&model, ds, &theme, cur_stage_w))
                .unwrap_or_default();
            let accent = active_agent_accent(&model, &theme);
            let active_space = model.active_space().cloned();
            let active_app = active_space
                .as_ref()
                .and_then(|s| model.space_appearances.get(&s.id).cloned());
            let swipe_target_id = space_editing.and_then(|se| {
                let sw = se.swipe.read();
                (sw.mode == SpaceSwipeMode::SwitchSpace)
                    .then_some(sw.target_space_id)
                    .flatten()
            });
            let swipe_target_space = swipe_target_id.and_then(|id| model.space(id).cloned());
            let swipe_target_app = swipe_target_id.and_then(|id| model.space_appearances.get(&id).cloned());
            let active_in_split = model.active_split_group().is_some();
            let agent_driving_active = model.active_tab.is_some_and(|tab_id| {
                model
                    .conversations
                    .iter()
                    .any(|conv| conv.tab_id == Some(tab_id) && conv.status == ConversationStatus::Working)
            });
            (
                model.layout.clone(),
                model.settings.clone(),
                model.active_tab,
                active_in_split,
                model.glance.is_some(),
                active_space,
                active_app,
                swipe_target_space,
                swipe_target_app,
                model.contributions.toolbar_items.clone(),
                model.vim_mode_enabled,
                model.input_mode,
                model.focus.toolbar_from_sidebar,
                crate::ui::vim::statusline::effective_statusline_mode_with_library(
                    model.input_mode,
                    model.focus.selection.is_empty(),
                    library_inserting,
                ),
                resolved_panels,
                slide_handle_els,
                accent,
                agent_driving_active,
            )
        };

        // Keep the vim statusline clear of any floating/sliding side panel on its resting corner edge.
        if let Some(sl) = statusline_state {
            let statusline_on_right = layout.sidebar_side == SidebarSide::Left;
            let clearance = resolved_panels
                .iter()
                .filter(|p| {
                    let panel_on_right = p.prefs.side == SidePanelSide::Right;
                    panel_on_right == statusline_on_right
                })
                .map(|p| p.overlay_clearance())
                .fold(0.0, f32::max);
            sl.set_side_panel_clearance(clearance);
        }

        // Clear stale compact-mode transient flags whenever compact mode turns off (XCM-07 / XIX-05). When it turns on
        // with the pointer over the sidebar, ignore hover only while the sidebar collapses, as the fork does (XCM-04).
        if *last_compact_mode.peek() != layout.compact_mode {
            last_compact_mode.set(layout.compact_mode);
            if !layout.compact_mode {
                toolbar_revealed.set_if_modified(false);
                if let Some(state) = sidebar_state {
                    let mut revealed = state.compact_revealed;
                    revealed.set_if_modified(false);
                    let mut pinned = state.compact_pinned;
                    pinned.set_if_modified(false);
                    let mut flash = state.compact_flash;
                    flash.set_if_modified(false);
                    let mut ignore = state.compact_ignore_next_hover;
                    ignore.set_if_modified(false);
                }
            } else if *last_pointer_in_sidebar.peek()
                && let Some(state) = sidebar_state
            {
                let mut ignore = state.compact_ignore_next_hover;
                ignore.set_if_modified(true);
                spawn(async move {
                    timer(motion::COMPACT_MODE_MARGIN).await;
                    ignore.set_if_modified(false);
                });
            }
        }

        // Compact mode tab-switch flash (CM-01 / XCM-06): flash sidebar for 800ms on tab switch when enabled,
        // suppressed during split view or Glance.
        if *last_active_tab.peek() != active_tab_id {
            let had_prev = last_active_tab.peek().is_some();
            last_active_tab.set(active_tab_id);
            if had_prev
                && layout.compact_hides_sidebar()
                && settings.compact_toolbar_flash_popup
                && !active_in_split
                && !glance_open
                && let Some(state) = sidebar_state
            {
                let mut flash = state.compact_flash;
                let mut seq_state = state.compact_flash_seq;
                let seq = seq_state.peek().wrapping_add(1);
                seq_state.set(seq);
                flash.set_if_modified(true);
                spawn(async move {
                    timer(motion::COMPACT_FLASH_DURATION).await;
                    if *seq_state.peek() == seq {
                        flash.set_if_modified(false);
                    }
                });
            }
        }

        let draft_appearance = space_editing.and_then(|se| se.active_draft_appearance());
        let effective_appearance = draft_appearance.as_ref().or(active_appearance.as_ref());
        let space_opacity = effective_appearance
            .map(|a| a.opacity)
            .or_else(|| active_space.as_ref().map(|s| s.gradient.opacity))
            .unwrap_or(zendia_browser_core::space_appearance::DEFAULT_GRADIENT_OPACITY);

        let base_target_endpoints = if vim_mode_enabled {
            crate::ui::vim::statusline::vim_mode_backdrop_endpoints_with_opacity(
                input_mode,
                theme.scheme,
                space_opacity,
            )
        } else {
            window_backdrop_endpoints_for_appearance(active_space.as_ref(), effective_appearance, theme.scheme)
        };
        let target_endpoints = if let (Some(se), Some(target_sp)) = (space_editing, swipe_target_space.as_ref()) {
            let sw = se.swipe.read();
            let blend = (sw.offset_x.abs() / layout.effective_sidebar_width().max(120.0)).clamp(0.0, 1.0);
            let neighbor = window_backdrop_endpoints_for_appearance(
                Some(target_sp),
                swipe_target_appearance.as_ref(),
                theme.scheme,
            );
            base_target_endpoints.lerp(&neighbor, blend)
        } else {
            base_target_endpoints
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
        } else if *to_endpoints.peek() != target_endpoints {
            // Live appearance edits or mid-swipe blending update immediately without restarting a space crossfade.
            from_endpoints.set(target_endpoints);
            to_endpoints.set(target_endpoints);
        }

        let texture_overlay = effective_appearance.and_then(|app| {
            (app.texture_amount > 0.005 && app.texture_kind != zendia_browser_core::space_appearance::TextureKind::None)
                .then_some(SpaceTextureOverlay {
                    kind: app.texture_kind,
                    amount: app.texture_amount,
                })
        });

        let crossfade_duration = if vim_mode_enabled {
            motion::VIM_MODE_TINT
        } else {
            SPACE_GRADIENT_CROSSFADE
        };
        let crossfade = use_animation_with_dependencies(&gradient_key, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(crossfade_duration)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let compact_sidebar = layout.compact_hides_sidebar();
        let compact_toolbar = layout.compact_hides_toolbar() && layout.has_top_toolbar();
        let compact_single_top_bar = layout.has_compact_top_bar();
        let has_collapsible_top_bar = compact_toolbar || compact_single_top_bar;
        let animate_sidebar = settings.compact_animate_sidebar;

        // Read keep-open conditions across popups, menus, rename, drag, space editors, and URL bar (CM-02 / XCM-03).
        let menu_open = menu_state.is_some_and(|m| m.has_open_overlay());
        let app_or_ext_menu_open =
            menu_state.is_some_and(|m| *m.app_menu_open.read() || *m.extensions_menu_open.read());
        let create_new_open = menu_state.is_some_and(|m| *m.create_new_open.read());
        let context_menu_open = menu_state.is_some_and(|m| m.context_menu.read().is_some());
        let address_bar_open = address_bar_state.is_some_and(|a| *a.open.read());
        let site_panel_open = address_bar_state.is_some_and(|a| *a.site_panel_open.read());
        let extension_popup_open =
            crate::ui::extensions::try_use_extension_popup_state().is_some_and(|p| p.open.read().is_some());
        let space_editor_open = space_editing.is_some_and(|se| {
            *se.create_form_open.read() || *se.theme_picker_open.read() || se.live_icon_picker_space.read().is_some()
        });
        let sidebar_renaming = sidebar_state.is_some_and(|s| {
            s.renaming_tab.read().is_some() || s.renaming_folder.read().is_some() || s.renaming_space.read().is_some()
        });
        let sidebar_dragging = sidebar_state.is_some_and(|s| s.drag_state.read().is_some());

        // Anchored sidebar/toolbar popups, renames, and drags force the surface open; context menus only keep an
        // already-revealed surface open so right-clicking the page never pops a hidden compact sidebar out.
        let sidebar_force_open = sidebar_renaming
            || sidebar_dragging
            || space_editor_open
            || create_new_open
            || (!layout.has_top_toolbar() && (app_or_ext_menu_open || site_panel_open || extension_popup_open));
        let sidebar_keep_open = sidebar_force_open || context_menu_open;
        let toolbar_force_open = self.location_a11y_id.is_focused()
            || (layout.has_top_toolbar() && (app_or_ext_menu_open || site_panel_open || extension_popup_open));
        let toolbar_keep_open = toolbar_force_open || context_menu_open;

        let sidebar_pinned = sidebar_state.is_some_and(|state| *state.compact_pinned.read());
        let sidebar_flashing = sidebar_state.is_some_and(|state| *state.compact_flash.read());
        let sidebar_hovered = sidebar_state.is_some_and(|state| *state.compact_revealed.read());
        // Auto-reveal compact sidebar/toolbar while Explorer or Toolbar focus mode is active (`VIM-EX-05`, `VIM-TB-02`).
        let focus_reveals_sidebar = vim_mode_enabled
            && (raw_input_mode == InputMode::Explorer
                || (raw_input_mode == InputMode::Toolbar
                    && !layout.has_top_toolbar()
                    && (!compact_single_top_bar || toolbar_from_sidebar)));
        let focus_reveals_toolbar = vim_mode_enabled && raw_input_mode == InputMode::Toolbar;
        let sidebar_out = compact_sidebar
            && !library_open
            && (focus_reveals_sidebar || sidebar_hovered || sidebar_pinned || sidebar_flashing || sidebar_force_open);
        let toolbar_out = (compact_toolbar
            && (focus_reveals_toolbar || *toolbar_revealed.read() || toolbar_force_open))
            || (compact_single_top_bar && (focus_reveals_toolbar || *toolbar_revealed.read()));

        // CM-06: animate docked sidebar margin collapse/expand over 100ms when toggling compact mode.
        let compact_margin_anim = use_animation_with_dependencies(&compact_sidebar, move |conf, is_compact| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *is_compact { (1.0, 0.0) } else { (0.0, 1.0) };
            let dur = if animate_sidebar {
                motion::COMPACT_MODE_MARGIN
            } else {
                Duration::ZERO
            };
            AnimNum::new(from, to)
                .duration(dur)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        // CM-03: 250ms spring open, 150ms non-overshooting ease-out hide.
        let sidebar_slide = use_animation_with_dependencies(&sidebar_out, move |conf, out| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *out {
                let dur = if animate_sidebar {
                    motion::COMPACT_SIDEBAR_OPEN
                } else {
                    Duration::ZERO
                };
                AnimNum::new(0.0, 1.0)
                    .duration(dur)
                    .ease(Ease::Out)
                    .function(motion::spring_fn())
            } else {
                let dur = if animate_sidebar {
                    motion::COMPACT_SIDEBAR_HIDE
                } else {
                    Duration::ZERO
                };
                AnimNum::new(1.0, 0.0)
                    .duration(dur)
                    .ease(Ease::Out)
                    .function(motion::ease_out_fn())
            }
        });

        // CM-03: 150ms ease-in-out height + opacity reveal for compact top toolbar.
        let toolbar_slide = use_animation_with_dependencies(&toolbar_out, move |conf, out| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *out { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::COMPACT_TOOLBAR_TOGGLE)
                .ease(Ease::InOut)
                .function(Function::Quad)
        });

        // DF-03: 2px accent ring on page card when any side panel is open and page holds focus.
        let any_panel_focused = resolved_panels.iter().any(|p| p.focused);
        let page_has_panel_focus_ring = !resolved_panels.is_empty() && !any_panel_focused;
        let page_focus_ring_anim = use_animation_with_dependencies(&page_has_panel_focus_ring, move |conf, on| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *on { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::DOCK_UI_TRANSITION)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        // WIN-05: top-edge hover window controls in Single + Left layout outside compact mode (in compact mode the
        // collapsible CompactTopBar holds Agents, Chat, and the window controls).
        let needs_hover_window_controls = !layout.has_top_toolbar()
            && !compact_single_top_bar
            && layout.sidebar_side == SidebarSide::Left
            && !compact_toolbar;
        let top_controls_shown = needs_hover_window_controls && *top_controls_revealed.read();
        let top_controls_anim = use_animation_with_dependencies(&top_controls_shown, move |conf, shown| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *shown { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::COMPACT_TOOLBAR_TOGGLE)
                .ease(Ease::InOut)
                .function(Function::Quad)
        });

        let t = crossfade.get().value().clamp(0.0, 1.0);
        let (mid_stop, overlay_stop) = if vim_mode_enabled {
            (None, None)
        } else {
            window_backdrop_extra_stops(effective_appearance, theme.scheme)
        };
        let window_focused =
            !self.interactive_window_controls || *Platform::get().current_window().is_app_focused.read();
        let inactive_dim = !window_focused && settings.grey_out_inactive_windows;
        let backdrop = if inactive_dim {
            let grey = match theme.scheme {
                ResolvedScheme::Dark => Rgba::rgb(34, 34, 38),
                ResolvedScheme::Light => Rgba::rgb(228, 228, 232),
            };
            BackdropEndpoints {
                start: grey,
                end: grey,
                angle_deg: 135.0,
            }
            .to_gradient()
        } else {
            BackdropEndpoints::lerp(&from_endpoints.read(), &to_endpoints.read(), t).to_gradient_with_mid(mid_stop)
        };
        // With vim mode on, the floating compact sidebar carries the mode's tint as the window does: in compact mode it is
        // most of the chrome there is, so a plain acrylic card would read as vim mode being off.
        let compact_card_tint = (vim_mode_enabled && !inactive_dim).then(|| {
            let endpoints = BackdropEndpoints::lerp(&from_endpoints.read(), &to_endpoints.read(), t);
            BackdropEndpoints {
                start: endpoints.start.with_alpha(COMPACT_TINT_ALPHA),
                end: endpoints.end.with_alpha(COMPACT_TINT_ALPHA),
                ..endpoints
            }
            .to_gradient()
        });
        let third_stop_overlay = (!inactive_dim).then_some(overlay_stop).flatten().map(|tint| {
            let grad = LinearGradient::new()
                .angle(180.0)
                .stop((Color::from(tint), 0.0))
                .stop((Color::from(tint.with_alpha(0.0)), 100.0));
            rect()
                .position(Position::new_absolute().left(0.0).top(0.0))
                .width(Size::percent(100.0))
                .height(Size::percent(100.0))
                .interactive(Interactive::No)
                .background(grad)
        });
        let slots = ToolbarSlots::resolve(&toolbar_items, &layout);
        let (mut card_top, mut card_right, card_bottom, mut card_left) = page_card_margins(&layout);
        if compact_sidebar {
            card_left = ELEMENT_SEPARATION;
            card_right = ELEMENT_SEPARATION;
        }
        if has_collapsible_top_bar {
            // The in-flow collapsible top bar already provides `ELEMENT_SEPARATION` at rest and expands to toolbar
            // height on hover, pushing the page, open side panels, and toasts down smoothly.
            card_top = 0.0;
        }

        let make_top_bar = || TopToolbar {
            slots: slots.clone(),
            location: self.location,
            location_a11y_id: self.location_a11y_id,
            interactive_window_controls: self.interactive_window_controls,
        };
        let toolbar_shown = toolbar_slide.get().value().clamp(0.0, 1.0);
        let (in_flow_top_bar, in_flow_top_bar_height): (Option<Element>, f32) =
            if layout.has_top_toolbar() && !compact_toolbar {
                (Some(make_top_bar().into_element()), TOP_TOOLBAR_HEIGHT)
            } else if compact_toolbar {
                let h = ELEMENT_SEPARATION + (TOP_TOOLBAR_HEIGHT - ELEMENT_SEPARATION) * toolbar_shown;
                let interactive = if toolbar_shown > 0.1 {
                    Interactive::Yes
                } else {
                    Interactive::No
                };
                let bar = rect()
                    .width(Size::fill())
                    .height(Size::px(h))
                    .overflow(Overflow::Clip)
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(TOP_TOOLBAR_HEIGHT))
                            .opacity(toolbar_shown)
                            .interactive(interactive)
                            .child(make_top_bar()),
                    )
                    .into_element();
                (Some(bar), h)
            } else if compact_single_top_bar {
                let h = ELEMENT_SEPARATION + (COMPACT_TOP_BAR_HEIGHT - ELEMENT_SEPARATION) * toolbar_shown;
                let bar = CompactTopBar {
                    slots: slots.clone(),
                    location: self.location,
                    location_a11y_id: self.location_a11y_id,
                    interactive_window_controls: self.interactive_window_controls,
                    revealed: toolbar_shown,
                }
                .into_element();
                (Some(bar), h)
            } else {
                (None, 0.0)
            };

        let window_w = shell_area.read().2;
        let sidebar_width = effective_sidebar_width_for_window(&layout, window_w);

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

        // WIN-02: `--zen-big-shadow: rgba(0, 0, 0, 0.24) 0px 3px 8px` unified across dark and light.
        let card_shadow = Shadow::new()
            .x(0.0)
            .y(3.0)
            .blur(8.0)
            .color(Color::from_argb(61, 0, 0, 0));

        let build_panel_content = |resolved: &crate::ui::panels::ResolvedDockPanel| -> Element {
            match resolved.panel_id.0.as_str() {
                AGENT_PANEL_ID => AgentPanel::new(resolved.clone()).into_element(),
                AGENTS_PANEL_ID => AgentsPanel::new(resolved.clone()).into_element(),
                WEBCHAT_PANEL_ID => WebChatPanel::new(resolved.clone()).into_element(),
                _ => crate::ui::panels::GenericSidePanel::new(resolved.clone(), agent_accent).into_element(),
            }
        };

        // Every open panel shows at once. Split panels sit in the row beside the page, the first opened nearest the
        // window edge; floating and sliding panels float over the stage at their side.
        let split_el = |p: &crate::ui::panels::ResolvedDockPanel| {
            DockFrame::new(p.clone(), agent_accent, build_panel_content(p))
                .key(format!("split:{}", p.panel_id.0))
                .into_element()
        };
        let split_left: Vec<Element> = resolved_panels
            .iter()
            .filter(|p| p.effective_layout == SidePanelLayout::Split && p.prefs.side == SidePanelSide::Left)
            .map(split_el)
            .collect();
        let split_right: Vec<Element> = resolved_panels
            .iter()
            .rev()
            .filter(|p| p.effective_layout == SidePanelLayout::Split && p.prefs.side == SidePanelSide::Right)
            .map(split_el)
            .collect();
        let overlay_panel_els: Vec<Element> = resolved_panels
            .iter()
            .filter(|p| p.effective_layout != SidePanelLayout::Split)
            .map(|p| {
                DockFrame::new(p.clone(), agent_accent, build_panel_content(p))
                    .key(format!("overlay:{}", p.panel_id.0))
                    .into_element()
            })
            .collect();

        let page_focus_t = page_focus_ring_anim.get().value().clamp(0.0, 1.0);
        let page_border = if page_focus_t > 0.02 {
            let accent_rgba = theme.rgba(ThemeRole::Accent);
            Border::new()
                .width(1.0 + page_focus_t)
                .alignment(BorderAlignment::Inner)
                .fill(accent_rgba.with_alpha(page_focus_t).to_color())
        } else {
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(palette.page_card_border)
        };

        let page_border_overlay = rect()
            .layer(Layer::Relative(36))
            .position(Position::new_absolute().left(0.0).top(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .corner_radius(CornerRadius::new_all(PAGE_CARD_RADIUS))
            .interactive(Interactive::No)
            .border(page_border);

        let page_card_inner = rect()
            .width(Size::flex(1.0))
            .height(Size::fill())
            .corner_radius(CornerRadius::new_all(PAGE_CARD_RADIUS))
            .overflow(Overflow::Clip)
            .shadow(card_shadow)
            .on_pointer_down({
                let page_click_core = core.clone();
                move |_| {
                    if let Some(mut ds) = dock_state {
                        ds.set_focus(None);
                        ds.model_card_open.set_if_modified(false);
                    }
                    if let Some(lib) = library_state
                        && *lib.open.peek()
                    {
                        lib.suspend_keyboard_focus();
                        page_click_core.invoke_or_log(Command::LibraryFocusSet { focused: false });
                    }
                    // Clicking outside sidebar/toolbar/chips closes active Explorer, Toolbar, or Window focus mode
                    // (`VIM-EX-02`, `VIM-TB-02`, `VIM-WM-06`).
                    let (vim_on, active_mode) = {
                        let m = page_click_core.model().peek();
                        (m.vim_mode_enabled, m.input_mode)
                    };
                    if vim_on {
                        match active_mode {
                            InputMode::Explorer => {
                                page_click_core.invoke_or_log(Command::ExplorerClose);
                            }
                            InputMode::Toolbar => {
                                page_click_core.invoke_or_log(Command::ToolbarFocusClose);
                            }
                            InputMode::Window => {
                                page_click_core.invoke_or_log(Command::WindowModeClose);
                            }
                            _ => {}
                        }
                    }
                }
            })
            .child(self.page.clone())
            .maybe_child(agent_driving_active.then_some(AgentDrivingPageGlow {
                accent: agent_accent,
                static_snapshot: dock_state.is_some_and(|ds| ds.pinned_ring_phase.read().is_some()),
            }))
            .child(page_border_overlay);

        let stage_inner = rect()
            .width(Size::fill())
            .height(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .spacing(SPLIT_GAP)
            .children(split_left)
            .child(page_card_inner)
            .children(split_right);

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
                    .children(overlay_panel_els)
                    .maybe_child(dock_state.is_some().then_some(DockClosedPill))
                    .children(slide_handle_els),
            );

        let chrome_opacity = if inactive_dim { 0.8 } else { 1.0 };
        // With multiple toolbars and the sidebar in place, the top toolbar belongs to the page's column, so the sidebar
        // keeps the window's full height beside it.
        let (window_top_bar, column_top_bar) = if layout.top_toolbar_beside_sidebar() {
            (None, in_flow_top_bar)
        } else {
            (in_flow_top_bar, None)
        };
        let page_card = match column_top_bar {
            Some(bar) => rect()
                .width(Size::flex(1.0))
                .height(Size::fill())
                .content(Content::Flex)
                .child(rect().width(Size::fill()).opacity(chrome_opacity).child(bar))
                .child(page_card.width(Size::fill()).height(Size::flex(1.0))),
            None => page_card,
        };

        let sidebar_side = layout.sidebar_side;
        // XCM-01: keep floating compact sidebar cleanly below the in-flow top toolbar / compact top bar.
        let floating_sidebar_top_inset = if layout.has_top_toolbar() && !compact_toolbar {
            TOP_TOOLBAR_HEIGHT + ELEMENT_SEPARATION
        } else if has_collapsible_top_bar {
            in_flow_top_bar_height.max(ELEMENT_SEPARATION)
        } else {
            ELEMENT_SEPARATION
        };
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
            // CM-04 & XCM-08: translucent acrylic surface (`rgba(19,19,19,0.80)` dark / `rgba(233,233,233,0.80)` light)
            // with `42px` backdrop blur, 8px radius, subtle inner outline, and `--zen-big-shadow`.
            let acrylic_bg = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(204, 19, 19, 19),
                ResolvedScheme::Light => Color::from_argb(204, 233, 233, 233),
            };
            let compact_border = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(38, 255, 255, 255),
                ResolvedScheme::Light => Color::from_argb(30, 0, 0, 0),
            };
            let card = rect()
                .height(Size::fill())
                .corner_radius(CornerRadius::new_all(COMPACT_RADIUS))
                .overflow(Overflow::Clip);
            let mut card = match compact_card_tint.clone() {
                Some(tint) => card.background(tint),
                None => card.background(acrylic_bg),
            }
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(compact_border),
            )
            .shadow(Shadow::new().y(3.0).blur(8.0).color(Color::from_argb(61, 0, 0, 0)))
            .child(sidebar.clone());
            if theme.glass.blur > 0.0 {
                card = card.backdrop_blur(COMPACT_SIDEBAR_BLUR);
            }
            rect()
                .layer(Layer::OverlayLevel(COMPACT_CHROME_OVERLAY_LEVEL))
                .position(position)
                .height(Size::percent(100.0))
                .padding(Gaps::new(
                    floating_sidebar_top_inset,
                    ELEMENT_SEPARATION,
                    ELEMENT_SEPARATION,
                    ELEMENT_SEPARATION,
                ))
                .offset_x(offset)
                .child(card)
        });

        // WIN-05: top-right hover window controls in Single + Left layout.
        let top_controls_t = top_controls_anim.get().value().clamp(0.0, 1.0);
        let floating_window_controls = (needs_hover_window_controls && top_controls_t > 0.01).then(|| {
            let pill_bg = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(215, 24, 25, 34),
                ResolvedScheme::Light => Color::from_argb(225, 242, 244, 248),
            };
            rect()
                .layer(Layer::OverlayLevel(COMPACT_CHROME_OVERLAY_LEVEL))
                .position(Position::new_absolute().right(ELEMENT_SEPARATION).top(4.0))
                .padding(Gaps::new(2.0, 4.0, 2.0, 4.0))
                .corner_radius(CornerRadius::new_all(8.0))
                .background(pill_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(palette.page_card_border),
                )
                .opacity(top_controls_t)
                .offset_y(-6.0 * (1.0 - top_controls_t))
                .shadow(Shadow::new().y(2.0).blur(8.0).color(Color::from_argb(50, 0, 0, 0)))
                .child(WindowControls {
                    interactive: self.interactive_window_controls,
                    compact: true,
                })
        });

        // XIX-04: offset toasts clear of any open side panels on the toast's corner side so panel header buttons stay clickable.
        let toast_panel_side = match sidebar_side {
            SidebarSide::Left => SidePanelSide::Right,
            SidebarSide::Right => SidePanelSide::Left,
        };
        let toast_panel_clearance: f32 = resolved_panels
            .iter()
            .filter(|p| p.prefs.side == toast_panel_side)
            .map(|p| match p.effective_layout {
                SidePanelLayout::Split => p.width + SPLIT_GAP,
                SidePanelLayout::Float | SidePanelLayout::Slide => p.overlay_clearance(),
            })
            .fold(0.0f32, f32::max);

        let toasts = rect()
            .layer(Layer::OverlayLevel(CHROME_OVERLAY_LEVEL))
            .position(match sidebar_side {
                SidebarSide::Left => Position::new_absolute()
                    .right(card_right + 12.0 + toast_panel_clearance)
                    .top(card_top + 12.0 + in_flow_top_bar_height),
                SidebarSide::Right => Position::new_absolute()
                    .left(card_left + 12.0 + toast_panel_clearance)
                    .top(card_top + 12.0 + in_flow_top_bar_height),
            })
            .child(ToastHost {
                align_left: sidebar_side == SidebarSide::Right,
            });

        let library_visible = library_state.is_some() && (library_open || library_progress > 0.001);
        let docked_margin_factor = compact_margin_anim.get().value().clamp(0.0, 1.0);
        let body = if library_visible {
            let space_count = core.model().read().spaces.len();
            let shell_w = shell_area.read().2.max(800.0);
            if let Some(lib) = library_state {
                let mut vw = lib.viewport_width;
                if (*vw.peek() - shell_w).abs() > 1.0 {
                    vw.set(shell_w);
                }
            }
            let total_lib_w = library_state
                .map(|state| state.total_width_for(space_count, shell_w))
                .unwrap_or(420.0);
            let base_w = if compact_sidebar {
                0.0
            } else {
                sidebar_width + RESIZE_HANDLE_WIDTH
            };
            let clamped_progress = library_progress.clamp(0.0, 1.0);
            let slot_w = (base_w + (total_lib_w - base_w) * library_progress).max(0.0);
            // Keep the library's inner edge flush against the moving page card as it slides in.
            let slide_offset = (slot_w - total_lib_w).min(0.0);
            let lib_pos = match layout.sidebar_side {
                SidebarSide::Left => Position::new_absolute().top(0.0).left(slide_offset),
                SidebarSide::Right => Position::new_absolute().top(0.0).right(slide_offset),
            };
            let toolbox_opacity = (1.0 - clamped_progress * 4.0).clamp(0.0, 1.0);
            let lib_opacity = ((clamped_progress - 0.15) * 3.0).clamp(0.0, 1.0);
            let fading_sidebar = (!compact_sidebar && toolbox_opacity > 0.01).then(|| {
                let sb_pos = match layout.sidebar_side {
                    SidebarSide::Left => Position::new_absolute().top(0.0).left(0.0),
                    SidebarSide::Right => Position::new_absolute().top(0.0).right(0.0),
                };
                rect()
                    .position(sb_pos)
                    .height(Size::percent(100.0))
                    .opacity(toolbox_opacity)
                    .child(sidebar.clone())
            });
            let library_slot = rect()
                .width(Size::px(slot_w))
                .height(Size::fill())
                .overflow(Overflow::Clip)
                .maybe_child(fading_sidebar)
                .child(
                    rect()
                        .position(lib_pos)
                        .width(Size::px(total_lib_w))
                        .height(Size::percent(100.0))
                        .opacity(lib_opacity)
                        .child(Library),
                );
            match layout.sidebar_side {
                SidebarSide::Left => rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .child(library_slot)
                    .child(page_card),
                SidebarSide::Right => rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .child(page_card)
                    .child(library_slot),
            }
        } else if compact_sidebar && docked_margin_factor <= 0.005 {
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .child(page_card)
        } else if docked_margin_factor < 0.995 {
            // CM-06: 100ms margin collapse/expand transition between docked and compact layouts.
            let full_w = sidebar_width + RESIZE_HANDLE_WIDTH;
            let slot_w = (full_w * docked_margin_factor).max(0.0);
            let slide_x = match layout.sidebar_side {
                SidebarSide::Left => -(full_w - slot_w),
                SidebarSide::Right => full_w - slot_w,
            };
            let animated_slot = rect()
                .width(Size::px(slot_w))
                .height(Size::fill())
                .overflow(Overflow::Clip)
                .child(
                    rect()
                        .width(Size::px(full_w))
                        .height(Size::fill())
                        .direction(Direction::Horizontal)
                        .offset_x(slide_x)
                        .child(sidebar),
                );
            match layout.sidebar_side {
                SidebarSide::Left => rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .child(animated_slot)
                    .child(page_card),
                SidebarSide::Right => rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .child(page_card)
                    .child(animated_slot),
            }
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

        // Compact mode & top-edge hover controller:
        // - Edge reveal respects `compact_show_on_hover` and suppresses while floating overlays cover the edge.
        // - Sidebar uses Zen's 150ms keep-hover delay (`CM-01`) and stays open while keep-open conditions hold (`CM-02`).
        // - Top toolbar hides when pointer moves down into page (staying open when keep-open conditions hold, `XCM-03`).
        let panel_resizing = dock_state.is_some_and(|ds| ds.is_any_panel_resizing());
        let suppress_compact_reveal = !settings.compact_show_on_hover
            || address_bar_open
            || menu_open
            || site_panel_open
            || glance_open
            || library_open
            || panel_resizing;

        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            let (x0, y0, width, _) = *shell_area.peek();
            let x = e.global_location().x as f32 - x0;
            let y = e.global_location().y as f32 - y0;
            let in_sidebar_zone = match sidebar_side {
                SidebarSide::Left => x >= 0.0 && x <= sidebar_width + 2.0 * ELEMENT_SEPARATION + COMPACT_SLACK,
                SidebarSide::Right => {
                    x <= width && x >= width - (sidebar_width + 2.0 * ELEMENT_SEPARATION + COMPACT_SLACK)
                }
            };
            last_pointer_in_sidebar.set_if_modified(in_sidebar_zone);

            if needs_hover_window_controls {
                let in_controls_zone = (0.0..=38.0).contains(&y) && x >= (width - 140.0).max(0.0) && x <= width;
                let at_top_edge = (0.0..=COMPACT_EDGE).contains(&y) && x >= (width - 140.0).max(0.0) && x <= width;
                if at_top_edge && !*top_controls_revealed.peek() && !suppress_compact_reveal {
                    let seq = top_controls_seq.peek().wrapping_add(1);
                    top_controls_seq.set(seq);
                    spawn(async move {
                        timer(TOP_WINDOW_CONTROLS_DELAY).await;
                        if *top_controls_seq.peek() == seq {
                            top_controls_revealed.set_if_modified(true);
                        }
                    });
                } else if !in_controls_zone {
                    let seq = top_controls_seq.peek().wrapping_add(1);
                    top_controls_seq.set(seq);
                    top_controls_revealed.set_if_modified(false);
                }
            }

            if !compact_sidebar && !has_collapsible_top_bar {
                return;
            }

            if compact_sidebar && let Some(state) = sidebar_state {
                let reach = sidebar_width + 2.0 * ELEMENT_SEPARATION + COMPACT_SLACK;
                let (at_edge, inside_zone, away) = match sidebar_side {
                    SidebarSide::Left => (x <= COMPACT_EDGE, x <= reach, x > reach),
                    SidebarSide::Right => (x >= width - COMPACT_EDGE, x >= width - reach, x < width - reach),
                };
                let mut revealed = state.compact_revealed;
                let mut hide_seq = state.compact_hide_seq;
                let ignore_next = state.compact_ignore_next_hover;

                if at_edge && x >= 0.0 && x <= width && !suppress_compact_reveal && !*ignore_next.peek() {
                    // Cancel any pending hide timer and reveal.
                    let seq = hide_seq.peek().wrapping_add(1);
                    hide_seq.set(seq);
                    revealed.set_if_modified(true);
                } else if inside_zone && *revealed.peek() {
                    // Pointer returned inside revealed sidebar: cancel pending 150ms hide timer.
                    let seq = hide_seq.peek().wrapping_add(1);
                    hide_seq.set(seq);
                } else if away && *revealed.peek() && !sidebar_pinned && !sidebar_keep_open {
                    // CM-01: keep sidebar open for 150ms before hiding.
                    let seq = hide_seq.peek().wrapping_add(1);
                    hide_seq.set(seq);
                    spawn(async move {
                        timer(motion::COMPACT_SIDEBAR_KEEP_HOVER).await;
                        if *hide_seq.peek() == seq {
                            revealed.set_if_modified(false);
                        }
                    });
                }
            }

            if has_collapsible_top_bar {
                let expanded_bar_h = if compact_single_top_bar {
                    COMPACT_TOP_BAR_HEIGHT
                } else {
                    TOP_TOOLBAR_HEIGHT
                };
                if (0.0..=COMPACT_EDGE).contains(&y) && !suppress_compact_reveal {
                    let seq = toolbar_hide_seq.peek().wrapping_add(1);
                    toolbar_hide_seq.set(seq);
                    toolbar_revealed.set_if_modified(true);
                } else if y <= expanded_bar_h + COMPACT_SLACK && *toolbar_revealed.peek() {
                    let seq = toolbar_hide_seq.peek().wrapping_add(1);
                    toolbar_hide_seq.set(seq);
                } else if y > expanded_bar_h + COMPACT_SLACK && !toolbar_keep_open {
                    let seq = toolbar_hide_seq.peek().wrapping_add(1);
                    toolbar_hide_seq.set(seq);
                    toolbar_revealed.set_if_modified(false);
                }
            }
        };

        // XCM-07: When the pointer leaves the window, hold any revealed compact edge open for Linux's 1000ms fallback
        // (`zen.view.compact.toolbar-hide-after-hover.duration`) before collapsing if it did not re-enter.
        let on_pointer_leave = move |_: Event<PointerEventData>| {
            if compact_sidebar
                && let Some(state) = sidebar_state
                && *state.compact_revealed.peek()
                && !sidebar_pinned
                && !sidebar_keep_open
            {
                let mut revealed = state.compact_revealed;
                let mut hide_seq = state.compact_hide_seq;
                let seq = hide_seq.peek().wrapping_add(1);
                hide_seq.set(seq);
                spawn(async move {
                    timer(motion::COMPACT_WINDOW_LEAVE_HIDE).await;
                    if *hide_seq.peek() == seq {
                        revealed.set_if_modified(false);
                    }
                });
            }
            if has_collapsible_top_bar && *toolbar_revealed.peek() && !toolbar_keep_open {
                let seq = toolbar_hide_seq.peek().wrapping_add(1);
                toolbar_hide_seq.set(seq);
                spawn(async move {
                    timer(motion::COMPACT_WINDOW_LEAVE_HIDE).await;
                    if *toolbar_hide_seq.peek() == seq {
                        toolbar_revealed.set_if_modified(false);
                    }
                });
            }
            if *top_controls_revealed.peek() {
                top_controls_revealed.set_if_modified(false);
            }
        };

        // WIN-04: Top-edge chrome drag strip above the page card when no top toolbar is shown.
        let top_drag_strip = (self.interactive_window_controls && card_top > 0.0 && settings.window_drag_from_content)
            .then(|| {
                rect()
                    .layer(Layer::Relative(2))
                    .position(Position::new_absolute().left(0.0).top(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::px(card_top))
                    .window_drag()
            });

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
                    // XCM-07: clear transient hover reveals on window resize.
                    if let Some(state) = sidebar_state
                        && !*state.compact_pinned.peek()
                    {
                        let mut revealed = state.compact_revealed;
                        revealed.set_if_modified(false);
                    }
                    toolbar_revealed.set_if_modified(false);
                }
            })
            .on_global_pointer_move(on_global_pointer_move)
            .on_pointer_leave(on_pointer_leave)
            .maybe_child(third_stop_overlay)
            .maybe_child(texture_overlay)
            .maybe_child(top_drag_strip)
            .maybe_child(window_top_bar.map(|tb| rect().width(Size::fill()).opacity(chrome_opacity).child(tb)))
            .child(body)
            .child(toasts)
            .maybe_child(floating_sidebar)
            .maybe_child(floating_window_controls)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Breathing accent glow overlay drawn around the page card while an AI agent is actively driving the tab (`DF-15`).
///
/// Only mounted while an agent conversation on the active tab has `status == Working`, so idle tabs incur zero redraws.
#[derive(Clone, PartialEq)]
struct AgentDrivingPageGlow {
    accent: Color,
    static_snapshot: bool,
}

impl Component for AgentDrivingPageGlow {
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
                .duration(Duration::from_millis(3200))
                .function(Function::Linear)
        });

        let raw = if static_snapshot {
            0.5
        } else {
            let p = cycle.get().value().clamp(0.0, 1.0);
            let tri = 1.0 - (2.0 * p - 1.0).abs();
            tri * tri * (3.0 - 2.0 * tri)
        };
        let pulse = raw;
        let border_alpha = (130.0 + 115.0 * pulse).round() as u8;
        let glow_alpha = (35.0 + 55.0 * pulse).round() as u8;
        let accent = self.accent;
        let border_color = Color::from_argb(border_alpha, accent.r(), accent.g(), accent.b());
        let glow_color = Color::from_argb(glow_alpha, accent.r(), accent.g(), accent.b());

        rect()
            .layer(Layer::Relative(34))
            .position(Position::new_absolute().left(0.0).top(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .corner_radius(CornerRadius::new_all(PAGE_CARD_RADIUS))
            .interactive(Interactive::No)
            .border(
                Border::new()
                    .width(2.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .shadow(Shadow::new().x(0.0).y(0.0).blur(14.0).spread(0.0).color(glow_color))
    }
}
