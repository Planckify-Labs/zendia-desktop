/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Native Freya Web chat side panel (`zendia.webchat`).
//!
//! Only the chat site itself renders in a Hub web view (lazily created per site and kept alive across switches so
//! replies, drafts, and scroll stay put). The header, scrollable site strip with spring-expanding labels & custom-site
//! remove buttons, auto-focused add-site bar, anchored accounts menu, animated loading bar, pulsing waiting emblem,
//! drag-drop overlay, and dock controls are all native Freya.

use crate::ui::agent::{agent_input_colors, dashed_rounded_box_fill};
use crate::ui::core_view::use_core;
use crate::ui::kit::{
    Icon, IndeterminateBar, KitTooltip, ListNavAction, ListNavConfig, PopupPresence, TextField, classify_list_key,
    focus_ring_border, mix_color, use_interactive_feedback,
};
use crate::ui::page::{PageSurface, PageSurfaceContext};
use crate::ui::panels::{DockLayoutControls, ResolvedDockPanel, try_use_dock_state};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, ThemeRole, motion, use_theme};
use crate::ui::webchat::logos::{chat_site_logo, parse_hex_color};
use crate::ui::webchat::views::try_use_webchat_views;
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{SidePanelId, SidePanelLayout};
use zendia_browser_core::model::ChatSite;
use zendia_browser_core::webchat::{WEBCHAT_PANEL_ID, normalize_https_chat_site_url};

const NEW_TAB_POPOUT_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect x="2.2" y="2.8" width="11.6" height="10.4" rx="2" fill="none" stroke="currentColor" stroke-width="1.4"/><path d="M8.5 7.5L13 3M9.8 3H13v3.2" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

const ACCOUNTS_SOLID_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><circle cx="5.8" cy="5.2" r="2.3" fill="currentColor"/><path d="M1.8 13.2c.3-2.5 1.9-3.9 4-3.9s3.7 1.4 4 3.9H1.8z" fill="currentColor"/><circle cx="11.1" cy="6.1" r="1.8" fill="currentColor"/><path d="M9.9 9.7c1.8.2 3.4 1.3 3.9 3.5h-2.9c-.2-1.4-.7-2.5-1-3.5z" fill="currentColor"/></svg>"#;

const RELOAD_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M13.2 8a5.2 5.2 0 1 1-1.45-3.6M13.2 3v3.1h-3.1" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

const PUT_BACK_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M3.5 3.5v4.8a2 2 0 0 0 2 2h7M9.8 7.5l2.7 2.8-2.7 2.8" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

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
        let mut accounts_anchor_left = use_state(|| 10.0f32);
        let mut panel_origin_x = use_state(|| 0.0f32);
        let mut same_as_browser = use_state(|| true);
        let drag_over = use_state(|| false);
        let add_input_a11y_id = use_a11y();
        let mut last_add_open = use_state(|| false);

        let (sites, active_site, pending_prompt, dictating) = {
            let model = core.model().read();
            let sites = model.chat_sites.clone();
            let active = model.active_chat_site().cloned();
            let pending = model.webchat_pending_prompt.clone();
            let dictating = model.webchat_dictating;
            (sites, active, pending, dictating)
        };
        let _ = dictating;

        // Lazily create/fetch the persistent Hub view for the active site when running in the real browser.
        let active_view_id = active_site
            .as_ref()
            .and_then(|site| webchat_views.and_then(|mut reg| reg.ensure_view_for_site(site)));

        let has_presented_frame = active_view_id.is_some_and(|vid| {
            page_surfaces.as_ref().is_some_and(|ctx| {
                ctx.surfaces()
                    .read()
                    .get(&vid)
                    .is_some_and(|surface| surface.frame.is_some())
            })
        });
        let is_loading = match (&active_site, webchat_views) {
            (Some(site), Some(reg)) => reg.is_loading(&site.id) || !has_presented_frame,
            _ => !sites.is_empty(),
        };

        let accounts_open = dock_state.is_some_and(|ds| *ds.webchat_accounts_open.read());
        let add_site_open = dock_state.is_some_and(|ds| *ds.webchat_add_site_open.read());
        let static_snapshot = dock_state.is_some_and(|ds| ds.pinned_ring_phase.read().is_some());

        // WC-03: Auto-focus add-site input when opened; clear draft & error whenever closed.
        if *last_add_open.peek() != add_site_open {
            last_add_open.set(add_site_open);
            if add_site_open {
                custom_error.set_if_modified(None);
                if !static_snapshot {
                    add_input_a11y_id.request_focus();
                }
            } else {
                custom_url.set_if_modified(String::new());
                custom_error.set_if_modified(None);
            }
        }

        let theme_accent = theme.color(ThemeRole::Accent);
        let divider_col = theme.rgba(ThemeRole::Border).with_alpha(0.70).to_color();
        let site_accent = active_site
            .as_ref()
            .map(|s| parse_hex_color(&s.accent_hex, theme_accent))
            .unwrap_or(theme_accent);
        let has_sites = !sites.is_empty();

        let core_new = core.clone();
        let core_reload = core.clone();
        let core_popout = core.clone();
        let core_putback = core.clone();
        let core_agent = core.clone();
        let core_close = core.clone();

        // --- Top action bar (`WC-01`, `P3-02`: 24x24 buttons, 14px crisp icons, hover wash, tooltips, disabled when empty) ---
        let action_bar = rect()
            .width(Size::fill())
            .height(Size::px(42.0))
            .padding(Gaps::new(0.0, 10.0, 0.0, 12.0))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .main_align(Alignment::SpaceBetween)
            .spacing(4.0)
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(2.0)
                    .child(WebChatHeaderButton {
                        icon: WebChatHeaderIcon::Named("plus", 14.0),
                        tooltip: "New chat on every site",
                        shortcut: None,
                        enabled: has_sites,
                        active: false,
                        on_sized: None,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_new.invoke_or_log(Command::WebChatNewChat { site_id: None });
                        }),
                        key: DiffKey::from(&"wc-hdr-new"),
                    })
                    .child(WebChatHeaderButton {
                        icon: WebChatHeaderIcon::Svg("zendia-webchat-reload", RELOAD_SVG, 14.0),
                        tooltip: "Reload",
                        shortcut: None,
                        enabled: has_sites,
                        active: false,
                        on_sized: None,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_reload.invoke_or_log(Command::WebChatReload { site_id: None });
                        }),
                        key: DiffKey::from(&"wc-hdr-reload"),
                    })
                    .child(WebChatHeaderButton {
                        icon: WebChatHeaderIcon::Svg("zendia-webchat-popout", NEW_TAB_POPOUT_SVG, 14.5),
                        tooltip: "Open in a tab",
                        shortcut: None,
                        enabled: has_sites,
                        active: false,
                        on_sized: None,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_popout.invoke_or_log(Command::WebChatOpenInTab { site_id: None });
                        }),
                        key: DiffKey::from(&"wc-hdr-popout"),
                    })
                    .child(WebChatHeaderButton {
                        icon: WebChatHeaderIcon::Svg("zendia-webchat-accounts", ACCOUNTS_SOLID_SVG, 15.0),
                        tooltip: "Accounts",
                        shortcut: None,
                        enabled: has_sites,
                        active: accounts_open,
                        on_sized: Some(EventHandler::new(move |min_x: f32| {
                            let local = (min_x - *panel_origin_x.peek()).max(8.0);
                            accounts_anchor_left.set_if_modified(local);
                        })),
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            if let Some(mut ds) = dock_state {
                                let next = !*ds.webchat_accounts_open.peek();
                                ds.webchat_accounts_open.set(next);
                                if next {
                                    ds.webchat_add_site_open.set_if_modified(false);
                                }
                            }
                        }),
                        key: DiffKey::from(&"wc-hdr-accounts"),
                    })
                    .maybe_child(pending_prompt.is_some().then(|| WebChatHeaderButton {
                        icon: WebChatHeaderIcon::Svg("zendia-webchat-putback", PUT_BACK_SVG, 14.0),
                        tooltip: "Put the reply back in the page",
                        shortcut: None,
                        enabled: has_sites,
                        active: false,
                        on_sized: None,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_putback.invoke_or_log(Command::WebChatNewChat { site_id: None });
                        }),
                        key: DiffKey::from(&"wc-hdr-putback"),
                    }))
                    .child(WebChatHeaderButton {
                        icon: WebChatHeaderIcon::Named("sparkles", 14.5),
                        tooltip: "Your agent",
                        shortcut: None,
                        enabled: true,
                        active: false,
                        on_sized: None,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_agent.invoke_or_log(Command::AgentAsk);
                        }),
                        key: DiffKey::from(&"wc-hdr-agent"),
                    }),
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
                    .child(WebChatHeaderButton {
                        icon: WebChatHeaderIcon::Named("close", 14.0),
                        tooltip: "Close",
                        shortcut: Some("Esc"),
                        enabled: true,
                        active: false,
                        on_sized: None,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_close.invoke_or_log(Command::PanelClose {
                                panel_id: SidePanelId::new(WEBCHAT_PANEL_ID),
                            });
                        }),
                        key: DiffKey::from(&"wc-hdr-close"),
                    }),
            );

        // --- Scrollable Site strip (`WC-02`, `P3-03`) ---
        let strip_bg = theme.rgba(ThemeRole::SurfaceRaised).with_alpha(0.55).to_color();
        let sites_for_keys = sites.clone();
        let active_idx = active_site
            .as_ref()
            .and_then(|a| sites.iter().position(|s| s.id == a.id))
            .unwrap_or(0);
        let core_strip_keys = core.clone();

        let mut track_items = rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(2.0);

        for site in &sites {
            let is_active = active_site.as_ref().is_some_and(|a| a.id == site.id);
            let site_loading = is_active && is_loading && !static_snapshot;
            let sid = site.id.clone();
            let sid_remove = site.id.clone();
            let core_pick = core.clone();
            let core_remove = core.clone();

            track_items = track_items.child(WebChatSiteTab {
                site: site.clone(),
                active: is_active,
                replying: site_loading,
                static_snapshot,
                on_select: EventHandler::new(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    core_pick.invoke_or_log(Command::WebChatSelectSite { site_id: sid.clone() });
                }),
                on_remove: EventHandler::new(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    core_remove.invoke_or_log(Command::WebChatRemoveSite {
                        site_id: sid_remove.clone(),
                    });
                }),
                key: DiffKey::from(&format!("wc-tab-{}", site.id.0)),
            });
        }

        // Trailing `+` button to add a custom chat site (no pink box at rest, matching `P3-05` & `zendia.css:4285`).
        track_items = track_items.child(WebChatAddSiteButton {
            open: add_site_open,
            on_press: EventHandler::new(move |e: Event<PressEventData>| {
                e.stop_propagation();
                if let Some(mut ds) = dock_state {
                    let next = !*ds.webchat_add_site_open.peek();
                    ds.webchat_add_site_open.set(next);
                    if next {
                        ds.webchat_accounts_open.set_if_modified(false);
                    }
                }
            }),
            key: DiffKey::from(&"wc-add-site-btn"),
        });

        let strip_track = rect()
            .a11y_role(AccessibilityRole::TabList)
            .a11y_alt("Chat sites")
            .width(Size::fill())
            .height(Size::px(36.0))
            .padding(Gaps::new_all(3.0))
            .corner_radius(CornerRadius::new_all(14.0))
            .background(strip_bg)
            .border(
                Border::new()
                    .width(0.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(divider_col),
            )
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if sites_for_keys.len() < 2 {
                    return;
                }
                match e.key {
                    Key::Named(NamedKey::ArrowRight) => {
                        let next = (active_idx + 1) % sites_for_keys.len();
                        core_strip_keys.invoke_or_log(Command::WebChatSelectSite {
                            site_id: sites_for_keys[next].id.clone(),
                        });
                        e.stop_propagation();
                    }
                    Key::Named(NamedKey::ArrowLeft) => {
                        let next = (active_idx + sites_for_keys.len() - 1) % sites_for_keys.len();
                        core_strip_keys.invoke_or_log(Command::WebChatSelectSite {
                            site_id: sites_for_keys[next].id.clone(),
                        });
                        e.stop_propagation();
                    }
                    _ => {}
                }
            })
            .child(
                ScrollView::new()
                    .direction(Direction::Horizontal)
                    .show_scrollbar(false)
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(track_items),
            );

        let strip_container = rect()
            .width(Size::fill())
            .padding(Gaps::new(8.0, 10.0, 9.0, 10.0))
            .child(strip_track);

        // --- Inline Add custom site bar (`WC-03`, `P3-05`: transparent/raised input, pink focus ring, hint below, no inline buttons) ---
        let add_site_bar = add_site_open.then(|| {
            let core_enter = core.clone();
            let err_msg = custom_error.read().clone();
            let is_invalid = err_msg.is_some();
            let input_bg = theme.rgba(ThemeRole::SurfaceRaised).with_alpha(0.70).to_color();
            let border_fill = if is_invalid {
                theme.color(ThemeRole::Warning)
            } else {
                theme_accent
            };
            let hint_text = err_msg
                .clone()
                .unwrap_or_else(|| "Enter adds it \u{00b7} Esc cancels".to_string());
            let hint_color = if is_invalid {
                theme.color(ThemeRole::Warning)
            } else {
                theme.rgba(ThemeRole::Text).with_alpha(0.60).to_color()
            };
            let input_colors = agent_input_colors(&theme, true);
            let pref_color = |pref: Option<Preference<Color>>, fallback: Color| match pref {
                Some(Preference::Specific(c)) => c,
                _ => fallback,
            };

            rect()
                .width(Size::fill())
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(8.0, 12.0, 10.0, 12.0))
                        .spacing(4.0)
                        .child(
                            rect()
                                .width(Size::fill())
                                .height(Size::px(32.0))
                                .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
                                .corner_radius(CornerRadius::new_all(8.0))
                                .background(input_bg)
                                .border(
                                    Border::new()
                                        .width(1.5)
                                        .alignment(BorderAlignment::Inner)
                                        .fill(border_fill),
                                )
                                .font_size(12.0)
                                .main_align(Alignment::center())
                                .child(
                                    TextField::new(custom_url)
                                        .a11y_id(add_input_a11y_id)
                                        .auto_focus(true)
                                        .placeholder("Address of a chat site, like chat.mistral.ai")
                                        .width(Size::fill())
                                        .height(Size::px(24.0))
                                        .color(pref_color(input_colors.color, theme.color(ThemeRole::Text)))
                                        .placeholder_color(pref_color(
                                            input_colors.placeholder_color,
                                            theme.color(ThemeRole::TextMuted),
                                        ))
                                        .padding(Gaps::new(0.0, 0.0, 0.0, 0.0))
                                        .on_change(move |_| {
                                            custom_error.set_if_modified(None);
                                        })
                                        .on_pre_key_down(move |e: Event<KeyboardEventData>| {
                                            if e.key == Key::Named(NamedKey::Enter) {
                                                e.stop_propagation();
                                                e.prevent_default();
                                                let raw = custom_url.peek().clone();
                                                if let Some(normalized) = normalize_https_chat_site_url(&raw) {
                                                    core_enter.invoke_or_log(Command::WebChatAddSite {
                                                        name: None,
                                                        url: normalized,
                                                    });
                                                    custom_url.set(String::new());
                                                    custom_error.set(None);
                                                    if let Some(mut ds) = dock_state {
                                                        ds.webchat_add_site_open.set(false);
                                                    }
                                                } else {
                                                    custom_error.set(Some(
                                                        "Needs an https address, like chat.mistral.ai".to_string(),
                                                    ));
                                                }
                                                return false;
                                            }
                                            if e.key == Key::Named(NamedKey::Escape) {
                                                e.stop_propagation();
                                                e.prevent_default();
                                                custom_url.set(String::new());
                                                custom_error.set(None);
                                                if let Some(mut ds) = dock_state {
                                                    ds.webchat_add_site_open.set(false);
                                                }
                                                return false;
                                            }
                                            true
                                        }),
                                ),
                        )
                        .child(label().text(hint_text).font_size(11.0).color(hint_color).max_lines(1)),
                )
                .child(rect().width(Size::fill()).height(Size::px(0.5)).background(divider_col))
        });

        // --- Loading progress bar along top of site body (`WC-04`) ---
        let progress_bar = rect()
            .width(Size::fill())
            .height(Size::px(2.0))
            .background(Color::TRANSPARENT)
            .maybe_child((is_loading && !static_snapshot).then(|| IndeterminateBar::new(2.0, site_accent)))
            .maybe_child((is_loading && static_snapshot).then(|| {
                // Static snapshot anchor matches fork's left-anchored bar segment (`P3-03`).
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::percent(40.0))
                    .height(Size::px(2.0))
                    .corner_radius(CornerRadius::new_all(1.0))
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

        // --- Site body: empty-sites notice, or live Hub PageSurface + brighter 44x44 waiting emblem (`P3-02`) ---
        let waiting_visible = has_sites && (is_loading || active_view_id.is_none());
        let site_id_owned = active_site
            .as_ref()
            .map(|s| s.id.0.clone())
            .unwrap_or_else(|| "chatgpt".to_string());
        let emblem_col = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(235, 240, 244, 255),
            ResolvedScheme::Light => Color::from_argb(220, 31, 35, 53),
        };
        let waiting_emblem = WaitingEmblem {
            site_id: site_id_owned,
            color: emblem_col,
            visible: waiting_visible,
            pulsing: is_loading && !static_snapshot,
        };

        // WC-05: Dashed site-colored drag-over attachment overlay.
        let drop_overlay = (*drag_over.read()).then(|| {
            let site_name = active_site.as_ref().map(|s| s.name.as_str()).unwrap_or("the chat");
            let bg = Color::from_argb(36, site_accent.r(), site_accent.g(), site_accent.b());
            rect()
                .layer(Layer::Relative(6))
                .position(Position::new_absolute().top(8.0).left(8.0).right(8.0).bottom(8.0))
                .corner_radius(CornerRadius::new_all(14.0))
                .background(dashed_rounded_box_fill(bg, site_accent, 2.0, 14.0, 6.0, 4.0))
                .center()
                .interactive(Interactive::No)
                .child(
                    label()
                        .text(format!("Drop to attach to {site_name}"))
                        .font_size(14.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(theme.color(ThemeRole::Text)),
                )
        });

        let site_stage = rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .content(Content::Flex)
            .overflow(Overflow::Clip)
            .maybe_child(pending_banner)
            .child(if !has_sites {
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .padding(Gaps::new_all(24.0))
                    .center()
                    .child(
                        label()
                            .text("No chat sites. Add one with +.")
                            .font_size(13.0)
                            .color(theme.color(ThemeRole::TextMuted)),
                    )
                    .into_element()
            } else if let Some(view_id) = active_view_id {
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .overflow(Overflow::Clip)
                    .child(
                        rect()
                            .position(Position::new_absolute().top(0.0).left(0.0))
                            .width(Size::percent(100.0))
                            .height(Size::percent(100.0))
                            .child(PageSurface::for_view(view_id).key(format!("webchat-view:{view_id}"))),
                    )
                    .child(
                        rect()
                            .layer(Layer::Relative(10))
                            .position(Position::new_absolute().top(0.0).left(0.0))
                            .width(Size::percent(100.0))
                            .height(Size::percent(100.0))
                            .interactive(Interactive::No)
                            .child(waiting_emblem),
                    )
                    .into_element()
            } else {
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .child(waiting_emblem)
                    .into_element()
            })
            .maybe_child(drop_overlay);

        // --- Floating Accounts dropdown menu (`WC-06`, `P3-04`: anchored, animated, full-contrast, keyboard nav) ---
        let core_refresh = core.clone();
        let anchor_x = (*accounts_anchor_left.read()).clamp(8.0, 180.0);
        let accounts_overlay = WebChatAccountsMenu {
            open: accounts_open,
            left: anchor_x,
            same_as_browser: *same_as_browser.read(),
            on_toggle_same_as_browser: EventHandler::new(move |()| {
                let next = !*same_as_browser.peek();
                same_as_browser.set(next);
            }),
            on_refresh_logins: EventHandler::new(move |()| {
                if let Some(mut ds) = dock_state {
                    ds.webchat_accounts_open.set(false);
                }
                core_refresh.invoke_or_log(Command::WebChatReload { site_id: None });
            }),
            on_close: EventHandler::new(move |()| {
                if let Some(mut ds) = dock_state {
                    ds.webchat_accounts_open.set(false);
                }
            }),
            key: DiffKey::from(&"wc-accounts-menu"),
        };

        let on_panel_press = move |_: Event<PointerEventData>| {
            if let Some(mut ds) = dock_state
                && *ds.webchat_accounts_open.peek()
            {
                ds.webchat_accounts_open.set(false);
            }
        };

        // WC-05: Escape closes accounts menu -> add-site bar -> panel (or returns focus to page in split).
        let panel_focused = self.resolved.focused;
        let eff_layout = self.resolved.effective_layout;
        let core_esc = core.clone();
        let on_global_escape = move |e: Event<KeyboardEventData>| {
            if !panel_focused || e.key != Key::Named(NamedKey::Escape) {
                return;
            }
            if let Some(mut ds) = dock_state {
                if *ds.webchat_accounts_open.peek() {
                    ds.webchat_accounts_open.set(false);
                    e.stop_propagation();
                    e.prevent_default();
                    return;
                }
                if *ds.webchat_add_site_open.peek() {
                    ds.webchat_add_site_open.set(false);
                    custom_url.set_if_modified(String::new());
                    custom_error.set_if_modified(None);
                    e.stop_propagation();
                    e.prevent_default();
                    return;
                }
                if eff_layout == SidePanelLayout::Split {
                    ds.set_focus(None);
                    if let Some(ctx) = &page_surfaces {
                        ctx.request_focus_active_surface();
                    }
                    e.stop_propagation();
                    e.prevent_default();
                    return;
                }
            }
            e.stop_propagation();
            e.prevent_default();
            core_esc.invoke_or_log(Command::PanelClose {
                panel_id: SidePanelId::new(WEBCHAT_PANEL_ID),
            });
            if let Some(ctx) = &page_surfaces {
                ctx.request_focus_active_surface();
            }
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .on_sized(move |e: Event<SizedEventData>| {
                panel_origin_x.set_if_modified(e.area.min_x());
            })
            .on_pointer_down(on_panel_press)
            .on_global_key_down(on_global_escape)
            .child(action_bar)
            .child(rect().width(Size::fill()).height(Size::px(0.5)).background(divider_col))
            .child(strip_container)
            .child(rect().width(Size::fill()).height(Size::px(0.5)).background(divider_col))
            .maybe_child(add_site_bar)
            .child(progress_bar)
            .child(site_stage)
            .child(accounts_overlay)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
enum WebChatHeaderIcon {
    Named(&'static str, f32),
    Svg(&'static str, &'static [u8], f32),
}

#[derive(Clone, PartialEq)]
struct WebChatHeaderButton {
    icon: WebChatHeaderIcon,
    tooltip: &'static str,
    shortcut: Option<&'static str>,
    enabled: bool,
    active: bool,
    on_sized: Option<EventHandler<f32>>,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for WebChatHeaderButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for WebChatHeaderButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(self.enabled);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let rest_bg = if self.active {
            theme.rgba(ThemeRole::Accent).with_alpha(0.20).to_color()
        } else {
            Color::TRANSPARENT
        };
        let bg = fb.background(
            rest_bg,
            theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.16).to_color(),
        );
        let icon_role = if self.active {
            ThemeRole::Accent
        } else if fb.is_hovered() && self.enabled {
            ThemeRole::Text
        } else {
            ThemeRole::TextMuted
        };
        let icon_col = theme.color(icon_role);
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let enabled = self.enabled;
        let on_press = self.on_press.clone();
        let on_sized = self.on_sized.clone();

        let glyph: Element = match self.icon {
            WebChatHeaderIcon::Named(name, sz) => Icon::new(name).size(sz).role(icon_role).into_element(),
            WebChatHeaderIcon::Svg(key, bytes, sz) => header_svg(key, bytes, sz, icon_col),
        };

        let mut tip = KitTooltip::new(self.tooltip);
        if let Some(sc) = self.shortcut {
            tip = tip.shortcut(sc);
        }

        tip.child(
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(enabled)
                .a11y_enabled(enabled)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(self.tooltip)
                .width(Size::px(24.0))
                .height(Size::px(24.0))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(bg)
                .border(border)
                .opacity(if enabled { 1.0 } else { 0.40 })
                .scale(fb.scale())
                .center()
                .cursor(if enabled {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                })
                .on_sized(move |e: Event<SizedEventData>| {
                    if let Some(cb) = &on_sized {
                        cb.call(e.area.min_x());
                    }
                })
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| {
                    if enabled {
                        on_press.call(e);
                    }
                })
                .child(glyph),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// One tab on the Web chat site strip with `20 px` logo, spring label reveal, strong active accent tint (`P3-03`),
/// replying dot, and hover `×` remove button on custom sites (`WC-02`).
#[derive(Clone, PartialEq)]
struct WebChatSiteTab {
    site: ChatSite,
    active: bool,
    replying: bool,
    static_snapshot: bool,
    on_select: EventHandler<Event<PressEventData>>,
    on_remove: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for WebChatSiteTab {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for WebChatSiteTab {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let snap = self.static_snapshot;
        let active_anim = use_animation_with_dependencies(&self.active, move |conf, is_act| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *is_act { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(if snap {
                    std::time::Duration::ZERO
                } else {
                    std::time::Duration::from_millis(260)
                })
                .ease(Ease::Out)
                .function(motion::spring_fn())
        });

        let pulse_active = self.replying && !self.static_snapshot;
        let dot_pulse = use_animation_with_dependencies(&pulse_active, |conf, running| {
            if *running {
                conf.on_creation(OnCreation::Run);
                conf.on_change(OnChange::Rerun);
                conf.on_finish(OnFinish::reverse());
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(0.42, 1.0)
                .duration(motion::TOOLBAR_DOT_PULSE)
                .ease(Ease::InOut)
                .function(Function::Sine)
        });

        let p = active_anim.get().value().clamp(0.0, 1.0);
        let accent = parse_hex_color(&self.site.accent_hex, theme.color(ThemeRole::Accent));
        let surface = theme.color(ThemeRole::Surface);
        let hovered = fb.is_hovered();

        // `zendia.css:4205`: `color-mix(in srgb, var(--zendia-webchat-color) 20%, var(--zendia-agent-surface))`
        let active_bg = mix_color(surface, accent, 0.24);
        let hover_bg = theme.rgba(ThemeRole::Text).with_alpha(0.08).to_color();
        let bg = if self.active {
            active_bg
        } else {
            fb.background(Color::TRANSPARENT, hover_bg, hover_bg)
        };
        let border = if matches!(focus(), Focus::Keyboard) {
            focus_ring_border(true, &theme, Border::new())
        } else if self.active {
            Border::new()
                .width(0.5)
                .alignment(BorderAlignment::Inner)
                .fill(Color::from_argb(115, accent.r(), accent.g(), accent.b()))
        } else {
            Border::new()
        };

        let logo_scale = if hovered { 1.1 } else { 1.0 };
        let logo_opacity = if self.active || hovered { 1.0 } else { 0.72 };
        let logo_tint = if self.active || hovered {
            theme.color(ThemeRole::Text)
        } else {
            theme.color(ThemeRole::TextMuted)
        };
        let show_label = self.active || p > 0.05;
        let label_opacity = if self.active { p } else { (p * 0.7).clamp(0.0, 1.0) };
        let show_remove = self.site.is_custom && (hovered || matches!(focus(), Focus::Keyboard));
        let on_select = self.on_select.clone();
        let on_remove = self.on_remove.clone();
        let remove_name = self.site.name.clone();

        let tab_btn = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_alt(if self.active {
                format!("{} (active)", self.site.name)
            } else {
                format!("Switch to {}", self.site.name)
            })
            .height(Size::px(30.0))
            .padding(if self.active {
                Gaps::new(0.0, 12.0, 0.0, 8.0)
            } else {
                Gaps::new(0.0, 7.0, 0.0, 7.0)
            })
            .corner_radius(CornerRadius::new_all(11.0))
            .background(bg)
            .border(border)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(if show_label { 7.0 * p } else { 0.0 })
            .cursor(CursorIcon::Pointer)
            .on_press(move |e: Event<PressEventData>| on_select.call(e))
            .child(
                rect()
                    .width(Size::px(20.0))
                    .height(Size::px(20.0))
                    .opacity(logo_opacity)
                    .scale(logo_scale)
                    .center()
                    .child(chat_site_logo(&self.site.id.0, 18.0, logo_tint)),
            )
            .maybe_child(show_label.then(|| {
                rect().opacity(label_opacity).child(
                    label()
                        .text(self.site.name.clone())
                        .font_size(12.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(theme.color(ThemeRole::Text))
                        .max_lines(1),
                )
            }))
            .maybe_child(self.replying.then(|| {
                rect()
                    .position(Position::new_absolute().top(3.0).left(21.0))
                    .width(Size::px(7.0))
                    .height(Size::px(7.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .opacity(dot_pulse.get().value().clamp(0.42, 1.0))
                    .background(accent)
                    .border(Border::new().width(1.5).alignment(BorderAlignment::Outer).fill(surface))
            }));

        let remove_btn = show_remove.then(|| {
            let rem_bg = mix_color(surface, theme.color(ThemeRole::Text), 0.22);
            rect()
                .layer(Layer::Relative(4))
                .position(Position::new_absolute().top(-2.0).right(-2.0))
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(format!("Remove {remove_name}"))
                .width(Size::px(14.0))
                .height(Size::px(14.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(rem_bg)
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                .on_press(move |e: Event<PressEventData>| on_remove.call(e))
                .child(Icon::new("close").size(8.0).role(ThemeRole::Text))
        });

        KitTooltip::new(self.site.name.clone()).enabled(!self.active).child(
            rect()
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .child(tab_btn)
                .maybe_child(remove_btn),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct WebChatAddSiteButton {
    open: bool,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for WebChatAddSiteButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for WebChatAddSiteButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let bg = fb.background(
            Color::TRANSPARENT,
            theme.rgba(ThemeRole::Text).with_alpha(0.08).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.14).to_color(),
        );
        let icon_role = if self.open || fb.is_hovered() {
            ThemeRole::Text
        } else {
            ThemeRole::TextMuted
        };
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let on_press = self.on_press.clone();

        KitTooltip::new("Add a chat site").child(
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Add a chat site")
                .width(Size::px(30.0))
                .height(Size::px(30.0))
                .corner_radius(CornerRadius::new_all(11.0))
                .background(bg)
                .border(border)
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| on_press.call(e))
                .child(Icon::new("plus").size(14.0).role(icon_role)),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Anchored accounts dropdown menu with `200 ms` pop-in, `accent 16%` hover wash, `0.5 px` separator under Account 1,
/// full-contrast `+ Add new account`, and keyboard navigation (`WC-06`, `P3-04`).
#[derive(Clone, PartialEq)]
struct WebChatAccountsMenu {
    open: bool,
    left: f32,
    same_as_browser: bool,
    on_toggle_same_as_browser: EventHandler<()>,
    on_refresh_logins: EventHandler<()>,
    on_close: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for WebChatAccountsMenu {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for WebChatAccountsMenu {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let a11y_id = use_a11y();
        let mut active_idx = use_state(|| 0usize);

        let is_open = self.open;
        use_side_effect_with_deps(&is_open, move |open: &bool| {
            if *open {
                active_idx.set(0);
                a11y_id.request_focus();
            }
        });

        let menu_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(242, 30, 34, 52),
            ResolvedScheme::Light => Color::from_argb(248, 250, 251, 255),
        };
        let border_col = theme.rgba(ThemeRole::Border).with_alpha(0.80).to_color();
        let sep_col = theme.rgba(ThemeRole::Border).with_alpha(0.80).to_color();
        let active_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.16).to_color();
        let text_col = theme.color(ThemeRole::Text);

        let cur = *active_idx.read();
        let on_toggle_same = self.on_toggle_same_as_browser.clone();
        let on_refresh = self.on_refresh_logins.clone();
        let on_close = self.on_close.clone();

        let on_toggle_key = self.on_toggle_same_as_browser.clone();
        let on_refresh_key = self.on_refresh_logins.clone();
        let on_close_key = self.on_close.clone();

        let menu_card = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Menu)
            .min_width(Size::px(220.0))
            .width(Size::px(228.0))
            .padding(Gaps::new_all(5.0))
            .spacing(1.0)
            .corner_radius(CornerRadius::new_all(12.0))
            .overflow(Overflow::Clip)
            .background(menu_bg)
            .border(
                Border::new()
                    .width(0.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_col),
            )
            .shadow(theme.shadows.card.to_shadow())
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if let Some(action) = classify_list_key(&e, ListNavConfig::menu()) {
                    match action {
                        ListNavAction::MoveNext => {
                            let next = (*active_idx.peek() + 1) % 4;
                            active_idx.set(next);
                            e.stop_propagation();
                        }
                        ListNavAction::MovePrev => {
                            let c = *active_idx.peek();
                            active_idx.set(if c == 0 { 3 } else { c - 1 });
                            e.stop_propagation();
                        }
                        ListNavAction::First => {
                            active_idx.set(0);
                            e.stop_propagation();
                        }
                        ListNavAction::Last => {
                            active_idx.set(3);
                            e.stop_propagation();
                        }
                        ListNavAction::Activate => {
                            match *active_idx.peek() {
                                0 => on_close_key.call(()),
                                1 => on_close_key.call(()),
                                2 => on_toggle_key.call(()),
                                3 => on_refresh_key.call(()),
                                _ => {}
                            }
                            e.stop_propagation();
                        }
                        ListNavAction::Dismiss => {
                            on_close_key.call(());
                            e.stop_propagation();
                        }
                        _ => {}
                    }
                }
            })
            .child(
                // Row 0: Account 1 (active/checked)
                rect()
                    .a11y_role(AccessibilityRole::MenuItemRadio)
                    .a11y_alt("Account 1")
                    .width(Size::fill())
                    .padding(Gaps::new(7.0, 10.0, 7.0, 10.0))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .background(if cur == 0 { active_bg } else { Color::TRANSPARENT })
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| active_idx.set_if_modified(0))
                    .on_press({
                        let close = on_close.clone();
                        move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            close.call(());
                        }
                    })
                    .child(rect().width(Size::px(14.0)).height(Size::px(14.0)).center())
                    .child(
                        label()
                            .text("Account 1")
                            .font_size(12.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(text_col),
                    ),
            )
            .child(
                // 0.5px separator under Account 1 (`P3-04`)
                rect()
                    .width(Size::fill())
                    .height(Size::px(0.5))
                    .margin(Gaps::new(4.0, 6.0, 4.0, 6.0))
                    .background(sep_col),
            )
            .child(
                // Row 1: + Add new account (full primary Text color, `P3-04`)
                rect()
                    .a11y_role(AccessibilityRole::MenuItem)
                    .a11y_alt("Add new account")
                    .width(Size::fill())
                    .padding(Gaps::new(7.0, 10.0, 7.0, 10.0))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .background(if cur == 1 { active_bg } else { Color::TRANSPARENT })
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| active_idx.set_if_modified(1))
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        on_close.call(());
                    })
                    .child(Icon::new("plus").size(14.0).role(ThemeRole::Text))
                    .child(label().text("Add new account").font_size(12.0).color(text_col)),
            )
            .child(
                // Row 2: Same as regular browsing (primary Text color, matching `account-menu-dark.png`)
                rect()
                    .a11y_role(AccessibilityRole::MenuItemCheckBox)
                    .a11y_alt("Same as regular browsing")
                    .width(Size::fill())
                    .padding(Gaps::new(7.0, 10.0, 7.0, 10.0))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .background(if cur == 2 { active_bg } else { Color::TRANSPARENT })
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| active_idx.set_if_modified(2))
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        on_toggle_same.call(());
                    })
                    .child(
                        rect()
                            .width(Size::px(14.0))
                            .height(Size::px(14.0))
                            .center()
                            .maybe_child(
                                (!self.same_as_browser).then(|| Icon::new("check").size(12.0).role(ThemeRole::Text)),
                            ),
                    )
                    .child(label().text("Same as regular browsing").font_size(12.0).color(text_col)),
            )
            .child(
                // Row 3: Refresh logins from browser
                rect()
                    .a11y_role(AccessibilityRole::MenuItem)
                    .a11y_alt("Refresh logins from browser")
                    .width(Size::fill())
                    .padding(Gaps::new(7.0, 10.0, 7.0, 10.0))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .background(if cur == 3 { active_bg } else { Color::TRANSPARENT })
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| active_idx.set_if_modified(3))
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        on_refresh.call(());
                    })
                    .child(header_svg("zendia-webchat-refresh-logins", RELOAD_SVG, 14.0, text_col))
                    .child(
                        label()
                            .text("Refresh logins from browser")
                            .font_size(12.0)
                            .color(text_col),
                    ),
            );

        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_absolute().top(88.0).left(self.left))
            .child(PopupPresence::inline(self.open).child(menu_card))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Centered waiting emblem (`44×44 px`, `P3-02`) that pulses (`1.4 s` ease-in-out) while loading and fades out
/// (`220 ms`) once hidden, stopping redraws completely when no longer visible.
#[derive(Clone, PartialEq)]
struct WaitingEmblem {
    site_id: String,
    color: Color,
    visible: bool,
    pulsing: bool,
}

impl Component for WaitingEmblem {
    fn render(&self) -> impl IntoElement {
        let fade = use_animation_with_dependencies(&self.visible, |conf, shown| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *shown { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::WAITING_FADE_OUT)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let pulse_active = self.visible && self.pulsing;
        let pulse = use_animation_with_dependencies(&pulse_active, |conf, active| {
            if *active {
                conf.on_creation(OnCreation::Run);
                conf.on_change(OnChange::Rerun);
                conf.on_finish(OnFinish::reverse());
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(0.45, 1.0)
                .duration(motion::WAITING_PULSE)
                .ease(Ease::InOut)
                .function(Function::Sine)
        });

        let fade_alpha = fade.get().value().clamp(0.0, 1.0);
        if !self.visible && fade_alpha <= 0.01 {
            return rect().into_element();
        }
        let pulse_alpha = if pulse_active {
            pulse.get().value().clamp(0.45, 1.0)
        } else {
            1.0
        };

        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .interactive(Interactive::No)
            .center()
            .opacity(fade_alpha * pulse_alpha)
            .child(chat_site_logo(&self.site_id, 44.0, self.color))
            .into_element()
    }
}
