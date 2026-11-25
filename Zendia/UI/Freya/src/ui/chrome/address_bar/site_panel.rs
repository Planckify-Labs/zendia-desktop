/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Anchored Site Data Panel popover matching Zen's unified site control panel:
//! top action row (bookmark, edit style, screenshot, share), Extensions grid, Settings (tracking protection,
//! site permissions, cookies/site data), and security status footer with more menu.

use crate::ui::chrome::address_bar::state::use_address_bar_state;
use crate::ui::chrome::menus::AnchorRect;
use crate::ui::chrome::menus::state::clamp_popup_origin;
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::kit::icons::prepared_icon_bytes;
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::site_info::{PermissionState, SiteExtension, SiteInfo};

const SITE_PANEL_WIDTH: f32 = 236.0;

/// Computes the global `(x, y)` origin for the Site Data Panel anchored to `anchor`, clamped inside `window_size`.
pub fn compute_site_panel_origin(anchor: AnchorRect, panel_size: (f32, f32), window_size: (f32, f32)) -> (f32, f32) {
    let desired = (anchor.x + anchor.width - 14.0, anchor.y + anchor.height + 6.0);
    clamp_popup_origin(desired, panel_size, window_size, 8.0)
}

/// Window-root overlay rendering [`SiteDataPanelCard`] anchored to the LocationBar's identity/site-data button.
#[derive(Clone, PartialEq)]
pub struct SiteDataPanelOverlay;

impl Component for SiteDataPanelOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let state = use_address_bar_state();
        let is_open = *state.site_panel_open.read();
        let anchor = *state.site_panel_anchor.read();

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

        let opacity = fade.get().value().clamp(0.0, 1.0);
        if !is_open && opacity <= 0.01 {
            return rect().into_element();
        }

        let (site_info, extensions) = {
            let model = core.model().read();
            (model.active_site_info(), model.site_extensions.clone())
        };

        let root_size = *Platform::get().current_window().root_size.read();
        let window_size = (root_size.width.max(320.0), root_size.height.max(240.0));
        let est_height = 240.0 + site_info.visible_permissions().len() as f32 * 44.0;
        let (x, y) = compute_site_panel_origin(anchor, (SITE_PANEL_WIDTH, est_height), window_size);

        rect()
            .layer(Layer::Overlay)
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .opacity(opacity)
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if is_open && e.key == Key::Named(NamedKey::Escape) {
                    e.stop_propagation();
                    state.close_site_panel();
                }
            })
            .child(
                rect()
                    .position(Position::new_global().left(0.0).top(0.0))
                    .width(Size::window_percent(100.0))
                    .height(Size::window_percent(100.0))
                    .on_mouse_down(move |_| {
                        state.close_site_panel();
                    }),
            )
            .child(
                rect()
                    .position(Position::new_global().left(x).top(y))
                    .child(SiteDataPanelCard::new(site_info, extensions).on_close(move |_| state.close_site_panel())),
            )
            .into_element()
    }
}

/// Reusable Site Data Panel card rendered both inside [`SiteDataPanelOverlay`] and inline in the gallery.
#[derive(Clone, PartialEq)]
pub struct SiteDataPanelCard {
    site_info: SiteInfo,
    extensions: Vec<SiteExtension>,
    on_close: Option<EventHandler<()>>,
    key: DiffKey,
}

impl KeyExt for SiteDataPanelCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl SiteDataPanelCard {
    pub fn new(site_info: SiteInfo, extensions: Vec<SiteExtension>) -> Self {
        Self {
            site_info,
            extensions,
            on_close: None,
            key: DiffKey::None,
        }
    }

    pub fn on_close(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_close = Some(handler.into());
        self
    }
}

impl Component for SiteDataPanelCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut more_open = use_state(|| false);

        let panel_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(33, 33, 36, 0.99).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(241, 241, 244, 0.99).to_color(),
        };
        let border_color = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.11).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.12).to_color(),
        };
        let divider_color = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.08).to_color(),
        };
        let primary_text = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(235, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(232, 22, 24, 34),
        };
        let muted_text = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(145, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(145, 48, 52, 66),
        };

        let is_web = self.site_info.security.is_web_page();
        let origin = self.site_info.origin.clone();

        // Top 4-button action row: Bookmark, Edit Style, Screenshot, Share.
        let bookmark_core = core.clone();
        let style_core = core.clone();
        let shot_core = core.clone();
        let share_core = core.clone();
        let close_shot = self.on_close.clone();
        let close_share = self.on_close.clone();

        let action_row = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .spacing(7.0)
            .child(SiteActionTile {
                icon: if self.site_info.bookmarked {
                    "bookmark"
                } else {
                    "bookmark-hollow"
                },
                label: if self.site_info.bookmarked {
                    "Remove Bookmark"
                } else {
                    "Bookmark Page"
                },
                active: self.site_info.bookmarked,
                enabled: is_web,
                on_press: EventHandler::new(move |_| {
                    bookmark_core.invoke_or_log(Command::SiteBookmarkToggle { tab_id: None });
                }),
                key: DiffKey::from(&"site-act-bookmark"),
            })
            .child(SiteActionTile {
                icon: "paintbrush",
                label: "Customize Site Style",
                active: false,
                enabled: is_web,
                on_press: EventHandler::new(move |_| {
                    style_core.invoke_or_log(Command::SiteEditStyle { tab_id: None });
                }),
                key: DiffKey::from(&"site-act-style"),
            })
            .child(SiteActionTile {
                icon: "screenshot",
                label: "Capture Screenshot",
                active: false,
                enabled: true,
                on_press: EventHandler::new(move |_| {
                    shot_core.invoke_or_log(Command::SiteScreenshot { tab_id: None });
                    if let Some(close) = &close_shot {
                        close.call(());
                    }
                }),
                key: DiffKey::from(&"site-act-screenshot"),
            })
            .child(SiteActionTile {
                icon: "share",
                label: "Share Link",
                active: false,
                enabled: is_web,
                on_press: EventHandler::new(move |_| {
                    share_core.invoke_or_log(Command::SiteShare { tab_id: None });
                    if let Some(close) = &close_share {
                        close.call(());
                    }
                }),
                key: DiffKey::from(&"site-act-share"),
            });

        // Extensions section.
        let add_ext_core = core.clone();
        let close_add_ext = self.on_close.clone();

        let ext_tiles = self.extensions.iter().map(|ext| {
            let ext_core = core.clone();
            let ext_cmd = ext.command.clone();
            let close_ext = self.on_close.clone();
            SiteExtensionTile {
                extension: ext.clone(),
                enabled: ext.enabled && is_web,
                on_press: EventHandler::new(move |_| {
                    if let Ok(parsed) = Command::from_name_and_args(&ext_cmd, &[]) {
                        ext_core.invoke_or_log(parsed);
                    }
                    if let Some(close) = &close_ext {
                        close.call(());
                    }
                }),
                key: DiffKey::from(&ext.id.0),
            }
            .into_element()
        });

        let extensions_section = rect()
            .width(Size::fill())
            .spacing(7.0)
            .child(
                label()
                    .text("Extensions")
                    .font_size(12.5)
                    .font_weight(FontWeight::MEDIUM)
                    .color(primary_text),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(7.0)
                    .children(ext_tiles)
                    .child(SiteAddExtensionTile {
                        on_press: EventHandler::new(move |_| {
                            add_ext_core.invoke_or_log(Command::Custom {
                                name: "extensions.open".to_string(),
                                args: Vec::new(),
                            });
                            if let Some(close) = &close_add_ext {
                                close.call(());
                            }
                        }),
                    }),
            );

        // Settings section (Tracking Protection + active permissions + cookies/site data).
        let tp_core = core.clone();
        let tp_origin = origin.clone();
        let tp_row = is_web.then(|| {
            let enabled = self.site_info.tracking_protection_enabled;
            SiteSettingRow {
                icon: if enabled {
                    "tracking-protection-fill"
                } else {
                    "tracking-protection"
                },
                title: "Tracking Protection".to_string(),
                subtitle: if enabled {
                    "Enabled".to_string()
                } else {
                    "Disabled".to_string()
                },
                active: enabled,
                on_press: EventHandler::new(move |_| {
                    tp_core.invoke_or_log(Command::SiteTrackingProtectionToggle {
                        origin: Some(tp_origin.clone()),
                    });
                }),
                key: DiffKey::from(&"site-setting-tracking"),
            }
        });

        let perm_rows: Vec<Element> = self
            .site_info
            .visible_permissions()
            .into_iter()
            .map(|perm| {
                let perm_core = core.clone();
                let perm_origin = origin.clone();
                let kind = perm.kind;
                let next_state = perm.state.toggled();
                SiteSettingRow {
                    icon: kind.icon(),
                    title: kind.label().to_string(),
                    subtitle: perm.state.label().to_string(),
                    active: perm.state == PermissionState::Allow,
                    on_press: EventHandler::new(move |_| {
                        perm_core.invoke_or_log(Command::SitePermissionSet {
                            origin: Some(perm_origin.clone()),
                            permission: kind,
                            state: next_state,
                        });
                    }),
                    key: DiffKey::from(&kind.as_str()),
                }
                .into_element()
            })
            .collect();

        let clear_core = core.clone();
        let clear_origin = origin.clone();
        let data_row = (!self.site_info.data.is_empty()).then(|| SiteSettingRow {
            icon: "cookies-fill",
            title: "Cookies & Site Data".to_string(),
            subtitle: format!("{} · Clear", self.site_info.data.format_summary()),
            active: false,
            on_press: EventHandler::new(move |_| {
                clear_core.invoke_or_log(Command::SiteClearData {
                    origin: Some(clear_origin.clone()),
                });
            }),
            key: DiffKey::from(&"site-setting-data"),
        });

        let internal_note = (!is_web && perm_rows.is_empty()).then(|| {
            label()
                .text("Built-in browser page — site permissions do not apply.")
                .font_size(11.5)
                .color(muted_text)
        });

        let settings_section = rect()
            .width(Size::fill())
            .spacing(8.0)
            .child(
                label()
                    .text("Settings")
                    .font_size(12.5)
                    .font_weight(FontWeight::MEDIUM)
                    .color(primary_text),
            )
            .maybe_child(tp_row)
            .children(perm_rows)
            .maybe_child(data_row)
            .maybe_child(internal_note);

        // Footer: Security status badge + "..." more button.
        let is_warning = self.site_info.security.is_warning();
        let (sec_bg, sec_fg) = if is_warning {
            match theme.scheme {
                ResolvedScheme::Dark => (
                    Rgba::from_rgb_alpha(214, 60, 74, 0.16).to_color(),
                    Color::from_rgb(238, 108, 118),
                ),
                ResolvedScheme::Light => (
                    Rgba::from_rgb_alpha(206, 48, 62, 0.12).to_color(),
                    Color::from_rgb(184, 34, 48),
                ),
            }
        } else {
            match theme.scheme {
                ResolvedScheme::Dark => (
                    Rgba::from_rgb_alpha(255, 255, 255, 0.06).to_color(),
                    Color::from_argb(225, 255, 255, 255),
                ),
                ResolvedScheme::Light => (
                    Rgba::from_rgb_alpha(0, 0, 0, 0.055).to_color(),
                    Color::from_argb(220, 22, 24, 34),
                ),
            }
        };

        let sec_icon: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(self.site_info.security.icon()) {
            SvgViewer::new((("zendia-icon", canonical), bytes))
                .width(Size::px(14.0))
                .height(Size::px(14.0))
                .color(sec_fg)
                .show_loader(false)
                .into_element()
        } else {
            Icon::new(self.site_info.security.icon())
                .size(14.0)
                .role(if is_warning { ThemeRole::Danger } else { ThemeRole::Text })
                .into_element()
        };

        let more_clear_core = core.clone();
        let more_clear_origin = origin;
        let more_settings_core = core.clone();
        let close_more = self.on_close.clone();

        let more_drawer = more_open().then(|| {
            let clear_bg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.06).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.05).to_color(),
            };
            rect()
                .width(Size::fill())
                .padding(Gaps::new(6.0, 6.0, 6.0, 6.0))
                .spacing(4.0)
                .corner_radius(CornerRadius::new_all(8.0))
                .background(clear_bg)
                .child(
                    rect()
                        .a11y_focusable(true)
                        .a11y_role(AccessibilityRole::MenuItem)
                        .a11y_alt("Clear Cookies and Site Data")
                        .width(Size::fill())
                        .padding(Gaps::new(6.0, 8.0, 6.0, 8.0))
                        .spacing(8.0)
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .corner_radius(CornerRadius::new_all(6.0))
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            more_clear_core.invoke_or_log(Command::SiteClearData {
                                origin: Some(more_clear_origin.clone()),
                            });
                            more_open.set(false);
                        })
                        .child(Icon::new("trash").size(13.0).role(ThemeRole::TextMuted))
                        .child(
                            label()
                                .text("Clear Cookies & Site Data")
                                .font_size(12.0)
                                .color(primary_text),
                        ),
                )
                .child(
                    rect()
                        .a11y_focusable(true)
                        .a11y_role(AccessibilityRole::MenuItem)
                        .a11y_alt("Site Settings")
                        .width(Size::fill())
                        .padding(Gaps::new(6.0, 8.0, 6.0, 8.0))
                        .spacing(8.0)
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .corner_radius(CornerRadius::new_all(6.0))
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            more_settings_core.invoke_or_log(Command::Custom {
                                name: "settings.open".to_string(),
                                args: Vec::new(),
                            });
                            more_open.set(false);
                            if let Some(close) = &close_more {
                                close.call(());
                            }
                        })
                        .child(Icon::new("settings").size(13.0).role(ThemeRole::TextMuted))
                        .child(label().text("Site Settings").font_size(12.0).color(primary_text)),
                )
        });

        let more_btn_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.07).to_color(),
        };

        let footer = rect()
            .width(Size::fill())
            .spacing(8.0)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(1.0))
                    .background(divider_color),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::SpaceBetween)
                    .cross_align(Alignment::center())
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Status)
                            .a11y_alt(format!("Connection security: {}", self.site_info.security.label()))
                            .height(Size::px(28.0))
                            .padding(Gaps::new(0.0, 10.0, 0.0, 8.0))
                            .spacing(6.0)
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .corner_radius(CornerRadius::new_all(6.0))
                            .background(sec_bg)
                            .child(sec_icon)
                            .child(
                                label()
                                    .text(self.site_info.security.label())
                                    .font_size(12.5)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(sec_fg),
                            ),
                    )
                    .child(
                        rect()
                            .a11y_focusable(true)
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("More site actions")
                            .width(Size::px(28.0))
                            .height(Size::px(28.0))
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(more_btn_bg)
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                let next = !*more_open.peek();
                                more_open.set(next);
                            })
                            .child(Icon::new("menu").size(14.0).role(ThemeRole::Text)),
                    ),
            )
            .maybe_child(more_drawer);

        rect()
            .a11y_role(AccessibilityRole::Dialog)
            .a11y_alt("Site Data Panel")
            .width(Size::px(SITE_PANEL_WIDTH))
            .padding(Gaps::new(10.0, 10.0, 10.0, 10.0))
            .spacing(12.0)
            .corner_radius(CornerRadius::new_all(10.0))
            .background(panel_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .shadow(theme.shadows.float_drop.to_shadow())
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .child(action_row)
            .child(extensions_section)
            .child(settings_section)
            .child(footer)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SiteActionTile {
    icon: &'static str,
    label: &'static str,
    active: bool,
    enabled: bool,
    on_press: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for SiteActionTile {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SiteActionTile {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let bg = if self.active {
            theme.rgba(ThemeRole::Accent).with_alpha(0.22).to_color()
        } else if hovered() && self.enabled {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.13).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.11).to_color(),
            }
        } else {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.07).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.06).to_color(),
            }
        };

        let icon_color = if self.active {
            theme.color(ThemeRole::Accent)
        } else if theme.scheme.is_dark() {
            Color::from_argb(225, 255, 255, 255)
        } else {
            Color::from_argb(220, 22, 24, 34)
        };

        let glyph: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(self.icon) {
            SvgViewer::new((("zendia-icon", canonical), bytes))
                .width(Size::px(16.0))
                .height(Size::px(16.0))
                .color(icon_color)
                .show_loader(false)
                .into_element()
        } else {
            Icon::new(self.icon).size(16.0).role(ThemeRole::Text).into_element()
        };

        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::FocusRing))
        } else {
            Border::new()
        };

        let enabled = self.enabled;
        let on_press = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label)
            .width(Size::flex(1.0))
            .height(Size::px(34.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .border(border)
            .opacity(if enabled { 1.0 } else { 0.42 })
            .center()
            .cursor(if enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                if enabled {
                    on_press.call(());
                }
            })
            .child(glyph)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SiteExtensionTile {
    extension: SiteExtension,
    enabled: bool,
    on_press: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for SiteExtensionTile {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SiteExtensionTile {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let bg = if hovered() && self.enabled {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.13).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.11).to_color(),
            }
        } else {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.07).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.06).to_color(),
            }
        };

        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::FocusRing))
        } else {
            Border::new()
        };

        let enabled = self.enabled;
        let on_press = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.extension.name.clone())
            .width(Size::px(44.0))
            .height(Size::px(32.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .border(border)
            .opacity(if enabled { 1.0 } else { 0.48 })
            .center()
            .cursor(if enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                if enabled {
                    on_press.call(());
                }
            })
            .child(
                Icon::new(self.extension.icon.clone())
                    .size(16.0)
                    .role(ThemeRole::Accent),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SiteAddExtensionTile {
    on_press: EventHandler<()>,
}

impl Component for SiteAddExtensionTile {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let bg = if hovered() {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.13).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.11).to_color(),
            }
        } else {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.07).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.06).to_color(),
            }
        };

        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::FocusRing))
        } else {
            Border::new()
        };

        let on_press = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Add Extension")
            .width(Size::px(44.0))
            .height(Size::px(32.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .border(border)
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_press.call(());
            })
            .child(Icon::new("plus").size(15.0).role(ThemeRole::Text))
    }
}

#[derive(Clone, PartialEq)]
struct SiteSettingRow {
    icon: &'static str,
    title: String,
    subtitle: String,
    active: bool,
    on_press: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for SiteSettingRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SiteSettingRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        // Matches Zen's high-contrast circular setting badge: bright circle + dark glyph on dark theme,
        // deep slate/accent circle + white glyph on light theme when active/enabled.
        let (circle_bg, glyph_color) = if self.active {
            match theme.scheme {
                ResolvedScheme::Dark => (Color::from_rgb(232, 234, 242), Color::from_rgb(24, 26, 36)),
                ResolvedScheme::Light => (Color::from_rgb(44, 56, 102), Color::WHITE),
            }
        } else {
            match theme.scheme {
                ResolvedScheme::Dark => (
                    Rgba::from_rgb_alpha(255, 255, 255, 0.13).to_color(),
                    Color::from_argb(200, 255, 255, 255),
                ),
                ResolvedScheme::Light => (
                    Rgba::from_rgb_alpha(0, 0, 0, 0.10).to_color(),
                    Color::from_argb(190, 26, 28, 40),
                ),
            }
        };

        let row_bg = if hovered() {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.06).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.05).to_color(),
            }
        } else {
            Color::TRANSPARENT
        };

        let title_color = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(235, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(230, 22, 24, 34),
        };
        let sub_color = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(150, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(145, 48, 52, 66),
        };

        let glyph: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(self.icon) {
            SvgViewer::new((("zendia-icon", canonical), bytes))
                .width(Size::px(15.0))
                .height(Size::px(15.0))
                .color(glyph_color)
                .show_loader(false)
                .into_element()
        } else {
            Icon::new(self.icon).size(15.0).role(ThemeRole::Text).into_element()
        };

        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::FocusRing))
        } else {
            Border::new()
        };

        let on_press = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("{}: {}", self.title, self.subtitle))
            .width(Size::fill())
            .padding(Gaps::new(3.0, 4.0, 3.0, 2.0))
            .spacing(9.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(8.0))
            .background(row_bg)
            .border(border)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_press.call(());
            })
            .child(
                rect()
                    .width(Size::px(32.0))
                    .height(Size::px(32.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(circle_bg)
                    .center()
                    .child(glyph),
            )
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .spacing(1.0)
                    .child(
                        label()
                            .text(self.title.clone())
                            .font_size(12.0)
                            .font_weight(FontWeight::MEDIUM)
                            .color(title_color)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    )
                    .child(
                        label()
                            .text(self.subtitle.clone())
                            .font_size(10.5)
                            .color(sub_color)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_panel_origin_clamps_inside_window() {
        let anchor = AnchorRect {
            x: 152.0,
            y: 44.0,
            width: 24.0,
            height: 24.0,
        };
        let (x, y) = compute_site_panel_origin(anchor, (236.0, 260.0), (1600.0, 1000.0));
        assert_eq!((x, y), (162.0, 74.0));

        // Near right edge flips/clamps cleanly.
        let right_anchor = AnchorRect {
            x: 1550.0,
            y: 44.0,
            width: 24.0,
            height: 24.0,
        };
        let (rx, _) = compute_site_panel_origin(right_anchor, (236.0, 260.0), (1600.0, 1000.0));
        assert!(rx + 236.0 <= 1600.0);
    }
}
