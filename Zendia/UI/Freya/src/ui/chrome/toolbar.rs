/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Toolbar surfaces: contributed toolbar buttons, the address bar slot, window controls, and the top toolbar.

use crate::ui::chrome::address_bar::try_use_address_bar_state;
use crate::ui::chrome::layout::{
    ChromePalette, TOP_TOOLBAR_HEIGHT, ToolbarSlots, URL_BAR_HEIGHT_SINGLE, URL_BAR_HEIGHT_TOP,
    show_window_controls_in_top_bar,
};
use crate::ui::chrome::menus::{AnchorRect, try_use_menu_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::page::download_arc::try_use_download_arc;
use crate::ui::theme::{ResolvedScheme, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::address_bar::display_url_for_sidebar;
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{BadgeTone, ToolbarItem, ToolbarItemKind};
use zendia_browser_core::model::SidebarSide;

const ICON_MINIMIZE_SVG: &[u8] =
    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="M2.5 6.5h7" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg>"#;
const ICON_MAXIMIZE_SVG: &[u8] =
    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><rect x="2.5" y="2.5" width="7" height="7" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.3"/></svg>"#;
const ICON_CLOSE_SVG: &[u8] =
    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg>"#;

/// Renders one contributed [`ToolbarItem`] from the core's toolbar model.
#[derive(Clone, PartialEq)]
pub struct ToolbarItemView {
    pub item: ToolbarItem,
    pub location: State<String>,
    pub location_a11y_id: AccessibilityId,
    pub compact_urlbar: bool,
    pub dense: bool,
    pub tight: bool,
    key: DiffKey,
}

impl KeyExt for ToolbarItemView {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ToolbarItemView {
    pub fn new(item: ToolbarItem, location: State<String>, location_a11y_id: AccessibilityId) -> Self {
        Self {
            item,
            location,
            location_a11y_id,
            compact_urlbar: false,
            dense: false,
            tight: false,
            key: DiffKey::None,
        }
    }

    pub fn compact_urlbar(mut self, compact: bool) -> Self {
        self.compact_urlbar = compact;
        self
    }

    pub fn dense(mut self, dense: bool) -> Self {
        self.dense = dense;
        self
    }

    pub fn tight(mut self, tight: bool) -> Self {
        self.tight = tight;
        self
    }
}

/// The library button, where downloads land.
const LIBRARY_ITEM_ID: &str = "builtin.library";

impl Component for ToolbarItemView {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let menu_state = try_use_menu_state();
        let download_arc = try_use_download_arc();
        let mut hovered = use_state(|| false);

        if self.item.kind == ToolbarItemKind::UrlBar {
            return LocationBar {
                location: self.location,
                location_a11y_id: self.location_a11y_id,
                compact_sidebar: self.compact_urlbar,
            }
            .into_element();
        }

        let (active_tab_id, can_back, can_forward, is_loading, sidebar_side) = {
            let model = core.model().read();
            let active = model.active_tab();
            (
                active.map(|tab| tab.id),
                active.is_some_and(|tab| tab.can_navigate_back),
                active.is_some_and(|tab| tab.can_navigate_forward),
                active.is_some_and(|tab| tab.is_loading),
                model.layout.sidebar_side,
            )
        };

        let effective_enabled = if !self.item.enabled {
            false
        } else {
            match self.item.command.as_str() {
                "navigation.back" => can_back,
                "navigation.forward" => can_forward,
                "navigation.reload" => active_tab_id.is_some(),
                _ => true,
            }
        };

        let effective_icon = match self.item.command.as_str() {
            "navigation.reload" if is_loading => "close".to_string(),
            "sidebar.toggle_expanded" if sidebar_side == SidebarSide::Right => "sidebar-right".to_string(),
            _ => self.item.icon.clone(),
        };

        let is_app_menu = self.item.kind == ToolbarItemKind::AppMenu;
        let is_pressed = is_app_menu && menu_state.is_some_and(|state| *state.app_menu_open.read());
        let is_zendia_capsule = self.item.id.as_str().starts_with("zendia.");

        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let item = self.item.clone();
        let press_core = core.clone();
        let on_press = move |_: Event<PressEventData>| {
            if !effective_enabled {
                return;
            }
            if item.command == "navigation.reload" && is_loading {
                press_core.invoke_or_log(Command::NavigationStop { tab_id: active_tab_id });
                return;
            }
            match Command::from_name_and_args(&item.command, &item.command_args) {
                Ok(command) => {
                    press_core.invoke_or_log(command);
                }
                Err(error) => {
                    eprintln!("zendia: invalid toolbar command `{}`: {error}", item.command);
                }
            }
        };

        let focus_border = if focus() == Focus::Keyboard {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::FocusRing))
        } else {
            Border::new().width(0.0).fill(Color::TRANSPARENT)
        };

        let a11y_label = self.item.tooltip.clone().unwrap_or_else(|| self.item.title.clone());

        if self.item.kind == ToolbarItemKind::PillButton {
            let bg = if is_pressed || hovered() {
                palette.surface_pill_hover
            } else {
                palette.surface_pill
            };
            let pill_h = if self.tight { 24.0 } else { 26.0 };
            let pill_pad = if self.tight {
                Gaps::new(0.0, 7.0, 0.0, 6.0)
            } else {
                Gaps::new(0.0, 10.0, 0.0, 8.0)
            };
            return rect()
                .a11y_id(a11y_id)
                .a11y_focusable(effective_enabled)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(a11y_label)
                .height(Size::px(pill_h))
                .padding(pill_pad)
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .main_align(Alignment::center())
                .spacing(if self.tight { 4.0 } else { 5.0 })
                .corner_radius(CornerRadius::new_all(999.0))
                .background(bg)
                .border(focus_border)
                .opacity(if effective_enabled { 1.0 } else { 0.4 })
                .cursor(if effective_enabled {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                })
                .on_pointer_enter(move |_| hovered.set_if_modified(true))
                .on_pointer_leave(move |_| hovered.set_if_modified(false))
                .on_press(on_press)
                .child(
                    Icon::new(effective_icon)
                        .size(if self.tight { 12.5 } else { 13.5 })
                        .role(ThemeRole::Text),
                )
                .child(
                    label()
                        .text(self.item.title.clone())
                        .font_size(if self.tight { 11.0 } else { 12.0 })
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(palette.text)
                        .max_lines(1),
                )
                .into_element();
        }

        let button_width = if self.tight {
            20.0
        } else if self.dense {
            26.0
        } else {
            28.0
        };
        let button_height = if self.tight {
            24.0
        } else if self.dense {
            26.0
        } else {
            28.0
        };
        let icon_size = if is_zendia_capsule {
            13.5
        } else if self.tight {
            12.5
        } else {
            15.0
        };
        let radius = if is_zendia_capsule { 999.0 } else { 6.0 };

        let bg = if is_pressed {
            palette.surface_pill_hover
        } else if is_zendia_capsule {
            if hovered() && effective_enabled {
                palette.surface_pill_hover
            } else {
                palette.surface_pill
            }
        } else if hovered() && effective_enabled {
            palette.surface_pill
        } else {
            Color::TRANSPARENT
        };

        let icon_role = if effective_enabled {
            if is_zendia_capsule || hovered() || is_pressed {
                ThemeRole::Text
            } else {
                ThemeRole::TextMuted
            }
        } else {
            ThemeRole::TextMuted
        };

        let badge = self.item.badge.clone().map(|badge| {
            let (badge_bg, badge_fg) = match badge.tone {
                BadgeTone::Neutral => (theme.color(ThemeRole::SurfaceRaised), theme.color(ThemeRole::Text)),
                BadgeTone::Accent => (theme.color(ThemeRole::Accent), theme.color(ThemeRole::TextOnAccent)),
                BadgeTone::Warning => (theme.color(ThemeRole::Warning), theme.color(ThemeRole::Ink)),
                BadgeTone::Error => (theme.color(ThemeRole::Danger), Color::WHITE),
            };
            rect()
                .position(Position::new_absolute().top(-3.0).right(-3.0))
                .min_width(Size::px(14.0))
                .height(Size::px(14.0))
                .padding(Gaps::new(0.0, 3.0, 0.0, 3.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(badge_bg)
                .center()
                .child(
                    label()
                        .text(badge.text)
                        .font_size(9.0)
                        .font_weight(FontWeight::BOLD)
                        .color(badge_fg),
                )
        });

        let button = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(effective_enabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(a11y_label)
            .width(Size::px(if is_zendia_capsule {
                if self.tight { 24.0 } else { 26.0 }
            } else {
                button_width
            }))
            .height(Size::px(if is_zendia_capsule {
                if self.tight { 24.0 } else { 26.0 }
            } else {
                button_height
            }))
            .corner_radius(CornerRadius::new_all(radius))
            .background(bg)
            .border(focus_border)
            .opacity(if effective_enabled { 1.0 } else { 0.36 })
            .center()
            .cursor(if effective_enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(on_press)
            .child(Icon::new(effective_icon).size(icon_size).role(icon_role))
            .maybe_child(badge);

        if is_app_menu {
            button
                .on_sized(move |e: Event<SizedEventData>| {
                    if let Some(state) = menu_state {
                        let mut anchor = state.app_menu_anchor;
                        anchor.set_if_modified(AnchorRect::from_area(e.area));
                    }
                })
                .into_element()
        } else if self.item.id.as_str() == LIBRARY_ITEM_ID
            && let Some(arc) = download_arc
        {
            // Downloads fly into the library button, so it records where it sits.
            button
                .on_sized(move |e: Event<SizedEventData>| {
                    let mut anchor = arc.anchor;
                    let center = (e.area.center().x, e.area.center().y);
                    anchor.set_if_modified(Some(center));
                })
                .into_element()
        } else {
            button.into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Address bar slot styled for either the compact single-toolbar sidebar or the centered top bar.
#[derive(Clone, PartialEq)]
pub struct LocationBar {
    pub location: State<String>,
    pub location_a11y_id: AccessibilityId,
    pub compact_sidebar: bool,
}

impl Component for LocationBar {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let addr_state = try_use_address_bar_state();
        let mut hovered = use_state(|| false);
        let mut copy_hovered = use_state(|| false);
        let mut site_btn_hovered = use_state(|| false);
        let focus = use_focus(self.location_a11y_id);
        let is_focused = focus() != Focus::Not;

        let (active_url, site_info) = {
            let model = core.model().read();
            let url = model
                .active_tab()
                .map(|tab| tab.url.clone())
                .unwrap_or_else(|| self.location.read().clone());
            let info = model.active_site_info();
            (url, info)
        };

        let floating_open = addr_state.is_some_and(|s| *s.open.read());
        let site_panel_open = addr_state.is_some_and(|s| *s.site_panel_open.read());
        let is_web_page = site_info.security.is_web_page();

        let shown_text = if floating_open {
            String::new()
        } else if self.compact_sidebar {
            display_url_for_sidebar(&active_url)
        } else {
            active_url.clone()
        };
        let is_placeholder = shown_text.is_empty();
        let display_label = if floating_open {
            String::new()
        } else if is_placeholder {
            if self.compact_sidebar {
                "Search or enter URL".to_string()
            } else {
                "Search or enter an address".to_string()
            }
        } else {
            shown_text
        };

        let height = if self.compact_sidebar {
            URL_BAR_HEIGHT_SINGLE
        } else {
            URL_BAR_HEIGHT_TOP
        };
        let radius = if self.compact_sidebar { 10.0 } else { 8.0 };

        let bg = if is_focused {
            palette.urlbar_focus_bg
        } else if hovered() {
            palette.surface_pill_hover
        } else {
            palette.urlbar_bg
        };
        let border_color = if focus() == Focus::Keyboard {
            theme.color(ThemeRole::FocusRing)
        } else {
            palette.urlbar_border
        };

        let open_bar_core = core.clone();
        let open_bar_url = active_url.clone();
        let open_bar = move || {
            let initial = if open_bar_url.is_empty() || open_bar_url == "about:blank" || open_bar_url == "about:newtab"
            {
                None
            } else {
                Some(open_bar_url.clone())
            };
            open_bar_core.invoke_or_log(Command::AddressBarOpen {
                new_tab: false,
                initial_query: initial,
            });
        };
        let open_bar_click = open_bar.clone();
        let open_bar_key = open_bar;

        let copy_core = core.clone();
        let show_copy = !floating_open && is_web_page && (!self.compact_sidebar || hovered() || site_panel_open);
        let copy_button = show_copy.then(|| {
            rect()
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Copy Current URL")
                .width(Size::px(22.0))
                .height(Size::px(22.0))
                .corner_radius(CornerRadius::new_all(5.0))
                .background(if copy_hovered() {
                    palette.surface_pill_hover
                } else {
                    Color::TRANSPARENT
                })
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| copy_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| copy_hovered.set_if_modified(false))
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    copy_core.invoke_or_log(Command::AddressBarCopyCurrentUrl);
                })
                .child(Icon::new("link").size(12.5).role(ThemeRole::TextMuted))
        });

        let site_core = core.clone();
        let site_btn_bg = if site_panel_open || site_btn_hovered() {
            palette.surface_pill_hover
        } else {
            Color::TRANSPARENT
        };
        let site_button = (!floating_open).then(|| {
            rect()
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Site Data and Permissions")
                .width(Size::px(22.0))
                .height(Size::px(22.0))
                .corner_radius(CornerRadius::new_all(5.0))
                .background(site_btn_bg)
                .center()
                .cursor(CursorIcon::Pointer)
                .on_sized(move |e: Event<SizedEventData>| {
                    if let Some(state) = addr_state {
                        let mut anchor = state.site_panel_anchor;
                        anchor.set_if_modified(AnchorRect::from_area(e.area));
                    }
                })
                .on_pointer_enter(move |_| site_btn_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| site_btn_hovered.set_if_modified(false))
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    site_core.invoke_or_log(Command::SitePanelToggle);
                })
                .child(Icon::new("sliders").size(12.5).role(if site_panel_open {
                    ThemeRole::Text
                } else {
                    ThemeRole::TextMuted
                }))
        });

        let id_core = core.clone();
        let identity_icon = (!floating_open).then(|| {
            let role = if site_info.security.is_warning() {
                ThemeRole::Danger
            } else {
                ThemeRole::TextMuted
            };
            rect()
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(format!("Site Identity: {}", site_info.security.label()))
                .width(Size::px(20.0))
                .height(Size::px(20.0))
                .corner_radius(CornerRadius::new_all(5.0))
                .center()
                .cursor(CursorIcon::Pointer)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    id_core.invoke_or_log(Command::SitePanelToggle);
                })
                .child(Icon::new(site_info.security.toolbar_icon()).size(13.0).role(role))
        });

        let container = rect()
            .a11y_id(self.location_a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::TextInput)
            .a11y_alt("Address bar")
            .width(Size::fill())
            .height(Size::px(height))
            .padding(Gaps::new(0.0, 6.0, 0.0, 6.0))
            .spacing(4.0)
            .font_size(if self.compact_sidebar { 12.0 } else { 12.5 })
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(radius))
            .background(bg)
            .border(
                Border::new()
                    .width(if focus() == Focus::Keyboard { 1.5 } else { 1.0 })
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .cursor(CursorIcon::Text)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| {
                open_bar_click();
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                let is_space = matches!(&e.key, Key::Character(ch) if ch == " ");
                if matches!(e.key, Key::Named(NamedKey::Enter)) || is_space {
                    e.stop_propagation();
                    open_bar_key();
                }
            })
            .maybe_child(identity_icon)
            .child(
                label()
                    .text(display_label)
                    .width(Size::flex(1.0))
                    .font_size(if self.compact_sidebar { 12.0 } else { 12.5 })
                    .font_weight(FontWeight::MEDIUM)
                    .color(if is_placeholder {
                        palette.text_muted
                    } else {
                        palette.text
                    })
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(copy_button)
            .maybe_child(site_button);

        if self.compact_sidebar {
            container
        } else {
            container.max_width(Size::px(640.0))
        }
    }
}

/// Minimize, maximize/restore, and close window buttons.
#[derive(Clone, Copy, PartialEq)]
pub struct WindowControls {
    pub interactive: bool,
    pub compact: bool,
}

impl Component for WindowControls {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let interactive = self.interactive;
        let btn_w = if self.compact { 19.0 } else { 26.0 };
        let btn_h = if self.compact { 24.0 } else { 26.0 };
        let icon_px = if self.compact { 9.5 } else { 11.0 };

        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(if self.compact { 1.0 } else { 2.0 })
            .child(WindowControlButton {
                label: "Minimize window",
                svg: ICON_MINIMIZE_SVG,
                svg_key: "zendia-win-min",
                width: btn_w,
                height: btn_h,
                icon_size: icon_px,
                hover_bg: palette.surface_pill,
                icon_color: palette.text_muted,
                hover_icon_color: palette.text,
                on_press: EventHandler::new(move |_| {
                    if interactive {
                        Platform::get().with_window(Platform::window_id(), |window| {
                            window.set_minimized(true);
                        });
                    }
                }),
            })
            .child(WindowControlButton {
                label: "Maximize or restore window",
                svg: ICON_MAXIMIZE_SVG,
                svg_key: "zendia-win-max",
                width: btn_w,
                height: btn_h,
                icon_size: icon_px,
                hover_bg: palette.surface_pill,
                icon_color: palette.text_muted,
                hover_icon_color: palette.text,
                on_press: EventHandler::new(move |_| {
                    if interactive {
                        Platform::get().with_window(Platform::window_id(), |window| {
                            window.set_maximized(!window.is_maximized());
                        });
                    }
                }),
            })
            .child(WindowControlButton {
                label: "Close window",
                svg: ICON_CLOSE_SVG,
                svg_key: "zendia-win-close",
                width: btn_w,
                height: btn_h,
                icon_size: icon_px,
                hover_bg: palette.window_close_hover,
                icon_color: palette.text_muted,
                hover_icon_color: if theme.scheme == ResolvedScheme::Light {
                    Color::WHITE
                } else {
                    palette.text
                },
                on_press: EventHandler::new(move |_| {
                    if interactive {
                        Platform::get().close_window(Platform::window_id());
                    }
                }),
            })
    }
}

#[derive(Clone, PartialEq)]
struct WindowControlButton {
    label: &'static str,
    svg: &'static [u8],
    svg_key: &'static str,
    width: f32,
    height: f32,
    icon_size: f32,
    hover_bg: Color,
    icon_color: Color,
    hover_icon_color: Color,
    on_press: EventHandler<Event<PressEventData>>,
}

impl Component for WindowControlButton {
    fn render(&self) -> impl IntoElement {
        let mut hovered = use_state(|| false);
        let on_press = self.on_press.clone();

        rect()
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label)
            .width(Size::px(self.width))
            .height(Size::px(self.height))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(if hovered() { self.hover_bg } else { Color::TRANSPARENT })
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e| on_press.call(e))
            .child(
                SvgViewer::new((self.svg_key, Bytes::from_static(self.svg)))
                    .width(Size::px(self.icon_size))
                    .height(Size::px(self.icon_size))
                    .color(if hovered() {
                        self.hover_icon_color
                    } else {
                        self.icon_color
                    })
                    .show_loader(false),
            )
    }
}

/// Full-width top toolbar rendered in multiple-toolbar and collapsed-sidebar layouts.
#[derive(Clone, PartialEq)]
pub struct TopToolbar {
    pub slots: ToolbarSlots,
    pub location: State<String>,
    pub location_a11y_id: AccessibilityId,
    pub interactive_window_controls: bool,
}

impl Component for TopToolbar {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let layout = core.model().read().layout.clone();
        let show_controls = show_window_controls_in_top_bar(&layout);
        let location = self.location;
        let location_a11y_id = self.location_a11y_id;
        let sidebar_width = layout.effective_sidebar_width();

        let sidebar_top_items: Vec<_> = self
            .slots
            .top_bar_start
            .iter()
            .filter(|item| item.placement == zendia_browser_core::contributions::ToolbarPlacement::SidebarTop)
            .cloned()
            .collect();
        let nav_start_items: Vec<_> = self
            .slots
            .top_bar_start
            .iter()
            .filter(|item| item.placement != zendia_browser_core::contributions::ToolbarPlacement::SidebarTop)
            .cloned()
            .collect();

        let render_item = move |item: zendia_browser_core::contributions::ToolbarItem| {
            ToolbarItemView::new(item.clone(), location, location_a11y_id)
                .key(item.id.as_str().to_string())
                .into_element()
        };

        let left_cluster = match layout.sidebar_side {
            SidebarSide::Left => rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .child(
                    rect()
                        .width(Size::px(sidebar_width))
                        .height(Size::fill())
                        .padding(Gaps::new(
                            0.0,
                            0.0,
                            0.0,
                            if layout.sidebar_expanded { 8.0 } else { 0.0 },
                        ))
                        .direction(Direction::Horizontal)
                        .main_align(if layout.sidebar_expanded {
                            Alignment::Start
                        } else {
                            Alignment::center()
                        })
                        .cross_align(Alignment::center())
                        .spacing(4.0)
                        .children(sidebar_top_items.iter().cloned().map(render_item)),
                )
                .child(
                    rect()
                        .padding(Gaps::new(0.0, 0.0, 0.0, 6.0))
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(2.0)
                        .children(nav_start_items.iter().cloned().map(render_item)),
                ),
            SidebarSide::Right => rect()
                .padding(Gaps::new(0.0, 0.0, 0.0, 8.0))
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(2.0)
                .children(nav_start_items.iter().cloned().map(render_item)),
        };

        let center_items = self.slots.top_bar_center.iter().cloned().map(render_item);
        let end_items = self.slots.top_bar_end.iter().cloned().map(render_item);
        let right_sidebar_toggles = (layout.sidebar_side == SidebarSide::Right)
            .then(|| sidebar_top_items.iter().cloned().map(render_item).collect::<Vec<_>>());

        let bar = rect()
            .width(Size::fill())
            .height(Size::px(TOP_TOOLBAR_HEIGHT))
            .padding(Gaps::new(0.0, 8.0, 0.0, 0.0))
            .spacing(8.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .child(left_cluster)
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .height(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::center())
                    .cross_align(Alignment::center())
                    .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
                    .children(center_items),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(4.0)
                    .children(end_items)
                    .maybe_child(right_sidebar_toggles.map(|items| {
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(2.0)
                            .children(items)
                    }))
                    .maybe_child(show_controls.then(|| {
                        rect().padding(Gaps::new(0.0, 0.0, 0.0, 4.0)).child(WindowControls {
                            interactive: self.interactive_window_controls,
                            compact: false,
                        })
                    })),
            );

        if self.interactive_window_controls {
            bar.window_drag()
        } else {
            bar
        }
    }
}
