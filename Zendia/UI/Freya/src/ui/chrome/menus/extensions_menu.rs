/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The extensions menu: every installed extension in one popup, opened from the extensions toolbar button, as
//! Firefox's and Zen's puzzle-piece panel does. A row runs the extension's action; its "..." menu pins it to the
//! toolbar, opens its settings, or removes it. Extensions that are not pinned live only here.

use crate::ui::chrome::menus::state::{compute_popup_origin_above_or_below, use_menu_state};
use crate::ui::core_view::{CoreView, use_core};
use crate::ui::kit::Icon;
use crate::ui::theme::POPUP_OVERLAY_LEVEL;
use crate::ui::theme::{ResolvedTheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{ServiceId, ServiceStatus};
use zendia_browser_core::modules::{ExtensionId, ExtensionInfo, service_status_in};
use zendia_browser_core::settings::SettingsGroupId;

const MENU_WIDTH: f32 = 320.0;
const ROW_HEIGHT: f32 = 52.0;
const HEADER_HEIGHT: f32 = 40.0;
const FOOTER_HEIGHT: f32 = 44.0;
const EMPTY_HEIGHT: f32 = 92.0;
const ROW_MENU_WIDTH: f32 = 196.0;

/// Height the menu takes for `count` extensions, used to place it above or below its button.
fn menu_height(count: usize) -> f32 {
    let body = if count == 0 {
        EMPTY_HEIGHT
    } else {
        ROW_HEIGHT * count as f32
    };
    HEADER_HEIGHT + body + FOOTER_HEIGHT + 12.0
}

fn panel_background(theme: &ResolvedTheme) -> Color {
    if theme.scheme.is_dark() {
        theme
            .rgba(ThemeRole::Surface)
            .mix(theme.rgba(ThemeRole::SurfaceRaised), 0.35)
            .to_color()
    } else {
        theme.color(ThemeRole::Surface)
    }
}

/// The line under an extension's name.
fn status_line(ext: &ExtensionInfo, paused: bool) -> &'static str {
    if !ext.enabled {
        "Disabled"
    } else if paused {
        "Paused — ExtensionServer is not running"
    } else if ext.has_site_access {
        "Can read and change data on this site"
    } else {
        "Has no access to this site"
    }
}

/// Anchored, animated host for [`ExtensionsMenuPanel`], drawn by the menu layer while the menu is open.
#[derive(Clone, PartialEq)]
pub struct ExtensionsMenuOverlay;

impl Component for ExtensionsMenuOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let menu_state = use_menu_state();
        let is_open = *menu_state.extensions_menu_open.read();
        let anchor = *menu_state.extensions_menu_anchor.read();
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

        let count = core.model().read().extensions.len();
        let root_size = *Platform::get().current_window().root_size.read();
        let window_size = (root_size.width.max(320.0), root_size.height.max(240.0));
        let origin = compute_popup_origin_above_or_below(anchor, (MENU_WIDTH, menu_height(count)), window_size);

        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .opacity(opacity)
            .child(
                rect()
                    .position(Position::new_global().left(0.0).top(0.0))
                    .width(Size::window_percent(100.0))
                    .height(Size::window_percent(100.0))
                    .on_mouse_down(move |_| menu_state.close_extensions_menu()),
            )
            .child(
                rect()
                    .position(Position::new_global().left(origin.0).top(origin.1))
                    .child(ExtensionsMenuPanel::new().capture_keys(is_open)),
            )
            .into_element()
    }
}

/// The menu's card: header, a row per extension, and the "Manage extensions" footer. The gallery draws it inline.
#[derive(Clone, PartialEq)]
pub struct ExtensionsMenuPanel {
    capture_keys: bool,
    initial_row_menu: Option<ExtensionId>,
    key: DiffKey,
}

impl KeyExt for ExtensionsMenuPanel {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ExtensionsMenuPanel {
    pub fn new() -> Self {
        Self {
            capture_keys: false,
            initial_row_menu: None,
            key: DiffKey::None,
        }
    }

    /// Handles arrows, Enter and Escape from anywhere in the window while set.
    pub fn capture_keys(mut self, capture: bool) -> Self {
        self.capture_keys = capture;
        self
    }

    /// Opens one row's "..." menu from the start (gallery states).
    pub fn row_menu(mut self, extension_id: ExtensionId) -> Self {
        self.initial_row_menu = Some(extension_id);
        self
    }
}

impl Default for ExtensionsMenuPanel {
    fn default() -> Self {
        Self::new()
    }
}

fn close_menu(menu_state: Option<crate::ui::chrome::menus::MenuViewState>) {
    if let Some(state) = menu_state {
        state.close_extensions_menu();
    }
}

fn manage_extensions(core: &CoreView) {
    core.invoke_or_log(Command::SettingsOpen {
        group: Some(SettingsGroupId::Extensions),
    });
}

impl Component for ExtensionsMenuPanel {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = crate::ui::chrome::menus::try_use_menu_state();
        let mut cursor = use_state(|| None::<usize>);
        let initial_row_menu = self.initial_row_menu.clone();
        let mut row_menu = use_state(move || initial_row_menu);

        let (extensions, paused) = {
            let model = core.model().read();
            let status = service_status_in(&model.services, ServiceId::ExtensionServer);
            (model.extensions.clone(), !matches!(status, ServiceStatus::Running))
        };
        let count = extensions.len();

        let handle_key = {
            let core = core.clone();
            let extensions = extensions.clone();
            move |e: Event<KeyboardEventData>| match &e.key {
                Key::Named(NamedKey::Escape) => {
                    if row_menu.peek().is_some() {
                        row_menu.set(None);
                    } else {
                        close_menu(menu_state);
                    }
                }
                Key::Named(NamedKey::ArrowDown) if count > 0 => {
                    let next = cursor.peek().map_or(0, |i| (i + 1) % count);
                    cursor.set(Some(next));
                }
                Key::Named(NamedKey::ArrowUp) if count > 0 => {
                    let next = cursor.peek().map_or(count - 1, |i| (i + count - 1) % count);
                    cursor.set(Some(next));
                }
                Key::Named(NamedKey::Enter) => {
                    if let Some(ext) = cursor.peek().and_then(|i| extensions.get(i)) {
                        core.invoke_or_log(Command::ExtensionInvokeAction {
                            extension_id: ext.id.clone(),
                        });
                        close_menu(menu_state);
                    }
                }
                _ => {}
            }
        };

        let text = theme.color(ThemeRole::Text);
        let muted = theme.color(ThemeRole::TextMuted);
        let header = rect()
            .width(Size::fill())
            .height(Size::px(HEADER_HEIGHT))
            .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
            .main_align(Alignment::Center)
            .child(
                label()
                    .text("Extensions")
                    .font_size(14.0)
                    .font_weight(FontWeight::BOLD)
                    .color(text),
            );

        let mut body = rect().width(Size::fill());
        if extensions.is_empty() {
            body = body.child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(EMPTY_HEIGHT))
                    .center()
                    .spacing(6.0)
                    .child(Icon::new("extension").size(22.0).role(ThemeRole::TextMuted))
                    .child(label().text("No extensions installed").font_size(13.0).color(muted)),
            );
        }
        for (index, ext) in extensions.iter().enumerate() {
            body = body.child(ExtensionMenuRow {
                ext: ext.clone(),
                paused,
                highlighted: *cursor.read() == Some(index),
                menu_open: row_menu.read().as_ref() == Some(&ext.id),
                row_menu,
            });
        }

        let footer = {
            let core = core.clone();
            rect()
                .width(Size::fill())
                .height(Size::px(FOOTER_HEIGHT))
                .padding(Gaps::new(0.0, 6.0, 0.0, 6.0))
                .spacing(5.0)
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(1.0))
                        .background(theme.color(ThemeRole::BorderSubtle)),
                )
                .child(MenuItemRow {
                    icon: "settings",
                    title: "Manage extensions",
                    role: ThemeRole::Text,
                    height: 32.0,
                    on_press: EventHandler::new(move |_| {
                        manage_extensions(&core);
                        close_menu(menu_state);
                    }),
                })
        };

        let card = rect()
            .a11y_role(AccessibilityRole::Menu)
            .a11y_alt("Extensions")
            .width(Size::px(MENU_WIDTH))
            .padding(Gaps::new(6.0, 0.0, 6.0, 0.0))
            .corner_radius(CornerRadius::new_all(10.0))
            .background(panel_background(&theme))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::Border)),
            )
            .shadow(theme.shadows.float_drop.to_shadow())
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .child(header)
            .child(body)
            .child(footer);

        if self.capture_keys {
            card.on_global_key_down(handle_key).into_element()
        } else {
            card.into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// One extension in the menu, with its "..." button and, when open, its options menu.
#[derive(Clone, PartialEq)]
struct ExtensionMenuRow {
    ext: ExtensionInfo,
    paused: bool,
    highlighted: bool,
    menu_open: bool,
    row_menu: State<Option<ExtensionId>>,
}

impl Component for ExtensionMenuRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = crate::ui::chrome::menus::try_use_menu_state();
        let ext = self.ext.clone();
        let mut row_menu = self.row_menu;
        let mut row_hovered = use_state(|| false);
        let mut more_hovered = use_state(|| false);
        let usable = ext.enabled && !self.paused;
        let hover = theme.color(ThemeRole::Hover);

        let run_action = {
            let core = core.clone();
            let id = ext.id.clone();
            move |_| {
                core.invoke_or_log(Command::ExtensionInvokeAction {
                    extension_id: id.clone(),
                });
                close_menu(menu_state);
            }
        };
        let toggle_row_menu = {
            let id = ext.id.clone();
            move |e: Event<PressEventData>| {
                e.stop_propagation();
                let next = if row_menu.peek().as_ref() == Some(&id) {
                    None
                } else {
                    Some(id.clone())
                };
                row_menu.set(next);
            }
        };

        let badge = ext.badge_text.clone().filter(|_| usable).map(|text| {
            rect()
                .position(Position::new_absolute().right(-4.0).top(-4.0))
                .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
                .min_width(Size::px(14.0))
                .height(Size::px(14.0))
                .corner_radius(CornerRadius::new_all(7.0))
                .background(theme.color(ThemeRole::Accent))
                .center()
                .child(
                    label()
                        .text(text)
                        .font_size(9.0)
                        .font_weight(FontWeight::BOLD)
                        .color(theme.color(ThemeRole::TextOnAccent)),
                )
        });
        let icon = rect()
            .width(Size::px(28.0))
            .height(Size::px(28.0))
            .corner_radius(CornerRadius::new_all(7.0))
            .background(theme.color(ThemeRole::SurfaceRaised))
            .center()
            .opacity(if usable { 1.0 } else { 0.5 })
            .child(Icon::new(ext.icon.clone()).size(17.0).role(ThemeRole::Text))
            .maybe_child(badge);

        let name_row = rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(6.0)
            .child(
                label()
                    .text(ext.name.clone())
                    .font_size(13.0)
                    .font_weight(FontWeight::MEDIUM)
                    .max_lines(1)
                    .color(theme.color(ThemeRole::Text)),
            )
            .maybe_child(
                ext.pinned_to_toolbar
                    .then(|| Icon::new("pin").size(11.0).role(ThemeRole::TextMuted)),
            );
        let status_role = if self.paused && ext.enabled {
            ThemeRole::Warning
        } else {
            ThemeRole::TextMuted
        };
        let texts = rect().width(Size::flex(1.0)).spacing(2.0).child(name_row).child(
            label()
                .text(status_line(&ext, self.paused))
                .font_size(11.0)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .color(theme.color(status_role)),
        );

        let more_button = rect()
            .width(Size::px(28.0))
            .height(Size::px(28.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .center()
            .background(if self.menu_open || *more_hovered.read() {
                hover
            } else {
                Color::TRANSPARENT
            })
            .on_pointer_enter(move |_| more_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| more_hovered.set_if_modified(false))
            .cursor(CursorIcon::Pointer)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("{} options", ext.name))
            .on_press(toggle_row_menu)
            .child(
                label()
                    .text("···")
                    .font_size(14.0)
                    .color(theme.color(ThemeRole::TextMuted)),
            );

        let options = self.menu_open.then(|| ExtensionRowOptions {
            ext: ext.clone(),
            row_menu,
        });

        rect()
            .width(Size::fill())
            .padding(Gaps::new(0.0, 6.0, 0.0, 6.0))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(ROW_HEIGHT))
                    .padding(Gaps::new(0.0, 6.0, 0.0, 10.0))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::Center)
                    .spacing(10.0)
                    .background(if self.highlighted || *row_hovered.read() {
                        hover
                    } else {
                        Color::TRANSPARENT
                    })
                    .on_pointer_enter(move |_| row_hovered.set_if_modified(true))
                    .on_pointer_leave(move |_| row_hovered.set_if_modified(false))
                    .cursor(CursorIcon::Pointer)
                    .a11y_role(AccessibilityRole::MenuItem)
                    .a11y_alt(ext.name.clone())
                    .on_press(run_action)
                    .child(icon)
                    .child(texts)
                    .child(more_button),
            )
            .maybe_child(options)
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.ext.id.0)
    }
}

/// The "..." menu of one extension: pin or unpin, manage, remove.
#[derive(Clone, PartialEq)]
struct ExtensionRowOptions {
    ext: ExtensionInfo,
    row_menu: State<Option<ExtensionId>>,
}

impl Component for ExtensionRowOptions {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = crate::ui::chrome::menus::try_use_menu_state();
        let mut row_menu = self.row_menu;
        let id = self.ext.id.clone();
        let pinned = self.ext.pinned_to_toolbar;

        let pin = {
            let core = core.clone();
            let id = id.clone();
            MenuItemRow {
                icon: if pinned { "unpin" } else { "pin" },
                title: if pinned { "Unpin from Toolbar" } else { "Pin to Toolbar" },
                role: ThemeRole::Text,
                height: 30.0,
                on_press: EventHandler::new(move |_| {
                    core.invoke_or_log(Command::ExtensionSetToolbarPinned {
                        extension_id: id.clone(),
                        pinned: !pinned,
                    });
                    row_menu.set(None);
                }),
            }
        };
        let manage = {
            let core = core.clone();
            MenuItemRow {
                icon: "settings",
                title: "Manage Extension",
                role: ThemeRole::Text,
                height: 30.0,
                on_press: EventHandler::new(move |_| {
                    manage_extensions(&core);
                    row_menu.set(None);
                    close_menu(menu_state);
                }),
            }
        };
        let remove = {
            let core = core.clone();
            MenuItemRow {
                icon: "trash",
                title: "Remove Extension",
                role: ThemeRole::Danger,
                height: 30.0,
                on_press: EventHandler::new(move |_| {
                    core.invoke_or_log(Command::ExtensionRemove {
                        extension_id: id.clone(),
                    });
                    row_menu.set(None);
                }),
            }
        };

        rect().width(Size::fill()).height(Size::px(0.0)).child(
            rect()
                .layer(Layer::Relative(8))
                .position(Position::new_absolute().right(10.0).top(-6.0))
                .width(Size::px(ROW_MENU_WIDTH))
                .padding(Gaps::new_all(4.0))
                .corner_radius(CornerRadius::new_all(8.0))
                .background(panel_background(&theme))
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::Border)),
                )
                .shadow(theme.shadows.float_drop.to_shadow())
                .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
                .child(pin)
                .child(manage)
                .child(remove),
        )
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&format!("options:{}", self.ext.id.0))
    }
}

/// A menu entry with an icon and a title that highlights under the pointer.
#[derive(Clone, PartialEq)]
struct MenuItemRow {
    icon: &'static str,
    title: &'static str,
    role: ThemeRole,
    height: f32,
    on_press: EventHandler<()>,
}

impl Component for MenuItemRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let on_press = self.on_press.clone();
        rect()
            .width(Size::fill())
            .height(Size::px(self.height))
            .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
            .corner_radius(CornerRadius::new_all(7.0))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(10.0)
            .background(if *hovered.read() {
                theme.color(ThemeRole::Hover)
            } else {
                Color::TRANSPARENT
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .cursor(CursorIcon::Pointer)
            .a11y_role(AccessibilityRole::MenuItem)
            .a11y_alt(self.title)
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_press.call(());
            })
            .child(Icon::new(self.icon).size(14.0).role(self.role))
            .child(label().text(self.title).font_size(13.0).color(theme.color(self.role)))
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.title.to_string())
    }
}
