/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! About Zendia modal dialog matching the fork's layout with version copy feedback and core-owned links.

use crate::ui::chrome::menus::state::use_menu_state;
use crate::ui::core_view::use_core;
use crate::ui::theme::{ResolvedTheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::menus::{AboutInfo, AboutLink, about_info};

const ABOUT_CARD_WIDTH: f32 = 296.0;

fn hex_rgb(c: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// Generates an original geometric multi-petal Zendia placeholder emblem SVG colored from the active Tokyonight
/// palette so it renders crisply at 1x and 2x in both light and dark themes.
fn zendia_emblem_svg(theme: &ResolvedTheme) -> Bytes {
    let pink = hex_rgb(theme.palette.pink);
    let green = hex_rgb(theme.palette.green);
    let purple = hex_rgb(theme.palette.purple);
    let blue = hex_rgb(theme.palette.blue);
    let yellow = hex_rgb(theme.palette.yellow);
    let teal = hex_rgb(theme.palette.teal);
    let ink = hex_rgb(theme.palette.ink);
    let surface = hex_rgb(theme.rgba(ThemeRole::Surface));

    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80" viewBox="0 0 100 100">
  <circle cx="50" cy="24" r="18" fill="{pink}"/>
  <circle cx="72" cy="37" r="18" fill="{green}"/>
  <circle cx="72" cy="63" r="18" fill="{purple}"/>
  <circle cx="50" cy="76" r="18" fill="{blue}"/>
  <circle cx="28" cy="63" r="18" fill="{yellow}"/>
  <circle cx="28" cy="37" r="18" fill="{teal}"/>
  <circle cx="50" cy="50" r="19" fill="{surface}" stroke="{ink}" stroke-width="2.5"/>
  <rect x="42" y="45" width="4.5" height="9" rx="2.2" fill="{ink}"/>
  <rect x="53.5" y="45" width="4.5" height="9" rx="2.2" fill="{ink}"/>
</svg>"##
    );
    Bytes::from(svg.into_bytes())
}

/// Modal overlay presenting [`AboutDialogCard`] centered over a dimmed backdrop, closing on Escape or outside click.
#[derive(Clone, PartialEq)]
pub struct AboutDialogOverlay;

impl Component for AboutDialogOverlay {
    fn render(&self) -> impl IntoElement {
        let menu_state = use_menu_state();
        if !*menu_state.about_open.read() {
            return rect().into_element();
        }

        rect()
            .layer(Layer::Overlay)
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .center()
            .background(Color::from_argb(110, 0, 0, 0))
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if e.key == Key::Named(NamedKey::Escape) {
                    e.stop_propagation();
                    menu_state.close_about();
                }
            })
            .on_mouse_down(move |_| {
                menu_state.close_about();
            })
            .child(AboutDialogCard::new().on_close(move |_| menu_state.close_about()))
            .into_element()
    }
}

/// Reusable About Zendia card showing logo, version, Ladybird attribution, and clickable links.
#[derive(Clone, PartialEq)]
pub struct AboutDialogCard {
    on_close: Option<EventHandler<()>>,
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
            key: DiffKey::None,
        }
    }

    pub fn on_close(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_close = Some(handler.into());
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

        let card_bg = if theme.scheme.is_dark() {
            theme
                .rgba(ThemeRole::Surface)
                .mix(theme.rgba(ThemeRole::SurfaceRaised), 0.65)
                .to_color()
        } else {
            theme.color(ThemeRole::Surface)
        };

        let emblem_bytes = zendia_emblem_svg(&theme);
        let emblem_key = if theme.scheme.is_dark() {
            "zendia-about-emblem-dark"
        } else {
            "zendia-about-emblem-light"
        };

        let version_line = info.version_line();
        let clipboard_version = info.clipboard_version();
        let copy_core = core.clone();

        let primary_top = &info.primary_links[..2.min(info.primary_links.len())];
        let primary_bottom = if info.primary_links.len() > 2 {
            &info.primary_links[2..]
        } else {
            &[]
        };

        let render_link_row = |links: &[AboutLink], row_key: &'static str| {
            let mut row_children: Vec<Element> = Vec::new();
            for (idx, link) in links.iter().enumerate() {
                if idx > 0 {
                    row_children.push(
                        label()
                            .key((row_key, "dot", idx))
                            .text("·")
                            .font_size(12.0)
                            .color(theme.color(ThemeRole::TextMuted))
                            .into_element(),
                    );
                }
                let url = link.url.to_string();
                let link_core = core.clone();
                let close_handler = self.on_close.clone();
                row_children.push(
                    rect()
                        .key((row_key, "link", idx))
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            link_core.invoke_or_log(Command::TabNew {
                                url: Some(url.clone()),
                                after: None,
                                activate: true,
                            });
                            if let Some(close) = &close_handler {
                                close.call(());
                            }
                        })
                        .child(
                            label()
                                .text(link.label)
                                .font_size(12.0)
                                .color(theme.color(ThemeRole::Text))
                                .text_decoration(TextDecoration::Underline),
                        )
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

        rect()
            .a11y_role(AccessibilityRole::Dialog)
            .a11y_alt("About Zendia")
            .width(Size::px(ABOUT_CARD_WIDTH))
            .padding(Gaps::new(22.0, 24.0, 18.0, 24.0))
            .spacing(4.0)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(10.0))
            .background(card_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::Border)),
            )
            .shadow(theme.shadows.float_drop.to_shadow())
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .child(
                rect().margin(Gaps::new(0.0, 0.0, 8.0, 0.0)).child(
                    SvgViewer::new((emblem_key, emblem_bytes))
                        .width(Size::px(78.0))
                        .height(Size::px(78.0))
                        .show_loader(false),
                ),
            )
            .child(
                label()
                    .text(info.app_name)
                    .font_size(17.0)
                    .font_weight(FontWeight::BOLD)
                    .color(theme.color(ThemeRole::Text)),
            )
            .child(
                Button::new()
                    .flat()
                    .compact()
                    .padding(Gaps::new(2.0, 8.0, 2.0, 8.0))
                    .corner_radius(CornerRadius::new_all(6.0))
                    .background(Color::TRANSPARENT)
                    .hover_background(theme.color(ThemeRole::Hover))
                    .border_fill(Color::TRANSPARENT)
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        copy_core.invoke_or_log(Command::PageCopyLink {
                            url: clipboard_version.clone(),
                        });
                        copied.set(true);
                    })
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(6.0)
                            .child(
                                label()
                                    .text(version_line)
                                    .font_size(12.5)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            )
                            .maybe_child(copied().then(|| {
                                label()
                                    .text("Copied")
                                    .font_size(11.5)
                                    .font_weight(FontWeight::MEDIUM)
                                    .color(theme.color(ThemeRole::Accent))
                            })),
                    ),
            )
            .child(
                label()
                    .text(info.engine_line)
                    .font_size(12.5)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .margin(Gaps::new(10.0, 0.0, 12.0, 0.0))
                    .cross_align(Alignment::center())
                    .spacing(2.0)
                    .child(
                        label()
                            .text(format!("{} ", info.community_prefix.trim_end()))
                            .text_align(TextAlign::Center)
                            .font_size(12.0)
                            .color(theme.color(ThemeRole::TextMuted)),
                    )
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .main_align(Alignment::center())
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                community_core.invoke_or_log(Command::TabNew {
                                    url: Some(community_url.clone()),
                                    after: None,
                                    activate: true,
                                });
                                if let Some(close) = &close_community {
                                    close.call(());
                                }
                            })
                            .child(
                                label()
                                    .text(info.community_link_label)
                                    .font_size(12.0)
                                    .color(theme.color(ThemeRole::Text))
                                    .text_decoration(TextDecoration::Underline),
                            )
                            .child(
                                label()
                                    .text(info.community_suffix)
                                    .font_size(12.0)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .cross_align(Alignment::center())
                    .spacing(5.0)
                    .child(render_link_row(primary_top, "primary-top"))
                    .maybe_child((!primary_bottom.is_empty()).then(|| render_link_row(primary_bottom, "primary-bot")))
                    .child(render_link_row(info.legal_links, "legal")),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
