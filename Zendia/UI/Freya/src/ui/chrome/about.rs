/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! About Zendia modal dialog matching the fork's layout (`zen-about.inc.xhtml` / `zen-about.css` / `zen-about.js`),
//! with the Living Emblem mascot, timed version copy feedback, update status section, inline community attribution,
//! and keyboard-focusable links.

use crate::ui::chrome::menus::state::use_menu_state;
use crate::ui::core_view::use_core;
use crate::ui::kit::{
    Icon, KitTooltip, PopupPresence, Spinner, focus_ring_border, handle_modal_tab_key, use_interactive_feedback,
};
use crate::ui::theme::{ResolvedScheme, ResolvedTheme, Rgba, ThemeRole, motion, use_theme};
use crate::ui::vim::mascot::compute_mascot_frame;
use crate::ui::vim::mascot_data::{VIEWBOX_HEIGHT, VIEWBOX_WIDTH, VIEWBOX_X, VIEWBOX_Y};
use freya::engine::prelude as sk;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::menus::{AboutInfo, AboutLink, AboutUpdateStatus, about_info};
use zendia_browser_core::model::InputMode;

/// Matches `--zen-about-width: 280px` (`zen-about.css:8`).
const ABOUT_CARD_WIDTH: f32 = 280.0;
/// Matches `#zen-about-logo` `80px × 80px` (`zen-about.css:40-42`).
const ABOUT_LOGO_SIZE: f32 = 80.0;
const MANUAL_DOWNLOAD_URL: &str = "https://github.com/Planckify-Labs/zendia/releases";

fn sk_color(rgba: Rgba) -> sk::Color {
    sk::Color::from_argb(rgba.a, rgba.r, rgba.g, rgba.b)
}

fn about_ring_colors(scheme: ResolvedScheme) -> [Rgba; 6] {
    match scheme {
        ResolvedScheme::Dark => [
            Rgba::rgb(0xfc, 0xa7, 0xea),
            Rgba::rgb(0xc3, 0xe8, 0x8d),
            Rgba::rgb(0xc0, 0x99, 0xff),
            Rgba::rgb(0x82, 0xaa, 0xff),
            Rgba::rgb(0xff, 0xc7, 0x77),
            Rgba::rgb(0x4f, 0xd6, 0xbe),
        ],
        ResolvedScheme::Light => [
            Rgba::rgb(0xb0, 0x30, 0x7f),
            Rgba::rgb(0x47, 0x66, 0x28),
            Rgba::rgb(0x73, 0x40, 0xcc),
            Rgba::rgb(0x21, 0x60, 0xc4),
            Rgba::rgb(0x7a, 0x5c, 0x14),
            Rgba::rgb(0x0b, 0x74, 0x61),
        ],
    }
}

fn about_card_bg(theme: &ResolvedTheme) -> Rgba {
    match theme.scheme {
        ResolvedScheme::Dark => Rgba::rgb(0x33, 0x33, 0x33),
        ResolvedScheme::Light => Rgba::rgb(0xf5, 0xf6, 0xf5),
    }
}

fn about_card_border(scheme: ResolvedScheme) -> Color {
    match scheme {
        ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.10).to_color(),
        ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.09).to_color(),
    }
}

fn about_text_color(scheme: ResolvedScheme) -> Color {
    match scheme {
        ResolvedScheme::Dark => Color::from_rgb(0xf2, 0xf2, 0xf4),
        ResolvedScheme::Light => Color::from_rgb(0x1c, 0x1c, 0x1e),
    }
}

fn about_muted_color(scheme: ResolvedScheme) -> Color {
    match scheme {
        ResolvedScheme::Dark => Rgba::from_rgb_alpha(0xf2, 0xf2, 0xf4, 0.60).to_color(),
        ResolvedScheme::Light => Rgba::from_rgb_alpha(0x1c, 0x1c, 0x1e, 0.60).to_color(),
    }
}

fn about_button_colors(scheme: ResolvedScheme, primary: bool) -> (Color, Color, Color) {
    if primary {
        match scheme {
            ResolvedScheme::Dark => (
                Color::from_rgb(0xe8, 0xe8, 0xea),
                Rgba::from_rgb_alpha(0xe8, 0xe8, 0xea, 0.90).to_color(),
                Color::from_rgb(0x1c, 0x1c, 0x1e),
            ),
            ResolvedScheme::Light => (
                Color::from_rgb(0x1c, 0x1c, 0x1e),
                Rgba::from_rgb_alpha(0x1c, 0x1c, 0x1e, 0.90).to_color(),
                Color::WHITE,
            ),
        }
    } else {
        match scheme {
            ResolvedScheme::Dark => (
                Rgba::from_rgb_alpha(255, 255, 255, 0.10).to_color(),
                Rgba::from_rgb_alpha(255, 255, 255, 0.16).to_color(),
                Color::from_rgb(0xf2, 0xf2, 0xf4),
            ),
            ResolvedScheme::Light => (
                Rgba::from_rgb_alpha(0, 0, 0, 0.06).to_color(),
                Rgba::from_rgb_alpha(0, 0, 0, 0.10).to_color(),
                Color::from_rgb(0x1c, 0x1c, 0x1e),
            ),
        }
    }
}

fn build_polygon_path(pts: &[[f32; 2]]) -> sk::Path {
    let mut builder = sk::PathBuilder::new();
    if let Some((first, rest)) = pts.split_first() {
        builder.move_to(sk::Point::new(first[0], first[1]));
        for &pt in rest {
            builder.line_to(sk::Point::new(pt[0], pt[1]));
        }
        builder.close();
    }
    builder.detach()
}

/// Paints the Zendia Living Emblem mascot (`about-logo.svg` appearance) on Skia using our Voronoi mascot geometry
/// with scheme-aware Tokyo Night colors and high-contrast eyes punched in the dialog surface color.
fn paint_about_emblem(canvas: &sk::Canvas, width: f32, height: f32, scheme: ResolvedScheme, eye_cutout: Rgba) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let frame = compute_mascot_frame(InputMode::Normal, InputMode::Normal, 1.0, 0.0);
    let colors = about_ring_colors(scheme);

    let pad = 20.0;
    let vb_x = VIEWBOX_X - pad;
    let vb_y = VIEWBOX_Y - pad;
    let vb_w = VIEWBOX_WIDTH + pad * 2.0;
    let vb_h = VIEWBOX_HEIGHT + pad * 2.0;
    let scale = (width / vb_w).min(height / vb_h);
    let offset_x = (width - vb_w * scale) * 0.5;
    let offset_y = (height - vb_h * scale) * 0.5;

    let save = canvas.save();
    canvas.translate((offset_x, offset_y));
    canvas.scale((scale, scale));
    canvas.translate((-vb_x, -vb_y));

    // Draw background seam using the dialog surface color so Voronoi petals sit separated cleanly like `about-logo.svg`.
    let mut seam = sk::Paint::default();
    seam.set_anti_alias(true);
    seam.set_style(sk::PaintStyle::Stroke);
    seam.set_stroke_width(22.0);
    seam.set_stroke_join(sk::PaintJoin::Round);
    seam.set_stroke_cap(sk::PaintCap::Round);
    seam.set_color(sk_color(eye_cutout));

    let mut fill = sk::Paint::default();
    fill.set_anti_alias(true);
    fill.set_style(sk::PaintStyle::Fill);

    for (i, pts) in frame.cells.iter().enumerate() {
        let path = build_polygon_path(pts);
        fill.set_color(sk_color(colors[i]));
        canvas.draw_path(&path, &fill);
        canvas.draw_path(&path, &seam);
    }

    // Center rabbit head & antenna in `.cn` pink/magenta with crisp seam and eyes cut out to dialog background.
    let head_path = build_polygon_path(&frame.head);
    canvas.draw_path(&head_path, &seam);
    fill.set_color(sk_color(colors[0]));
    canvas.draw_path(&head_path, &fill);

    let mut eye_paint = sk::Paint::default();
    eye_paint.set_anti_alias(true);
    eye_paint.set_style(sk::PaintStyle::Stroke);
    eye_paint.set_stroke_cap(sk::PaintCap::Round);
    eye_paint.set_color(sk_color(eye_cutout));
    for &(p0, p1, stroke_w) in &frame.eyes {
        if stroke_w > 0.1 {
            eye_paint.set_stroke_width(stroke_w);
            canvas.draw_line(sk::Point::new(p0[0], p0[1]), sk::Point::new(p1[0], p1[1]), &eye_paint);
        }
    }

    canvas.restore_to_count(save);
}

#[derive(Clone, PartialEq)]
struct AboutEmblem {
    scheme: ResolvedScheme,
    cutout: Rgba,
}

impl Component for AboutEmblem {
    fn render(&self) -> impl IntoElement {
        let scheme = self.scheme;
        let cutout = self.cutout;
        let callback = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_about_emblem(canvas, ctx.size.width, ctx.size.height, scheme, cutout);
            }
        });
        rect()
            .width(Size::px(ABOUT_LOGO_SIZE))
            .height(Size::px(ABOUT_LOGO_SIZE))
            .margin(Gaps::new(0.0, 0.0, 8.0, 0.0))
            .background(Fill::Callback(callback))
    }
}

/// Modal overlay presenting [`AboutDialogCard`] centered over a dimmed backdrop, closing on Escape or outside click.
#[derive(Clone, PartialEq)]
pub struct AboutDialogOverlay;

impl Component for AboutDialogOverlay {
    fn render(&self) -> impl IntoElement {
        let menu_state = use_menu_state();
        let is_open = *menu_state.about_open.read();

        PopupPresence::centered(is_open)
            .backdrop_color(Color::from_argb(110, 0, 0, 0))
            .on_escape(move |_| menu_state.close_about())
            .on_backdrop_mouse_down(move |_| menu_state.close_about())
            .child(AboutDialogCard::new().on_close(move |_| menu_state.close_about()))
    }
}

/// Reusable About Zendia card showing logo, version, update status, Ladybird attribution, and clickable links.
#[derive(Clone, PartialEq)]
pub struct AboutDialogCard {
    on_close: Option<EventHandler<()>>,
    update_status_override: Option<AboutUpdateStatus>,
    key: DiffKey,
}

impl KeyExt for AboutDialogCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl AboutDialogCard {
    pub fn new() -> Self {
        Self {
            on_close: None,
            update_status_override: None,
            key: DiffKey::None,
        }
    }

    pub fn on_close(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_close = Some(handler.into());
        self
    }

    pub fn update_status_override(mut self, status: Option<AboutUpdateStatus>) -> Self {
        self.update_status_override = status;
        self
    }
}

impl Default for AboutDialogCard {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for AboutDialogCard {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let core = use_core();
        let info: AboutInfo = about_info();
        let mut copied = use_state(|| false);
        let mut copied_seq = use_state(|| 0u64);
        let mut checking_seq = use_state(|| 0u64);
        let mut was_checking = use_state(|| false);

        let version_a11y = use_a11y();
        let version_focus = use_focus(version_a11y);
        let version_fb = use_interactive_feedback(true);

        let community_a11y = use_a11y();
        let community_focus = use_focus(community_a11y);
        let community_fb = use_interactive_feedback(true);

        let update_btn_a11y = use_a11y();
        let link_a11y_0 = use_a11y();
        let link_a11y_1 = use_a11y();
        let link_a11y_2 = use_a11y();
        let link_a11y_3 = use_a11y();
        let link_a11y_4 = use_a11y();
        let link_a11y_5 = use_a11y();

        let update_status = self
            .update_status_override
            .clone()
            .unwrap_or_else(|| core.model().read().about_update_status.clone());

        // When the stub enters `Checking`, hold for `kCheckingMinDisplayMs` (`1500 ms`) then settle to `UpToDate`.
        let is_checking_live =
            self.update_status_override.is_none() && matches!(update_status, AboutUpdateStatus::Checking);
        if *was_checking.peek() != is_checking_live {
            was_checking.set(is_checking_live);
            if is_checking_live {
                let seq = checking_seq.peek().wrapping_add(1);
                checking_seq.set(seq);
                let settle_core = core.clone();
                spawn(async move {
                    timer(motion::ABOUT_CHECKING_MIN).await;
                    if *checking_seq.peek() == seq {
                        settle_core.invoke_or_log(Command::AppSetUpdateStatus {
                            status: AboutUpdateStatus::UpToDate,
                        });
                    }
                });
            }
        }

        let card_bg = about_card_bg(&theme);
        let text_color = about_text_color(theme.scheme);
        let muted_color = about_muted_color(theme.scheme);
        let (btn_bg, btn_hover, _) = about_button_colors(theme.scheme, false);

        let version_line = info.version_line();
        let clipboard_version = info.clipboard_version();
        let copy_core = core.clone();
        let trigger_copy = move || {
            copy_core.invoke_or_log(Command::PageCopyLink {
                url: clipboard_version.clone(),
            });
            let seq = copied_seq.peek().wrapping_add(1);
            copied_seq.set(seq);
            copied.set(true);
            spawn(async move {
                timer(motion::ABOUT_COPIED_HINT).await;
                if *copied_seq.peek() == seq {
                    copied.set(false);
                }
            });
        };
        let mut copy_on_press = trigger_copy.clone();
        let mut copy_on_key = trigger_copy;

        let primary_top = &info.primary_links[..2.min(info.primary_links.len())];
        let primary_bottom = if info.primary_links.len() > 2 {
            &info.primary_links[2..]
        } else {
            &[]
        };

        let link_ids = [
            link_a11y_0,
            link_a11y_1,
            link_a11y_2,
            link_a11y_3,
            link_a11y_4,
            link_a11y_5,
        ];

        let render_link_row = |links: &[AboutLink], id_offset: usize, row_key: &'static str| {
            let mut row_children: Vec<Element> = Vec::new();
            for (idx, link) in links.iter().enumerate() {
                if idx > 0 {
                    row_children.push(
                        label()
                            .key((row_key, "dot", idx))
                            .text("·")
                            .font_size(12.0)
                            .color(muted_color)
                            .into_element(),
                    );
                }
                row_children.push(
                    AboutFooterLink {
                        label: link.label,
                        url: link.url,
                        a11y_id: link_ids[(id_offset + idx).min(link_ids.len() - 1)],
                        on_close: self.on_close.clone(),
                        key: DiffKey::from(&format!("{row_key}-link-{idx}")),
                    }
                    .into_element(),
                );
            }
            rect()
                .direction(Direction::Horizontal)
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .spacing(6.0)
                .children(row_children)
        };

        let community_url = info.community_link_url.to_string();
        let community_core = core.clone();
        let close_community = self.on_close.clone();
        let open_community = move || {
            community_core.invoke_or_log(Command::TabNew {
                url: Some(community_url.clone()),
                after: None,
                activate: true,
            });
            if let Some(close) = &close_community {
                close.call(());
            }
        };
        let open_community_press = open_community.clone();
        let open_community_key = open_community;

        let update_has_button = matches!(
            update_status,
            AboutUpdateStatus::NeverChecked
                | AboutUpdateStatus::Available { .. }
                | AboutUpdateStatus::ReadyForRestart
                | AboutUpdateStatus::Failed { .. }
                | AboutUpdateStatus::Blocked {
                    learn_more_url: Some(_),
                    ..
                }
        );

        let mut focus_ring = vec![version_a11y];
        if update_has_button {
            focus_ring.push(update_btn_a11y);
        }
        focus_ring.push(community_a11y);
        focus_ring.extend_from_slice(&link_ids);

        let version_pill = KitTooltip::new("Copy build information").child(
            rect()
                .a11y_id(version_a11y)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Copy build information")
                .padding(Gaps::new(2.0, 8.0, 2.0, 8.0))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(version_fb.background(Color::TRANSPARENT, btn_bg, btn_hover))
                .border(focus_ring_border(
                    version_focus() == Focus::Keyboard,
                    &theme,
                    Border::new(),
                ))
                .scale(version_fb.scale())
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(version_fb.on_pointer_enter())
                .on_pointer_leave(version_fb.on_pointer_leave())
                .on_pointer_down(version_fb.on_pointer_down())
                .on_global_pointer_up(version_fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    copy_on_press();
                })
                .on_key_down(move |e: Event<KeyboardEventData>| {
                    let is_space = matches!(&e.key, Key::Character(c) if c == " ");
                    if matches!(e.key, Key::Named(NamedKey::Enter)) || is_space {
                        e.stop_propagation();
                        e.prevent_default();
                        copy_on_key();
                    }
                })
                .child(
                    rect()
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(6.0)
                        .child(label().text(version_line).font_size(12.5).color(muted_color))
                        .maybe_child(copied().then(|| {
                            label()
                                .text("Copied")
                                .font_size(11.5)
                                .font_weight(FontWeight::MEDIUM)
                                .color(theme.color(ThemeRole::Accent))
                        })),
                ),
        );

        let update_section = (!matches!(update_status, AboutUpdateStatus::Hidden)).then(|| AboutUpdateSection {
            status: update_status,
            button_a11y_id: update_btn_a11y,
            on_close: self.on_close.clone(),
        });

        let community_link_color = if community_fb.is_hovered() {
            theme.color(ThemeRole::Accent)
        } else {
            text_color
        };

        rect()
            .a11y_role(AccessibilityRole::Dialog)
            .a11y_alt("About Zendia")
            .width(Size::px(ABOUT_CARD_WIDTH))
            .padding(Gaps::new(18.0, 24.0, 14.0, 24.0))
            .spacing(2.0)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(8.0))
            .background(card_bg.to_color())
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(about_card_border(theme.scheme)),
            )
            .shadow(theme.shadows.float_drop.to_shadow())
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                let _ = handle_modal_tab_key(&e, &focus_ring);
            })
            .child(AboutEmblem {
                scheme: theme.scheme,
                cutout: card_bg,
            })
            .child(
                label()
                    .margin(Gaps::new(0.0, 0.0, 4.0, 0.0))
                    .text(info.app_name)
                    .font_size(17.0)
                    .font_weight(FontWeight::BOLD)
                    .color(text_color),
            )
            .child(version_pill)
            .child(label().text(info.engine_line).font_size(12.5).color(muted_color))
            .maybe_child(update_section)
            .child(
                rect()
                    .width(Size::fill())
                    .margin(Gaps::new(10.0, 0.0, 0.0, 0.0))
                    .cross_align(Alignment::center())
                    .spacing(14.0)
                    .child(
                        rect()
                            .a11y_id(community_a11y)
                            .a11y_focusable(true)
                            .a11y_role(AccessibilityRole::Link)
                            .a11y_alt(info.community_link_label)
                            .width(Size::fill())
                            .padding(Gaps::new(2.0, 2.0, 2.0, 2.0))
                            .corner_radius(CornerRadius::new_all(4.0))
                            .border(focus_ring_border(
                                community_focus() == Focus::Keyboard,
                                &theme,
                                Border::new(),
                            ))
                            .cursor(CursorIcon::Pointer)
                            .on_pointer_enter(community_fb.on_pointer_enter())
                            .on_pointer_leave(community_fb.on_pointer_leave())
                            .on_pointer_down(community_fb.on_pointer_down())
                            .on_global_pointer_up(community_fb.on_pointer_up())
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                open_community_press();
                            })
                            .on_key_down(move |e: Event<KeyboardEventData>| {
                                let is_space = matches!(&e.key, Key::Character(c) if c == " ");
                                if matches!(e.key, Key::Named(NamedKey::Enter)) || is_space {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    open_community_key();
                                }
                            })
                            .child(
                                paragraph()
                                    .width(Size::fill())
                                    .text_align(TextAlign::Center)
                                    .line_height(1.4)
                                    .span(Span::new(info.community_prefix).font_size(12.0).color(muted_color))
                                    .span(
                                        Span::new(info.community_link_label)
                                            .font_size(12.0)
                                            .font_weight(if community_fb.is_hovered() {
                                                FontWeight::MEDIUM
                                            } else {
                                                FontWeight::NORMAL
                                            })
                                            .color(community_link_color)
                                            .text_decoration(TextDecoration::Underline),
                                    )
                                    .span(Span::new(info.community_suffix).font_size(12.0).color(muted_color)),
                            ),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .cross_align(Alignment::center())
                            .spacing(4.0)
                            .child(render_link_row(primary_top, 0, "primary-top"))
                            .maybe_child(
                                (!primary_bottom.is_empty()).then(|| render_link_row(primary_bottom, 2, "primary-bot")),
                            )
                            .child(render_link_row(info.legal_links, 3, "legal")),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Update status row & action button matching `#zen-about-update` (`zen-about.css:86-200`, `zen-about.js:267-411`).
#[derive(Clone, PartialEq)]
struct AboutUpdateSection {
    status: AboutUpdateStatus,
    button_a11y_id: AccessibilityId,
    on_close: Option<EventHandler<()>>,
}

impl Component for AboutUpdateSection {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let core = use_core();
        let focus = use_focus(self.button_a11y_id);
        let fb = use_interactive_feedback(true);

        let text_color = about_text_color(theme.scheme);

        #[derive(Clone)]
        enum UpdateAction {
            Check,
            Download,
            Restart,
            OpenUrl(String),
        }

        type UpdateButtonSpec = (&'static str, bool, UpdateAction);
        type UpdateViewParts = (Option<Element>, Option<String>, Option<UpdateButtonSpec>);

        let (icon_el, text, button_spec): UpdateViewParts = match &self.status {
            AboutUpdateStatus::Hidden => (None, None, None),
            AboutUpdateStatus::NeverChecked => (None, None, Some(("Check for updates", false, UpdateAction::Check))),
            AboutUpdateStatus::Checking => (
                Some(Spinner::new(14.0, text_color).into_element()),
                Some("Checking for updates…".to_string()),
                None,
            ),
            AboutUpdateStatus::UpToDate => (
                Some(Icon::new("check").size(14.0).role(ThemeRole::Success).into_element()),
                Some("Zendia is up to date".to_string()),
                None,
            ),
            AboutUpdateStatus::Available { version } => (
                Some(Icon::new("sparkles").size(14.0).role(ThemeRole::Text).into_element()),
                Some(format!("Zendia {version} is available")),
                Some(("Download and install", true, UpdateAction::Download)),
            ),
            AboutUpdateStatus::Downloading { transfer } => (
                Some(Spinner::new(14.0, text_color).into_element()),
                Some(format!("Downloading update — {transfer}")),
                None,
            ),
            AboutUpdateStatus::Applying => (
                Some(Spinner::new(14.0, text_color).into_element()),
                Some("Applying update…".to_string()),
                None,
            ),
            AboutUpdateStatus::ReadyForRestart => (
                Some(Icon::new("reload").size(14.0).role(ThemeRole::Text).into_element()),
                Some("The update is ready to install".to_string()),
                Some(("Restart to update", true, UpdateAction::Restart)),
            ),
            AboutUpdateStatus::Restarting => (
                Some(Spinner::new(14.0, text_color).into_element()),
                Some("Restarting…".to_string()),
                None,
            ),
            AboutUpdateStatus::Failed { message, retryable } => (
                Some(Icon::new("warning").size(14.0).role(ThemeRole::Warning).into_element()),
                Some(message.clone()),
                Some(if *retryable {
                    ("Try again", false, UpdateAction::Check)
                } else {
                    (
                        "Download manually",
                        false,
                        UpdateAction::OpenUrl(MANUAL_DOWNLOAD_URL.to_string()),
                    )
                }),
            ),
            AboutUpdateStatus::Blocked {
                message,
                learn_more_url,
            } => (
                Some(Icon::new("warning").size(14.0).role(ThemeRole::Warning).into_element()),
                Some(message.clone()),
                learn_more_url
                    .clone()
                    .map(|url| ("Learn more", false, UpdateAction::OpenUrl(url))),
            ),
        };

        let status_row = text.map(|msg| {
            rect()
                .direction(Direction::Horizontal)
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .min_height(Size::px(18.0))
                .spacing(6.0)
                .maybe_child(icon_el)
                .child(
                    label()
                        .text(msg)
                        .font_size(12.0)
                        .color(text_color)
                        .text_align(TextAlign::Center),
                )
        });

        let button_el = button_spec.map(|(btn_label, is_primary, action)| {
            let (bg_rest, bg_hover, fg) = about_button_colors(theme.scheme, is_primary);
            let run_core = core.clone();
            let close_cb = self.on_close.clone();
            let run_action = move || match &action {
                UpdateAction::Check => {
                    run_core.invoke_or_log(Command::AppCheckUpdates);
                }
                UpdateAction::Download => {
                    run_core.invoke_or_log(Command::AppSetUpdateStatus {
                        status: AboutUpdateStatus::Downloading {
                            transfer: "18.4 of 64.0 MB".to_string(),
                        },
                    });
                }
                UpdateAction::Restart => {
                    run_core.invoke_or_log(Command::AppSetUpdateStatus {
                        status: AboutUpdateStatus::Restarting,
                    });
                }
                UpdateAction::OpenUrl(url) => {
                    run_core.invoke_or_log(Command::TabNew {
                        url: Some(url.clone()),
                        after: None,
                        activate: true,
                    });
                    if let Some(close) = &close_cb {
                        close.call(());
                    }
                }
            };
            let run_press = run_action.clone();
            let run_key = run_action;

            rect()
                .a11y_id(self.button_a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(btn_label)
                .padding(Gaps::new(4.0, 10.0, 4.0, 10.0))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(fb.background(bg_rest, bg_hover, bg_hover))
                .border(focus_ring_border(focus() == Focus::Keyboard, &theme, Border::new()))
                .scale(fb.scale())
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    run_press();
                })
                .on_key_down(move |e: Event<KeyboardEventData>| {
                    let is_space = matches!(&e.key, Key::Character(c) if c == " ");
                    if matches!(e.key, Key::Named(NamedKey::Enter)) || is_space {
                        e.stop_propagation();
                        e.prevent_default();
                        run_key();
                    }
                })
                .child(
                    label()
                        .text(btn_label)
                        .font_size(12.0)
                        .font_weight(FontWeight::MEDIUM)
                        .color(fg),
                )
        });

        rect()
            .a11y_role(AccessibilityRole::Status)
            .width(Size::fill())
            .min_height(Size::px(44.0))
            .margin(Gaps::new(8.0, 0.0, 0.0, 0.0))
            .main_align(Alignment::center())
            .cross_align(Alignment::center())
            .spacing(6.0)
            .maybe_child(status_row)
            .maybe_child(button_el)
    }
}

/// Keyboard-focusable footer link with hover highlight and focus ring (`ABT-05`).
#[derive(Clone, PartialEq)]
struct AboutFooterLink {
    label: &'static str,
    url: &'static str,
    a11y_id: AccessibilityId,
    on_close: Option<EventHandler<()>>,
    key: DiffKey,
}

impl KeyExt for AboutFooterLink {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for AboutFooterLink {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let core = use_core();
        let focus = use_focus(self.a11y_id);
        let fb = use_interactive_feedback(true);

        let url = self.url.to_string();
        let link_core = core.clone();
        let close_handler = self.on_close.clone();
        let activate = move || {
            link_core.invoke_or_log(Command::TabNew {
                url: Some(url.clone()),
                after: None,
                activate: true,
            });
            if let Some(close) = &close_handler {
                close.call(());
            }
        };
        let on_press_cb = activate.clone();
        let on_key_cb = activate;

        let link_color = if fb.is_hovered() {
            theme.color(ThemeRole::Accent)
        } else {
            about_text_color(theme.scheme)
        };

        rect()
            .a11y_id(self.a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Link)
            .a11y_alt(self.label)
            .padding(Gaps::new(1.0, 2.0, 1.0, 2.0))
            .corner_radius(CornerRadius::new_all(3.0))
            .border(focus_ring_border(focus() == Focus::Keyboard, &theme, Border::new()))
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_press_cb();
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                let is_space = matches!(&e.key, Key::Character(c) if c == " ");
                if matches!(e.key, Key::Named(NamedKey::Enter)) || is_space {
                    e.stop_propagation();
                    e.prevent_default();
                    on_key_cb();
                }
            })
            .child(
                label()
                    .text(self.label)
                    .font_size(12.0)
                    .font_weight(if fb.is_hovered() {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .color(link_color)
                    .text_decoration(TextDecoration::Underline),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
