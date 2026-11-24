/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Icon Table gallery section rendering every icon in [`ICON_TABLE`] with its canonical name.

use crate::gallery::sections::demo_card;
use crate::ui::kit::{ICON_TABLE, Icon, canonical_icon_name};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;

#[derive(Clone, PartialEq)]
pub struct IconsSection {
    pub state_id: String,
}

const FEATURED_ALIASES: &[&str] = &[
    "chat",
    "agents",
    "webchat",
    "vim",
    "briefcase",
    "book",
    "code",
    "compass",
    "search",
    "layout-float",
    "layout-slide",
    "layout-split",
    "new-tab",
    "copy",
    "cut",
    "paste",
    "delete",
    "sun",
    "moon",
    "audio-playing",
    "audio-muted",
    "audio-blocked",
    "put-back",
    "captcha",
];

fn icon_tile(name: &str, subtitle: Option<&str>, size: f32, role: ThemeRole) -> Element {
    let theme = use_theme();
    rect()
        .width(Size::px(148.0))
        .padding(Gaps::new_all(theme.spacing.md))
        .spacing(theme.spacing.sm)
        .cross_align(Alignment::center())
        .corner_radius(CornerRadius::new_all(theme.radii.md))
        .background(theme.color(ThemeRole::Surface))
        .border(
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::BorderSubtle)),
        )
        .child(
            rect()
                .width(Size::px(size + 16.0))
                .height(Size::px(size + 16.0))
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .corner_radius(CornerRadius::new_all(theme.radii.sm))
                .background(theme.color(ThemeRole::SurfaceRaised))
                .child(Icon::new(name).size(size).role(role)),
        )
        .child(
            label()
                .text(name.to_string())
                .font_size(theme.font_sizes.xs)
                .font_family("monospace")
                .max_lines(2)
                .text_overflow(TextOverflow::Ellipsis)
                .color(theme.color(ThemeRole::Text)),
        )
        .maybe_child(subtitle.map(|sub| {
            label()
                .text(format!("→ {sub}"))
                .font_size(10.0)
                .font_family("monospace")
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .color(theme.color(ThemeRole::TextMuted))
        }))
        .into_element()
}

impl Component for IconsSection {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let role = if self.state_id == "accent" {
            ThemeRole::Accent
        } else {
            ThemeRole::Text
        };
        let size = if self.state_id == "large" { 28.0 } else { 20.0 };

        let content: Element = if self.state_id == "aliases" {
            demo_card(
                "Semantic Icon Aliases",
                "High-level Zendia icon names mapped to their canonical ICON_TABLE entries.",
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(theme.spacing.md)
                    .children(FEATURED_ALIASES.iter().map(|alias| {
                        let target = canonical_icon_name(alias).unwrap_or("missing");
                        rect().key(*alias).child(icon_tile(alias, Some(target), 20.0, role))
                    })),
            )
        } else {
            demo_card(
                &format!("Embedded Icons ({})", ICON_TABLE.len()),
                "Every SVG entry embedded in ICON_TABLE, preprocessed to inherit currentColor from ThemeRole.",
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(theme.spacing.md)
                    .children(
                        ICON_TABLE
                            .iter()
                            .map(|(name, _)| rect().key(*name).child(icon_tile(name, None, size, role))),
                    ),
            )
        };

        ScrollView::new().width(Size::fill()).height(Size::fill()).child(
            rect()
                .width(Size::fill())
                .padding(Gaps::new_all(theme.spacing.xxl))
                .child(content),
        )
    }
}
