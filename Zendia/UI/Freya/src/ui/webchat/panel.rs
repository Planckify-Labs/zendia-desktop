/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Native Freya Web chat side panel (`zendia.webchat`).
//!
//! Only the chat site itself renders in a Hub web view (lazily created per site and kept alive across switches so
//! replies, drafts, and scroll stay put). The header, site strip, add-site field, accounts menu, waiting emblem,
//! dictation banner, and dock controls are all native Freya.

use crate::ui::agent::{agent_input_colors, agent_input_layout};
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::page::{PageSurface, PageSurfaceContext};
use crate::ui::panels::{DockLayoutControls, ResolvedDockPanel, try_use_dock_state};
use crate::ui::theme::{ResolvedScheme, ThemeRole, use_theme};
use crate::ui::webchat::logos::{chat_site_logo, parse_hex_color};
use crate::ui::webchat::views::try_use_webchat_views;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::SidePanelId;
use zendia_browser_core::webchat::{WEBCHAT_PANEL_ID, normalize_chat_site_url};

const NEW_TAB_POPOUT_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect x="2.5" y="3" width="11" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.35"/><path d="M8.5 7.5L13 3M10 3h3v3" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

const ACCOUNTS_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><circle cx="6.2" cy="5.5" r="2.2" fill="none" stroke="currentColor" stroke-width="1.35"/><path d="M2.4 13c.4-2.1 1.9-3.2 3.8-3.2s3.4 1.1 3.8 3.2" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round"/><circle cx="11.2" cy="6.2" r="1.7" fill="none" stroke="currentColor" stroke-width="1.25"/><path d="M10.4 9.9c1.5.2 2.7 1.2 3.1 3.1" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round"/></svg>"#;

const RELOAD_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M13 8a5 5 0 1 1-1.4-3.5M13 3.2v2.9h-2.9" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

fn header_svg(key: &'static str, bytes: &'static [u8], size: f32, color: Color) -> Element {
    SvgViewer::new((key, Bytes::from_static(bytes)))
        .width(Size::px(size))
        .height(Size::px(size))
        .color(color)
        .show_loader(false)
        .into_element()
}

/// Native Web chat side panel rendered inside [`crate::ui::panels::DockFrame`].
#[derive(Clone, PartialEq)]
pub struct WebChatPanel {
    pub resolved: ResolvedDockPanel,
    key: DiffKey,
}

impl KeyExt for WebChatPanel {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl WebChatPanel {
    pub fn new(resolved: ResolvedDockPanel) -> Self {
        Self {
            resolved,
            key: DiffKey::None,
        }
    }
}

impl Component for WebChatPanel {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let webchat_views = try_use_webchat_views();
        let page_surfaces = try_consume_context::<PageSurfaceContext>();

        let mut custom_url = use_state(String::new);
        let mut custom_error = use_state(|| None::<String>);

        let (sites, active_site, pending_prompt, dictating) = {
            let model = core.model().read();
            let sites = model.chat_sites.clone();
            let active = model.active_chat_site().cloned();
            let pending = model.webchat_pending_prompt.clone();
            let dictating = model.webchat_dictating;
            (sites, active, pending, dictating)
        };

        // Lazily create/fetch the persistent Hub view for the active site when running in the real browser.
        let active_view_id = active_site
            .as_ref()
            .and_then(|site| webchat_views.and_then(|mut reg| reg.ensure_view_for_site(site)));

        let has_presented_frame = active_view_id.is_some_and(|vid| {
            page_surfaces.is_some_and(|ctx| {
                ctx.surfaces()
                    .read()
                    .get(&vid)
                    .is_some_and(|surface| surface.frame.is_some())
            })
        });
        let is_loading = match (&active_site, webchat_views) {
            (Some(site), Some(reg)) => reg.is_loading(&site.id) || !has_presented_frame,
            _ => true,
        };

        let accounts_open = dock_state.is_some_and(|ds| *ds.webchat_accounts_open.read());
        let add_site_open = dock_state.is_some_and(|ds| *ds.webchat_add_site_open.read());

        let theme_accent = theme.color(ThemeRole::Accent);
        let border_col = theme.color(ThemeRole::BorderSubtle);
        let muted_col = theme.color(ThemeRole::TextMuted);
        let site_accent = active_site
            .as_ref()
            .map(|s| parse_hex_color(&s.accent_hex, theme_accent))
            .unwrap_or(theme_accent);

        let core_new = core.clone();
        let core_reload = core.clone();
        let core_popout = core.clone();
        let core_agent = core.clone();
        let core_close = core.clone();

        // --- Top action bar (38px) ---
        let action_bar = rect()
            .width(Size::fill())
            .height(Size::px(38.0))
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .main_align(Alignment::SpaceBetween)
            .spacing(6.0)
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(3.0)
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("New web chat")
                            .width(Size::px(24.0))
                            .height(Size::px(24.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_new.invoke_or_log(Command::WebChatNewChat { site_id: None });
                            })
                            .child(Icon::new("plus").size(12.5).role(ThemeRole::TextMuted)),
                    )
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Reload chat site")
                            .width(Size::px(24.0))
                            .height(Size::px(24.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_reload.invoke_or_log(Command::WebChatReload { site_id: None });
                            })
                            .child(header_svg("zendia-webchat-reload", RELOAD_SVG, 13.0, muted_col)),
                    )
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Open chat site in tab")
                            .width(Size::px(24.0))
                            .height(Size::px(24.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_popout.invoke_or_log(Command::WebChatOpenInTab { site_id: None });
                            })
                            .child(header_svg("zendia-webchat-popout", NEW_TAB_POPOUT_SVG, 13.0, muted_col)),
                    )
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Web chat accounts")
                            .width(Size::px(24.0))
                            .height(Size::px(24.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .background(if accounts_open {
                                theme.rgba(ThemeRole::Accent).with_alpha(0.18).to_color()
                            } else {
                                Color::TRANSPARENT
                            })
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                if let Some(mut ds) = dock_state {
                                    let next = !*ds.webchat_accounts_open.peek();
                                    ds.webchat_accounts_open.set(next);
                                    if next {
                                        ds.webchat_add_site_open.set_if_modified(false);
                                    }
                                }
                            })
                            .child(header_svg(
                                "zendia-webchat-accounts",
                                ACCOUNTS_SVG,
                                13.5,
                                if accounts_open { theme_accent } else { muted_col },
                            )),
                    )
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Switch to Agent Chat")
                            .width(Size::px(24.0))
                            .height(Size::px(24.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_agent.invoke_or_log(Command::AgentAsk);
                            })
                            .child(Icon::new("sparkles").size(12.5).role(ThemeRole::TextMuted)),
                    ),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(4.0)
                    .child(DockLayoutControls {
                        panel_id: self.resolved.panel_id.clone(),
                        prefs: self.resolved.prefs,
                        split_fell_back: self.resolved.split_fell_back,
                        accent: theme_accent,
                    })
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Close web chat panel")
                            .width(Size::px(24.0))
                            .height(Size::px(24.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_close.invoke_or_log(Command::PanelClose {
                                    panel_id: SidePanelId::new(WEBCHAT_PANEL_ID),
                                });
                            })
                            .child(Icon::new("close").size(12.0).role(ThemeRole::TextMuted)),
                    ),
            );

        // --- Site strip (rounded pill bar matching reference) ---
        let strip_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(26, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(14, 20, 24, 38),
        };
        let mut strip_row = rect()
            .width(Size::fill())
            .height(Size::px(34.0))
            .padding(Gaps::new(3.0, 4.0, 3.0, 4.0))
            .corner_radius(CornerRadius::new_all(10.0))
            .background(strip_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_col),
            )
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(3.0);

        for site in &sites {
            let is_active = active_site.as_ref().is_some_and(|a| a.id == site.id);
            let accent = parse_hex_color(&site.accent_hex, theme_accent);
            let sid = site.id.clone();
            let core_pick = core.clone();

            if is_active {
                let active_bg = Color::from_argb(
                    match theme.scheme {
                        ResolvedScheme::Dark => 48,
                        ResolvedScheme::Light => 42,
                    },
                    accent.r(),
                    accent.g(),
                    accent.b(),
                );
                let active_border = Color::from_argb(95, accent.r(), accent.g(), accent.b());
                strip_row = strip_row.child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt(format!("{} (active)", site.name))
                        .height(Size::px(26.0))
                        .padding(Gaps::new(0.0, 9.0, 0.0, 7.0))
                        .corner_radius(CornerRadius::new_all(7.0))
                        .background(active_bg)
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(active_border),
                        )
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(6.0)
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_pick.invoke_or_log(Command::WebChatSelectSite { site_id: sid.clone() });
                        })
                        .child(chat_site_logo(&site.id.0, 15.0, theme.color(ThemeRole::Text)))
                        .child(
                            label()
                                .text(site.name.clone())
                                .font_size(11.5)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(theme.color(ThemeRole::Text))
                                .max_lines(1),
                        ),
                );
            } else {
                strip_row = strip_row.child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt(format!("Switch to {}", site.name))
                        .width(Size::px(26.0))
                        .height(Size::px(26.0))
                        .corner_radius(CornerRadius::new_all(7.0))
                        .center()
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_pick.invoke_or_log(Command::WebChatSelectSite { site_id: sid.clone() });
                        })
                        .child(chat_site_logo(&site.id.0, 15.0, muted_col)),
                );
            }
        }

        // Trailing `+` button to add a custom chat site.
        strip_row = strip_row.child(
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Add custom web chat site")
                .width(Size::px(24.0))
                .height(Size::px(24.0))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(if add_site_open {
                    theme.rgba(ThemeRole::Accent).with_alpha(0.18).to_color()
                } else {
                    Color::TRANSPARENT
                })
                .center()
                .cursor(CursorIcon::Pointer)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    if let Some(mut ds) = dock_state {
                        let next = !*ds.webchat_add_site_open.peek();
                        ds.webchat_add_site_open.set(next);
                        if next {
                            ds.webchat_accounts_open.set_if_modified(false);
                        }
                    }
                    custom_error.set(None);
                })
                .child(Icon::new("plus").size(12.0).role(if add_site_open {
                    ThemeRole::Accent
                } else {
                    ThemeRole::TextMuted
                })),
        );

        let strip_container = rect()
            .width(Size::fill())
            .padding(Gaps::new(0.0, 8.0, 8.0, 8.0))
            .child(strip_row);

        // --- Inline Add custom site bar ---
        let add_site_bar = add_site_open.then(|| {
            let core_submit = core.clone();
            let core_enter = core.clone();
            let input_bg = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(46, 255, 255, 255),
                ResolvedScheme::Light => Color::from_argb(24, 20, 24, 38),
            };
            let btn_fg = match theme.scheme {
                ResolvedScheme::Dark => Color::from_rgb(24, 24, 38),
                ResolvedScheme::Light => Color::WHITE,
            };
            let err_msg = custom_error.read().clone();

            rect()
                .width(Size::fill())
                .padding(Gaps::new(0.0, 8.0, 8.0, 8.0))
                .spacing(4.0)
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(4.0, 6.0, 4.0, 8.0))
                        .corner_radius(CornerRadius::new_all(9.0))
                        .background(input_bg)
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(theme.rgba(ThemeRole::Accent).with_alpha(0.35).to_color()),
                        )
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::center())
                        .spacing(6.0)
                        .child(
                            rect().width(Size::flex(1.0)).child(
                                Input::new(custom_url)
                                    .width(Size::fill())
                                    .placeholder("https://chat.example.com")
                                    .theme_colors(agent_input_colors(&theme, true))
                                    .theme_layout(agent_input_layout(Gaps::new(2.0, 2.0, 2.0, 2.0), 0.0))
                                    .on_pre_key_down(move |e: Event<KeyboardEventData>| {
                                        if e.key == Key::Named(NamedKey::Enter) {
                                            let url = custom_url.peek().clone();
                                            if normalize_chat_site_url(&url).is_some() {
                                                core_enter.invoke_or_log(Command::WebChatAddSite { name: None, url });
                                                custom_url.set(String::new());
                                                custom_error.set(None);
                                                if let Some(mut ds) = dock_state {
                                                    ds.webchat_add_site_open.set(false);
                                                }
                                            } else {
                                                custom_error
                                                    .set(Some("Enter a valid hostname (e.g. poe.com)".to_string()));
                                            }
                                            return false;
                                        }
                                        if e.key == Key::Named(NamedKey::Escape) {
                                            if let Some(mut ds) = dock_state {
                                                ds.webchat_add_site_open.set(false);
                                            }
                                            custom_error.set(None);
                                            return false;
                                        }
                                        true
                                    }),
                            ),
                        )
                        .child(
                            rect()
                                .a11y_role(AccessibilityRole::Button)
                                .a11y_alt("Add site")
                                .height(Size::px(24.0))
                                .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
                                .corner_radius(CornerRadius::new_all(6.0))
                                .background(theme_accent)
                                .center()
                                .cursor(CursorIcon::Pointer)
                                .on_press(move |e: Event<PressEventData>| {
                                    e.stop_propagation();
                                    let url = custom_url.peek().clone();
                                    if normalize_chat_site_url(&url).is_some() {
                                        core_submit.invoke_or_log(Command::WebChatAddSite { name: None, url });
                                        custom_url.set(String::new());
                                        custom_error.set(None);
                                        if let Some(mut ds) = dock_state {
                                            ds.webchat_add_site_open.set(false);
                                        }
                                    } else {
                                        custom_error.set(Some("Enter a valid hostname (e.g. poe.com)".to_string()));
                                    }
                                })
                                .child(
                                    label()
                                        .text("Add")
                                        .font_size(11.0)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .color(btn_fg),
                                ),
                        )
                        .child(
                            rect()
                                .a11y_role(AccessibilityRole::Button)
                                .a11y_alt("Cancel add site")
                                .width(Size::px(22.0))
                                .height(Size::px(22.0))
                                .corner_radius(CornerRadius::new_all(5.0))
                                .center()
                                .cursor(CursorIcon::Pointer)
                                .on_press(move |e: Event<PressEventData>| {
                                    e.stop_propagation();
                                    if let Some(mut ds) = dock_state {
                                        ds.webchat_add_site_open.set(false);
                                    }
                                    custom_error.set(None);
                                })
                                .child(Icon::new("close").size(11.0).role(ThemeRole::TextMuted)),
                        ),
                )
                .maybe_child(err_msg.map(|msg| label().text(msg).font_size(10.5).color(theme.color(ThemeRole::Danger))))
        });

        // --- Loading progress bar along top of site body ---
        let progress_bar = rect()
            .width(Size::fill())
            .height(Size::px(2.0))
            .background(border_col)
            .maybe_child(is_loading.then(|| {
                rect()
                    .width(Size::percent(42.0))
                    .height(Size::px(2.0))
                    .position(Position::new_absolute().left(88.0).top(0.0))
                    .background(site_accent)
            }));

        // --- Pending Ask prompt notice (stub mode before Automation World) ---
        let pending_banner = pending_prompt.as_ref().and_then(|(sid, prompt)| {
            active_site.as_ref().filter(|a| &a.id == sid).map(|site| {
                let core_clear = core.clone();
                rect()
                    .width(Size::fill())
                    .margin(Gaps::new(8.0, 10.0, 0.0, 10.0))
                    .padding(Gaps::new(7.0, 10.0, 7.0, 10.0))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .background(theme.rgba(ThemeRole::Accent).with_alpha(0.12).to_color())
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.rgba(ThemeRole::Accent).with_alpha(0.30).to_color()),
                    )
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .main_align(Alignment::SpaceBetween)
                    .spacing(8.0)
                    .child(
                        rect().width(Size::flex(1.0)).spacing(2.0).child(
                            label()
                                .text(format!("Ask {}: {}", site.name, prompt))
                                .font_size(11.0)
                                .color(theme.color(ThemeRole::Text))
                                .max_lines(2),
                        ),
                    )
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Dismiss prompt notice")
                            .width(Size::px(20.0))
                            .height(Size::px(20.0))
                            .corner_radius(CornerRadius::new_all(5.0))
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_clear.invoke_or_log(Command::WebChatNewChat { site_id: None });
                            })
                            .child(Icon::new("close").size(10.5).role(ThemeRole::TextMuted)),
                    )
            })
        });

        // --- Dictation listening indicator ---
        let dictation_pill = dictating.then(|| {
            let core_stop_dict = core.clone();
            rect()
                .position(Position::new_absolute().bottom(14.0).left(0.0))
                .width(Size::percent(100.0))
                .direction(Direction::Horizontal)
                .main_align(Alignment::center())
                .child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("Stop voice dictation")
                        .height(Size::px(30.0))
                        .padding(Gaps::new(0.0, 12.0, 0.0, 10.0))
                        .corner_radius(CornerRadius::new_all(999.0))
                        .background(theme.color(ThemeRole::SurfaceRaised))
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(theme.rgba(ThemeRole::Danger).with_alpha(0.55).to_color()),
                        )
                        .shadow(theme.shadows.card.to_shadow())
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(7.0)
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_stop_dict.invoke_or_log(Command::WebChatSetDictating { listening: false });
                        })
                        .child(
                            rect()
                                .width(Size::px(8.0))
                                .height(Size::px(8.0))
                                .corner_radius(CornerRadius::new_all(999.0))
                                .background(theme.color(ThemeRole::Danger)),
                        )
                        .child(
                            label()
                                .text("Listening…")
                                .font_size(11.5)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(theme.color(ThemeRole::Text)),
                        )
                        .child(
                            label()
                                .text("Click to stop")
                                .font_size(10.5)
                                .color(theme.color(ThemeRole::TextMuted)),
                        ),
                )
        });

        // --- Site body: real live Hub PageSurface (in zendia) or centered waiting emblem ---
        let waiting_emblem = {
            let site_id_str = active_site.as_ref().map(|s| s.id.0.as_str()).unwrap_or("chatgpt");
            let emblem_col = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(180, 210, 216, 232),
                ResolvedScheme::Light => Color::from_argb(175, 92, 100, 118),
            };
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .center()
                .child(chat_site_logo(site_id_str, 40.0, emblem_col))
        };

        let site_stage = rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .overflow(Overflow::Clip)
            .maybe_child(pending_banner)
            .child(if let Some(view_id) = active_view_id {
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .child(waiting_emblem)
                    .child(
                        rect()
                            .position(Position::new_absolute().top(0.0).left(0.0))
                            .width(Size::percent(100.0))
                            .height(Size::percent(100.0))
                            .child(PageSurface::for_view(view_id).key(format!("webchat-view:{view_id}"))),
                    )
                    .into_element()
            } else {
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .child(waiting_emblem)
                    .into_element()
            })
            .maybe_child(dictation_pill);

        // --- Floating Accounts dropdown menu (matching webchat-account-menu-{dark,light}.png) ---
        let accounts_overlay = accounts_open.then(|| {
            let menu_bg = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(248, 30, 34, 52),
                ResolvedScheme::Light => Color::from_argb(252, 250, 251, 255),
            };
            let active_row_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.18).to_color();
            let core_refresh = core.clone();

            rect()
                .layer(Layer::OverlayLevel(crate::ui::theme::POPUP_OVERLAY_LEVEL))
                .position(Position::new_absolute().top(78.0).left(10.0))
                .width(Size::px(220.0))
                .padding(Gaps::new_all(6.0))
                .spacing(3.0)
                .corner_radius(CornerRadius::new_all(12.0))
                .overflow(Overflow::Clip)
                .background(menu_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(border_col),
                )
                .shadow(theme.shadows.card.to_shadow())
                .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(30.0))
                        .padding(Gaps::new(0.0, 9.0, 0.0, 8.0))
                        .corner_radius(CornerRadius::new_all(7.0))
                        .background(active_row_bg)
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(8.0)
                        .child(Icon::new("check").size(11.5).role(ThemeRole::Text))
                        .child(
                            label()
                                .text("Account 1")
                                .font_size(12.0)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(theme.color(ThemeRole::Text)),
                        ),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(1.0))
                        .margin(Gaps::new(3.0, 4.0, 3.0, 4.0))
                        .background(border_col),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(28.0))
                        .padding(Gaps::new(0.0, 9.0, 0.0, 8.0))
                        .corner_radius(CornerRadius::new_all(7.0))
                        .opacity(0.62)
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(8.0)
                        .child(Icon::new("plus").size(11.5).role(ThemeRole::TextMuted))
                        .child(
                            label()
                                .text("Add new account")
                                .font_size(11.5)
                                .color(theme.color(ThemeRole::Text)),
                        ),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(3.0, 9.0, 5.0, 27.0))
                        .child(
                            label()
                                .text("Same as regular browsing")
                                .font_size(11.0)
                                .color(theme.color(ThemeRole::TextMuted)),
                        ),
                )
                .child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("Refresh logins from browser")
                        .width(Size::fill())
                        .height(Size::px(30.0))
                        .padding(Gaps::new(0.0, 9.0, 0.0, 8.0))
                        .corner_radius(CornerRadius::new_all(7.0))
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(8.0)
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            if let Some(mut ds) = dock_state {
                                ds.webchat_accounts_open.set(false);
                            }
                            core_refresh.invoke_or_log(Command::WebChatReload { site_id: None });
                        })
                        .child(header_svg("zendia-webchat-refresh-logins", RELOAD_SVG, 12.0, muted_col))
                        .child(
                            label()
                                .text("Refresh logins from browser")
                                .font_size(11.5)
                                .color(theme.color(ThemeRole::Text)),
                        ),
                )
        });

        let on_panel_press = move |_: Event<PointerEventData>| {
            if let Some(mut ds) = dock_state
                && *ds.webchat_accounts_open.peek()
            {
                ds.webchat_accounts_open.set(false);
            }
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .on_pointer_down(on_panel_press)
            .child(action_bar)
            .child(strip_container)
            .maybe_child(add_site_bar)
            .child(progress_bar)
            .child(site_stage)
            .maybe_child(accounts_overlay)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
