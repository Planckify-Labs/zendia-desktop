/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The extensions menu: every installed extension in one popup, opened from the extensions toolbar button, as
//! Firefox's and Zen's puzzle-piece panel does. A row runs the extension's action; its "..." menu pins it to the
//! toolbar, opens its settings, or removes it. Extensions that are not pinned live only here.

use crate::ui::chrome::menus::state::{AnchorRect, compute_popup_origin_above_or_below, use_menu_state};
use crate::ui::core_view::{CoreView, use_core};
use crate::ui::kit::{
    Icon, ListNavAction, ListNavConfig, PopupContent, PopupHorizontalAlign, PopupPresence, TypeaheadState,
    anchor_popup_origin, classify_list_key, focus_ring_border, max_popup_height, moved_index, scroll_item_into_view,
};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedTheme, ThemeRole, use_theme};
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
const ROW_MENU_HEIGHT: f32 = 98.0;
const MENU_MAX_HEIGHT: f32 = 480.0;

/// Height the menu takes for `count` extensions, clamped to `max_height`, used to place it above or below its button.
fn menu_height(count: usize, max_height: f32) -> f32 {
    let body = if count == 0 {
        EMPTY_HEIGHT
    } else {
        ROW_HEIGHT * count as f32
    };
    (HEADER_HEIGHT + body + FOOTER_HEIGHT + 12.0).min(max_height)
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
        let mut open_seq = use_state(|| 0u64);
        let mut was_open = use_state(|| is_open);
        if *was_open.peek() != is_open {
            was_open.set(is_open);
            if is_open {
                let next = open_seq.peek().wrapping_add(1);
                open_seq.set(next);
            }
        }

        let count = core.model().read().extensions.len();
        let root_size = *Platform::get().current_window().root_size.read();
        let window_size = (root_size.width.max(320.0), root_size.height.max(240.0));
        let max_h = max_popup_height(window_size.1, 12.0, MENU_MAX_HEIGHT);
        let effective_anchor = if anchor.width <= 0.0 && anchor.height <= 0.0 {
            AnchorRect {
                x: (window_size.0 - MENU_WIDTH - 16.0).max(16.0),
                y: 44.0,
                width: 28.0,
                height: 28.0,
            }
        } else {
            anchor
        };
        let origin =
            compute_popup_origin_above_or_below(effective_anchor, (MENU_WIDTH, menu_height(count, max_h)), window_size);
        let seq = *open_seq.read();

        PopupPresence::anchored(is_open, origin)
            .open_seq(seq)
            .on_backdrop_mouse_down(move |_| menu_state.close_extensions_menu())
            .child(PopupContent::new(seq).child(ExtensionsMenuPanel::new().max_height(max_h).capture_keys(is_open)))
    }
}

/// The menu's card: header, a row per extension, and the "Manage extensions" footer. The gallery draws it inline.
#[derive(Clone, PartialEq)]
pub struct ExtensionsMenuPanel {
    capture_keys: bool,
    initial_row_menu: Option<ExtensionId>,
    max_height: f32,
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
            max_height: MENU_MAX_HEIGHT,
            key: DiffKey::None,
        }
    }

    /// Handles arrows, Enter and Escape from anywhere in the window while set.
    pub fn capture_keys(mut self, capture: bool) -> Self {
        self.capture_keys = capture;
        self
    }

    /// Caps the card height so long extension lists scroll inside the menu.
    pub fn max_height(mut self, max_height: f32) -> Self {
        self.max_height = max_height;
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
        let mut typeahead = use_state(TypeaheadState::default);
        let mut list_scroll = use_scroll_controller(ScrollConfig::default);
        let initial_row_menu = self.initial_row_menu.clone();
        let mut row_menu = use_state(move || initial_row_menu);
        let row_menu_anchor = use_state(|| None::<AnchorRect>);

        let (extensions, paused) = {
            let model = core.model().read();
            let status = service_status_in(&model.services, ServiceId::ExtensionServer);
            (model.extensions.clone(), !matches!(status, ServiceStatus::Running))
        };
        let count = extensions.len();
        let root_size = *Platform::get().current_window().root_size.read();
        let window_size = (root_size.width.max(320.0), root_size.height.max(240.0));
        let max_h = max_popup_height(window_size.1, 12.0, self.max_height);
        let max_list_h = (max_h - HEADER_HEIGHT - FOOTER_HEIGHT - 12.0).max(ROW_HEIGHT);
        let raw_list_h = if count == 0 {
            EMPTY_HEIGHT
        } else {
            ROW_HEIGHT * count as f32
        };
        let scrolls = count > 0 && raw_list_h > max_list_h;

        let handle_key = {
            let core = core.clone();
            let extensions = extensions.clone();
            move |e: Event<KeyboardEventData>| {
                let Some(action) = classify_list_key(&e, ListNavConfig::default()) else {
                    return;
                };
                e.stop_propagation();
                e.prevent_default();
                match action {
                    ListNavAction::Dismiss => {
                        if row_menu.peek().is_some() {
                            row_menu.set(None);
                        } else {
                            close_menu(menu_state);
                        }
                    }
                    ListNavAction::Activate => {
                        if let Some(ext) = cursor.peek().and_then(|i| extensions.get(i))
                            && ext.enabled
                            && !paused
                        {
                            core.invoke_or_log(Command::ExtensionInvokeAction {
                                extension_id: ext.id.clone(),
                            });
                            close_menu(menu_state);
                        }
                    }
                    _ if count > 0 => {
                        let next = moved_index(
                            &action,
                            &mut typeahead.write(),
                            count,
                            *cursor.peek(),
                            true,
                            |i| extensions.get(i).map(|ext| ext.name.clone()),
                            |_| true,
                        );
                        if let Some(idx) = next {
                            cursor.set(Some(idx));
                            scroll_item_into_view(
                                &mut list_scroll,
                                idx as f32 * ROW_HEIGHT,
                                ROW_HEIGHT,
                                max_list_h,
                                raw_list_h,
                            );
                        }
                    }
                    _ => {}
                }
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

        let open_menu_id = row_menu.read().clone();
        let row_elements: Vec<Element> = extensions
            .iter()
            .enumerate()
            .map(|(index, ext)| {
                ExtensionMenuRow {
                    ext: ext.clone(),
                    paused,
                    highlighted: *cursor.read() == Some(index),
                    menu_open: open_menu_id.as_ref() == Some(&ext.id),
                    row_menu,
                    row_menu_anchor,
                }
                .into_element()
            })
            .collect();

        let body: Element = if extensions.is_empty() {
            rect()
                .width(Size::fill())
                .height(Size::px(EMPTY_HEIGHT))
                .center()
                .spacing(6.0)
                .child(Icon::new("extension").size(22.0).role(ThemeRole::TextMuted))
                .child(label().text("No extensions installed").font_size(13.0).color(muted))
                .into_element()
        } else if scrolls {
            ScrollView::new_controlled(list_scroll)
                .width(Size::fill())
                .height(Size::px(max_list_h))
                .children(row_elements)
                .into_element()
        } else {
            rect().width(Size::fill()).children(row_elements).into_element()
        };

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

        // Render the row "..." submenu clamped to window bounds so it never protrudes past window/sidebar edges.
        let open_options_overlay = open_menu_id
            .as_ref()
            .and_then(|id| extensions.iter().find(|e| &e.id == id))
            .and_then(|ext| {
                (*row_menu_anchor.read()).map(|anchor| {
                    let (ox, oy) = anchor_popup_origin(
                        anchor,
                        (ROW_MENU_WIDTH, ROW_MENU_HEIGHT),
                        window_size,
                        PopupHorizontalAlign::End,
                        4.0,
                        8.0,
                    );
                    rect()
                        .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                        .position(Position::new_global().left(ox).top(oy))
                        .child(ExtensionRowOptions {
                            ext: ext.clone(),
                            row_menu,
                        })
                })
            });

        let card = rect()
            .a11y_role(AccessibilityRole::Menu)
            .a11y_alt("Extensions")
            .width(Size::px(MENU_WIDTH))
            .max_height(Size::px(max_h))
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
            .on_mouse_down(move |e: Event<MouseEventData>| {
                e.stop_propagation();
                row_menu.set_if_modified(None);
            })
            .child(header)
            .child(body)
            .child(footer)
            .maybe_child(open_options_overlay);

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

/// One extension in the menu, with its "..." button.
#[derive(Clone, PartialEq)]
struct ExtensionMenuRow {
    ext: ExtensionInfo,
    paused: bool,
    highlighted: bool,
    menu_open: bool,
    row_menu: State<Option<ExtensionId>>,
    row_menu_anchor: State<Option<AnchorRect>>,
}

impl Component for ExtensionMenuRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = crate::ui::chrome::menus::try_use_menu_state();
        let ext = self.ext.clone();
        let mut row_menu = self.row_menu;
        let mut row_menu_anchor = self.row_menu_anchor;
        let mut row_hovered = use_state(|| false);
        let mut more_hovered = use_state(|| false);
        let mut more_area = use_state(AnchorRect::default);
        let row_a11y = use_a11y();
        let more_a11y = use_a11y();
        let row_focus = use_focus(row_a11y);
        let more_focus = use_focus(more_a11y);
        let usable = ext.enabled && !self.paused;
        let hover = theme.color(ThemeRole::Hover);

        let run_action = {
            let core = core.clone();
            let id = ext.id.clone();
            move |_| {
                if !usable {
                    return;
                }
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
                if row_menu.peek().as_ref() == Some(&id) {
                    row_menu.set(None);
                } else {
                    row_menu_anchor.set(Some(*more_area.peek()));
                    row_menu.set(Some(id.clone()));
                }
            }
        };

        let badge_bg = ext
            .badge_bg
            .map(|[r, g, b, a]| Color::from_argb(a, r, g, b))
            .unwrap_or_else(|| theme.color(ThemeRole::Accent));
        let badge_fg = ext
            .badge_fg
            .map(|[r, g, b, a]| Color::from_argb(a, r, g, b))
            .unwrap_or_else(|| theme.color(ThemeRole::TextOnAccent));
        let badge = ext.badge_text.clone().filter(|_| usable).map(|text| {
            rect()
                .position(Position::new_absolute().right(-4.0).top(-4.0))
                .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
                .min_width(Size::px(14.0))
                .height(Size::px(14.0))
                .corner_radius(CornerRadius::new_all(7.0))
                .background(badge_bg)
                .center()
                .child(
                    label()
                        .text(text)
                        .font_size(9.0)
                        .font_weight(FontWeight::BOLD)
                        .color(badge_fg),
                )
        });
        let icon = rect()
            .width(Size::px(28.0))
            .height(Size::px(28.0))
            .corner_radius(CornerRadius::new_all(7.0))
            .background(theme.color(ThemeRole::SurfaceRaised))
            .center()
            .opacity(if usable { 1.0 } else { 0.5 })
            .child(crate::ui::extensions::ExtensionIconView::new(
                format!("menu-ext-{}", ext.id.as_str()),
                ext.icon.clone(),
                ext.icon_data.clone(),
                17.0,
                ThemeRole::Text,
            ))
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

        let menu_open = self.menu_open;
        let more_button = rect()
            .a11y_id(more_a11y)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("{} options", ext.name))
            .width(Size::px(28.0))
            .height(Size::px(28.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .center()
            .background(if self.menu_open || *more_hovered.read() {
                hover
            } else {
                Color::TRANSPARENT
            })
            .border(focus_ring_border(
                more_focus() == Focus::Keyboard,
                &theme,
                Border::new(),
            ))
            .on_sized(move |e: Event<SizedEventData>| {
                let anchor = AnchorRect::from_area(e.area);
                more_area.set_if_modified(anchor);
                if menu_open {
                    row_menu_anchor.set_if_modified(Some(anchor));
                }
            })
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_pointer_enter(move |_| more_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| more_hovered.set_if_modified(false))
            .cursor(CursorIcon::Pointer)
            .on_press(toggle_row_menu)
            .child(
                label()
                    .text("···")
                    .font_size(14.0)
                    .color(theme.color(ThemeRole::TextMuted)),
            );

        rect().width(Size::fill()).padding(Gaps::new(0.0, 6.0, 0.0, 6.0)).child(
            rect()
                .a11y_id(row_a11y)
                .a11y_focusable(usable)
                .a11y_role(AccessibilityRole::MenuItem)
                .a11y_alt(ext.name.clone())
                .width(Size::fill())
                .height(Size::px(ROW_HEIGHT))
                .padding(Gaps::new(0.0, 6.0, 0.0, 10.0))
                .corner_radius(CornerRadius::new_all(8.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .cross_align(Alignment::Center)
                .spacing(10.0)
                .background(if self.highlighted || (*row_hovered.read() && usable) {
                    hover
                } else {
                    Color::TRANSPARENT
                })
                .border(focus_ring_border(row_focus() == Focus::Keyboard, &theme, Border::new()))
                .on_pointer_enter(move |_| row_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| row_hovered.set_if_modified(false))
                .cursor(if usable {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                })
                .on_press(run_action)
                .child(icon)
                .child(texts)
                .child(more_button),
        )
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
        let options = self.ext.options_url.as_ref().map(|_| {
            let core = core.clone();
            let id = id.clone();
            MenuItemRow {
                icon: "sliders",
                title: "Extension Options",
                role: ThemeRole::Text,
                height: 30.0,
                on_press: EventHandler::new(move |_| {
                    core.invoke_or_log(Command::ExtensionOpenOptions {
                        extension_id: id.clone(),
                    });
                    row_menu.set(None);
                    close_menu(menu_state);
                }),
            }
        });
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

        rect()
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
            .maybe_child(options)
            .child(manage)
            .child(remove)
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
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_press = self.on_press.clone();
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::MenuItem)
            .a11y_alt(self.title)
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
            .border(focus_ring_border(focus() == Focus::Keyboard, &theme, Border::new()))
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .cursor(CursorIcon::Pointer)
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
