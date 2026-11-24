/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Theme & Palette gallery section: light/dark toggle, Tokyonight palette, semantic roles, agent brand colors, glass
//! material, and layout scales.

use crate::gallery::sections::{demo_card, glass_backdrop, state_caption};
use crate::ui::core_view::use_core;
use crate::ui::kit::{GlassPanel, KitChip, KitSwitch};
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::ThemeScheme;

#[derive(Clone, PartialEq)]
pub struct ThemeSection {
    pub state_id: String,
}

fn hex_label(color: Rgba) -> String {
    if color.a == 255 {
        format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
    } else {
        format!(
            "#{:02x}{:02x}{:02x} ({}%)",
            color.r,
            color.g,
            color.b,
            ((color.a as f32 / 255.0) * 100.0).round() as u8
        )
    }
}

fn color_swatch(name: &str, color: Rgba) -> Element {
    let theme = use_theme();
    rect()
        .width(Size::px(148.0))
        .padding(Gaps::new_all(theme.spacing.md))
        .spacing(theme.spacing.xs)
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
                .width(Size::fill())
                .height(Size::px(34.0))
                .corner_radius(CornerRadius::new_all(theme.radii.sm))
                .background(color.to_color())
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::BorderSubtle)),
                ),
        )
        .child(
            label()
                .text(name.to_string())
                .font_size(theme.font_sizes.sm)
                .font_weight(FontWeight::MEDIUM)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .color(theme.color(ThemeRole::Text)),
        )
        .child(
            label()
                .text(hex_label(color))
                .font_size(theme.font_sizes.xs)
                .font_family("monospace")
                .color(theme.color(ThemeRole::TextMuted)),
        )
        .into_element()
}

fn scheme_controls() -> Element {
    let core = use_core();
    let theme = use_theme();
    let is_dark = theme.scheme == ResolvedScheme::Dark;
    let current_setting = core.model().read().theme.scheme;

    let toggle_core = core.clone();
    let light_core = core.clone();
    let dark_core = core.clone();
    let system_core = core.clone();

    demo_card(
        "Color Scheme",
        "Switches ThemeScheme via Command::ThemeSet through BrowserCore.",
        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(theme.spacing.xl)
            .child(
                KitSwitch::new(is_dark)
                    .label(if is_dark {
                        "Dark (Tokyonight Moon)"
                    } else {
                        "Light (Tokyonight Day)"
                    })
                    .on_toggle(move |next_dark| {
                        let scheme = if next_dark {
                            ThemeScheme::Dark
                        } else {
                            ThemeScheme::Light
                        };
                        toggle_core.invoke_or_log(Command::ThemeSet {
                            scheme: Some(scheme),
                            overrides: None,
                        });
                    }),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .spacing(theme.spacing.sm)
                    .child(
                        KitChip::new("Light")
                            .icon("sun")
                            .selected(current_setting == ThemeScheme::Light)
                            .on_press(move |_| {
                                light_core.invoke_or_log(Command::ThemeSet {
                                    scheme: Some(ThemeScheme::Light),
                                    overrides: None,
                                });
                            }),
                    )
                    .child(
                        KitChip::new("Dark")
                            .icon("moon")
                            .selected(current_setting == ThemeScheme::Dark)
                            .on_press(move |_| {
                                dark_core.invoke_or_log(Command::ThemeSet {
                                    scheme: Some(ThemeScheme::Dark),
                                    overrides: None,
                                });
                            }),
                    )
                    .child(
                        KitChip::new("System")
                            .icon("settings")
                            .selected(current_setting == ThemeScheme::System)
                            .on_press(move |_| {
                                system_core.invoke_or_log(Command::ThemeSet {
                                    scheme: Some(ThemeScheme::System),
                                    overrides: None,
                                });
                            }),
                    ),
            ),
    )
}

fn palette_card() -> Element {
    let theme = use_theme();
    let p = theme.palette;
    let tokens: &[(&str, Rgba)] = &[
        ("base", p.base),
        ("raised", p.raised),
        ("border", p.border),
        ("text", p.text),
        ("muted", p.muted),
        ("ink", p.ink),
        ("night", p.night),
        ("snow", p.snow),
        ("pink", p.pink),
        ("red", p.red),
        ("orange", p.orange),
        ("yellow", p.yellow),
        ("green", p.green),
        ("teal", p.teal),
        ("cyan", p.cyan),
        ("blue", p.blue),
        ("purple", p.purple),
    ];

    let agent_swatches = theme
        .agents
        .iter()
        .map(|(name, color)| rect().key(name.clone()).child(color_swatch(name, *color)));

    demo_card(
        "Tokyonight Palette & Agent Brand Colors",
        "17 base tokens resolved for the active scheme, plus per-agent brand hues.",
        rect()
            .spacing(theme.spacing.xl)
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(theme.spacing.md)
                    .children(
                        tokens
                            .iter()
                            .map(|(name, color)| rect().key(*name).child(color_swatch(name, *color))),
                    ),
            )
            .child(state_caption("AGENT BRAND COLORS"))
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(theme.spacing.md)
                    .children(agent_swatches),
            ),
    )
}

fn roles_card() -> Element {
    let theme = use_theme();
    let groups: &[(&str, &[(&str, ThemeRole)])] = &[
        (
            "SURFACE & UI ROLES",
            &[
                ("Surface", ThemeRole::Surface),
                ("SurfaceRaised", ThemeRole::SurfaceRaised),
                ("SurfaceGlass", ThemeRole::SurfaceGlass),
                ("Text", ThemeRole::Text),
                ("TextMuted", ThemeRole::TextMuted),
                ("Accent", ThemeRole::Accent),
                ("Border", ThemeRole::Border),
                ("Selection", ThemeRole::Selection),
                ("Hover", ThemeRole::Hover),
                ("Success", ThemeRole::Success),
                ("Warning", ThemeRole::Warning),
                ("Danger", ThemeRole::Danger),
                ("Info", ThemeRole::Info),
            ],
        ),
        (
            "VIM, HINTS & WHICH-KEY",
            &[
                ("ModeNormal", ThemeRole::ModeNormal),
                ("ModeInsert", ThemeRole::ModeInsert),
                ("ModeVisual", ThemeRole::ModeVisual),
                ("ModeExplorer", ThemeRole::ModeExplorer),
                ("ModeToolbar", ThemeRole::ModeToolbar),
                ("ModeLibrary", ThemeRole::ModeLibrary),
                ("ModeWindow", ThemeRole::ModeWindow),
                ("Hint", ThemeRole::Hint),
                ("WhichKeyKey", ThemeRole::WhichKeyKey),
                ("WhichKeyGroup", ThemeRole::WhichKeyGroup),
                ("StatuslineBg", ThemeRole::StatuslineBackground),
            ],
        ),
        (
            "MARKDOWN & EFFORT ROLES",
            &[
                ("MdH1", ThemeRole::MdH1),
                ("MdH2", ThemeRole::MdH2),
                ("MdH3", ThemeRole::MdH3),
                ("MdH4", ThemeRole::MdH4),
                ("MdH5", ThemeRole::MdH5),
                ("MdH6", ThemeRole::MdH6),
                ("MdCode", ThemeRole::MdCode),
                ("EffortMinimal", ThemeRole::EffortMinimal),
                ("EffortLow", ThemeRole::EffortLow),
                ("EffortMedium", ThemeRole::EffortMedium),
                ("EffortHigh", ThemeRole::EffortHigh),
                ("EffortMax", ThemeRole::EffortMax),
                ("EffortUltra", ThemeRole::EffortUltra),
            ],
        ),
    ];

    demo_card(
        "Semantic Theme Roles",
        "Components consume ThemeRole values rather than raw RGB colors.",
        rect()
            .spacing(theme.spacing.xl)
            .children(groups.iter().map(|(heading, items)| {
                rect()
                    .key(*heading)
                    .spacing(theme.spacing.sm)
                    .child(state_caption(heading))
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .content(Content::wrap())
                            .spacing(theme.spacing.md)
                            .children(
                                items.iter().map(|(label, role)| {
                                    rect().key(*label).child(color_swatch(label, theme.rgba(*role)))
                                }),
                            ),
                    )
            })),
    )
}

fn glass_and_scales_card() -> Element {
    let theme = use_theme();
    let radii: &[(&str, f32)] = &[
        ("xs", theme.radii.xs),
        ("sm", theme.radii.sm),
        ("md", theme.radii.md),
        ("lg", theme.radii.lg),
        ("card", theme.radii.card),
        ("dock", theme.radii.dock),
        ("float", theme.radii.float),
        ("pill", theme.radii.pill),
    ];
    let fonts: &[(&str, f32)] = &[
        ("xs (11px)", theme.font_sizes.xs),
        ("sm (12px)", theme.font_sizes.sm),
        ("md (13px)", theme.font_sizes.md),
        ("lg (14px)", theme.font_sizes.lg),
        ("xl (15px)", theme.font_sizes.xl),
        ("title (18px)", theme.font_sizes.title),
    ];

    demo_card(
        "Glass Material, Corner Radii & Typography",
        "Backdrop blur and glass opacity compared with the solid surface fallback.",
        rect()
            .spacing(theme.spacing.xl)
            .child(glass_backdrop(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .spacing(theme.spacing.xl)
                    .child(
                        GlassPanel::new()
                            .width(Size::flex(1.0))
                            .glass(true)
                            .child(
                                label()
                                    .text("GlassPanel (glass = true)")
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(theme.color(ThemeRole::Text)),
                            )
                            .child(
                                label()
                                    .text(format!(
                                        "opacity {:.2}, backdrop_blur {:.0}px",
                                        theme.glass.opacity, theme.glass.blur
                                    ))
                                    .font_size(theme.font_sizes.sm)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                    )
                    .child(
                        GlassPanel::new()
                            .width(Size::flex(1.0))
                            .glass(false)
                            .child(
                                label()
                                    .text("GlassPanel (solid fallback)")
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(theme.color(ThemeRole::Text)),
                            )
                            .child(
                                label()
                                    .text("Opaque surface color with ThemeRole::Border")
                                    .font_size(theme.font_sizes.sm)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                    ),
            ))
            .child(state_caption("CORNER RADII"))
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(theme.spacing.md)
                    .children(radii.iter().map(|(name, value)| {
                        rect()
                            .key(*name)
                            .width(Size::px(96.0))
                            .height(Size::px(44.0))
                            .main_align(Alignment::center())
                            .cross_align(Alignment::center())
                            .corner_radius(CornerRadius::new_all(*value))
                            .background(theme.color(ThemeRole::Surface))
                            .border(
                                Border::new()
                                    .width(1.0)
                                    .alignment(BorderAlignment::Inner)
                                    .fill(theme.color(ThemeRole::Accent)),
                            )
                            .child(
                                label()
                                    .text(format!("{name} {:.0}", value.min(999.0)))
                                    .font_size(theme.font_sizes.xs)
                                    .color(theme.color(ThemeRole::Text)),
                            )
                    })),
            )
            .child(state_caption("TYPOGRAPHY SCALE"))
            .child(
                rect()
                    .spacing(theme.spacing.xs)
                    .children(fonts.iter().map(|(name, size)| {
                        label()
                            .key(*name)
                            .text(format!("{name} — The quick brown fox jumps over the lazy dog"))
                            .font_size(*size)
                            .color(theme.color(ThemeRole::Text))
                    })),
            ),
    )
}

impl Component for ThemeSection {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let show_palette = self.state_id == "all" || self.state_id == "palette";
        let show_roles = self.state_id == "all" || self.state_id == "roles";
        let show_glass = self.state_id == "all" || self.state_id == "glass";

        ScrollView::new().width(Size::fill()).height(Size::fill()).child(
            rect()
                .width(Size::fill())
                .padding(Gaps::new_all(theme.spacing.xxl))
                .spacing(theme.spacing.xxl)
                .child(scheme_controls())
                .maybe_child(show_palette.then(palette_card))
                .maybe_child(show_roles.then(roles_card))
                .maybe_child(show_glass.then(glass_and_scales_card)),
        )
    }
}
