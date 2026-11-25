/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Native Freya GitHub-Flavored Markdown renderer for Zendia's agent replies.
//!
//! Renders the streaming-safe AST produced by [`zendia_browser_core::markdown::parse_markdown`] directly into Freya
//! `paragraph`, `span`, `rect`, and `ScrollView` nodes using Zendia's theme markdown roles (`MdH1`..`MdH6`,
//! `MdStrong`, `MdEm`, `MdLink`, `MdCode`, `MdCodeBackground`, `MdQuote`, `MdBullet`, `MdRule`, `MdTableHead`).

use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::theme::{ResolvedScheme, ResolvedTheme, ThemeRole, use_theme};
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::markdown::{ColumnAlign, MdBlock, MdInline, parse_markdown};

/// Renders a Markdown string using Zendia's GFM AST parser and theme markdown roles.
#[derive(Clone, PartialEq)]
pub struct MarkdownView {
    pub text: String,
    pub streaming: bool,
    pub veil_opacity: f32,
    key: DiffKey,
}

impl KeyExt for MarkdownView {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl MarkdownView {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            streaming: false,
            veil_opacity: 1.0,
            key: DiffKey::None,
        }
    }

    pub fn streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    pub fn veil_opacity(mut self, opacity: f32) -> Self {
        self.veil_opacity = opacity.clamp(0.0, 1.0);
        self
    }
}

impl Component for MarkdownView {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let blocks = parse_markdown(&self.text, self.streaming);

        let mut col = rect().width(Size::fill()).spacing(8.0);
        let total = blocks.len();
        for (idx, block) in blocks.into_iter().enumerate() {
            let is_last = idx + 1 == total;
            let el = render_block(block, &theme, &core);
            if is_last && self.streaming && self.veil_opacity < 0.999 {
                col = col.child(rect().width(Size::fill()).opacity(self.veil_opacity).child(el));
            } else {
                col = col.child(el);
            }
        }
        col
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn render_block(block: MdBlock, theme: &ResolvedTheme, core: &crate::ui::core_view::CoreView) -> Element {
    match block {
        MdBlock::Heading { level, inlines } => {
            let (role, size) = match level {
                1 => (ThemeRole::MdH1, 16.5),
                2 => (ThemeRole::MdH2, 15.0),
                3 => (ThemeRole::MdH3, 14.0),
                4 => (ThemeRole::MdH4, 13.5),
                5 => (ThemeRole::MdH5, 13.0),
                _ => (ThemeRole::MdH6, 12.5),
            };
            render_inline_paragraph(inlines, theme, core, size, theme.color(role), FontWeight::BOLD)
        }
        MdBlock::Paragraph { inlines } => render_inline_paragraph(
            inlines,
            theme,
            core,
            13.0,
            theme.color(ThemeRole::Text),
            FontWeight::NORMAL,
        ),
        MdBlock::Code { lang, text, .. } => MarkdownCodeBlock { lang, code: text }.into_element(),
        MdBlock::Quote { blocks: inner_blocks } => {
            let mut inner = rect().width(Size::fill()).spacing(6.0);
            for child in inner_blocks {
                inner = inner.child(render_block(child, theme, core));
            }
            rect()
                .width(Size::fill())
                .overflow(Overflow::Clip)
                .padding(Gaps::new(2.0, 0.0, 2.0, 14.0))
                .child(
                    rect()
                        .position(Position::new_absolute().left(2.0).top(2.0))
                        .width(Size::px(3.0))
                        .height(Size::percent(100.0))
                        .corner_radius(CornerRadius::new_all(999.0))
                        .background(theme.color(ThemeRole::MdQuote)),
                )
                .child(rect().width(Size::fill()).opacity(0.88).child(inner))
                .into_element()
        }
        MdBlock::List { ordered, start, items } => {
            let mut list = rect().width(Size::fill()).spacing(5.0);
            for (idx, item_blocks) in items.into_iter().enumerate() {
                let marker = if ordered {
                    format!("{}.", start + idx as u64)
                } else {
                    "•".to_string()
                };
                let mut item_col = rect().width(Size::flex(1.0)).spacing(4.0);
                for sub in item_blocks {
                    item_col = item_col.child(render_block(sub, theme, core));
                }
                list = list.child(
                    rect()
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::start())
                        .spacing(7.0)
                        .child(
                            rect()
                                .min_width(Size::px(if ordered { 18.0 } else { 10.0 }))
                                .padding(Gaps::new(1.0, 0.0, 0.0, 2.0))
                                .child(
                                    label()
                                        .text(marker)
                                        .font_size(12.5)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .color(theme.color(ThemeRole::MdBullet)),
                                ),
                        )
                        .child(item_col),
                );
            }
            list.into_element()
        }
        MdBlock::Rule => rect()
            .width(Size::fill())
            .height(Size::px(1.0))
            .margin(Gaps::new(4.0, 0.0, 4.0, 0.0))
            .background(theme.color(ThemeRole::MdRule))
            .into_element(),
        MdBlock::Table { header, align, rows } => render_table(header, align, rows, theme, core),
    }
}

fn render_table(
    headers: Vec<Vec<MdInline>>,
    aligns: Vec<ColumnAlign>,
    rows: Vec<Vec<Vec<MdInline>>>,
    theme: &ResolvedTheme,
    core: &crate::ui::core_view::CoreView,
) -> Element {
    let border_col = theme.color(ThemeRole::BorderSubtle);
    let head_bg = match theme.scheme {
        ResolvedScheme::Dark => Color::from_argb(26, 255, 255, 255),
        ResolvedScheme::Light => Color::from_argb(14, 20, 24, 38),
    };
    let stripe_bg = match theme.scheme {
        ResolvedScheme::Dark => Color::from_argb(12, 255, 255, 255),
        ResolvedScheme::Light => Color::from_argb(8, 20, 24, 38),
    };

    let mut head_row = rect()
        .width(Size::fill())
        .direction(Direction::Horizontal)
        .content(Content::Flex)
        .background(head_bg);
    for (idx, cell) in headers.into_iter().enumerate() {
        let align = aligns.get(idx).copied().unwrap_or(ColumnAlign::Left);
        head_row = head_row.child(
            rect()
                .width(Size::flex(1.0))
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(6.0, 9.0, 6.0, 9.0))
                        .direction(Direction::Horizontal)
                        .main_align(column_main_align(align))
                        .child(render_inline_paragraph(
                            cell,
                            theme,
                            core,
                            11.5,
                            theme.color(ThemeRole::MdTableHead),
                            FontWeight::SEMI_BOLD,
                        )),
                )
                .maybe_child((idx > 0).then(|| {
                    rect()
                        .position(Position::new_absolute().left(0.0).top(0.0))
                        .width(Size::px(1.0))
                        .height(Size::percent(100.0))
                        .background(border_col)
                })),
        );
    }

    let mut table = rect()
        .width(Size::fill())
        .corner_radius(CornerRadius::new_all(8.0))
        .overflow(Overflow::Clip)
        .border(
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(border_col),
        )
        .child(head_row);

    for (row_idx, row) in rows.into_iter().enumerate() {
        let mut r = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .background(if row_idx % 2 == 1 {
                stripe_bg
            } else {
                Color::TRANSPARENT
            });
        for (col_idx, cell) in row.into_iter().enumerate() {
            let align = aligns.get(col_idx).copied().unwrap_or(ColumnAlign::Left);
            r = r.child(
                rect()
                    .width(Size::flex(1.0))
                    .child(
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(6.0, 9.0, 6.0, 9.0))
                            .direction(Direction::Horizontal)
                            .main_align(column_main_align(align))
                            .child(render_inline_paragraph(
                                cell,
                                theme,
                                core,
                                11.5,
                                theme.color(ThemeRole::Text),
                                FontWeight::NORMAL,
                            )),
                    )
                    .maybe_child((col_idx > 0).then(|| {
                        rect()
                            .position(Position::new_absolute().left(0.0).top(0.0))
                            .width(Size::px(1.0))
                            .height(Size::percent(100.0))
                            .background(border_col)
                    })),
            );
        }
        table = table.child(rect().width(Size::fill()).height(Size::px(1.0)).background(border_col));
        table = table.child(r);
    }

    table.into_element()
}

fn column_main_align(align: ColumnAlign) -> Alignment {
    match align {
        ColumnAlign::None | ColumnAlign::Left => Alignment::start(),
        ColumnAlign::Center => Alignment::center(),
        ColumnAlign::Right => Alignment::end(),
    }
}

fn render_inline_paragraph(
    inlines: Vec<MdInline>,
    theme: &ResolvedTheme,
    core: &crate::ui::core_view::CoreView,
    base_size: f32,
    base_color: Color,
    base_weight: FontWeight,
) -> Element {
    let mut links: Vec<(String, String)> = Vec::new();
    let mut p = paragraph().width(Size::fill()).line_height(1.48);
    append_spans(
        &mut p,
        &inlines,
        theme,
        base_size,
        base_color,
        base_weight,
        FontSlant::Upright,
        false,
        &mut links,
    );

    if links.is_empty() {
        p.into_element()
    } else {
        // Render the paragraph plus subtle clickable link chips below when links are present so users can open any
        // referenced URL directly in a new tab.
        let mut link_row = rect().width(Size::fill()).direction(Direction::Horizontal).spacing(6.0);
        for (label_text, href) in links.into_iter().take(4) {
            let core_open = core.clone();
            let url = href.clone();
            link_row = link_row.child(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(format!("Open {href}"))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(4.0)
                    .padding(Gaps::new(2.0, 7.0, 2.0, 6.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(theme.rgba(ThemeRole::MdLink).with_alpha(0.14).to_color())
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        core_open.invoke_or_log(Command::TabNew {
                            url: Some(url.clone()),
                            after: None,
                            activate: true,
                        });
                    })
                    .child(Icon::new("link").size(10.5).role(ThemeRole::MdLink))
                    .child(
                        label()
                            .text(label_text)
                            .font_size(11.0)
                            .font_weight(FontWeight::MEDIUM)
                            .color(theme.color(ThemeRole::MdLink))
                            .max_lines(1),
                    ),
            );
        }
        rect()
            .width(Size::fill())
            .spacing(4.0)
            .child(p)
            .child(link_row)
            .into_element()
    }
}

#[allow(clippy::too_many_arguments)]
fn append_spans(
    p: &mut Paragraph,
    inlines: &[MdInline],
    theme: &ResolvedTheme,
    size: f32,
    color: Color,
    weight: FontWeight,
    slant: FontSlant,
    strike: bool,
    links: &mut Vec<(String, String)>,
) {
    for node in inlines {
        match node {
            MdInline::Text(text) => {
                let mut sp = Span::new(text.clone())
                    .font_size(size)
                    .color(color)
                    .font_weight(weight)
                    .font_slant(slant);
                if strike {
                    sp = sp.text_decoration(TextDecoration::LineThrough);
                }
                *p = std::mem::take(p).span(sp);
            }
            MdInline::Break => {
                *p = std::mem::take(p).span(Span::new("\n").font_size(size));
            }
            MdInline::Strong(children) => {
                append_spans(
                    p,
                    children,
                    theme,
                    size,
                    theme.color(ThemeRole::MdStrong),
                    FontWeight::BOLD,
                    slant,
                    strike,
                    links,
                );
            }
            MdInline::Em(children) => {
                append_spans(
                    p,
                    children,
                    theme,
                    size,
                    theme.color(ThemeRole::MdEm),
                    weight,
                    FontSlant::Italic,
                    strike,
                    links,
                );
            }
            MdInline::Del(children) => {
                append_spans(
                    p,
                    children,
                    theme,
                    size,
                    theme.color(ThemeRole::TextMuted),
                    weight,
                    slant,
                    true,
                    links,
                );
            }
            MdInline::Code(code) => {
                let sp = Span::new(format!(" {code} "))
                    .font_size((size - 0.8).max(10.5))
                    .font_family("monospace")
                    .color(theme.color(ThemeRole::MdCode));
                *p = std::mem::take(p).span(sp);
            }
            MdInline::Link { children, href } => {
                let link_text = flatten_inline_text(children);
                if !links.iter().any(|(_, existing)| existing == href) {
                    links.push((link_text.clone(), href.clone()));
                }
                let sp = Span::new(link_text)
                    .font_size(size)
                    .color(theme.color(ThemeRole::MdLink))
                    .font_weight(FontWeight::MEDIUM)
                    .text_decoration(TextDecoration::Underline);
                *p = std::mem::take(p).span(sp);
            }
        }
    }
}

fn flatten_inline_text(inlines: &[MdInline]) -> String {
    let mut out = String::new();
    for node in inlines {
        match node {
            MdInline::Text(t) | MdInline::Code(t) => out.push_str(t),
            MdInline::Strong(c) | MdInline::Em(c) | MdInline::Del(c) => {
                out.push_str(&flatten_inline_text(c));
            }
            MdInline::Link { children, .. } => out.push_str(&flatten_inline_text(children)),
            MdInline::Break => out.push(' '),
        }
    }
    out
}

/// Fenced code block with language tag, horizontal scroll, and hover Copy button.
#[derive(Clone, PartialEq)]
struct MarkdownCodeBlock {
    lang: Option<String>,
    code: String,
}

impl Component for MarkdownCodeBlock {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let mut copied = use_state(|| false);

        let bg = theme.color(ThemeRole::MdCodeBackground);
        let border_col = theme.color(ThemeRole::BorderSubtle);
        let accent_bar = theme.rgba(ThemeRole::Accent).with_alpha(0.65).to_color();
        let lang_label = self
            .lang
            .as_deref()
            .map(str::to_ascii_uppercase)
            .unwrap_or_else(|| "CODE".to_string());
        let is_copied = *copied.read();
        let show_copy = *hovered.read() || is_copied;
        let code_for_copy = self.code.clone();

        rect()
            .width(Size::fill())
            .corner_radius(CornerRadius::new_all(8.0))
            .overflow(Overflow::Clip)
            .background(bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_col),
            )
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .child(
                rect()
                    .position(Position::new_absolute().left(0.0).top(0.0))
                    .width(Size::px(3.0))
                    .height(Size::percent(100.0))
                    .background(accent_bar),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(7.0, 10.0, 9.0, 13.0))
                    .spacing(4.0)
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .main_align(Alignment::SpaceBetween)
                            .child(
                                label()
                                    .text(lang_label)
                                    .font_size(9.5)
                                    .font_weight(FontWeight::BOLD)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            )
                            .maybe_child(show_copy.then(|| {
                                let copy_text = code_for_copy.clone();
                                rect()
                                    .a11y_role(AccessibilityRole::Button)
                                    .a11y_alt("Copy code block")
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(4.0)
                                    .height(Size::px(20.0))
                                    .padding(Gaps::new(0.0, 7.0, 0.0, 6.0))
                                    .corner_radius(CornerRadius::new_all(5.0))
                                    .background(theme.rgba(ThemeRole::SurfaceRaised).with_alpha(0.75).to_color())
                                    .cursor(CursorIcon::Pointer)
                                    .on_press(move |e: Event<PressEventData>| {
                                        e.stop_propagation();
                                        copy_to_clipboard(&copy_text);
                                        copied.set(true);
                                        spawn(async move {
                                            timer(Duration::from_millis(1500)).await;
                                            copied.set(false);
                                        });
                                    })
                                    .child(Icon::new(if is_copied { "check" } else { "copy" }).size(10.5).role(
                                        if is_copied {
                                            ThemeRole::Success
                                        } else {
                                            ThemeRole::TextMuted
                                        },
                                    ))
                                    .child(
                                        label()
                                            .text(if is_copied { "Copied" } else { "Copy" })
                                            .font_size(10.0)
                                            .color(if is_copied {
                                                theme.color(ThemeRole::Success)
                                            } else {
                                                theme.color(ThemeRole::TextMuted)
                                            }),
                                    )
                            })),
                    )
                    .child(
                        label()
                            .text(self.code.clone())
                            .font_size(11.5)
                            .font_family("monospace")
                            .line_height(1.45)
                            .color(theme.color(ThemeRole::Text)),
                    ),
            )
    }
}

/// Best-effort system clipboard copy via `wl-copy` / `xclip` (never blocks UI thread).
pub fn copy_to_clipboard(text: &str) {
    let payload = text.to_string();
    std::thread::spawn(move || {
        use std::io::Write;
        use std::process::{Command as ProcCommand, Stdio};
        let candidates: &[(&str, &[&str])] = &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])];
        for (bin, args) in candidates {
            if let Ok(mut child) = ProcCommand::new(bin)
                .args(*args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(payload.as_bytes());
                }
                let _ = child.wait();
                return;
            }
        }
    });
}
