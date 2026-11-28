/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Anchored Site Data Panel popover matching Zen's `#zen-unified-site-data-panel`:
//! top action row (Share, Screenshot, Bookmark — mirrored on left single-toolbar; no Boosts per user decision),
//! Extensions tile grid (`44x34px` tiles with badge counts, right-click Pin/Manage/Remove menu, `+` add tile last,
//! overflow scroll + bottom fade), Settings rows (Tracking Protection, site permissions, cookies/site data),
//! and footer (clickable security pill, optional Reader Mode button, and `...` actions popup menu).

use crate::ui::chrome::address_bar::state::use_address_bar_state;
use crate::ui::chrome::menus::AnchorRect;
use crate::ui::chrome::menus::state::clamp_popup_origin;
use crate::ui::core_view::use_core;
use crate::ui::kit::icons::prepared_icon_bytes;
use crate::ui::kit::{
    Icon, KitTooltip, PopupContent, PopupHorizontalAlign, PopupPresence, anchor_popup_origin, focus_ring_border,
};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, Rgba, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{ServiceId, ServiceStatus, SidebarSide};
use zendia_browser_core::modules::{ExtensionId, SAMPLE_EXTENSION_ID, service_status_in};
use zendia_browser_core::settings::SettingsGroupId;
use zendia_browser_core::site_info::{PermissionState, SiteExtension, SiteInfo, SiteSecurityState};

/// Matches Zen's `#zen-unified-site-data-panel { --panel-width: 220px; }`.
const SITE_PANEL_WIDTH: f32 = 220.0;
/// Matches Zen's `#zen-site-data-addons[overflowing="true"] { max-height: 420px; }`.
const EXTENSIONS_MAX_HEIGHT: f32 = 420.0;
/// Exact tile dimensions from `#zen-site-data-addons > *`: `44px` wide × `34px` (`--size-item-large`) tall, `8px` gap.
/// Four tiles + three gaps = `44*4 + 8*3 = 200px`, fitting inside `220 - 2*9 = 202px`.
const EXT_TILE_WIDTH: f32 = 44.0;
const EXT_TILE_HEIGHT: f32 = 34.0;
const EXT_TILE_GAP: f32 = 8.0;
const SECTION_LINK_FADE: Duration = Duration::from_millis(150);
const EXT_MENU_WIDTH: f32 = 188.0;
const EXT_MENU_HEIGHT: f32 = 108.0;
const MORE_MENU_WIDTH: f32 = 216.0;
const MORE_MENU_HEIGHT: f32 = 152.0;

/// Computes the global `(x, y)` origin for the Site Data Panel anchored to `anchor`, clamped inside `window_size`.
pub fn compute_site_panel_origin(anchor: AnchorRect, panel_size: (f32, f32), window_size: (f32, f32)) -> (f32, f32) {
    let desired_x = if anchor.x > window_size.0 * 0.5 {
        // Right-side sidebar / trailing toolbar: right-align under the URL bar's site button.
        anchor.x + anchor.width + 6.0 - panel_size.0
    } else {
        // Left-side sidebar / leading toolbar: align panel left edge to the site button.
        anchor.x
    };
    let desired = (desired_x, anchor.y + anchor.height + 6.0);
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
        let mut open_seq = use_state(|| 0u64);
        let mut was_open = use_state(|| is_open);
        if *was_open.peek() != is_open {
            was_open.set(is_open);
            if is_open {
                let next = open_seq.peek().wrapping_add(1);
                open_seq.set(next);
            }
        }

        let (site_info, extensions) = {
            let model = core.model().read();
            (model.active_site_info(), model.site_extensions.clone())
        };

        let root_size = *Platform::get().current_window().root_size.read();
        let window_size = (root_size.width.max(320.0), root_size.height.max(240.0));
        let ext_rows = ((extensions.len() + 1) as f32 / 4.0).ceil().max(1.0);
        let ext_h = (ext_rows * (EXT_TILE_HEIGHT + EXT_TILE_GAP)).min(EXTENSIONS_MAX_HEIGHT);
        let est_height = 200.0 + ext_h + site_info.visible_permissions().len() as f32 * 42.0;
        let origin = compute_site_panel_origin(anchor, (SITE_PANEL_WIDTH, est_height), window_size);
        let seq = *open_seq.read();

        PopupPresence::anchored(is_open, origin)
            .open_seq(seq)
            .on_escape(move |_| state.close_site_panel())
            .on_backdrop_mouse_down(move |_| state.close_site_panel())
            .child(
                PopupContent::new(seq)
                    .child(SiteDataPanelCard::new(site_info, extensions).on_close(move |_| state.close_site_panel())),
            )
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

/// Computes Zen's `--zen-sidebar-themed-icon-fill` color (used on active bookmark tile and enabled setting circles).
fn themed_icon_fill(theme: &crate::ui::theme::ResolvedTheme) -> (Color, Color) {
    match theme.scheme {
        ResolvedScheme::Dark => {
            // color-mix(in srgb, var(--zen-colors-primary) 15%, #ebebeb) -> crisp light circle + dark ink glyph.
            (Color::from_rgb(235, 235, 238), Color::from_rgb(24, 26, 36))
        }
        ResolvedScheme::Light => {
            // color-mix(in srgb, var(--zen-primary-color) 50%, black) -> deep navy-blue circle + white glyph.
            let fill = theme.palette.blue.mix(Rgba::rgb(0, 0, 0), 0.50).to_color();
            (fill, Color::WHITE)
        }
    }
}

/// Returns the foreground icon color for an extension tile (uBlock uses its signature crimson shield).
fn extension_icon_color(ext: &SiteExtension, theme: &crate::ui::theme::ResolvedTheme) -> Color {
    let is_ublock = ext.extension_id().is_some_and(|id| id.as_str() == SAMPLE_EXTENSION_ID)
        || ext.name.to_ascii_lowercase().contains("ublock");
    if is_ublock {
        if theme.scheme.is_dark() {
            Color::from_rgb(212, 62, 62)
        } else {
            Color::from_rgb(138, 18, 22)
        }
    } else if theme.scheme.is_dark() {
        Color::from_argb(232, 255, 255, 255)
    } else {
        Color::from_argb(220, 24, 26, 36)
    }
}

impl Component for SiteDataPanelCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut more_open = use_state(|| false);
        let mut more_hovered = use_state(|| false);
        let mut more_pressed = use_state(|| false);
        let mut more_anchor = use_state(AnchorRect::default);
        let mut security_open = use_state(|| false);
        let mut security_hovered = use_state(|| false);
        let mut security_pressed = use_state(|| false);
        let mut reader_hovered = use_state(|| false);
        let mut reader_pressed = use_state(|| false);
        let mut ext_section_hovered = use_state(|| false);
        let mut settings_section_hovered = use_state(|| false);
        let mut manage_hovered = use_state(|| false);
        let mut settings_more_hovered = use_state(|| false);
        // Active right-click context menu for an extension tile: (extension_id, extension_name, anchor).
        let mut ext_ctx_menu = use_state(|| None::<(ExtensionId, String, AnchorRect)>);
        let sec_a11y = use_a11y();
        let reader_a11y = use_a11y();
        let more_a11y = use_a11y();
        let sec_focus = use_focus(sec_a11y);
        let reader_focus = use_focus(reader_a11y);
        let more_focus = use_focus(more_a11y);
        let mut vim_cursor = use_state(|| 0usize);
        let mut vim_kbd_active = use_state(|| false);
        let mut sub_cursor = use_state(|| 0usize);

        let ext_hover_state = *ext_section_hovered.read();
        let ext_link_anim = use_animation_with_dependencies(&ext_hover_state, move |conf, hovered| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *hovered { (0.0, 0.5) } else { (0.5, 0.0) };
            AnimNum::new(from, to)
                .duration(SECTION_LINK_FADE)
                .ease(Ease::InOut)
                .function(motion::ease_out_fn())
        });
        let settings_hover_state = *settings_section_hovered.read();
        let settings_link_anim = use_animation_with_dependencies(&settings_hover_state, move |conf, hovered| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *hovered { (0.0, 0.5) } else { (0.5, 0.0) };
            AnimNum::new(from, to)
                .duration(SECTION_LINK_FADE)
                .ease(Ease::InOut)
                .function(motion::ease_out_fn())
        });

        let (ext_service_status, reverse_header) = {
            let model = core.model().read();
            let status = service_status_in(&model.services, ServiceId::ExtensionServer);
            let reverse = model.layout.sidebar_side == SidebarSide::Left && !model.layout.has_top_toolbar();
            (status, reverse)
        };

        // Exact `--arrowpanel-background` from `zen-theme.css:260`: light `rgb(244, 244, 244)`, dark `rgb(31, 31, 31)`.
        let panel_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_rgb(31, 31, 31),
            ResolvedScheme::Light => Color::from_rgb(244, 244, 244),
        };
        let border_color = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.11).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.10).to_color(),
        };
        let divider_color = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.06).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.06).to_color(),
        };
        let primary_text = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(235, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(228, 22, 24, 34),
        };
        let muted_text = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(153, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(153, 22, 24, 34),
        };
        let link_hover_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.10).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.10).to_color(),
        };

        let is_web = self.site_info.security.is_web_page();
        let origin = self.site_info.origin.clone();
        let root_size = *Platform::get().current_window().root_size.read();
        let window_size = (root_size.width.max(320.0), root_size.height.max(240.0));

        // Top action row (`#zen-site-data-header`): Share, Screenshot, Bookmark (Boosts omitted per user decision).
        // Mirrors order on left single-toolbar via `flex-direction: row-reverse` (`zen-single-components.css:570-572`).
        let share_core = core.clone();
        let shot_core = core.clone();
        let bookmark_core = core.clone();
        let close_share = self.on_close.clone();
        let close_shot = self.on_close.clone();

        let mut header_tiles = vec![
            SiteActionTile {
                icon: "share",
                label: "Share This Page",
                active: false,
                enabled: is_web,
                on_press: EventHandler::new(move |_| {
                    share_core.invoke_or_log(Command::ShareOpen {
                        tab_id: None,
                        space_id: None,
                        folder_id: None,
                    });
                    if let Some(close) = &close_share {
                        close.call(());
                    }
                }),
                key: DiffKey::from(&"site-act-share"),
            }
            .into_element(),
            SiteActionTile {
                icon: "screenshot",
                label: "Take a Screenshot",
                active: false,
                enabled: true,
                on_press: EventHandler::new(move |_| {
                    shot_core.invoke_or_log(Command::SiteScreenshot { tab_id: None });
                    if let Some(close) = &close_shot {
                        close.call(());
                    }
                }),
                key: DiffKey::from(&"site-act-screenshot"),
            }
            .into_element(),
            SiteActionTile {
                icon: if self.site_info.bookmarked {
                    "bookmark"
                } else {
                    "bookmark-hollow"
                },
                label: if self.site_info.bookmarked {
                    "Remove Bookmark"
                } else {
                    "Bookmark This Page"
                },
                active: self.site_info.bookmarked,
                enabled: is_web,
                on_press: EventHandler::new(move |_| {
                    bookmark_core.invoke_or_log(Command::SiteBookmarkToggle { tab_id: None });
                }),
                key: DiffKey::from(&"site-act-bookmark"),
            }
            .into_element(),
        ];
        if reverse_header {
            header_tiles.reverse();
        }

        let action_row = rect()
            .width(Size::fill())
            .padding(Gaps::new(10.0, 9.0, 8.0, 9.0))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .spacing(8.0)
            .children(header_tiles);

        // Extensions section (`#zen-site-data-section-addons`): wrapping row of 44x34px tiles, badge count on icon,
        // right-click context menu (Pin to Toolbar / Manage / Remove), and `+` tile always last.
        let add_ext_core = core.clone();
        let manage_ext_core = core.clone();
        let close_add_ext = self.on_close.clone();
        let close_manage_ext = self.on_close.clone();
        let extensions_paused = match &ext_service_status {
            ServiceStatus::Running => false,
            ServiceStatus::Stopped { reason } if reason.as_deref() == Some("stub mode") => false,
            _ => true,
        };

        let ext_tiles: Vec<Element> = self
            .extensions
            .iter()
            .map(|ext| {
                let ext_core = core.clone();
                let ext_id = ext.extension_id();
                let ext_cmd = ext.command.clone();
                let close_ext = self.on_close.clone();
                let ctx_ext_id = ext_id.clone();
                let ctx_ext_name = ext.name.clone();
                SiteExtensionTile {
                    extension: ext.clone(),
                    paused: extensions_paused,
                    usable: ext.enabled && !extensions_paused,
                    on_press: EventHandler::new(move |_| {
                        if let Some(id) = &ext_id {
                            ext_core.invoke_or_log(Command::ExtensionInvokeAction {
                                extension_id: id.clone(),
                            });
                        } else if let Ok(parsed) = Command::from_name_and_args(&ext_cmd, &[]) {
                            ext_core.invoke_or_log(parsed);
                        }
                        if let Some(close) = &close_ext {
                            close.call(());
                        }
                    }),
                    on_context_menu: EventHandler::new(move |anchor: AnchorRect| {
                        if let Some(id) = &ctx_ext_id {
                            more_open.set_if_modified(false);
                            ext_ctx_menu.set(Some((id.clone(), ctx_ext_name.clone(), anchor)));
                        }
                    }),
                    key: DiffKey::from(&ext.id.0),
                }
                .into_element()
            })
            .collect();

        let ext_grid_inner = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::wrap())
            .spacing(EXT_TILE_GAP)
            .children(ext_tiles)
            .child(SiteAddExtensionTile {
                on_press: EventHandler::new(move |_| {
                    add_ext_core.invoke_or_log(Command::SettingsOpen {
                        group: Some(SettingsGroupId::Extensions),
                    });
                    if let Some(close) = &close_add_ext {
                        close.call(());
                    }
                }),
            });

        // `#zen-site-data-addons[overflowing="true"]`: cap at 420px with scrolling and subtle bottom fade mask.
        let estimated_ext_rows = ((self.extensions.len() + 1) as f32 / 4.0).ceil();
        let ext_overflows = estimated_ext_rows * (EXT_TILE_HEIGHT + EXT_TILE_GAP) > EXTENSIONS_MAX_HEIGHT;
        let ext_grid: Element = if ext_overflows {
            let fade_grad = LinearGradient::new()
                .angle(180.0)
                .stop((
                    Rgba::from_rgb_alpha(panel_bg.r(), panel_bg.g(), panel_bg.b(), 0.0).to_color(),
                    0.0,
                ))
                .stop((panel_bg, 100.0));
            rect()
                .width(Size::fill())
                .height(Size::px(EXTENSIONS_MAX_HEIGHT))
                .child(
                    ScrollView::new()
                        .width(Size::fill())
                        .height(Size::fill())
                        .child(ext_grid_inner),
                )
                .child(
                    rect()
                        .position(Position::new_absolute().bottom(0.0).left(0.0).right(0.0))
                        .width(Size::percent(100.0))
                        .height(Size::px(22.0))
                        .background(fade_grad)
                        .interactive(false),
                )
                .into_element()
        } else {
            ext_grid_inner.into_element()
        };

        let ext_manage_opacity = if *manage_hovered.read() {
            0.95
        } else {
            (0.55 + ext_link_anim.get().value().clamp(0.0, 0.4)).clamp(0.55, 0.95)
        };
        let extensions_section = rect()
            .width(Size::fill())
            .padding(Gaps::new(9.0, 9.0, 8.0, 9.0))
            .spacing(8.0)
            .on_pointer_over(move |_| ext_section_hovered.set_if_modified(true))
            .on_pointer_out(move |_| ext_section_hovered.set_if_modified(false))
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::SpaceBetween)
                    .cross_align(Alignment::center())
                    .child(
                        label()
                            .text("Extensions")
                            .font_size(13.0)
                            .font_weight(FontWeight::MEDIUM)
                            .color(primary_text),
                    )
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(4.0)
                            .maybe_child(extensions_paused.then(|| {
                                let status_label = if matches!(ext_service_status, ServiceStatus::Restarting) {
                                    "Restarting"
                                } else {
                                    "Paused"
                                };
                                rect()
                                    .padding(Gaps::new(1.5, 6.0, 1.5, 6.0))
                                    .corner_radius(CornerRadius::new_all(999.0))
                                    .background(theme.rgba(ThemeRole::Warning).with_alpha(0.18).to_color())
                                    .child(
                                        label()
                                            .text(status_label)
                                            .font_size(10.0)
                                            .font_weight(FontWeight::SEMI_BOLD)
                                            .color(theme.color(ThemeRole::Warning)),
                                    )
                            }))
                            .maybe_child((ext_manage_opacity > 0.01).then(|| {
                                rect()
                                    .padding(Gaps::new(2.0, 6.0, 2.0, 6.0))
                                    .corner_radius(CornerRadius::new_all(6.0))
                                    .background(if *manage_hovered.read() {
                                        link_hover_bg
                                    } else {
                                        Color::TRANSPARENT
                                    })
                                    .opacity(ext_manage_opacity)
                                    .cursor(CursorIcon::Pointer)
                                    .a11y_role(AccessibilityRole::Button)
                                    .a11y_alt("Manage extensions")
                                    .on_pointer_enter(move |_| manage_hovered.set_if_modified(true))
                                    .on_pointer_leave(move |_| manage_hovered.set_if_modified(false))
                                    .on_press(move |e: Event<PressEventData>| {
                                        e.stop_propagation();
                                        manage_ext_core.invoke_or_log(Command::SettingsOpen {
                                            group: Some(SettingsGroupId::Extensions),
                                        });
                                        if let Some(close) = &close_manage_ext {
                                            close.call(());
                                        }
                                    })
                                    .child(
                                        label()
                                            .text("Manage")
                                            .font_size(11.0)
                                            .font_weight(FontWeight::MEDIUM)
                                            .color(primary_text),
                                    )
                            })),
                    ),
            )
            .child(ext_grid);

        // Settings section (`#zen-site-data-settings-list`): Tracking Protection + permissions + cookies/site data.
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

        let settings_more_core = core.clone();
        let close_settings_more = self.on_close.clone();
        let settings_more_opacity = if *settings_more_hovered.read() {
            0.85
        } else {
            settings_link_anim.get().value().clamp(0.0, 1.0)
        };
        let settings_section = rect()
            .width(Size::fill())
            .padding(Gaps::new(9.0, 9.0, 8.0, 9.0))
            .spacing(8.0)
            .on_pointer_over(move |_| settings_section_hovered.set_if_modified(true))
            .on_pointer_out(move |_| settings_section_hovered.set_if_modified(false))
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::SpaceBetween)
                    .cross_align(Alignment::center())
                    .child(
                        label()
                            .text("Settings")
                            .font_size(13.0)
                            .font_weight(FontWeight::MEDIUM)
                            .color(primary_text),
                    )
                    .maybe_child((settings_more_opacity > 0.01).then(|| {
                        rect()
                            .padding(Gaps::new(2.0, 6.0, 2.0, 6.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .background(if *settings_more_hovered.read() {
                                link_hover_bg
                            } else {
                                Color::TRANSPARENT
                            })
                            .opacity(settings_more_opacity)
                            .cursor(CursorIcon::Pointer)
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("More site settings")
                            .on_pointer_enter(move |_| settings_more_hovered.set_if_modified(true))
                            .on_pointer_leave(move |_| settings_more_hovered.set_if_modified(false))
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                settings_more_core.invoke_or_log(Command::SettingsOpen { group: None });
                                if let Some(close) = &close_settings_more {
                                    close.call(());
                                }
                            })
                            .child(
                                label()
                                    .text("More")
                                    .font_size(11.0)
                                    .font_weight(FontWeight::MEDIUM)
                                    .color(primary_text),
                            )
                    })),
            )
            .maybe_child(tp_row)
            .children(perm_rows)
            .maybe_child(data_row)
            .maybe_child(internal_note);

        // Footer (`#zen-site-data-footer`): Security pill (hidden on internal pages) + optional Reader Mode + `...`.
        let show_security_badge = self.site_info.security != SiteSecurityState::Internal;
        let is_warning = self.site_info.security.is_warning();
        let interactive_rest = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.055).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.055).to_color(),
        };
        let interactive_hover = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.105).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.105).to_color(),
        };
        let sec_fg = if is_warning {
            match theme.scheme {
                ResolvedScheme::Dark => Color::from_rgb(240, 114, 126),
                ResolvedScheme::Light => Color::from_rgb(196, 36, 62),
            }
        } else {
            match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(215, 255, 255, 255),
                ResolvedScheme::Light => Color::from_argb(210, 22, 24, 34),
            }
        };

        let sec_icon: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(self.site_info.security.icon()) {
            SvgViewer::new((("zendia-icon", canonical), bytes))
                .width(Size::px(15.0))
                .height(Size::px(15.0))
                .color(sec_fg)
                .show_loader(false)
                .into_element()
        } else {
            Icon::new(self.site_info.security.icon())
                .size(15.0)
                .role(if is_warning { ThemeRole::Danger } else { ThemeRole::Text })
                .into_element()
        };

        let drawer_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.06).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.05).to_color(),
        };

        let security_details = (show_security_badge && *security_open.read()).then(|| {
            let detail_text = match self.site_info.security {
                SiteSecurityState::Secure => {
                    "Connection is encrypted (HTTPS). Information sent to this site is private."
                }
                SiteSecurityState::NotSecure => {
                    "Connection is not secure (HTTP). Sensitive information should not be entered here."
                }
                SiteSecurityState::File => "Viewing a local file on this device.",
                SiteSecurityState::Local => "Viewing a local browser resource.",
                SiteSecurityState::Extension => "Page provided by an installed extension.",
                SiteSecurityState::Internal => "Built-in Zendia page.",
            };
            rect()
                .width(Size::fill())
                .padding(Gaps::new(8.0, 9.0, 8.0, 9.0))
                .spacing(4.0)
                .corner_radius(CornerRadius::new_all(8.0))
                .background(drawer_bg)
                .child(
                    label()
                        .text(self.site_info.origin.clone())
                        .font_size(11.5)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(primary_text)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
                .child(label().text(detail_text).font_size(11.0).color(muted_text))
        });

        let security_badge = show_security_badge.then(|| {
            rect()
                .a11y_id(sec_a11y)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(format!(
                    "Connection security: {}. Click for details.",
                    self.site_info.security.label()
                ))
                .height(Size::px(30.0))
                .padding(Gaps::new(0.0, 10.0, 0.0, 8.0))
                .spacing(5.0)
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .corner_radius(CornerRadius::new_all(6.0))
                .background(if *security_hovered.read() || *security_open.read() {
                    interactive_hover
                } else {
                    interactive_rest
                })
                .scale(if *security_pressed.read() { 0.95 } else { 1.0 })
                .border(focus_ring_border(sec_focus() == Focus::Keyboard, &theme, Border::new()))
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| security_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| {
                    security_hovered.set_if_modified(false);
                    security_pressed.set_if_modified(false);
                })
                .on_pointer_down(move |_| security_pressed.set_if_modified(true))
                .on_global_pointer_up(move |_| security_pressed.set_if_modified(false))
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    let next = !*security_open.peek();
                    security_open.set(next);
                    if next {
                        more_open.set_if_modified(false);
                        ext_ctx_menu.set_if_modified(None);
                    }
                })
                .on_key_down(move |e: Event<KeyboardEventData>| {
                    if matches!(e.key, Key::Named(NamedKey::Enter)) || e.key == Key::Character(" ".into()) {
                        e.stop_propagation();
                        e.prevent_default();
                        let next = !*security_open.peek();
                        security_open.set(next);
                        if next {
                            more_open.set_if_modified(false);
                            ext_ctx_menu.set_if_modified(None);
                        }
                    }
                })
                .child(sec_icon)
                .child(
                    label()
                        .text(self.site_info.security.label())
                        .font_size(12.5)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(sec_fg),
                )
        });

        let reader_core = core.clone();
        let close_reader = self.on_close.clone();
        let reader_button = (is_web && self.site_info.reader_mode_available).then(|| {
            KitTooltip::new("Enter Reader Mode").child(
                rect()
                    .a11y_id(reader_a11y)
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Enter Reader Mode")
                    .width(Size::px(30.0))
                    .height(Size::px(30.0))
                    .corner_radius(CornerRadius::new_all(6.0))
                    .background(if *reader_hovered.read() {
                        interactive_hover
                    } else {
                        interactive_rest
                    })
                    .scale(if *reader_pressed.read() { 0.95 } else { 1.0 })
                    .border(focus_ring_border(
                        reader_focus() == Focus::Keyboard,
                        &theme,
                        Border::new(),
                    ))
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| reader_hovered.set_if_modified(true))
                    .on_pointer_leave(move |_| {
                        reader_hovered.set_if_modified(false);
                        reader_pressed.set_if_modified(false);
                    })
                    .on_pointer_down(move |_| reader_pressed.set_if_modified(true))
                    .on_global_pointer_up(move |_| reader_pressed.set_if_modified(false))
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        reader_core.invoke_or_log(Command::SiteReaderModeToggle { tab_id: None });
                        if let Some(close) = &close_reader {
                            close.call(());
                        }
                    })
                    .child(Icon::new("reader-mode").size(16.0).role(ThemeRole::Text)),
            )
        });

        let more_button = rect()
            .a11y_id(more_a11y)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("More site actions")
            .width(Size::px(30.0))
            .height(Size::px(30.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(if *more_hovered.read() || *more_open.read() {
                interactive_hover
            } else {
                interactive_rest
            })
            .scale(if *more_pressed.read() { 0.95 } else { 1.0 })
            .border(focus_ring_border(
                more_focus() == Focus::Keyboard,
                &theme,
                Border::new(),
            ))
            .center()
            .cursor(CursorIcon::Pointer)
            .on_sized(move |e: Event<SizedEventData>| {
                more_anchor.set_if_modified(AnchorRect::from_area(e.area));
            })
            .on_pointer_enter(move |_| more_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| {
                more_hovered.set_if_modified(false);
                more_pressed.set_if_modified(false);
            })
            .on_pointer_down(move |_| more_pressed.set_if_modified(true))
            .on_global_pointer_up(move |_| more_pressed.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                let next = !*more_open.peek();
                more_open.set(next);
                if next {
                    security_open.set_if_modified(false);
                    ext_ctx_menu.set_if_modified(None);
                }
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Enter)) || e.key == Key::Character(" ".into()) {
                    e.stop_propagation();
                    e.prevent_default();
                    let next = !*more_open.peek();
                    more_open.set(next);
                    if next {
                        security_open.set_if_modified(false);
                        ext_ctx_menu.set_if_modified(None);
                    }
                }
            })
            .child(Icon::new("menu").size(15.0).role(ThemeRole::Text));

        let footer = rect()
            .width(Size::fill())
            .padding(Gaps::new(2.0, 8.0, 8.0, 8.0))
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
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .maybe_child(security_badge)
                    .child(rect().width(Size::flex(1.0)))
                    .maybe_child(reader_button)
                    .child(more_button),
            )
            .maybe_child(security_details);

        // Anchored popup menu for `...` (`#zenSiteDataActions` in `popups.inc:65-72`).
        let more_clear_core = core.clone();
        let more_clear_origin = origin;
        let more_manage_core = core.clone();
        let more_addons_core = core.clone();
        let more_settings_core = core.clone();
        let close_more_manage = self.on_close.clone();
        let close_more_addons = self.on_close.clone();
        let close_more_settings = self.on_close.clone();
        let more_popup = (*more_open.read()).then(|| {
            let anchor = *more_anchor.read();
            let (ox, oy) = anchor_popup_origin(
                anchor,
                (MORE_MENU_WIDTH, MORE_MENU_HEIGHT),
                window_size,
                PopupHorizontalAlign::End,
                4.0,
                8.0,
            );
            rect()
                .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL + 1))
                .position(Position::new_global().left(ox).top(oy))
                .width(Size::px(MORE_MENU_WIDTH))
                .padding(Gaps::new_all(4.0))
                .spacing(2.0)
                .corner_radius(CornerRadius::new_all(8.0))
                .background(panel_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(border_color),
                )
                .shadow(theme.shadows.float_drop.to_shadow())
                .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
                .child(SitePopupMenuRow {
                    icon: "trash",
                    label: "Clear Cookies and Site Data",
                    danger: false,
                    on_press: EventHandler::new(move |_| {
                        more_clear_core.invoke_or_log(Command::SiteClearData {
                            origin: Some(more_clear_origin.clone()),
                        });
                        more_open.set(false);
                    }),
                })
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(1.0))
                        .margin(Gaps::new(2.0, 4.0, 2.0, 4.0))
                        .background(divider_color),
                )
                .child(SitePopupMenuRow {
                    icon: "settings",
                    label: "Manage Extensions",
                    danger: false,
                    on_press: EventHandler::new(move |_| {
                        more_manage_core.invoke_or_log(Command::SettingsOpen {
                            group: Some(SettingsGroupId::Extensions),
                        });
                        more_open.set(false);
                        if let Some(close) = &close_more_manage {
                            close.call(());
                        }
                    }),
                })
                .child(SitePopupMenuRow {
                    icon: "plus",
                    label: "Add Extensions",
                    danger: false,
                    on_press: EventHandler::new(move |_| {
                        more_addons_core.invoke_or_log(Command::SettingsOpen {
                            group: Some(SettingsGroupId::Extensions),
                        });
                        more_open.set(false);
                        if let Some(close) = &close_more_addons {
                            close.call(());
                        }
                    }),
                })
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(1.0))
                        .margin(Gaps::new(2.0, 4.0, 2.0, 4.0))
                        .background(divider_color),
                )
                .child(SitePopupMenuRow {
                    icon: "sliders",
                    label: "All Site Settings",
                    danger: false,
                    on_press: EventHandler::new(move |_| {
                        more_settings_core.invoke_or_log(Command::SettingsOpen { group: None });
                        more_open.set(false);
                        if let Some(close) = &close_more_settings {
                            close.call(());
                        }
                    }),
                })
        });

        // Anchored right-click context menu for an extension tile (`#unified-extensions-context-menu`).
        let ext_menu_popup = ext_ctx_menu.read().clone().map(|(ext_id, _ext_name, anchor)| {
            let pin_core = core.clone();
            let opt_core = core.clone();
            let manage_core = core.clone();
            let remove_core = core.clone();
            let pin_id = ext_id.clone();
            let opt_id = ext_id.clone();
            let remove_id = ext_id.clone();
            let has_options = core
                .model()
                .peek()
                .extension(&ext_id)
                .and_then(|e| e.options_url.as_ref())
                .is_some();
            let close_on_opt = self.on_close.clone();
            let close_on_manage = self.on_close.clone();
            let menu_height = if has_options {
                EXT_MENU_HEIGHT + 30.0
            } else {
                EXT_MENU_HEIGHT
            };
            let (ox, oy) = anchor_popup_origin(
                anchor,
                (EXT_MENU_WIDTH, menu_height),
                window_size,
                PopupHorizontalAlign::Auto,
                4.0,
                8.0,
            );
            rect()
                .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL + 1))
                .position(Position::new_global().left(ox).top(oy))
                .width(Size::px(EXT_MENU_WIDTH))
                .padding(Gaps::new_all(4.0))
                .spacing(2.0)
                .corner_radius(CornerRadius::new_all(8.0))
                .background(panel_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(border_color),
                )
                .shadow(theme.shadows.float_drop.to_shadow())
                .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
                .child(SitePopupMenuRow {
                    icon: "pin",
                    label: "Pin to Toolbar",
                    danger: false,
                    on_press: EventHandler::new(move |_| {
                        pin_core.invoke_or_log(Command::ExtensionSetToolbarPinned {
                            extension_id: pin_id.clone(),
                            pinned: true,
                        });
                        ext_ctx_menu.set(None);
                    }),
                })
                .maybe_child(has_options.then(|| SitePopupMenuRow {
                    icon: "sliders",
                    label: "Extension Options",
                    danger: false,
                    on_press: EventHandler::new(move |_| {
                        opt_core.invoke_or_log(Command::ExtensionOpenOptions {
                            extension_id: opt_id.clone(),
                        });
                        ext_ctx_menu.set(None);
                        if let Some(close) = &close_on_opt {
                            close.call(());
                        }
                    }),
                }))
                .child(SitePopupMenuRow {
                    icon: "settings",
                    label: "Manage Extension",
                    danger: false,
                    on_press: EventHandler::new(move |_| {
                        manage_core.invoke_or_log(Command::SettingsOpen {
                            group: Some(SettingsGroupId::Extensions),
                        });
                        ext_ctx_menu.set(None);
                        if let Some(close) = &close_on_manage {
                            close.call(());
                        }
                    }),
                })
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(1.0))
                        .margin(Gaps::new(2.0, 4.0, 2.0, 4.0))
                        .background(divider_color),
                )
                .child(SitePopupMenuRow {
                    icon: "trash",
                    label: "Remove Extension",
                    danger: true,
                    on_press: EventHandler::new(move |_| {
                        remove_core.invoke_or_log(Command::ExtensionRemove {
                            extension_id: remove_id.clone(),
                        });
                        ext_ctx_menu.set(None);
                    }),
                })
        });

        let core_vim = core.clone();
        let close_vim = self.on_close.clone();
        let vim_origin = self.site_info.origin.clone();
        let vim_perms: Vec<(zendia_browser_core::site_info::SitePermissionKind, PermissionState)> = self
            .site_info
            .visible_permissions()
            .into_iter()
            .map(|p| (p.kind, p.state))
            .collect();
        let vim_exts: Vec<SiteExtension> = self.extensions.clone();
        let vim_reader_avail = is_web && self.site_info.reader_mode_available;

        rect()
            .a11y_role(AccessibilityRole::Dialog)
            .a11y_alt("Site Data Panel")
            .width(Size::px(SITE_PANEL_WIDTH))
            .corner_radius(CornerRadius::new_all(10.0))
            .background(panel_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .shadow(theme.shadows.float_drop.to_shadow())
            .on_mouse_down(move |e: Event<MouseEventData>| {
                e.stop_propagation();
                vim_kbd_active.set_if_modified(false);
                if e.button != Some(MouseButton::Right) {
                    ext_ctx_menu.set_if_modified(None);
                    more_open.set_if_modified(false);
                }
            })
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                let vim_enabled = core_vim.model().peek().vim_mode_enabled;
                if e.key == Key::Named(NamedKey::Escape) {
                    if ext_ctx_menu.peek().is_some() {
                        e.stop_propagation();
                        ext_ctx_menu.set(None);
                        return;
                    }
                    if *more_open.peek() {
                        e.stop_propagation();
                        more_open.set(false);
                        return;
                    }
                    if vim_enabled {
                        e.stop_propagation();
                        e.prevent_default();
                        if let Some(close) = &close_vim {
                            close.call(());
                        }
                        return;
                    }
                }
                if !vim_enabled {
                    return;
                }

                let nav = crate::ui::kit::list_nav::classify_list_key(
                    &e,
                    crate::ui::kit::list_nav::ListNavConfig::menu().with_vim_keys(true),
                );
                // Sub-popup navigation (`ext_ctx_menu` or `more_open`, `MN-01`).
                let ext_ctx = ext_ctx_menu.peek().clone();
                if let Some((ctx_id, _, _)) = ext_ctx {
                    let has_opt = core_vim
                        .model()
                        .peek()
                        .extension(&ctx_id)
                        .and_then(|ex| ex.options_url.as_ref())
                        .is_some();
                    let n = if has_opt { 4 } else { 3 };
                    e.stop_propagation();
                    e.prevent_default();
                    match nav {
                        Some(crate::ui::kit::list_nav::ListNavAction::MoveNext) => {
                            let c = *sub_cursor.peek();
                            sub_cursor.set((c + 1) % n);
                        }
                        Some(crate::ui::kit::list_nav::ListNavAction::MovePrev) => {
                            let c = *sub_cursor.peek();
                            sub_cursor.set((c + n - 1) % n);
                        }
                        Some(
                            crate::ui::kit::list_nav::ListNavAction::Dismiss
                            | crate::ui::kit::list_nav::ListNavAction::CloseSubmenu,
                        ) => {
                            ext_ctx_menu.set(None);
                        }
                        Some(crate::ui::kit::list_nav::ListNavAction::Activate) => {
                            let c = (*sub_cursor.peek()).min(n - 1);
                            ext_ctx_menu.set(None);
                            match (has_opt, c) {
                                (_, 0) => {
                                    core_vim.invoke_or_log(Command::ExtensionSetToolbarPinned {
                                        extension_id: ctx_id,
                                        pinned: true,
                                    });
                                }
                                (true, 1) => {
                                    core_vim.invoke_or_log(Command::ExtensionOpenOptions { extension_id: ctx_id });
                                    if let Some(close) = &close_vim {
                                        close.call(());
                                    }
                                }
                                (true, 2) | (false, 1) => {
                                    core_vim.invoke_or_log(Command::SettingsOpen {
                                        group: Some(SettingsGroupId::Extensions),
                                    });
                                    if let Some(close) = &close_vim {
                                        close.call(());
                                    }
                                }
                                _ => {
                                    core_vim.invoke_or_log(Command::ExtensionRemove { extension_id: ctx_id });
                                }
                            }
                        }
                        _ => {}
                    }
                    return;
                }
                if *more_open.peek() {
                    let n = 4usize;
                    e.stop_propagation();
                    e.prevent_default();
                    match nav {
                        Some(crate::ui::kit::list_nav::ListNavAction::MoveNext) => {
                            let c = *sub_cursor.peek();
                            sub_cursor.set((c + 1) % n);
                        }
                        Some(crate::ui::kit::list_nav::ListNavAction::MovePrev) => {
                            let c = *sub_cursor.peek();
                            sub_cursor.set((c + n - 1) % n);
                        }
                        Some(
                            crate::ui::kit::list_nav::ListNavAction::Dismiss
                            | crate::ui::kit::list_nav::ListNavAction::CloseSubmenu,
                        ) => {
                            more_open.set(false);
                        }
                        Some(crate::ui::kit::list_nav::ListNavAction::Activate) => {
                            let c = (*sub_cursor.peek()).min(n - 1);
                            more_open.set(false);
                            match c {
                                0 => {
                                    core_vim.invoke_or_log(Command::SiteClearData {
                                        origin: Some(vim_origin.clone()),
                                    });
                                }
                                1 | 2 => {
                                    core_vim.invoke_or_log(Command::SettingsOpen {
                                        group: Some(SettingsGroupId::Extensions),
                                    });
                                    if let Some(close) = &close_vim {
                                        close.call(());
                                    }
                                }
                                _ => {
                                    core_vim.invoke_or_log(Command::SettingsOpen { group: None });
                                    if let Some(close) = &close_vim {
                                        close.call(());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    return;
                }

                // Main Site Data Panel items in reading order (`MN-01`).
                #[derive(Clone)]
                enum PanelSlot {
                    Share,
                    Screenshot,
                    Bookmark,
                    Extension(SiteExtension),
                    AddExtension,
                    TrackingProtection,
                    Permission(zendia_browser_core::site_info::SitePermissionKind, PermissionState),
                    ClearData,
                    Security,
                    ReaderMode,
                    MoreMenu,
                }
                let mut slots = Vec::new();
                let mut hdr = Vec::new();
                if is_web {
                    hdr.push(PanelSlot::Share);
                }
                hdr.push(PanelSlot::Screenshot);
                if is_web {
                    hdr.push(PanelSlot::Bookmark);
                }
                if reverse_header {
                    hdr.reverse();
                }
                slots.extend(hdr);
                for ext in &vim_exts {
                    if ext.enabled && !extensions_paused {
                        slots.push(PanelSlot::Extension(ext.clone()));
                    }
                }
                slots.push(PanelSlot::AddExtension);
                if is_web {
                    slots.push(PanelSlot::TrackingProtection);
                    for &(kind, st) in &vim_perms {
                        slots.push(PanelSlot::Permission(kind, st));
                    }
                    slots.push(PanelSlot::ClearData);
                }
                if show_security_badge {
                    slots.push(PanelSlot::Security);
                }
                if vim_reader_avail {
                    slots.push(PanelSlot::ReaderMode);
                }
                slots.push(PanelSlot::MoreMenu);

                let total = slots.len();
                if total == 0 {
                    return;
                }
                e.stop_propagation();
                e.prevent_default();
                vim_kbd_active.set_if_modified(true);

                match nav {
                    Some(
                        crate::ui::kit::list_nav::ListNavAction::MoveNext
                        | crate::ui::kit::list_nav::ListNavAction::OpenSubmenu,
                    ) => {
                        let c = *vim_cursor.peek();
                        vim_cursor.set((c + 1) % total);
                    }
                    Some(
                        crate::ui::kit::list_nav::ListNavAction::MovePrev
                        | crate::ui::kit::list_nav::ListNavAction::CloseSubmenu,
                    ) => {
                        let c = *vim_cursor.peek();
                        vim_cursor.set((c + total - 1) % total);
                    }
                    Some(crate::ui::kit::list_nav::ListNavAction::First) => {
                        vim_cursor.set(0);
                    }
                    Some(crate::ui::kit::list_nav::ListNavAction::Last) => {
                        vim_cursor.set(total - 1);
                    }
                    Some(crate::ui::kit::list_nav::ListNavAction::Dismiss) => {
                        if let Some(close) = &close_vim {
                            close.call(());
                        }
                    }
                    Some(crate::ui::kit::list_nav::ListNavAction::Activate) => {
                        let idx = (*vim_cursor.peek()).min(total - 1);
                        match &slots[idx] {
                            PanelSlot::Share => {
                                core_vim.invoke_or_log(Command::ShareOpen {
                                    tab_id: None,
                                    space_id: None,
                                    folder_id: None,
                                });
                                if let Some(close) = &close_vim {
                                    close.call(());
                                }
                            }
                            PanelSlot::Screenshot => {
                                core_vim.invoke_or_log(Command::SiteScreenshot { tab_id: None });
                                if let Some(close) = &close_vim {
                                    close.call(());
                                }
                            }
                            PanelSlot::Bookmark => {
                                core_vim.invoke_or_log(Command::SiteBookmarkToggle { tab_id: None });
                            }
                            PanelSlot::Extension(ext) => {
                                if let Some(id) = ext.extension_id() {
                                    core_vim
                                        .invoke_or_log(Command::ExtensionInvokeActionFromKeyboard { extension_id: id });
                                } else if let Ok(parsed) = Command::from_name_and_args(&ext.command, &[]) {
                                    core_vim.invoke_or_log(parsed);
                                }
                                if let Some(close) = &close_vim {
                                    close.call(());
                                }
                            }
                            PanelSlot::AddExtension => {
                                core_vim.invoke_or_log(Command::SettingsOpen {
                                    group: Some(SettingsGroupId::Extensions),
                                });
                                if let Some(close) = &close_vim {
                                    close.call(());
                                }
                            }
                            PanelSlot::TrackingProtection => {
                                core_vim.invoke_or_log(Command::SiteTrackingProtectionToggle {
                                    origin: Some(vim_origin.clone()),
                                });
                            }
                            PanelSlot::Permission(kind, state) => {
                                let next_state = match state {
                                    PermissionState::Allow => PermissionState::Block,
                                    _ => PermissionState::Allow,
                                };
                                core_vim.invoke_or_log(Command::SitePermissionSet {
                                    origin: Some(vim_origin.clone()),
                                    permission: *kind,
                                    state: next_state,
                                });
                            }
                            PanelSlot::ClearData => {
                                core_vim.invoke_or_log(Command::SiteClearData {
                                    origin: Some(vim_origin.clone()),
                                });
                            }
                            PanelSlot::Security => {
                                let next = !*security_open.peek();
                                security_open.set(next);
                            }
                            PanelSlot::ReaderMode => {
                                core_vim.invoke_or_log(Command::SiteReaderModeToggle { tab_id: None });
                                if let Some(close) = &close_vim {
                                    close.call(());
                                }
                            }
                            PanelSlot::MoreMenu => {
                                sub_cursor.set(0);
                                more_open.set(true);
                            }
                        }
                    }
                    _ => {}
                }
            })
            .child(action_row)
            .child(extensions_section)
            .child(settings_section)
            .child(footer)
            .maybe_child(more_popup)
            .maybe_child(ext_menu_popup)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SitePopupMenuRow {
    icon: &'static str,
    label: &'static str,
    danger: bool,
    on_press: EventHandler<()>,
}

impl Component for SitePopupMenuRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let text_color = if self.danger {
            theme.color(ThemeRole::Danger)
        } else {
            match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(235, 255, 255, 255),
                ResolvedScheme::Light => Color::from_argb(232, 22, 24, 34),
            }
        };
        let icon_role = if self.danger {
            ThemeRole::Danger
        } else {
            ThemeRole::TextMuted
        };
        let bg = if *hovered.read() {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.10).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.08).to_color(),
            }
        } else {
            Color::TRANSPARENT
        };
        let on_press = self.on_press.clone();
        let key_press = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::MenuItem)
            .a11y_alt(self.label)
            .width(Size::fill())
            .padding(Gaps::new(6.0, 8.0, 6.0, 8.0))
            .spacing(8.0)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .border(focus_ring_border(focus() == Focus::Keyboard, &theme, Border::new()))
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_press.call(());
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Enter)) || e.key == Key::Character(" ".into()) {
                    e.stop_propagation();
                    e.prevent_default();
                    key_press.call(());
                }
            })
            .child(Icon::new(self.icon).size(13.0).role(icon_role))
            .child(label().text(self.label).font_size(12.0).color(text_color).max_lines(1))
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.label)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TileMotion {
    Rest,
    Hover,
    Pressed,
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
        let mut pressed = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let motion_state = if !self.enabled {
            TileMotion::Rest
        } else if *pressed.read() {
            TileMotion::Pressed
        } else if *hovered.read() {
            TileMotion::Hover
        } else {
            TileMotion::Rest
        };
        let target_scale = match motion_state {
            TileMotion::Rest => 1.0f32,
            TileMotion::Hover => 1.03,
            TileMotion::Pressed => 0.97,
        };
        let mut prev_scale = use_state(|| 1.0f32);
        let scale_anim = use_animation_with_dependencies(&motion_state, move |conf, state| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let to = match state {
                TileMotion::Rest => 1.0f32,
                TileMotion::Hover => 1.03,
                TileMotion::Pressed => 0.97,
            };
            let from = *prev_scale.peek();
            prev_scale.set(to);
            AnimNum::new(from, to)
                .duration(motion::SITE_TILE_SCALE)
                .ease(Ease::InOut)
                .function(motion::ease_out_fn())
        });
        let tile_scale = if self.enabled {
            scale_anim.get().value()
        } else {
            target_scale
        };

        let (themed_bg, themed_fg) = themed_icon_fill(&theme);
        let bg = if self.active {
            themed_bg
        } else if *pressed.read() && self.enabled {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.11).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.10).to_color(),
            }
        } else if hovered() && self.enabled {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.09).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.08).to_color(),
            }
        } else {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.06).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.06).to_color(),
            }
        };

        let icon_color = if self.active {
            themed_fg
        } else if theme.scheme.is_dark() {
            Color::from_argb(204, 255, 255, 255)
        } else {
            Color::from_argb(204, 0, 0, 0)
        };

        let glyph: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(self.icon) {
            SvgViewer::new((("zendia-icon", canonical), bytes))
                .width(Size::px(18.0))
                .height(Size::px(18.0))
                .color(icon_color)
                .show_loader(false)
                .into_element()
        } else {
            Icon::new(self.icon).size(18.0).role(ThemeRole::Text).into_element()
        };

        let border = focus_ring_border(focus() == Focus::Keyboard, &theme, Border::new());
        let enabled = self.enabled;
        let on_press = self.on_press.clone();

        rect().width(Size::flex(1.0)).child(
            KitTooltip::new(self.label).fill_width(true).enabled(enabled).child(
                rect()
                    .a11y_id(a11y_id)
                    .a11y_focusable(enabled)
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(self.label)
                    .width(Size::fill())
                    .height(Size::px(EXT_TILE_HEIGHT))
                    .corner_radius(CornerRadius::new_all(6.0))
                    .background(bg)
                    .border(border)
                    .scale(tile_scale)
                    .opacity(if enabled { 1.0 } else { 0.5 })
                    .center()
                    .cursor(if enabled {
                        CursorIcon::Pointer
                    } else {
                        CursorIcon::Default
                    })
                    .on_pointer_enter(move |_| hovered.set_if_modified(true))
                    .on_pointer_leave(move |_| {
                        hovered.set_if_modified(false);
                        pressed.set_if_modified(false);
                    })
                    .on_pointer_down(move |_| {
                        if enabled {
                            pressed.set_if_modified(true);
                        }
                    })
                    .on_global_pointer_up(move |_| pressed.set_if_modified(false))
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        if enabled {
                            on_press.call(());
                        }
                    })
                    .child(glyph),
            ),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SiteExtensionTile {
    extension: SiteExtension,
    paused: bool,
    usable: bool,
    on_press: EventHandler<()>,
    on_context_menu: EventHandler<AnchorRect>,
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
        let mut pressed = use_state(|| false);
        let mut tile_area = use_state(AnchorRect::default);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let motion_state = if !self.usable {
            TileMotion::Rest
        } else if *pressed.read() {
            TileMotion::Pressed
        } else if *hovered.read() {
            TileMotion::Hover
        } else {
            TileMotion::Rest
        };
        let mut prev_scale = use_state(|| 1.0f32);
        let scale_anim = use_animation_with_dependencies(&motion_state, move |conf, state| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let to = match state {
                TileMotion::Rest => 1.0f32,
                TileMotion::Hover => 1.05,
                TileMotion::Pressed => 0.95,
            };
            let from = *prev_scale.peek();
            prev_scale.set(to);
            AnimNum::new(from, to)
                .duration(motion::SITE_TILE_SCALE)
                .ease(Ease::InOut)
                .function(motion::ease_out_fn())
        });
        let tile_scale = if self.usable { scale_anim.get().value() } else { 1.0 };

        // Exact `#zen-site-data-addons > *` fills: resting 6%, active/press 10%.
        let bg = if *pressed.read() && self.usable {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.10).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.10).to_color(),
            }
        } else if hovered() && self.usable {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.085).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.08).to_color(),
            }
        } else {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.06).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.06).to_color(),
            }
        };

        let core = use_core();
        let ext_info = self
            .extension
            .extension_id()
            .and_then(|id| core.model().read().extension(&id).cloned());
        let icon_color = extension_icon_color(&self.extension, &theme);
        let glyph: Element = if let Some(ext) = &ext_info
            && ext.icon_data.is_some()
        {
            crate::ui::extensions::ExtensionIconView::new(
                format!("site-ext-{}", ext.id.as_str()),
                self.extension.icon.clone(),
                ext.icon_data.clone(),
                16.0,
                ThemeRole::Text,
            )
            .into_element()
        } else if let Some((canonical, bytes)) = prepared_icon_bytes(&self.extension.icon) {
            SvgViewer::new((("zendia-icon", canonical), bytes))
                .width(Size::px(16.0))
                .height(Size::px(16.0))
                .color(icon_color)
                .show_loader(false)
                .into_element()
        } else {
            Icon::new(self.extension.icon.clone())
                .size(16.0)
                .role(ThemeRole::Text)
                .into_element()
        };

        // Badge count rendered on the icon's top-right corner, matching Firefox/Zen's `.toolbarbutton-badge`.
        let badge: Option<Element> = if self.paused && self.extension.enabled {
            Some(
                rect()
                    .position(Position::new_absolute().top(2.0).right(4.0))
                    .min_width(Size::px(14.0))
                    .height(Size::px(13.0))
                    .padding(Gaps::new(0.0, 2.5, 0.0, 2.5))
                    .corner_radius(CornerRadius::new_all(3.0))
                    .background(theme.color(ThemeRole::Warning))
                    .center()
                    .child(
                        label()
                            .text("II")
                            .font_size(8.5)
                            .font_weight(FontWeight::BOLD)
                            .color(theme.color(ThemeRole::Ink)),
                    )
                    .into_element(),
            )
        } else if self.usable
            && let Some(text) = self.extension.badge_text.as_ref().filter(|t| !t.is_empty())
        {
            let badge_bg = ext_info
                .as_ref()
                .and_then(|e| e.badge_bg)
                .map(|[r, g, b, a]| Color::from_argb(a, r, g, b))
                .unwrap_or_else(|| Color::from_rgb(96, 96, 102));
            let badge_fg = ext_info
                .as_ref()
                .and_then(|e| e.badge_fg)
                .map(|[r, g, b, a]| Color::from_argb(a, r, g, b))
                .unwrap_or(Color::WHITE);
            Some(
                rect()
                    .position(Position::new_absolute().top(2.0).right(4.0))
                    .min_width(Size::px(14.0))
                    .height(Size::px(13.0))
                    .padding(Gaps::new(0.0, 3.0, 0.0, 3.0))
                    .corner_radius(CornerRadius::new_all(3.0))
                    .background(badge_bg)
                    .center()
                    .child(
                        label()
                            .text(text.clone())
                            .font_size(9.0)
                            .font_weight(FontWeight::BOLD)
                            .color(badge_fg),
                    )
                    .into_element(),
            )
        } else {
            None
        };

        let tooltip = if !self.extension.enabled {
            format!("{} (Disabled)", self.extension.name)
        } else if self.paused {
            format!("{} (Paused — ExtensionServer unavailable)", self.extension.name)
        } else {
            self.extension.name.clone()
        };

        let border = focus_ring_border(focus() == Focus::Keyboard, &theme, Border::new());
        let usable = self.usable;
        let on_press = self.on_press.clone();
        let on_context_menu = self.on_context_menu.clone();

        KitTooltip::new(tooltip).child(
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(self.extension.name.clone())
                .width(Size::px(EXT_TILE_WIDTH))
                .height(Size::px(EXT_TILE_HEIGHT))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(bg)
                .border(border)
                .scale(tile_scale)
                .opacity(if usable { 1.0 } else { 0.48 })
                .center()
                .cursor(if usable {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                })
                .on_sized(move |e: Event<SizedEventData>| {
                    tile_area.set_if_modified(AnchorRect::from_area(e.area));
                })
                .on_pointer_enter(move |_| hovered.set_if_modified(true))
                .on_pointer_leave(move |_| {
                    hovered.set_if_modified(false);
                    pressed.set_if_modified(false);
                })
                .on_pointer_down(move |e: Event<PointerEventData>| {
                    if usable && e.button() == Some(MouseButton::Left) {
                        pressed.set_if_modified(true);
                    }
                })
                .on_global_pointer_up(move |_| pressed.set_if_modified(false))
                .on_mouse_down(move |e: Event<MouseEventData>| {
                    if e.button == Some(MouseButton::Right) {
                        e.stop_propagation();
                        let measured = *tile_area.peek();
                        let anchor = if measured.width > 0.0 && measured.height > 0.0 {
                            measured
                        } else {
                            AnchorRect {
                                x: e.global_location.x as f32,
                                y: e.global_location.y as f32,
                                width: 1.0,
                                height: 1.0,
                            }
                        };
                        on_context_menu.call(anchor);
                    }
                })
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    if usable {
                        on_press.call(());
                    }
                })
                .child(glyph)
                .maybe_child(badge),
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
        let mut pressed = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let motion_state = if *pressed.read() {
            TileMotion::Pressed
        } else if *hovered.read() {
            TileMotion::Hover
        } else {
            TileMotion::Rest
        };
        let mut prev_scale = use_state(|| 1.0f32);
        let scale_anim = use_animation_with_dependencies(&motion_state, move |conf, state| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let to = match state {
                TileMotion::Rest => 1.0f32,
                TileMotion::Hover => 1.05,
                TileMotion::Pressed => 0.95,
            };
            let from = *prev_scale.peek();
            prev_scale.set(to);
            AnimNum::new(from, to)
                .duration(motion::SITE_TILE_SCALE)
                .ease(Ease::InOut)
                .function(motion::ease_out_fn())
        });
        let tile_scale = scale_anim.get().value();

        let bg = if *pressed.read() {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.10).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.10).to_color(),
            }
        } else if hovered() {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.085).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.08).to_color(),
            }
        } else {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.06).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.06).to_color(),
            }
        };

        let border = focus_ring_border(focus() == Focus::Keyboard, &theme, Border::new());
        let on_press = self.on_press.clone();

        KitTooltip::new("Add Extensions").child(
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Add Extensions")
                .width(Size::px(EXT_TILE_WIDTH))
                .height(Size::px(EXT_TILE_HEIGHT))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(bg)
                .border(border)
                .scale(tile_scale)
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| hovered.set_if_modified(true))
                .on_pointer_leave(move |_| {
                    hovered.set_if_modified(false);
                    pressed.set_if_modified(false);
                })
                .on_pointer_down(move |_| pressed.set_if_modified(true))
                .on_global_pointer_up(move |_| pressed.set_if_modified(false))
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    on_press.call(());
                })
                .child(Icon::new("plus").size(16.0).role(ThemeRole::Text)),
        )
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
        let mut pressed = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let motion_state = if *pressed.read() {
            TileMotion::Pressed
        } else if *hovered.read() {
            TileMotion::Hover
        } else {
            TileMotion::Rest
        };
        let mut prev_scale = use_state(|| 1.0f32);
        let circle_scale_anim = use_animation_with_dependencies(&motion_state, move |conf, state| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let to = match state {
                TileMotion::Rest => 1.0f32,
                TileMotion::Hover => 1.05,
                TileMotion::Pressed => 0.95,
            };
            let from = *prev_scale.peek();
            prev_scale.set(to);
            AnimNum::new(from, to)
                .duration(motion::SITE_TILE_SCALE)
                .ease(Ease::InOut)
                .function(motion::ease_out_fn())
        });
        let circle_scale = circle_scale_anim.get().value();

        // Exact `--zen-sidebar-themed-icon-fill` with opacity 1.0 when enabled/allow, 0.6 when disabled/block.
        let (themed_bg, themed_fg) = themed_icon_fill(&theme);
        let (circle_bg, glyph_color, circle_opacity) = if self.active {
            (themed_bg, themed_fg, 1.0f32)
        } else {
            (themed_bg, themed_fg, 0.52f32)
        };

        let title_color = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(235, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(228, 22, 24, 34),
        };
        let sub_color = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(153, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(153, 22, 24, 34),
        };

        let glyph: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(self.icon) {
            SvgViewer::new((("zendia-icon", canonical), bytes))
                .width(Size::px(16.0))
                .height(Size::px(16.0))
                .color(glyph_color)
                .show_loader(false)
                .into_element()
        } else {
            Icon::new(self.icon).size(16.0).role(ThemeRole::Text).into_element()
        };

        let border = focus_ring_border(focus() == Focus::Keyboard, &theme, Border::new());
        let on_press = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("{}: {}", self.title, self.subtitle))
            .width(Size::fill())
            .padding(Gaps::new(2.0, 2.0, 2.0, 0.0))
            .spacing(8.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(8.0))
            .border(border)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| {
                hovered.set_if_modified(false);
                pressed.set_if_modified(false);
            })
            .on_pointer_down(move |_| pressed.set_if_modified(true))
            .on_global_pointer_up(move |_| pressed.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_press.call(());
            })
            .child(
                rect()
                    .width(Size::px(34.0))
                    .height(Size::px(34.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(circle_bg)
                    .opacity(circle_opacity)
                    .scale(circle_scale)
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
                            .font_size(10.0)
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
    fn site_panel_origin_aligns_left_and_right_and_clamps_inside_window() {
        let left_anchor = AnchorRect {
            x: 152.0,
            y: 44.0,
            width: 24.0,
            height: 24.0,
        };
        let (x, y) = compute_site_panel_origin(left_anchor, (SITE_PANEL_WIDTH, 260.0), (1600.0, 1000.0));
        assert_eq!((x, y), (152.0, 74.0));

        // Right-side sidebar: right-aligns beneath the right URL bar and stays clamped inside window.
        let right_anchor = AnchorRect {
            x: 1550.0,
            y: 44.0,
            width: 24.0,
            height: 24.0,
        };
        let (rx, ry) = compute_site_panel_origin(right_anchor, (SITE_PANEL_WIDTH, 260.0), (1600.0, 1000.0));
        assert_eq!(ry, 74.0);
        assert!(rx + SITE_PANEL_WIDTH <= 1600.0 - 8.0);
    }
}
