/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Vertical tab rows, inline tab/folder renamer, drag-reorder preview, and the New Tab row.

use crate::ui::chrome::layout::{ChromePalette, TAB_ROW_HEIGHT, TAB_ROW_RADIUS};
use crate::ui::chrome::menus::{open_tab_context_menu, try_use_menu_state};
use crate::ui::chrome::sidebar::state::{SidebarDragState, favicon_badge_colors, try_use_sidebar_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::kit::icons::prepared_icon_bytes;
use crate::ui::theme::POPUP_OVERLAY_LEVEL;
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, use_theme};
use crate::ui::vim::explorer::{apply_explorer_background, apply_explorer_border};
use crate::ui::vim::hints::hint_badge_for_row;
use freya::prelude::*;
use std::time::{Duration, Instant};
use zendia_browser_core::commands::Command;
use zendia_browser_core::focus::FocusTarget;
use zendia_browser_core::model::{Tab, TabKind, TabMoveTarget};

const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);
const DRAG_THRESHOLD_PX: f32 = 6.0;

const ICON_LOADING_SPINNER_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="5.5" stroke="currentColor" stroke-opacity="0.24" stroke-width="1.8"/><path d="M8 2.5A5.5 5.5 0 0 1 13.5 8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>"#;

/// Vertical tab row in Zen's sidebar style with favicon, title, inline renamer, audio/mute toggle, loading indicator,
/// close button, context menu, and drag-reorder support.
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
        let mut hovered = use_state(|| false);
        let mut close_hovered = use_state(|| false);
        let mut audio_hovered = use_state(|| false);
        let mut last_click = use_state(|| None::<Instant>);
        let mut press_origin = use_state(|| None::<(f32, f32)>);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let tab_id = self.tab.id;
        let drop_target = self.drop_target;
        let can_rename = !self.collapsed && self.tab.kind != TabKind::Essential;

        let is_renaming = sidebar_state.is_some_and(|s| *s.renaming_tab.read() == Some(tab_id));
        let force_hovered = sidebar_state.is_some_and(|s| *s.preview_hovered_tab.read() == Some(tab_id));
        let is_hovered = hovered() || force_hovered;

        let (is_being_dragged, is_drop_before) = match sidebar_state.and_then(|s| s.drag_state.read().clone()) {
            Some(drag) => (
                drag.tab_id == tab_id,
                drag.tab_id != tab_id && drag.target == Some(drop_target),
            ),
            None => (false, false),
        };

        let bg = if self.active || is_renaming {
            palette.tab_selected_bg
        } else if is_hovered {
            palette.tab_hover_bg
        } else {
            Color::TRANSPARENT
        };
        let focus_target = match self.tab.kind {
            TabKind::Pinned if self.tab.folder_id.is_none() => FocusTarget::PinnedTab(tab_id),
            TabKind::Essential => FocusTarget::Essential(tab_id),
            _ => FocusTarget::Tab(tab_id),
        };
        let (bg, explorer_hint_badge, explorer_border_override) = {
            let model = core.model().read();
            let base_border = if focus() == Focus::Keyboard {
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::FocusRing))
            } else {
                Border::new().width(0.0).fill(Color::TRANSPARENT)
            };
            (
                apply_explorer_background(bg, &model, &focus_target, &theme),
                hint_badge_for_row(&model, &focus_target, TAB_ROW_HEIGHT, &theme),
                apply_explorer_border(base_border, &model, &focus_target, &theme),
            )
        };

        let title_for_click = self.tab.display_title();
        let activate_core = core.clone();
        let on_press = move |_: Event<PressEventData>| {
            if is_renaming {
                return;
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
            }
        };

        let drag_title = self.tab.display_title();
        let drag_favicon = self.tab.favicon.clone();
        let on_pointer_down = move |e: Event<PointerEventData>| {
            if !is_renaming && e.button() == Some(MouseButton::Left) {
                press_origin.set(Some((e.global_location().x as f32, e.global_location().y as f32)));
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
                } else if dist >= DRAG_THRESHOLD_PX {
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
        let on_global_pointer_up = move |_: Event<PointerEventData>| {
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
                    if let Some(page_drop) = drag.page_drop {
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
                .child(
                    SvgViewer::new(("zendia-tab-loading", Bytes::from_static(ICON_LOADING_SPINNER_SVG)))
                        .width(Size::px(14.0))
                        .height(Size::px(14.0))
                        .color(theme.color(ThemeRole::Accent))
                        .show_loader(false),
                )
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

        let selected_shadow = (self.active && theme.scheme == ResolvedScheme::Light).then(|| {
            Shadow::new()
                .x(0.0)
                .y(1.0)
                .blur(3.0)
                .color(Color::from_argb(24, 0, 0, 0))
        });

        let focus_border = explorer_border_override;

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
                .a11y_alt(self.tab.display_title())
                .width(Size::px(38.0))
                .height(Size::px(TAB_ROW_HEIGHT))
                .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
                .background(bg)
                .border(focus_border)
                .opacity(if is_being_dragged { 0.42 } else { 1.0 })
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| {
                    hovered.set_if_modified(true);
                    if let Some(state) = sidebar_state {
                        state.set_drag_target(drop_target);
                    }
                })
                .on_pointer_leave(move |_| hovered.set_if_modified(false))
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
                .child(styled_tile)
                .into_element();
        }

        let title = self.tab.display_title();
        let show_audio = !is_renaming && (self.tab.is_audible || self.tab.is_muted);
        let show_close = !is_renaming
            && match self.tab.kind {
                TabKind::Pinned => is_hovered,
                TabKind::Normal | TabKind::Essential => is_hovered || self.active,
            };

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

        let close_core = core.clone();
        let close_button = show_close.then(|| {
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
        });

        let center_el: Element = if is_renaming && let Some(state) = sidebar_state {
            InlineRenameField {
                draft: state.rename_draft,
                placeholder: "Tab title",
                on_commit: EventHandler::new({
                    let core = core.clone();
                    move |text: String| {
                        state.cancel_rename();
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
        } else {
            label()
                .text(title.clone())
                .width(Size::flex(1.0))
                .font_size(12.5)
                .font_weight(if self.active {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .color(if self.active { palette.text } else { palette.text_muted })
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .into_element()
        };

        let row = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(!is_renaming)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_alt(title)
            .width(Size::fill())
            .height(Size::px(TAB_ROW_HEIGHT))
            .padding(Gaps::new(0.0, 5.0, 0.0, 8.0))
            .spacing(8.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(TAB_ROW_RADIUS))
            .background(bg)
            .border(focus_border)
            .opacity(if is_being_dragged { 0.42 } else { 1.0 })
            .cursor(if is_renaming {
                CursorIcon::Text
            } else {
                CursorIcon::Pointer
            })
            .on_pointer_enter(move |_| {
                hovered.set_if_modified(true);
                if let Some(state) = sidebar_state {
                    state.set_drag_target(drop_target);
                }
            })
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_pointer_down(on_pointer_down)
            .on_global_pointer_move(on_global_pointer_move)
            .on_global_pointer_up(on_global_pointer_up)
            .on_press(on_press)
            .on_mouse_down(on_mouse_down)
            .child(favicon_tile)
            .child(center_el)
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
            .child(styled_row)
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Inline text field mounted inside a tab or folder row while renaming (Enter commits, Escape cancels).
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

        let bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(15, 17, 24, 0.72).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(255, 255, 255, 0.96).to_color(),
        };

        rect()
            .width(Size::flex(1.0))
            .height(Size::px(24.0))
            .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
            .corner_radius(CornerRadius::new_all(5.0))
            .background(bg)
            .border(
                Border::new()
                    .width(1.25)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::FocusRing)),
            )
            .cross_align(Alignment::center())
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .on_press(|e: Event<PressEventData>| e.stop_propagation())
            .child(
                Input::new(draft)
                    .a11y_id(a11y_id)
                    .auto_focus(true)
                    .flat()
                    .compact()
                    .placeholder(self.placeholder)
                    .width(Size::fill())
                    .background(Color::TRANSPARENT)
                    .focus_background(Color::TRANSPARENT)
                    .border_fill(Color::TRANSPARENT)
                    .focus_border_fill(Color::TRANSPARENT)
                    .color(palette.text)
                    .placeholder_color(palette.text_muted)
                    .padding(Gaps::new(0.0, 2.0, 0.0, 2.0))
                    .corner_radius(CornerRadius::new_all(0.0))
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
                    }),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Accent drop-target insertion line drawn where a dragged tab will land.
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
            .height(Size::px(4.0))
            .padding(Gaps::new(0.0, 4.0, 0.0, 4.0 + self.indent))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .child(
                rect()
                    .width(Size::px(5.0))
                    .height(Size::px(5.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(accent),
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
            );

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
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let bg = if hovered() {
            palette.tab_hover_bg
        } else {
            Color::TRANSPARENT
        };

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
                        .child(Icon::new("plus").size(15.0).role(ThemeRole::TextMuted)),
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
                .child(Icon::new("plus").size(14.0).role(ThemeRole::TextMuted))
                .child(
                    label()
                        .text("New Tab")
                        .font_size(12.5)
                        .color(palette.text_muted)
                        .max_lines(1),
                )
        }
    }
}
