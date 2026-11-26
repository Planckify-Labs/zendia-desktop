/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 3 Address bar & Command palette gallery section covering every state in light and dark themes:
//! floating empty, floating results ("Architecture"), mixed results (URL/history/bookmark/tab/action),
//! Actions mode unfiltered/filtered/no-match, and Site Data Panel (not-secure HTTP, secure HTTPS with permissions,
//! and internal `about:` page).

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::address_bar::{
    AddressBarMode, FloatingAddressBarCard, SiteDataPanelCard, try_use_address_bar_state,
};
use crate::ui::core_view::use_core;
use crate::ui::kit::{KitChip, Pill};
use crate::ui::theme::{ResolvedScheme, ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{SIDEBAR_DEFAULT_WIDTH, SidebarSide, SpaceId, TabId, ToolbarLayout};
use zendia_browser_core::site_info::{PermissionState, SiteDataSummary, SiteInfo, SitePermission, SitePermissionKind};
use zendia_module_api::ui::UiTone;

#[derive(Clone)]
pub struct AddressBarSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
}

impl PartialEq for AddressBarSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

fn configure_address_bar_fixture(state_id: &str, gallery_core: &GalleryCore) {
    let stub = gallery_core.stub();
    let mut model = super::gallery_sample_model();
    model.layout.toolbar_layout = ToolbarLayout::Single;
    model.layout.sidebar_side = SidebarSide::Left;
    model.layout.sidebar_expanded = true;
    model.layout.sidebar_width = SIDEBAR_DEFAULT_WIDTH;

    match state_id {
        "site-not-secure" => {
            // Active tab on local HTTP page matching the reference shot (not-secure red badge, tracking protection on).
            if let Some(tab) = model.tabs.iter_mut().find(|t| t.id == TabId(1)) {
                tab.title = "Inbox — Workspace".to_string();
                tab.url = "http://127.0.0.1:8080/".to_string();
            }
            model.active_tab = Some(TabId(1));
            if let Some(window) = model.windows.first_mut() {
                window.active_tab = Some(TabId(1));
            }
            // Keep extensions empty only if needed? No — show the sample extension contribution as the plan specifies!
            let mut http_site = SiteInfo::for_url("http://127.0.0.1:8080/");
            http_site.tracking_protection_enabled = true;
            model.site_info.insert(http_site.origin.clone(), http_site);
            stub.replace_model(model);
        }
        "site-secure-perms" => {
            // Active tab on https://zendia.local/docs with permissions granted/blocked, cookies, and bookmark.
            model.active_tab = Some(TabId(6));
            if let Some(space) = model.spaces.iter_mut().find(|s| s.id == SpaceId(1)) {
                space.last_active_tab = Some(TabId(6));
            }
            if let Some(window) = model.windows.first_mut() {
                window.active_tab = Some(TabId(6));
            }
            let mut secure_site = SiteInfo::for_url("https://zendia.local/docs");
            secure_site.bookmarked = true;
            secure_site.tracking_protection_enabled = true;
            secure_site.reader_mode_available = true;
            secure_site.data = SiteDataSummary {
                cookie_count: 4,
                storage_bytes: 128 * 1024,
            };
            secure_site.permissions = vec![
                SitePermission {
                    kind: SitePermissionKind::Camera,
                    state: PermissionState::Allow,
                },
                SitePermission {
                    kind: SitePermissionKind::Location,
                    state: PermissionState::Block,
                },
                SitePermission {
                    kind: SitePermissionKind::Notifications,
                    state: PermissionState::Allow,
                },
            ];
            model.site_info.insert(secure_site.origin.clone(), secure_site);
            stub.replace_model(model);
        }
        "site-internal" => {
            // Active tab on an internal about:newtab page.
            if let Some(tab) = model.tabs.iter_mut().find(|t| t.id == TabId(12)) {
                tab.title = "New Tab".to_string();
                tab.url = "about:newtab".to_string();
            }
            model.active_tab = Some(TabId(12));
            if let Some(window) = model.windows.first_mut() {
                window.active_tab = Some(TabId(12));
            }
            stub.replace_model(model);
        }
        _ => {
            // Set active tab to Inbox (TabId(1)) so "Architecture — Zendia" appears as a Switch to Tab result.
            if let Some(tab) = model.tabs.iter_mut().find(|t| t.id == TabId(1)) {
                tab.title = "Inbox — Workspace".to_string();
                tab.url = "http://127.0.0.1:8080/".to_string();
            }
            if let Some(tab) = model.tabs.iter_mut().find(|t| t.id == TabId(2)) {
                tab.title = "Architecture — Zendia".to_string();
            }
            model.active_tab = Some(TabId(1));
            if let Some(window) = model.windows.first_mut() {
                window.active_tab = Some(TabId(1));
            }
            stub.replace_model(model);
        }
    }
}

fn initial_preview_config(state_id: &str) -> (String, AddressBarMode, Option<usize>) {
    match state_id {
        "floating-empty" => (String::new(), AddressBarMode::Normal { new_tab: true }, Some(1)),
        "floating-results" => (
            "Architecture".to_string(),
            AddressBarMode::Normal { new_tab: false },
            Some(0),
        ),
        "results-mixed" => (
            "zendia.local".to_string(),
            AddressBarMode::Normal { new_tab: false },
            Some(0),
        ),
        "actions-unfiltered" => (String::new(), AddressBarMode::Actions, Some(1)),
        "actions-filtered" => ("tab".to_string(), AddressBarMode::Actions, Some(0)),
        "actions-empty" => ("zzzzzz".to_string(), AddressBarMode::Actions, None),
        _ => (String::new(), AddressBarMode::Normal { new_tab: false }, None),
    }
}

fn mock_workspace_page(theme_dark: bool, active_title: &str) -> Element {
    let page_bg = if theme_dark {
        Color::from_rgb(18, 20, 28)
    } else {
        Color::from_rgb(245, 246, 250)
    };
    let header_border = if theme_dark {
        Color::from_argb(24, 255, 255, 255)
    } else {
        Color::from_argb(22, 0, 0, 0)
    };
    let card_bg = if theme_dark {
        Color::from_rgb(26, 29, 42)
    } else {
        Color::from_rgb(255, 255, 255)
    };
    let text = if theme_dark {
        Color::from_rgb(236, 238, 246)
    } else {
        Color::from_rgb(22, 24, 34)
    };
    let muted = if theme_dark {
        Color::from_rgb(148, 154, 174)
    } else {
        Color::from_rgb(104, 110, 130)
    };

    rect()
        .width(Size::fill())
        .height(Size::fill())
        .background(page_bg)
        .child(
            rect()
                .width(Size::fill())
                .height(Size::px(54.0))
                .padding(Gaps::new(0.0, 28.0, 0.0, 28.0))
                .direction(Direction::Horizontal)
                .main_align(Alignment::SpaceBetween)
                .cross_align(Alignment::center())
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(header_border),
                )
                .child(
                    label()
                        .text(active_title.to_string())
                        .font_size(14.0)
                        .font_weight(FontWeight::BOLD)
                        .color(text),
                )
                .child(
                    rect()
                        .direction(Direction::Horizontal)
                        .spacing(20.0)
                        .child(label().text("Documentation").font_size(12.5).color(muted))
                        .child(label().text("Architecture").font_size(12.5).color(muted))
                        .child(label().text("Releases").font_size(12.5).color(muted))
                        .child(label().text("Design System").font_size(12.5).color(muted)),
                ),
        )
        .child(
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .padding(Gaps::new(56.0, 140.0, 32.0, 160.0))
                .spacing(12.0)
                .child(
                    label()
                        .text("Team Updates")
                        .font_size(26.0)
                        .font_weight(FontWeight::BOLD)
                        .color(text),
                )
                .child(
                    label()
                        .text("3 unread messages in Engineering.")
                        .font_size(13.5)
                        .color(muted),
                )
                .child(
                    rect()
                        .width(Size::px(520.0))
                        .margin(Gaps::new(12.0, 0.0, 0.0, 0.0))
                        .padding(Gaps::new(22.0, 24.0, 22.0, 24.0))
                        .spacing(10.0)
                        .corner_radius(CornerRadius::new_all(12.0))
                        .background(card_bg)
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(header_border),
                        )
                        .child(
                            label()
                                .text("Weekly Sync Notes")
                                .font_size(17.0)
                                .font_weight(FontWeight::BOLD)
                                .color(text),
                        )
                        .child(
                            label()
                                .text("Address bar providers, command palette fuzzy ranking, and site controls are ready for review.")
                                .font_size(13.0)
                                .color(muted),
                        ),
                ),
        )
        .into_element()
}

impl Component for AddressBarSection {
    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.state_id)
    }

    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let addr_state = try_use_address_bar_state();

        let (init_query, init_mode, init_highlight) = initial_preview_config(&self.state_id);
        let mut preview_query = use_state(move || init_query);
        let mut preview_mode = use_state(move || init_mode);
        let mut preview_highlighted = use_state(move || init_highlight);

        let mut applied_state = use_state(String::new);
        if *applied_state.peek() != self.state_id {
            applied_state.set(self.state_id.clone());
            if let Some(state) = addr_state {
                state.close();
                state.close_site_panel();
            }
            configure_address_bar_fixture(&self.state_id, &self.gallery_core);
        }

        let (active_title, active_url, site_info, extensions, search_engine_name) = {
            let model = core.model().read();
            let active = model.active_tab();
            (
                active
                    .map(|t| t.display_title())
                    .unwrap_or_else(|| "Inbox — Workspace".to_string()),
                active
                    .map(|t| t.url.clone())
                    .unwrap_or_else(|| "about:blank".to_string()),
                model.active_site_info(),
                model.site_extensions.clone(),
                model.search_engine.name.clone(),
            )
        };

        let mut location = use_state(|| active_url.clone());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut shown_url = use_state(|| active_url.clone());
        if *shown_url.peek() != active_url {
            shown_url.set(active_url.clone());
            location.set(active_url.clone());
        }

        let is_site_state = self.state_id.starts_with("site-");
        let open_bar_core = core.clone();
        let open_palette_core = core.clone();
        let toggle_site_core = core.clone();
        let reset_core = Arc::clone(&self.gallery_core);
        let reset_state_id = self.state_id.clone();

        let info_strip = rect()
            .width(Size::fill())
            .padding(Gaps::new(6.0, 12.0, 6.0, 12.0))
            .direction(Direction::Horizontal)
            .content(Content::wrap())
            .cross_align(Alignment::center())
            .main_align(Alignment::SpaceBetween)
            .spacing(theme.spacing.sm)
            .background(theme.color(ThemeRole::SurfaceRaised))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(theme.spacing.sm)
                    .child(Pill::new(format!("Engine: {search_engine_name}")).tone(UiTone::Accent))
                    .child(Pill::new(format!("Security: {}", site_info.security.label())).tone(
                        if site_info.security.is_warning() {
                            UiTone::Danger
                        } else {
                            UiTone::Success
                        },
                    )),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(theme.spacing.xs)
                    .child(KitChip::new("Open Floating Bar").on_press(move |_| {
                        open_bar_core.invoke_or_log(Command::AddressBarOpen {
                            new_tab: true,
                            initial_query: None,
                        });
                    }))
                    .child(KitChip::new("Open Palette").on_press(move |_| {
                        open_palette_core.invoke_or_log(Command::PaletteOpen { initial_query: None });
                    }))
                    .child(KitChip::new("Toggle Site Panel").on_press(move |_| {
                        toggle_site_core.invoke_or_log(Command::SitePanelToggle);
                    }))
                    .child(KitChip::new("Reset State").on_press(move |_| {
                        configure_address_bar_fixture(&reset_state_id, &reset_core);
                        let (q, m, h) = initial_preview_config(&reset_state_id);
                        preview_query.set(q);
                        preview_mode.set(m);
                        preview_highlighted.set(h);
                    })),
            );

        let page = mock_workspace_page(theme.scheme == ResolvedScheme::Dark, &active_title);

        let stage_overlay: Element = if is_site_state {
            // Anchored directly beside the sidebar LocationBar's site-data button (matching reference shot).
            // Lift by +64 relative layers so the preview paints above WindowShell's deep page subtree while staying
            // clipped to the gallery stage frame.
            rect()
                .layer(Layer::Relative(64))
                .position(Position::new_absolute().left(196.0).top(46.0))
                .child(SiteDataPanelCard::new(site_info, extensions))
                .into_element()
        } else {
            // Centered floating address bar / Actions palette over the window shell (no dimming scrim, as in Zen).
            let current_mode = *preview_mode.read();
            rect()
                .layer(Layer::Relative(64))
                .position(Position::new_absolute().top(0.0).left(0.0))
                .width(Size::percent(100.0))
                .height(Size::percent(100.0))
                .padding(Gaps::new(190.0, 0.0, 0.0, 0.0))
                .main_align(Alignment::Start)
                .cross_align(Alignment::center())
                .child(
                    FloatingAddressBarCard::new(preview_query, preview_highlighted, current_mode)
                        .width(720.0)
                        .on_enter_actions(move |_| {
                            preview_mode.set(AddressBarMode::Actions);
                            preview_highlighted.set(Some(0));
                        })
                        .on_exit_actions(move |_| {
                            preview_mode.set(AddressBarMode::Normal { new_tab: false });
                            preview_highlighted.set(None);
                        }),
                )
                .into_element()
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .padding(Gaps::new_all(12.0))
            .spacing(10.0)
            .child(info_strip)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .corner_radius(CornerRadius::new_all(10.0))
                    .overflow(Overflow::Clip)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.color(ThemeRole::Border)),
                    )
                    .child(WindowShell::new(location, location_a11y_id, page).interactive_window_controls(false))
                    .child(stage_overlay),
            )
    }
}
