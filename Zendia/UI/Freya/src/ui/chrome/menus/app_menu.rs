/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Zen-styled application menu panel with icons, right-aligned shortcuts, submenu cascades, the built-in zoom row,
//! and the Vim mode switch backed by [`BrowserCore`](zendia_browser_core::BrowserCore).

use crate::ui::chrome::menus::state::{
    AnchorRect, compute_app_menu_origin, compute_submenu_origin, invoke_menu_command, next_selectable_index,
    use_menu_state,
};
use crate::ui::core_view::{CoreView, use_core};
use crate::ui::kit::{Icon, KitSwitch};
use crate::ui::theme::{ResolvedTheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{MenuEntryKind, MenuLocation};
use zendia_browser_core::menus::{
    APP_MENU_GROUPS, ResolvedMenuItem, ZOOM_CONTROLS_WIDGET_ID, resolve_contributed_menu,
};
use zendia_keymap::KeyTable;

const APP_MENU_WIDTH: f32 = 296.0;
const APP_SUBMENU_WIDTH: f32 = 248.0;
const APP_MENU_ROW_HEIGHT: f32 = 32.0;
const APP_MENU_SEP_HEIGHT: f32 = 11.0;

fn estimate_app_menu_height(items: &[ResolvedMenuItem]) -> f32 {
    let mut height = 14.0;
    for item in items {
        height += if item.is_separator() {
            APP_MENU_SEP_HEIGHT
        } else {
            APP_MENU_ROW_HEIGHT
        };
    }
    height
}

fn row_y_offset(items: &[ResolvedMenuItem], target_index: usize) -> f32 {
    let mut y = 7.0;
    for (idx, item) in items.iter().enumerate() {
        if idx == target_index {
            return y;
        }
        y += if item.is_separator() {
            APP_MENU_SEP_HEIGHT
        } else {
            APP_MENU_ROW_HEIGHT
        };
    }
    y
}

fn app_menu_panel_bg(theme: &ResolvedTheme) -> Color {
    if theme.scheme.is_dark() {
        theme
            .rgba(ThemeRole::Surface)
            .mix(theme.rgba(ThemeRole::SurfaceRaised), 0.35)
            .to_color()
    } else {
        theme.color(ThemeRole::Surface)
    }
}

/// Overlay host rendering the animated, anchored application menu when open in [`MenuViewState`].
#[derive(Clone, PartialEq)]
pub struct AppMenuOverlay;

impl Component for AppMenuOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let menu_state = use_menu_state();
        let key_table = use_hook(KeyTable::standard);
        let is_open = *menu_state.app_menu_open.read();
        let anchor = *menu_state.app_menu_anchor.read();

        let fade = use_animation_with_dependencies(&is_open, move |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *open {
                AnimNum::new(0.0, 1.0)
                    .duration(motion::FLOAT_CONTENT_IN)
                    .ease(Ease::Out)
                    .function(motion::ease_out_fn())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(motion::FLOAT_OUT)
                    .ease(Ease::In)
                    .function(motion::ease_in_fn())
            }
        });

        let opacity = fade.get().value();
        if !is_open && opacity <= 0.01 {
            return rect().into_element();
        }

        let (items, zoom_level) = {
            let model = core.model().read();
            let resolved = resolve_contributed_menu(
                &model.contributions.menu_entries,
                MenuLocation::AppMenu,
                Some(&key_table),
                APP_MENU_GROUPS,
            );
            (resolved, model.zoom_level)
        };

        let root_size = *Platform::get().current_window().root_size.read();
        let window_size = (root_size.width.max(320.0), root_size.height.max(240.0));
        let est_height = estimate_app_menu_height(&items);
        let origin = compute_app_menu_origin(anchor, (APP_MENU_WIDTH, est_height), window_size);

        rect()
            .layer(Layer::Overlay)
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .opacity(opacity)
            .child(
                // Full-window click-away dismiss backdrop.
                rect()
                    .position(Position::new_global().left(0.0).top(0.0))
                    .width(Size::window_percent(100.0))
                    .height(Size::window_percent(100.0))
                    .on_mouse_down(move |_| {
                        menu_state.close_app_menu();
                    }),
            )
            .child(
                AppMenuInteractive::new(items, zoom_level)
                    .origin(origin)
                    .window_size(window_size)
                    .capture_global_keys(is_open)
                    .on_close(move |_| menu_state.close_app_menu()),
            )
            .into_element()
    }
}

/// Interactive Zen-styled application menu panel with keyboard navigation and cascading submenu support.
///
/// Used both by [`AppMenuOverlay`] (anchored to the toolbar button) and by the gallery (inline or anchored preview).
#[derive(Clone, PartialEq)]
pub struct AppMenuInteractive {
    items: Vec<ResolvedMenuItem>,
    zoom_level: f32,
    origin: Option<(f32, f32)>,
    window_size: (f32, f32),
    initial_open_submenu: Option<String>,
    capture_global_keys: bool,
    on_close: Option<EventHandler<()>>,
    key: DiffKey,
}

impl KeyExt for AppMenuInteractive {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl AppMenuInteractive {
    pub fn new(items: Vec<ResolvedMenuItem>, zoom_level: f32) -> Self {
        Self {
            items,
            zoom_level,
            origin: None,
            window_size: (1600.0, 1000.0),
            initial_open_submenu: None,
            capture_global_keys: false,
            on_close: None,
            key: DiffKey::None,
        }
    }

    pub fn origin(mut self, origin: (f32, f32)) -> Self {
        self.origin = Some(origin);
        self
    }

    pub fn window_size(mut self, window_size: (f32, f32)) -> Self {
        self.window_size = window_size;
        self
    }

    pub fn initial_open_submenu(mut self, id: impl Into<String>) -> Self {
        self.initial_open_submenu = Some(id.into());
        self
    }

    pub fn capture_global_keys(mut self, capture: bool) -> Self {
        self.capture_global_keys = capture;
        self
    }

    pub fn on_close(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_close = Some(handler.into());
        self
    }
}

impl Component for AppMenuInteractive {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let core = use_core();

        let initial_submenu_idx = self
            .initial_open_submenu
            .as_deref()
            .and_then(|id| self.items.iter().position(|item| item.id.as_str() == id));

        let mut active_index = use_state(move || initial_submenu_idx);
        let mut open_submenu_index = use_state(move || initial_submenu_idx);
        let mut submenu_active_index = use_state(|| None::<usize>);

        let items_for_keys = self.items.clone();
        let core_for_keys = core.clone();
        let on_close_keys = self.on_close.clone();
        let capture_keys = self.capture_global_keys;

        let handle_key = move |e: Event<KeyboardEventData>| {
            if !capture_keys {
                return;
            }
            match &e.key {
                Key::Named(NamedKey::Escape) => {
                    e.stop_propagation();
                    if open_submenu_index.peek().is_some() {
                        open_submenu_index.set(None);
                        submenu_active_index.set(None);
                    } else if let Some(close) = &on_close_keys {
                        close.call(());
                    }
                }
                Key::Named(NamedKey::ArrowDown) => {
                    e.stop_propagation();
                    if let Some(sub_idx) = *open_submenu_index.peek()
                        && let Some(parent) = items_for_keys.get(sub_idx)
                    {
                        let next = next_selectable_index(&parent.children, *submenu_active_index.peek(), 1);
                        submenu_active_index.set(next);
                    } else {
                        let next = next_selectable_index(&items_for_keys, *active_index.peek(), 1);
                        active_index.set(next);
                    }
                }
                Key::Named(NamedKey::ArrowUp) => {
                    e.stop_propagation();
                    if let Some(sub_idx) = *open_submenu_index.peek()
                        && let Some(parent) = items_for_keys.get(sub_idx)
                    {
                        let next = next_selectable_index(&parent.children, *submenu_active_index.peek(), -1);
                        submenu_active_index.set(next);
                    } else {
                        let next = next_selectable_index(&items_for_keys, *active_index.peek(), -1);
                        active_index.set(next);
                    }
                }
                Key::Named(NamedKey::ArrowRight) => {
                    e.stop_propagation();
                    if open_submenu_index.peek().is_none()
                        && let Some(idx) = *active_index.peek()
                        && let Some(item) = items_for_keys.get(idx)
                        && item.is_submenu()
                    {
                        open_submenu_index.set(Some(idx));
                        submenu_active_index.set(next_selectable_index(&item.children, None, 1));
                    }
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    e.stop_propagation();
                    if open_submenu_index.peek().is_some() {
                        open_submenu_index.set(None);
                        submenu_active_index.set(None);
                    }
                }
                Key::Named(NamedKey::Enter) => {
                    e.stop_propagation();
                    if let Some(sub_idx) = *open_submenu_index.peek()
                        && let Some(parent) = items_for_keys.get(sub_idx)
                        && let Some(child_idx) = *submenu_active_index.peek()
                        && let Some(child) = parent.children.get(child_idx)
                        && child.enabled
                    {
                        invoke_menu_command(&core_for_keys, &child.command, &child.command_args);
                        if !child.keeps_menu_open
                            && let Some(close) = &on_close_keys
                        {
                            close.call(());
                        }
                    } else if let Some(idx) = *active_index.peek()
                        && let Some(item) = items_for_keys.get(idx)
                        && item.enabled
                    {
                        if item.is_submenu() {
                            open_submenu_index.set(Some(idx));
                            submenu_active_index.set(next_selectable_index(&item.children, None, 1));
                        } else {
                            invoke_menu_command(&core_for_keys, &item.command, &item.command_args);
                            if !item.keeps_menu_open
                                && let Some(close) = &on_close_keys
                            {
                                close.call(());
                            }
                        }
                    }
                }
                _ => {}
            }
        };

        let current_active = *active_index.read();
        let current_submenu = *open_submenu_index.read();
        let current_sub_active = *submenu_active_index.read();

        let rows = self.items.iter().enumerate().map(|(idx, item)| {
            if item.is_separator() {
                return app_menu_separator(&theme, idx);
            }
            if let MenuEntryKind::Widget { widget_id } = &item.kind
                && widget_id == ZOOM_CONTROLS_WIDGET_ID
            {
                return app_menu_zoom_row(&theme, &core, self.zoom_level, idx);
            }
            let highlighted = current_active == Some(idx) || current_submenu == Some(idx);
            let is_submenu = item.is_submenu();
            let enabled = item.enabled;
            let keeps_menu_open = item.keeps_menu_open;
            let command = item.command.clone();
            let command_args = item.command_args.clone();
            let core_press = core.clone();
            let on_close_press = self.on_close.clone();

            app_menu_row(
                &theme,
                &core,
                item,
                idx,
                highlighted,
                move |_| {
                    active_index.set(Some(idx));
                    if is_submenu {
                        open_submenu_index.set(Some(idx));
                        submenu_active_index.set(None);
                    } else {
                        open_submenu_index.set(None);
                        submenu_active_index.set(None);
                    }
                },
                move |_| {
                    if !enabled {
                        return;
                    }
                    if is_submenu {
                        let next = if *open_submenu_index.peek() == Some(idx) {
                            None
                        } else {
                            Some(idx)
                        };
                        open_submenu_index.set(next);
                        submenu_active_index.set(None);
                    } else {
                        invoke_menu_command(&core_press, &command, &command_args);
                        if !keeps_menu_open && let Some(close) = &on_close_press {
                            close.call(());
                        }
                    }
                },
            )
        });

        let main_card = rect()
            .a11y_role(AccessibilityRole::Menu)
            .width(Size::px(APP_MENU_WIDTH))
            .padding(Gaps::new(6.0, 6.0, 6.0, 6.0))
            .corner_radius(CornerRadius::new_all(10.0))
            .background(app_menu_panel_bg(&theme))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::Border)),
            )
            .shadow(theme.shadows.float_drop.to_shadow())
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .children(rows);

        let submenu_card = current_submenu
            .and_then(|idx| self.items.get(idx).map(|parent| (idx, parent)))
            .filter(|(_, parent)| !parent.children.is_empty())
            .map(|(parent_idx, parent)| {
                let sub_rows = parent.children.iter().enumerate().map(|(sub_idx, child)| {
                    if child.is_separator() {
                        return app_menu_separator(&theme, sub_idx);
                    }
                    let enabled = child.enabled;
                    let keeps_menu_open = child.keeps_menu_open;
                    let command = child.command.clone();
                    let command_args = child.command_args.clone();
                    let core_sub = core.clone();
                    let on_close_sub = self.on_close.clone();
                    let sub_highlighted = current_sub_active == Some(sub_idx);
                    app_menu_row(
                        &theme,
                        &core,
                        child,
                        sub_idx,
                        sub_highlighted,
                        move |_| {
                            submenu_active_index.set(Some(sub_idx));
                        },
                        move |_| {
                            if !enabled {
                                return;
                            }
                            invoke_menu_command(&core_sub, &command, &command_args);
                            if !keeps_menu_open && let Some(close) = &on_close_sub {
                                close.call(());
                            }
                        },
                    )
                });

                let card = rect()
                    .a11y_role(AccessibilityRole::Menu)
                    .width(Size::px(APP_SUBMENU_WIDTH))
                    .padding(Gaps::new(6.0, 6.0, 6.0, 6.0))
                    .corner_radius(CornerRadius::new_all(10.0))
                    .background(app_menu_panel_bg(&theme))
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.color(ThemeRole::Border)),
                    )
                    .shadow(theme.shadows.float_drop.to_shadow())
                    .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
                    .children(sub_rows);

                let y_off = row_y_offset(&self.items, parent_idx);
                let sub_height = estimate_app_menu_height(&parent.children);
                (card, y_off, sub_height)
            });

        if let Some((ox, oy)) = self.origin {
            let positioned_sub = submenu_card.map(|(card, y_off, sub_h)| {
                let (sx, sy) = compute_submenu_origin(
                    (ox, oy),
                    APP_MENU_WIDTH,
                    y_off,
                    (APP_SUBMENU_WIDTH, sub_h),
                    self.window_size,
                );
                rect().position(Position::new_global().left(sx).top(sy)).child(card)
            });
            rect()
                .on_global_key_down(handle_key)
                .child(
                    rect()
                        .position(Position::new_global().left(ox).top(oy))
                        .child(main_card),
                )
                .maybe_child(positioned_sub)
                .into_element()
        } else {
            // Inline layout for gallery showcase (main card + side-by-side cascading submenu aligned to row).
            let main_height = estimate_app_menu_height(&self.items);
            rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Start)
                .on_global_key_down(handle_key)
                .child(main_card)
                .maybe_child(submenu_card.map(|(card, y_off, sub_h)| {
                    let top_margin = (y_off - 6.0).min((main_height - sub_h).max(0.0)).max(0.0);
                    rect().margin(Gaps::new(top_margin, 0.0, 0.0, 6.0)).child(card)
                }))
                .into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn app_menu_separator(theme: &ResolvedTheme, key: usize) -> Element {
    rect()
        .key(key)
        .width(Size::fill())
        .padding(Gaps::new(5.0, 6.0, 5.0, 6.0))
        .child(
            rect()
                .width(Size::fill())
                .height(Size::px(1.0))
                .background(theme.color(ThemeRole::BorderSubtle)),
        )
        .into_element()
}

fn app_menu_row(
    theme: &ResolvedTheme,
    core: &CoreView,
    item: &ResolvedMenuItem,
    key: usize,
    highlighted: bool,
    on_hover: impl FnMut(Event<PointerEventData>) + 'static,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let text_role = if item.enabled {
        ThemeRole::Text
    } else {
        ThemeRole::TextMuted
    };
    let bg = if highlighted && item.enabled {
        theme.color(ThemeRole::Hover)
    } else {
        Color::TRANSPARENT
    };

    let trailing: Option<Element> = match &item.kind {
        MenuEntryKind::Submenu => Some(
            Icon::new("arrow-right")
                .size(12.0)
                .role(ThemeRole::TextMuted)
                .into_element(),
        ),
        MenuEntryKind::Toggle { checked } => {
            let toggle_core = core.clone();
            let cmd = item.command.clone();
            Some(
                rect()
                    .on_press(|e: Event<PressEventData>| e.stop_propagation())
                    .child(
                        KitSwitch::new(*checked)
                            .disabled(!item.enabled)
                            .on_toggle(move |next: bool| {
                                invoke_menu_command(&toggle_core, &cmd, &[("enabled".to_string(), next.to_string())]);
                            }),
                    )
                    .into_element(),
            )
        }
        MenuEntryKind::Checkable { checked } => {
            if *checked {
                Some(Icon::new("check").size(13.0).role(ThemeRole::Accent).into_element())
            } else {
                None
            }
        }
        _ => item.shortcut_hint.as_ref().map(|shortcut| {
            label()
                .text(shortcut.clone())
                .font_size(12.0)
                .color(theme.color(ThemeRole::TextMuted))
                .into_element()
        }),
    };

    rect()
        .key(key)
        .a11y_role(AccessibilityRole::MenuItem)
        .a11y_alt(item.label.clone())
        .width(Size::fill())
        .height(Size::px(APP_MENU_ROW_HEIGHT))
        .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
        .spacing(10.0)
        .direction(Direction::Horizontal)
        .content(Content::Flex)
        .cross_align(Alignment::center())
        .corner_radius(CornerRadius::new_all(6.0))
        .background(bg)
        .cursor(if item.enabled {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        })
        .on_pointer_enter(on_hover)
        .on_press(on_press)
        .child(
            rect()
                .width(Size::px(16.0))
                .height(Size::px(16.0))
                .center()
                .maybe_child(
                    item.icon
                        .as_ref()
                        .map(|icon| Icon::new(icon.clone()).size(15.0).role(text_role)),
                ),
        )
        .child(
            label()
                .text(item.label.clone())
                .width(Size::flex(1.0))
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .font_size(theme.font_sizes.md)
                .color(theme.color(text_role)),
        )
        .maybe_child(trailing)
        .into_element()
}

fn app_menu_zoom_row(theme: &ResolvedTheme, core: &CoreView, zoom_level: f32, key: usize) -> Element {
    let pct_text = format!("{}%", (zoom_level * 100.0).round() as i32);
    let out_core = core.clone();
    let reset_core = core.clone();
    let in_core = core.clone();
    let fs_core = core.clone();

    let zoom_btn = |text: &'static str, alt: &'static str, mut on_click: Box<dyn FnMut()>| {
        Button::new()
            .flat()
            .compact()
            .width(Size::px(26.0))
            .height(Size::px(26.0))
            .padding(Gaps::new_all(0.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(Color::TRANSPARENT)
            .hover_background(theme.color(ThemeRole::Hover))
            .border_fill(Color::TRANSPARENT)
            .color(theme.color(ThemeRole::Text))
            .cursor(CursorIcon::Pointer)
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_click();
            })
            .child(
                rect()
                    .a11y_alt(alt)
                    .center()
                    .child(label().text(text).font_size(15.0).font_weight(FontWeight::MEDIUM)),
            )
    };

    rect()
        .key(key)
        .width(Size::fill())
        .height(Size::px(34.0))
        .padding(Gaps::new(0.0, 6.0, 0.0, 36.0))
        .spacing(4.0)
        .direction(Direction::Horizontal)
        .content(Content::Flex)
        .cross_align(Alignment::center())
        .child(
            label()
                .text("Zoom")
                .width(Size::flex(1.0))
                .font_size(theme.font_sizes.md)
                .color(theme.color(ThemeRole::Text)),
        )
        .child(zoom_btn(
            "−",
            "Zoom out",
            Box::new(move || {
                out_core.invoke_or_log(Command::ZoomOut { tab_id: None });
            }),
        ))
        .child(
            Button::new()
                .flat()
                .compact()
                .width(Size::px(48.0))
                .height(Size::px(26.0))
                .padding(Gaps::new(0.0, 6.0, 0.0, 6.0))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(Color::TRANSPARENT)
                .hover_background(theme.color(ThemeRole::Hover))
                .border_fill(Color::TRANSPARENT)
                .color(theme.color(ThemeRole::Text))
                .cursor(CursorIcon::Pointer)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    reset_core.invoke_or_log(Command::ZoomReset { tab_id: None });
                })
                .child(
                    rect()
                        .a11y_alt("Reset zoom")
                        .center()
                        .child(label().text(pct_text).font_size(12.5).font_weight(FontWeight::MEDIUM)),
                ),
        )
        .child(zoom_btn(
            "+",
            "Zoom in",
            Box::new(move || {
                in_core.invoke_or_log(Command::ZoomIn { tab_id: None });
            }),
        ))
        .child(
            rect()
                .width(Size::px(1.0))
                .height(Size::px(18.0))
                .margin(Gaps::new(0.0, 4.0, 0.0, 4.0))
                .background(theme.color(ThemeRole::BorderSubtle)),
        )
        .child(
            Button::new()
                .flat()
                .compact()
                .width(Size::px(26.0))
                .height(Size::px(26.0))
                .padding(Gaps::new_all(0.0))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(Color::TRANSPARENT)
                .hover_background(theme.color(ThemeRole::Hover))
                .border_fill(Color::TRANSPARENT)
                .color(theme.color(ThemeRole::Text))
                .cursor(CursorIcon::Pointer)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    invoke_menu_command(&fs_core, "window.toggle_fullscreen", &[]);
                })
                .child(
                    rect()
                        .a11y_alt("Toggle fullscreen")
                        .center()
                        .child(Icon::new("fullscreen").size(14.0).role(ThemeRole::Text)),
                ),
        )
        .into_element()
}

/// Helper for callers (such as a future toolbar button) to compute a standard [`AnchorRect`] from a Freya [`Area`].
#[allow(dead_code)]
pub fn anchor_from_area(area: Area) -> AnchorRect {
    AnchorRect::from_area(area)
}
