/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Vertical tab rows, inline tab/folder renamer, drag-reorder preview, and the New Tab row.

use crate::ui::chrome::address_bar::try_use_address_bar_state;
use crate::ui::chrome::fade_text::FadeText;
use crate::ui::chrome::layout::{ChromePalette, TAB_ROW_HEIGHT, TAB_ROW_RADIUS};
use crate::ui::chrome::menus::{open_tab_context_menu, try_use_menu_state};
use crate::ui::chrome::sidebar::state::{SidebarDragState, favicon_badge_colors, try_use_sidebar_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::icons::prepared_icon_bytes;
use crate::ui::kit::{Icon, KitTooltip, Spinner, TextField, use_interactive_feedback};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, Rgba, ThemeRole, motion, use_theme};
use crate::ui::vim::explorer::{apply_explorer_background, apply_explorer_border};
use crate::ui::vim::hints::hint_badge_for_row;
use freya::animation::*;
use freya::engine::prelude as sk;
use freya::prelude::*;
use std::time::{Duration, Instant};
use zendia_browser_core::commands::Command;
use zendia_browser_core::focus::FocusTarget;
use zendia_browser_core::model::{Tab, TabId, TabKind, TabMoveTarget};
use zendia_browser_core::settings::PinnedTabCloseBehavior;

const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);
const DRAG_THRESHOLD_PX: f32 = 6.0;

fn extract_host_label(url: &str) -> String {
    let trimmed = url.trim();
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let host = without_scheme.split('/').next().unwrap_or(without_scheme);
    if host.is_empty() {
        trimmed.to_string()
    } else {
        host.to_string()
    }
}

/// Vertical tab row in Zen's sidebar style with favicon, title, inline renamer, audio/mute toggle, loading indicator,
/// morphing pinned reset/close button, URL-drift reset button, multi-select, tooltips, context menu, and drag-reorder.
#[derive(Clone, PartialEq)]
pub struct VerticalTabRow {
    pub tab: Tab,
    pub active: bool,
    pub collapsed: bool,
    pub indent: f32,
    pub drop_target: TabMoveTarget,
    key: DiffKey,
}

impl KeyExt for VerticalTabRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl VerticalTabRow {
    pub fn new(tab: Tab, active: bool, collapsed: bool, indent: f32, drop_target: TabMoveTarget) -> Self {
        let key = DiffKey::from(&tab.id.0);
        Self {
            tab,
            active,
            collapsed,
            indent,
            drop_target,
            key,
        }
    }
}

impl Component for VerticalTabRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let menu_state = try_use_menu_state();
        let sidebar_state = try_use_sidebar_state();
        let address_bar_state = try_use_address_bar_state();
        let mut hovered = use_state(|| false);
        let mut close_hovered = use_state(|| false);
        let mut audio_hovered = use_state(|| false);
        let mut last_click = use_state(|| None::<Instant>);
        let mut press_origin = use_state(|| None::<(f32, f32)>);
        let mut row_bounds = use_state(|| (0.0_f32, TAB_ROW_HEIGHT));
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let tab_id = self.tab.id;
        let tab_kind = self.tab.kind;
        let drop_target = self.drop_target;
        let can_rename = !self.collapsed && tab_kind != TabKind::Essential;

        let is_renaming = sidebar_state.is_some_and(|s| *s.renaming_tab.read() == Some(tab_id));
        let force_hovered = sidebar_state.is_some_and(|s| *s.preview_hovered_tab.read() == Some(tab_id));
        let is_hovered = hovered() || force_hovered;
        let is_multi_selected = sidebar_state.is_some_and(|s| s.is_tab_selected(tab_id));
        let is_recently_renamed = sidebar_state.is_some_and(|s| *s.recently_renamed_tab.read() == Some(tab_id));

        // TAB-05: 120ms tab entrance animation (opacity 0->1, scale 0.95->1) + :active 0.98 press feedback.
        let feedback = use_interactive_feedback(!is_renaming);
        let enter_anim = use_animation(|conf| {
            conf.on_creation(OnCreation::Run);
            (
                AnimNum::new(0.0, 1.0)
                    .duration(motion::TAB_OPEN)
                    .ease(Ease::Out)
                    .function(motion::ease_out_fn()),
                AnimNum::new(0.95, 1.0)
                    .duration(motion::TAB_OPEN)
                    .ease(Ease::Out)
                    .function(motion::ease_out_fn()),
            )
        });
        // REN-03: 250ms confirmation pulse (1.0 -> 0.98 -> 1.0) on rename commit.
        let rename_pulse = use_animation_with_dependencies(&is_recently_renamed, move |conf, pulsing| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *pulsing {
                AnimNum::new(0.98, 1.0)
                    .duration(motion::TAB_RENAME_PULSE)
                    .ease(Ease::Out)
                    .function(Function::Quad)
            } else {
                AnimNum::new(1.0, 1.0).duration(Duration::from_millis(1))
            }
        });

        let (is_being_dragged, is_drop_before) = match sidebar_state.and_then(|s| s.drag_state.read().clone()) {
            Some(drag) => (
                drag.tab_id == tab_id,
                drag.tab_id != tab_id && drag.target == Some(drop_target),
            ),
            None => (false, false),
        };
        let is_split_merge_target = sidebar_state.is_some_and(|s| *s.split_merge_target.read() == Some(tab_id));

        let new_tab_urlbar_open = address_bar_state.is_some_and(|s| s.is_new_tab_open());
        let visually_active = self.active && !new_tab_urlbar_open;
        let multi_select_bg = {
            let accent = theme.rgba(ThemeRole::Accent);
            accent
                .with_alpha(if theme.scheme == ResolvedScheme::Dark {
                    0.22
                } else {
                    0.16
                })
                .to_color()
        };
        let rest_bg = if visually_active || is_renaming {
            palette.tab_selected_bg
        } else if is_multi_selected {
            multi_select_bg
        } else {
            Color::TRANSPARENT
        };
        let hover_bg = if visually_active || is_renaming {
            palette.tab_selected_bg
        } else if is_multi_selected {
            multi_select_bg
        } else {
            palette.tab_hover_bg
        };
        let press_bg = palette.surface_pill_hover;
        let bg = if force_hovered {
            hover_bg
        } else {
            feedback.background(rest_bg, hover_bg, press_bg)
        };

        let focus_target = match tab_kind {
            TabKind::Pinned if self.tab.folder_id.is_none() => FocusTarget::PinnedTab(tab_id),
            TabKind::Essential => FocusTarget::Essential(tab_id),
            _ => FocusTarget::Tab(tab_id),
        };
        let (
            bg,
            explorer_hint_badge,
            explorer_border_override,
            agent_driving_tab,
            glance_favicon,
            is_unloaded,
            pinned_drift_origin,
            pinned_close_behavior,
        ) = {
            let model = core.model().read();
            let base_border = if is_split_merge_target {
                Border::new()
                    .width(2.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::Accent))
            } else if focus() == Focus::Keyboard {
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::FocusRing))
            } else if is_multi_selected && !self.active {
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.rgba(ThemeRole::Accent).with_alpha(0.45).to_color())
            } else {
                Border::new().width(0.0).fill(Color::TRANSPARENT)
            };
            let driving = model.conversations.iter().any(|conv| {
                conv.tab_id == Some(tab_id) && conv.status == zendia_browser_core::model::ConversationStatus::Working
            });
            let glance_favicon = model
                .glance
                .as_ref()
                .filter(|g| g.parent_tab_id == Some(tab_id))
                .and_then(|g| model.tab(g.tab_id))
                .map(|tab| tab.favicon.clone().unwrap_or_else(|| "globe".to_string()));
            let unloaded = model.is_tab_unloaded(tab_id);
            let drift_origin = model
                .has_pinned_url_drift(tab_id)
                .then(|| model.pinned_url_for(tab_id).map(extract_host_label))
                .flatten();
            (
                apply_explorer_background(bg, &model, &focus_target, &theme),
                hint_badge_for_row(&model, &focus_target, TAB_ROW_HEIGHT, &theme),
                apply_explorer_border(base_border, &model, &focus_target, &theme),
                driving,
                glance_favicon,
                unloaded,
                drift_origin,
                model.settings.pinned_tab_close_behavior,
            )
        };

        // PIN-02: Smoothly shift title up -5px on hover when pinned URL drifted so origin subtitle shows underneath.
        let drift_hover = pinned_drift_origin.is_some() && is_hovered && !is_renaming;
        let drift_shift_anim = use_animation_with_dependencies(&drift_hover, move |conf, shifted| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *shifted { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::PINNED_CLOSE_MORPH)
                .ease(Ease::Out)
                .function(Function::Quad)
        });

        let title_for_click = self.tab.display_title();
        let activate_core = core.clone();
        let on_press = move |_: Event<PressEventData>| {
            if is_renaming {
                return;
            }
            if let Some(state) = sidebar_state {
                let ctrl = *state.ctrl_held.peek();
                let shift = *state.shift_held.peek();
                if ctrl {
                    let active_tab = activate_core.model().peek().active_tab;
                    state.toggle_tab_selection(tab_id, active_tab);
                    return;
                }
                if shift {
                    let model = activate_core.model().peek();
                    let active_tab = model.active_tab;
                    let ordered = model
                        .active_space
                        .map(|sp| model.ordered_vertical_tab_ids_in_space(sp))
                        .unwrap_or_default();
                    drop(model);
                    state.select_tab_range(tab_id, active_tab, &ordered);
                    return;
                }
                state.clear_tab_selection();
            }
            let now = Instant::now();
            let is_double = last_click
                .peek()
                .is_some_and(|prev| now.duration_since(prev) <= DOUBLE_CLICK_WINDOW);
            if is_double && can_rename {
                last_click.set(None);
                if let Some(state) = sidebar_state {
                    state.start_tab_rename(tab_id, title_for_click.clone());
                }
            } else {
                last_click.set(Some(now));
                activate_core.invoke_or_log(Command::TabActivate { tab_id });
            }
        };

        let menu_core = core.clone();
        let on_mouse_down = move |e: Event<MouseEventData>| {
            if e.button == Some(MouseButton::Right) {
                e.stop_propagation();
                if let Some(menu_state) = &menu_state {
                    let model = menu_core.model().peek().clone();
                    let position = (e.global_location.x as f32, e.global_location.y as f32);
                    open_tab_context_menu(menu_state, &model, tab_id, position);
                }
            } else if e.button == Some(MouseButton::Middle) {
                // TAB-01: Middle-click closes normal tabs or runs pinned-tab close/reset/unload behavior.
                e.stop_propagation();
                if matches!(tab_kind, TabKind::Pinned | TabKind::Essential) {
                    menu_core.invoke_or_log(Command::TabPinnedCloseAction { tab_id: Some(tab_id) });
                } else {
                    menu_core.invoke_or_log(Command::TabClose {
                        tab_id: Some(tab_id),
                        force: false,
                    });
                }
            }
        };

        let drag_title = self.tab.display_title();
        let drag_favicon = self.tab.favicon.clone();
        let mut fb_down = feedback.on_pointer_down();
        let on_pointer_down = move |e: Event<PointerEventData>| {
            let is_left = e.button() == Some(MouseButton::Left);
            let loc = (e.global_location().x as f32, e.global_location().y as f32);
            fb_down(e);
            if !is_renaming && is_left {
                press_origin.set(Some(loc));
            }
        };

        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            let Some((ox, oy)) = *press_origin.peek() else {
                return;
            };
            let px = e.global_location().x as f32;
            let py = e.global_location().y as f32;
            let dist = ((px - ox).powi(2) + (py - oy).powi(2)).sqrt();
            if let Some(state) = sidebar_state {
                let mut drag = state.drag_state;
                let already_dragging_this = drag
                    .peek()
                    .as_ref()
                    .is_some_and(|d| d.tab_id == tab_id && !d.is_preview);
                if already_dragging_this {
                    if let Some(current) = drag.write().as_mut() {
                        current.pointer_x = px;
                        current.pointer_y = py;
                    }
                    // DND-04: Detect pointer outside window bounds for tear-off preview (`200x130`).
                    let win_sz = *Platform::get().current_window().root_size.peek();
                    let out = px < 2.0 || py < 2.0 || px > win_sz.width - 2.0 || py > win_sz.height - 2.0;
                    let mut det = state.detached_out_of_bounds;
                    det.set_if_modified(out);
                } else if dist >= DRAG_THRESHOLD_PX {
                    state.clear_drag_extras();
                    drag.set(Some(SidebarDragState {
                        tab_id,
                        tab_title: drag_title.clone(),
                        tab_favicon: drag_favicon.clone(),
                        pointer_x: px,
                        pointer_y: py,
                        target: Some(drop_target),
                        page_drop: None,
                        is_preview: false,
                    }));
                }
            }
        };

        let drop_core = core.clone();
        let mut fb_up = feedback.on_pointer_up();
        let on_global_pointer_up = move |e: Event<PointerEventData>| {
            fb_up(e);
            if press_origin.peek().is_none() {
                return;
            }
            press_origin.set(None);
            if let Some(state) = sidebar_state {
                let finished = {
                    let mut drag = state.drag_state;
                    if drag
                        .peek()
                        .as_ref()
                        .is_some_and(|d| d.tab_id == tab_id && !d.is_preview)
                    {
                        drag.write().take()
                    } else {
                        None
                    }
                };
                if let Some(drag) = finished {
                    let split_target = *state.split_merge_target.peek();
                    let detached = *state.detached_out_of_bounds.peek();
                    state.clear_drag_extras();
                    if detached {
                        drop_core.invoke_or_log(Command::TabDetachToNewWindow { tab_id });
                    } else if let Some(target_tab_id) = split_target {
                        drop_core.invoke_or_log(Command::SplitDrop {
                            tab_id,
                            target_tab_id,
                            zone: zendia_browser_core::split::DropZone::Right,
                        });
                    } else if let Some(page_drop) = drag.page_drop {
                        drop_core.invoke_or_log(Command::SplitDrop {
                            tab_id,
                            target_tab_id: page_drop.target_tab,
                            zone: page_drop.zone,
                        });
                    } else if let Some(target) = drag.target {
                        drop_core.invoke_or_log(Command::TabMove { tab_id, target });
                    }
                }
            }
        };

        let (badge_bg, badge_fg) = favicon_badge_colors(self.tab.favicon.as_deref(), &theme);
        let favicon_tile: Element = if self.tab.is_loading {
            rect()
                .width(Size::px(18.0))
                .height(Size::px(18.0))
                .corner_radius(CornerRadius::new_all(4.0))
                .center()
                .child(Spinner::new(14.0, theme.color(ThemeRole::Accent)))
                .into_element()
        } else {
            let icon_name = self.tab.favicon.as_deref().unwrap_or("globe");
            let svg_el: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(icon_name) {
                SvgViewer::new((("zendia-icon", canonical), bytes))
                    .width(Size::px(11.5))
                    .height(Size::px(11.5))
                    .color(badge_fg)
                    .show_loader(false)
                    .into_element()
            } else {
                Icon::new("globe").size(11.5).role(ThemeRole::TextMuted).into_element()
            };
            rect()
                .width(Size::px(18.0))
                .height(Size::px(18.0))
                .corner_radius(CornerRadius::new_all(4.5))
                .background(badge_bg)
                .center()
                .child(svg_el)
                .into_element()
        };

        let selected_shadow = (visually_active && theme.scheme == ResolvedScheme::Light).then(|| {
            Shadow::new()
                .x(0.0)
                .y(1.0)
                .blur(3.0)
                .color(Color::from_argb(24, 0, 0, 0))
        });

        let focus_border = explorer_border_override;
        let (enter_opacity, enter_scale) = {
            let anim = enter_anim.get();
            (anim.0.value().clamp(0.0, 1.0), anim.1.value())
        };
        let row_opacity = enter_opacity
            * if is_being_dragged {
                0.42
            } else if is_unloaded {
                0.60
            } else {
                1.0
            };
        let row_scale = enter_scale * feedback.scale() * rename_pulse.get().value();

        let title = self.tab.display_title();
        let tooltip_label = if !self.tab.url.is_empty() && self.tab.url != "about:blank" {
            format!("{title}\n{}", self.tab.url)
        } else {
            title.clone()
        };
        let tooltip_enabled = !is_renaming && !is_being_dragged;

        let mut fb_enter = feedback.on_pointer_enter();
        let mut fb_leave = feedback.on_pointer_leave();

        if self.collapsed {
            let audio_dot = (self.tab.is_audible || self.tab.is_muted).then(|| {
                rect()
                    .position(Position::new_absolute().top(3.0).right(3.0))
                    .width(Size::px(7.0))
                    .height(Size::px(7.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(if self.tab.is_muted {
                        theme.color(ThemeRole::TextMuted)
                    } else {
                        theme.color(ThemeRole::Accent)
                    })
            });
            let tile = rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Tab)
                .a11y_alt(title)
                .width(Size::px(38.0))
                .height(Size::px(TAB_ROW_HEIGHT))
                .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
                .background(bg)
                .border(focus_border)
                .opacity(row_opacity)
                .scale(row_scale)
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |e| {
                    hovered.set_if_modified(true);
                    fb_enter(e);
                    if let Some(state) = sidebar_state {
                        state.set_drag_target(drop_target);
                    }
                })
                .on_pointer_leave(move |e| {
                    hovered.set_if_modified(false);
                    fb_leave(e);
                })
                .on_pointer_down(on_pointer_down)
                .on_global_pointer_move(on_global_pointer_move)
                .on_global_pointer_up(on_global_pointer_up)
                .on_press(on_press)
                .on_mouse_down(on_mouse_down)
                .child(favicon_tile)
                .maybe_child(audio_dot)
                .maybe_child(explorer_hint_badge);
            let styled_tile = match selected_shadow {
                Some(shadow) => tile.shadow(shadow),
                None => tile,
            };
            return rect()
                .width(Size::fill())
                .cross_align(Alignment::center())
                .maybe_child(is_drop_before.then_some(DropIndicatorBar { indent: 6.0 }))
                .child(
                    KitTooltip::new(tooltip_label)
                        .enabled(tooltip_enabled)
                        .child(styled_tile),
                )
                .into_element();
        }

        let show_audio = !is_renaming && (self.tab.is_audible || self.tab.is_muted);
        // TAB-02: Close button is shown ONLY on hover (or when multi-selected & active), never permanently on active tab.
        let show_close = !is_renaming && (is_hovered || (is_multi_selected && self.active));

        let mute_core = core.clone();
        let audio_button = show_audio.then(|| {
            let icon = if self.tab.is_muted {
                "audio-muted"
            } else {
                "audio-playing"
            };
            let alt = if self.tab.is_muted { "Unmute Tab" } else { "Mute Tab" };
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(alt)
                .width(Size::px(20.0))
                .height(Size::px(20.0))
                .corner_radius(CornerRadius::new_all(5.0))
                .background(if audio_hovered() {
                    palette.surface_pill
                } else {
                    Color::TRANSPARENT
                })
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| audio_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| audio_hovered.set_if_modified(false))
                .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    mute_core.invoke_or_log(Command::TabToggleMute { tab_id: Some(tab_id) });
                })
                .child(Icon::new(icon).size(12.0).role(if self.tab.is_muted {
                    ThemeRole::TextMuted
                } else {
                    ThemeRole::Accent
                }))
        });

        // PIN-01: Pinned tabs show Zen's morphing `—` <-> `×` reset/unload button; normal tabs show `×` close.
        let close_core = core.clone();
        let close_button: Option<Element> = show_close.then(|| {
            if tab_kind == TabKind::Pinned {
                let show_cross = is_unloaded || pinned_close_behavior == PinnedTabCloseBehavior::Close;
                PinnedResetCloseButton {
                    tab_id,
                    show_cross,
                    key: DiffKey::from(&("pin-reset", tab_id.0)),
                }
                .into_element()
            } else {
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Close Tab")
                    .width(Size::px(20.0))
                    .height(Size::px(20.0))
                    .corner_radius(CornerRadius::new_all(5.0))
                    .background(if close_hovered() {
                        palette.surface_pill
                    } else {
                        Color::TRANSPARENT
                    })
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| close_hovered.set_if_modified(true))
                    .on_pointer_leave(move |_| close_hovered.set_if_modified(false))
                    .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        if let Some(state) = sidebar_state {
                            let sel = state.selected_tabs.peek().clone();
                            if sel.len() > 1 && sel.contains(&tab_id) {
                                state.clear_tab_selection();
                                close_core.invoke_or_log(Command::TabCloseMultiple { tab_ids: sel });
                                return;
                            }
                        }
                        let force = close_core.model().peek().tab(tab_id).is_some_and(|t| t.closing);
                        close_core.invoke_or_log(Command::TabClose {
                            tab_id: Some(tab_id),
                            force,
                        });
                    })
                    .child(Icon::new("close").size(11.0).role(if close_hovered() {
                        ThemeRole::Text
                    } else {
                        ThemeRole::TextMuted
                    }))
                    .into_element()
            }
        });

        let glance_badge = glance_favicon.map(|icon_name| {
            let (g_bg, g_fg) = favicon_badge_colors(Some(&icon_name), &theme);
            let glyph: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(&icon_name) {
                SvgViewer::new((("zendia-icon", canonical), bytes))
                    .width(Size::px(10.5))
                    .height(Size::px(10.5))
                    .color(g_fg)
                    .show_loader(false)
                    .into_element()
            } else {
                Icon::new("globe").size(10.5).role(ThemeRole::Text).into_element()
            };
            rect()
                .a11y_role(AccessibilityRole::Status)
                .a11y_alt("Glance open")
                .width(Size::px(18.0))
                .height(Size::px(18.0))
                .corner_radius(CornerRadius::new_all(4.5))
                .background(g_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::Accent)),
                )
                .center()
                .child(glyph)
        });

        // PIN-02: Left-edge Reset Pin button with 15deg slanted divider when pinned tab drifted from its pinned URL.
        let reset_pin_button = pinned_drift_origin.as_ref().map(|origin| ResetPinButton {
            tab_id,
            origin_label: origin.clone(),
            key: DiffKey::from(&("reset-pin-url", tab_id.0)),
        });

        let drift_progress = drift_shift_anim.get().value().clamp(0.0, 1.0);
        let title_color = if visually_active {
            palette.text
        } else {
            palette.text_muted
        };
        let title_weight = if visually_active {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        };
        let center_el: Element = if is_renaming && let Some(state) = sidebar_state {
            InlineRenameField {
                draft: state.rename_draft,
                placeholder: "Tab title",
                on_commit: EventHandler::new({
                    let core = core.clone();
                    move |text: String| {
                        state.cancel_rename();
                        state.trigger_rename_pulse(tab_id);
                        core.invoke_or_log(Command::TabRename {
                            tab_id: Some(tab_id),
                            title: text,
                        });
                    }
                }),
                on_cancel: EventHandler::new(move |_| {
                    state.cancel_rename();
                }),
                key: DiffKey::from(&("tab-rename", tab_id.0)),
            }
            .into_element()
        } else if let Some(origin) = &pinned_drift_origin {
            rect()
                .width(Size::flex(1.0))
                .height(Size::fill())
                .overflow(Overflow::Clip)
                .child(
                    FadeText::new(title.clone(), 12.5, title_weight, title_color)
                        .fade_px(18.0)
                        .y_offset(-5.5 * drift_progress),
                )
                .maybe_child((drift_progress > 0.05).then(|| {
                    rect()
                        .position(Position::new_absolute().left(0.0).right(0.0).bottom(2.0))
                        .opacity(drift_progress * 0.8)
                        .interactive(Interactive::No)
                        .child(
                            label()
                                .text(origin.clone())
                                .font_size(9.5)
                                .color(palette.text_muted)
                                .max_lines(1)
                                .text_overflow(TextOverflow::Clip),
                        )
                }))
                .into_element()
        } else {
            FadeText::new(title.clone(), 12.5, title_weight, title_color)
                .fade_px(18.0)
                .into_element()
        };

        let agent_badge = (!is_renaming && agent_driving_tab).then(|| {
            let accent = theme.color(ThemeRole::Accent);
            rect()
                .width(Size::px(14.0))
                .height(Size::px(14.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(Color::from_argb(48, accent.r(), accent.g(), accent.b()))
                .border(Border::new().width(1.2).alignment(BorderAlignment::Inner).fill(accent))
                .center()
                .child(Icon::new("sparkles").size(8.5).role(ThemeRole::Accent))
        });

        let row = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(!is_renaming)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_alt(title)
            .width(Size::fill())
            .height(Size::px(TAB_ROW_HEIGHT))
            .padding(Gaps::new(
                0.0,
                5.0,
                0.0,
                if reset_pin_button.is_some() { 3.0 } else { 8.0 },
            ))
            .spacing(if reset_pin_button.is_some() { 6.0 } else { 8.0 })
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
            .background(bg)
            .border(focus_border)
            .opacity(row_opacity)
            .scale(row_scale)
            .cursor(if is_renaming {
                CursorIcon::Text
            } else {
                CursorIcon::Pointer
            })
            .on_sized(move |e: Event<SizedEventData>| {
                row_bounds.set_if_modified((e.area.min_y(), e.area.height().max(1.0)));
            })
            .on_pointer_enter(move |e| {
                hovered.set_if_modified(true);
                fb_enter(e);
                if let Some(state) = sidebar_state {
                    state.set_drag_target(drop_target);
                }
            })
            .on_pointer_move(move |e: Event<PointerEventData>| {
                // DND-05: Split row into top/bottom halves for precise before/after insertion, and
                // DND-03: Dwell in center 50% vertical band for 300ms to offer sidebar split-view merge.
                let Some(state) = sidebar_state else {
                    return;
                };
                let dragging_other = state
                    .drag_state
                    .peek()
                    .as_ref()
                    .is_some_and(|d| !d.is_preview && d.tab_id != tab_id);
                if !dragging_other {
                    return;
                }
                let (top_y, height) = *row_bounds.peek();
                let rel = ((e.global_location().y as f32 - top_y) / height).clamp(0.0, 1.0);
                let adjusted_target = if rel >= 0.5 {
                    match drop_target {
                        TabMoveTarget::Normal { space_id, index } => TabMoveTarget::Normal {
                            space_id,
                            index: index + 1,
                        },
                        TabMoveTarget::Pinned { space_id, index } => TabMoveTarget::Pinned {
                            space_id,
                            index: index + 1,
                        },
                        TabMoveTarget::Folder { folder_id, index } => TabMoveTarget::Folder {
                            folder_id,
                            index: index + 1,
                        },
                        TabMoveTarget::Essential { index } => TabMoveTarget::Essential { index: index + 1 },
                    }
                } else {
                    drop_target
                };
                state.set_drag_target(adjusted_target);

                let in_center_band = (0.25..=0.75).contains(&rel) && tab_kind == TabKind::Normal;
                if in_center_band {
                    if *state.split_merge_target.peek() != Some(tab_id) {
                        let mut dwell_seq = state.split_dwell_seq;
                        let seq = dwell_seq.peek().wrapping_add(1);
                        dwell_seq.set(seq);
                        let mut merge = state.split_merge_target;
                        spawn(async move {
                            timer(Duration::from_millis(300)).await;
                            if *dwell_seq.peek() == seq
                                && state.drag_state.peek().as_ref().is_some_and(|d| !d.is_preview)
                            {
                                merge.set_if_modified(Some(tab_id));
                            }
                        });
                    }
                } else if *state.split_merge_target.peek() == Some(tab_id) {
                    let mut merge = state.split_merge_target;
                    merge.set_if_modified(None);
                    let mut dwell_seq = state.split_dwell_seq;
                    let next = dwell_seq.peek().wrapping_add(1);
                    dwell_seq.set(next);
                }
            })
            .on_pointer_leave(move |e| {
                hovered.set_if_modified(false);
                fb_leave(e);
                if let Some(state) = sidebar_state
                    && *state.split_merge_target.peek() == Some(tab_id)
                {
                    let mut merge = state.split_merge_target;
                    merge.set_if_modified(None);
                }
            })
            .on_pointer_down(on_pointer_down)
            .on_global_pointer_move(on_global_pointer_move)
            .on_global_pointer_up(on_global_pointer_up)
            .on_press(on_press)
            .on_mouse_down(on_mouse_down)
            .maybe_child(reset_pin_button)
            .child(favicon_tile)
            .child(center_el)
            .maybe_child(agent_badge)
            .maybe_child(glance_badge)
            .maybe_child(audio_button)
            .maybe_child(close_button)
            .maybe_child(explorer_hint_badge);

        let styled_row = match selected_shadow {
            Some(shadow) => row.shadow(shadow),
            None => row,
        };

        rect()
            .width(Size::fill())
            .padding(Gaps::new(0.0, 0.0, 0.0, self.indent))
            .maybe_child(is_drop_before.then_some(DropIndicatorBar { indent: 0.0 }))
            .child(
                KitTooltip::new(tooltip_label)
                    .enabled(tooltip_enabled)
                    .child(styled_row),
            )
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// PIN-01: Zen's morphing pinned tab reset/unload button (`—` `0deg` `12px` <-> `×` `±45deg` `15px` over `120ms`).
#[derive(Clone, PartialEq)]
struct PinnedResetCloseButton {
    tab_id: TabId,
    show_cross: bool,
    key: DiffKey,
}

impl KeyExt for PinnedResetCloseButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for PinnedResetCloseButton {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);

        // Morphs smoothly over 120ms between horizontal bar (0deg, 12px) and cross (±45deg, 15px).
        let morph = use_animation_with_dependencies(&self.show_cross, move |conf, cross| {
            conf.on_creation(OnCreation::Run);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *cross { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::PINNED_CLOSE_MORPH)
                .ease(Ease::InOut)
                .function(Function::Quad)
        });

        let t = morph.get().value().clamp(0.0, 1.0);
        let angle_deg = 45.0 * t;
        let line_len = 12.0 + 3.0 * t;
        let stroke_color = if hovered() { palette.text } else { palette.text_muted };
        let tab_id = self.tab_id;
        let alt = if self.show_cross {
            "Close Pinned Tab"
        } else {
            "Unload / Reset Pinned Tab"
        };

        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(alt)
            .width(Size::px(20.0))
            .height(Size::px(20.0))
            .corner_radius(CornerRadius::new_all(5.0))
            .background(if hovered() {
                palette.surface_pill
            } else {
                Color::TRANSPARENT
            })
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                core.invoke_or_log(Command::TabPinnedCloseAction { tab_id: Some(tab_id) });
            })
            .child(
                rect()
                    .width(Size::px(16.0))
                    .height(Size::px(16.0))
                    .interactive(Interactive::No)
                    .background(Fill::Callback(RenderCallback::new(move |ctx| {
                        let RenderTarget::Background { canvas, .. } = &mut ctx.target else {
                            return;
                        };
                        let cx = ctx.size.width * 0.5;
                        let cy = ctx.size.height * 0.5;
                        let mut paint = sk::Paint::default();
                        paint.set_anti_alias(true);
                        paint.set_style(sk::PaintStyle::Fill);
                        paint.set_color(sk::Color::from_argb(
                            stroke_color.a(),
                            stroke_color.r(),
                            stroke_color.g(),
                            stroke_color.b(),
                        ));

                        let draw_bar = |canvas: &sk::Canvas, deg: f32| {
                            canvas.save();
                            canvas.translate((cx, cy));
                            canvas.rotate(deg, None);
                            let rr = sk::RRect::new_rect_xy(
                                sk::Rect::from_xywh(-line_len * 0.5, -0.75, line_len, 1.5),
                                0.75,
                                0.75,
                            );
                            canvas.draw_rrect(rr, &paint);
                            canvas.restore();
                        };
                        draw_bar(canvas, angle_deg);
                        draw_bar(canvas, -angle_deg);
                    }))),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// PIN-02: Left-edge Reset Pin button with Zen's `2.5px` `15deg`-rotated slanted divider when a pinned tab has
/// navigated away from its original URL.
#[derive(Clone, PartialEq)]
struct ResetPinButton {
    tab_id: TabId,
    origin_label: String,
    key: DiffKey,
}

impl KeyExt for ResetPinButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ResetPinButton {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);
        let tab_id = self.tab_id;
        let is_hovered = hovered();
        let divider_color = match theme.scheme {
            ResolvedScheme::Dark => sk::Color::from_argb(if is_hovered { 0 } else { 77 }, 255, 255, 255),
            ResolvedScheme::Light => sk::Color::from_argb(if is_hovered { 0 } else { 77 }, 66, 61, 61),
        };

        KitTooltip::new(format!("Reset to {}", self.origin_label)).child(
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Reset pinned tab URL")
                .width(Size::px(24.0))
                .height(Size::px(TAB_ROW_HEIGHT - 4.0))
                .padding(Gaps::new(0.0, 4.0, 0.0, 2.0))
                .corner_radius(CornerRadius::new(6.0, 0.0, 0.0, 6.0))
                .background(if is_hovered {
                    palette.surface_pill
                } else {
                    Color::TRANSPARENT
                })
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| hovered.set_if_modified(true))
                .on_pointer_leave(move |_| hovered.set_if_modified(false))
                .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    core.invoke_or_log(Command::TabResetPin { tab_id: Some(tab_id) });
                })
                .background(Fill::Callback(RenderCallback::new(move |ctx| {
                    let RenderTarget::Background { canvas, .. } = &mut ctx.target else {
                        return;
                    };
                    if divider_color.a() == 0 {
                        return;
                    }
                    let cx = ctx.size.width - 2.0;
                    let cy = ctx.size.height * 0.5;
                    let mut paint = sk::Paint::default();
                    paint.set_anti_alias(true);
                    paint.set_style(sk::PaintStyle::Fill);
                    paint.set_color(divider_color);
                    canvas.save();
                    canvas.translate((cx, cy));
                    canvas.rotate(15.0, None);
                    let rr = sk::RRect::new_rect_xy(sk::Rect::from_xywh(-1.25, -7.0, 2.5, 14.0), 1.25, 1.25);
                    canvas.draw_rrect(rr, &paint);
                    canvas.restore();
                })))
                .child(Icon::new("reload").size(11.0).role(if is_hovered {
                    ThemeRole::Text
                } else {
                    ThemeRole::TextMuted
                })),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Inline text field mounted inside a tab, folder, or space row while renaming (Enter commits, Escape cancels).
///
/// Styled flush and borderless (`REN-05`) matching Zen's `#tab-label-input` (`vertical-tabs.css:1446-1456`).
#[derive(Clone, PartialEq)]
pub struct InlineRenameField {
    pub draft: State<String>,
    pub placeholder: &'static str,
    pub on_commit: EventHandler<String>,
    pub on_cancel: EventHandler<()>,
    pub key: DiffKey,
}

impl KeyExt for InlineRenameField {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for InlineRenameField {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let a11y_id = use_hook(AccessibilityId::new_unique);

        let draft = self.draft;
        let on_commit_submit = self.on_commit.clone();
        let on_commit_key = self.on_commit.clone();
        let on_cancel_key = self.on_cancel.clone();
        let on_cancel_blur = self.on_cancel.clone();

        rect()
            .width(Size::flex(1.0))
            .height(Size::px(22.0))
            .background(Color::TRANSPARENT)
            .cross_align(Alignment::center())
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .on_press(|e: Event<PressEventData>| e.stop_propagation())
            .child(
                TextField::new(draft)
                    .a11y_id(a11y_id)
                    .auto_focus(true)
                    .select_all_on_open(true)
                    .placeholder(self.placeholder)
                    .width(Size::fill())
                    .color(palette.text)
                    .placeholder_color(palette.text_muted)
                    .padding(Gaps::new(0.0, 0.0, 0.0, 0.0))
                    .on_pre_key_down(move |e: Event<KeyboardEventData>| {
                        e.stop_propagation();
                        match &e.key {
                            Key::Named(NamedKey::Escape) => {
                                e.prevent_default();
                                on_cancel_key.call(());
                                false
                            }
                            Key::Named(NamedKey::Enter) => {
                                e.prevent_default();
                                on_commit_key.call(draft.peek().clone());
                                false
                            }
                            _ => true,
                        }
                    })
                    .on_submit(move |text: String| {
                        on_commit_submit.call(text);
                    })
                    .on_blur(move |_| {
                        on_cancel_blur.call(());
                    }),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Accent drop-target insertion line drawn where a dragged tab will land (`DND-07`: hollow circle + `2px` bar).
#[derive(Clone, Copy, PartialEq)]
pub struct DropIndicatorBar {
    pub indent: f32,
}

impl Component for DropIndicatorBar {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let accent = theme.color(ThemeRole::Accent);

        rect()
            .width(Size::fill())
            .height(Size::px(6.0))
            .padding(Gaps::new(0.0, 4.0, 0.0, 4.0 + self.indent))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .child(
                rect()
                    .width(Size::px(6.0))
                    .height(Size::px(6.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(Color::TRANSPARENT)
                    .border(Border::new().width(2.0).alignment(BorderAlignment::Inner).fill(accent)),
            )
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .height(Size::px(2.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(accent),
            )
    }
}

/// Vertical accent drop-target insertion bar between horizontal Essential tiles (`DND-07`).
#[derive(Clone, Copy, PartialEq)]
pub struct VerticalDropIndicatorBar {
    pub height: f32,
}

impl Component for VerticalDropIndicatorBar {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let accent = theme.color(ThemeRole::Accent);

        rect()
            .width(Size::px(6.0))
            .height(Size::px(self.height))
            .direction(Direction::Vertical)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .child(
                rect()
                    .width(Size::px(6.0))
                    .height(Size::px(6.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(Color::TRANSPARENT)
                    .border(Border::new().width(2.0).alignment(BorderAlignment::Inner).fill(accent)),
            )
            .child(
                rect()
                    .width(Size::px(2.0))
                    .height(Size::flex(1.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(accent),
            )
    }
}

/// Floating ghost pill shown while a tab drag reorder is in progress.
#[derive(Clone, PartialEq)]
pub struct DragGhostOverlay {
    pub drag: SidebarDragState,
    pub sidebar_width: f32,
}

impl Component for DragGhostOverlay {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let sidebar_state = try_use_sidebar_state();
        let selected_count = sidebar_state
            .map(|s| {
                let list = s.selected_tabs.read();
                if list.len() > 1 && list.contains(&self.drag.tab_id) {
                    list.len()
                } else {
                    1
                }
            })
            .unwrap_or(1);

        let (badge_bg, badge_fg) = favicon_badge_colors(self.drag.tab_favicon.as_deref(), &theme);
        let icon_name = self.drag.tab_favicon.as_deref().unwrap_or("globe");
        let svg_el: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(icon_name) {
            SvgViewer::new((("zendia-icon", canonical), bytes))
                .width(Size::px(11.5))
                .height(Size::px(11.5))
                .color(badge_fg)
                .show_loader(false)
                .into_element()
        } else {
            Icon::new("globe").size(11.5).role(ThemeRole::Text).into_element()
        };

        let bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(38, 42, 62, 0.92).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(255, 255, 255, 0.95).to_color(),
        };
        let ghost_width = (self.sidebar_width - 16.0).clamp(140.0, 220.0);
        let detached_window = sidebar_state.is_some_and(|s| *s.detached_out_of_bounds.read());

        if detached_window && !self.drag.is_preview {
            // DND-04: 200x130 tear-off detached window preview when pointer leaves window bounds.
            let card = rect()
                .width(Size::px(200.0))
                .height(Size::px(130.0))
                .padding(Gaps::new_all(10.0))
                .spacing(8.0)
                .corner_radius(CornerRadius::new_all(10.0))
                .background(bg)
                .border(
                    Border::new()
                        .width(1.5)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::Accent)),
                )
                .shadow(Shadow::new().y(8.0).blur(24.0).color(Color::from_argb(110, 0, 0, 0)))
                .child(
                    rect()
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(6.0)
                        .child(
                            rect()
                                .width(Size::px(16.0))
                                .height(Size::px(16.0))
                                .corner_radius(CornerRadius::new_all(4.0))
                                .background(badge_bg)
                                .center()
                                .child(svg_el),
                        )
                        .child(
                            label()
                                .text(self.drag.tab_title.clone())
                                .font_size(11.5)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(palette.text)
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis),
                        ),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::flex(1.0))
                        .corner_radius(CornerRadius::new_all(6.0))
                        .background(palette.surface_pill)
                        .center()
                        .child(label().text("New Window").font_size(11.0).color(palette.text_muted)),
                );
            return rect()
                .position(
                    Position::new_global()
                        .left((self.drag.pointer_x - 100.0).max(4.0))
                        .top((self.drag.pointer_y - 24.0).max(4.0)),
                )
                .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                .interactive(false)
                .child(card)
                .into_element();
        }

        let count_badge = (selected_count > 1).then(|| {
            rect()
                .min_width(Size::px(18.0))
                .height(Size::px(18.0))
                .padding(Gaps::new(0.0, 5.0, 0.0, 5.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(theme.color(ThemeRole::Accent))
                .center()
                .child(
                    label()
                        .text(selected_count.to_string())
                        .font_size(10.5)
                        .font_weight(FontWeight::BOLD)
                        .color(theme.color(ThemeRole::TextOnAccent)),
                )
        });

        let pill = rect()
            .width(Size::px(ghost_width))
            .height(Size::px(TAB_ROW_HEIGHT))
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .spacing(8.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
            .background(bg)
            .border(
                Border::new()
                    .width(1.25)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::Accent)),
            )
            .shadow(
                Shadow::new()
                    .x(0.0)
                    .y(6.0)
                    .blur(16.0)
                    .color(Color::from_argb(80, 0, 0, 0)),
            )
            .opacity(0.94)
            .child(
                rect()
                    .width(Size::px(18.0))
                    .height(Size::px(18.0))
                    .corner_radius(CornerRadius::new_all(4.5))
                    .background(badge_bg)
                    .center()
                    .child(svg_el),
            )
            .child(
                label()
                    .text(self.drag.tab_title.clone())
                    .width(Size::flex(1.0))
                    .font_size(12.5)
                    .font_weight(FontWeight::MEDIUM)
                    .color(palette.text)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(count_badge);

        if self.drag.is_preview {
            rect()
                .position(
                    Position::new_absolute()
                        .left(self.drag.pointer_x)
                        .top(self.drag.pointer_y),
                )
                .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                .interactive(false)
                .child(pill)
                .into_element()
        } else {
            rect()
                .position(
                    Position::new_global()
                        .left((self.drag.pointer_x - 24.0).max(4.0))
                        .top((self.drag.pointer_y - TAB_ROW_HEIGHT * 0.5).max(4.0)),
                )
                .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                .interactive(false)
                .child(pill)
                .into_element()
        }
    }
}

/// `+ New Tab` row rendered directly below the pinned separator.
#[derive(Clone, PartialEq)]
pub struct NewTabRow {
    pub collapsed: bool,
}

impl Component for NewTabRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let address_bar_state = try_use_address_bar_state();
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let in_urlbar = address_bar_state.is_some_and(|s| s.is_new_tab_open());
        let bg = if in_urlbar {
            palette.tab_selected_bg
        } else if hovered() {
            palette.tab_hover_bg
        } else {
            Color::TRANSPARENT
        };
        let fg_role = if in_urlbar {
            ThemeRole::Text
        } else {
            ThemeRole::TextMuted
        };
        let label_color = if in_urlbar { palette.text } else { palette.text_muted };

        let focus_border = if focus() == Focus::Keyboard {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::FocusRing))
        } else {
            Border::new().width(0.0).fill(Color::TRANSPARENT)
        };

        let on_press = move |_: Event<PressEventData>| {
            core.invoke_or_log(Command::TabNew {
                url: None,
                after: None,
                activate: true,
            });
        };

        if self.collapsed {
            rect()
                .width(Size::fill())
                .center()
                .padding(Gaps::new(2.0, 0.0, 2.0, 0.0))
                .child(
                    rect()
                        .a11y_id(a11y_id)
                        .a11y_focusable(true)
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("New Tab")
                        .width(Size::px(38.0))
                        .height(Size::px(TAB_ROW_HEIGHT))
                        .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
                        .background(bg)
                        .border(focus_border)
                        .center()
                        .cursor(CursorIcon::Pointer)
                        .on_pointer_enter(move |_| hovered.set_if_modified(true))
                        .on_pointer_leave(move |_| hovered.set_if_modified(false))
                        .on_press(on_press)
                        .child(Icon::new("plus").size(15.0).role(fg_role)),
                )
        } else {
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("New Tab")
                .width(Size::fill())
                .height(Size::px(TAB_ROW_HEIGHT))
                .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                .spacing(9.0)
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
                .background(bg)
                .border(focus_border)
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| hovered.set_if_modified(true))
                .on_pointer_leave(move |_| hovered.set_if_modified(false))
                .on_press(on_press)
                .child(Icon::new("plus").size(14.0).role(fg_role))
                .child(
                    label()
                        .text("New Tab")
                        .font_size(12.5)
                        .font_weight(if in_urlbar {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        })
                        .color(label_color)
                        .max_lines(1),
                )
        }
    }
}
