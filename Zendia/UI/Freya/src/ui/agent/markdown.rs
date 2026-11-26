/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Native Freya GitHub-Flavored Markdown renderer for Zendia's agent replies.
//!
//! Renders the streaming-safe AST produced by [`zendia_browser_core::markdown::parse_markdown`] directly into Freya
//! `paragraph`, `span`, `rect`, and horizontal `ScrollView` nodes using Zendia's theme markdown roles (`MdH1`..`MdH6`,
//! `MdStrong`, `MdEm`, `MdLink`, `MdCode`, `MdCodeBackground`, `MdQuote`, `MdBullet`, `MdRule`, `MdTableHead`).
//!
//! Includes per-chunk streaming `VeilTracker` fade-in (`AM-02`), inline code background pills (`MD-02`), full wrapped
//! link chips with hover/URL tooltips (`MD-03`), horizontally scrolling fenced code blocks with `120 ms` hover Copy
//! (`MD-04`), horizontally scrolling Markdown tables (`MD-05`), and exact fork heading/blockquote styling (`MD-06`).

use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, KitTooltip, focus_ring_border, use_interactive_feedback};
use crate::ui::theme::{ResolvedTheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::{Duration, Instant};
use zendia_browser_core::agent::{VeilTracker, veil_alpha};
use zendia_browser_core::commands::Command;
use zendia_browser_core::markdown::{ColumnAlign, MdBlock, MdInline, parse_markdown};

/// Renders a Markdown string using Zendia's GFM AST parser, theme markdown roles, and streaming veil fade-in.
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
        let started = use_hook(Instant::now);
        let mut veil = use_state(VeilTracker::default);
        let mut last_text = use_state(|| self.text.clone());
        let mut veil_tick_running = use_state(|| false);
        let mut veil_frame = use_state(|| 0u64);

        let now_ms = started.elapsed().as_secs_f32() * 1000.0;
        let baseline = !self.streaming;
        if *last_text.peek() != self.text || (baseline && veil.peek().has_active_chunks()) {
            last_text.set(self.text.clone());
            let mut tracker = veil.peek().clone();
            tracker.update(&self.text, now_ms, baseline);
            let active = tracker.has_active_chunks();
            veil.set(tracker);
            if active && !*veil_tick_running.peek() {
                veil_tick_running.set(true);
                let frame_step = Duration::from_millis(24);
                let _min_ms = motion::AGENT_VEIL_MIN.as_millis();
                let _max_ms = motion::AGENT_VEIL_MAX.as_millis();
                spawn(async move {
                    loop {
                        timer(frame_step).await;
                        let t_ms = started.elapsed().as_secs_f32() * 1000.0;
                        let mut tr = veil.peek().clone();
                        tr.tick(t_ms);
                        let still = tr.has_active_chunks();
                        veil.set(tr);
                        let next_f = veil_frame.peek().wrapping_add(1);
                        veil_frame.set(next_f);
                        if !still {
                            veil_tick_running.set(false);
                            break;
                        }
                    }
                });
            }
        }

        let _ = *veil_frame.read();
        let tracker_snapshot = veil.read().clone();
        let blocks = parse_markdown(&self.text, self.streaming);

        let mut col = rect().width(Size::fill()).spacing(8.0);
        let total = blocks.len();
        let mut char_cursor = 0usize;
        let ctx = RenderCtx {
            streaming: self.streaming,
            base_text_override: None,
            theme: &theme,
            core: &core,
            veil: &tracker_snapshot,
        };
        for (idx, block) in blocks.into_iter().enumerate() {
            let is_last = idx + 1 == total;
            let el = render_block(block, idx == 0, &ctx, &mut char_cursor);
            if is_last && self.streaming && self.veil_opacity < 0.999 {
                let alpha = veil_alpha(self.veil_opacity);
                col = col.child(rect().width(Size::fill()).opacity(alpha).child(el));
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

#[derive(Clone, Copy)]
struct SpanStyle {
    size: f32,
    color: Color,
    weight: FontWeight,
    slant: FontSlant,
    strike: bool,
}

struct RenderCtx<'a> {
    streaming: bool,
    base_text_override: Option<Color>,
    theme: &'a ResolvedTheme,
    core: &'a crate::ui::core_view::CoreView,
    veil: &'a VeilTracker,
}

fn render_block(block: MdBlock, is_first: bool, ctx: &RenderCtx<'_>, char_cursor: &mut usize) -> Element {
    let theme = ctx.theme;
    let core = ctx.core;
    let veil = ctx.veil;
    match block {
        MdBlock::Heading { level, inlines } => {
            // MD-06: Exact fork heading sizes (1.3 / 1.15 / 1.05 / 1 / 1 / 1 em on 13px base).
            let (role, size) = match level {
                1 => (ThemeRole::MdH1, 16.9),
                2 => (ThemeRole::MdH2, 14.95),
                3 => (ThemeRole::MdH3, 13.65),
                4 => (ThemeRole::MdH4, 13.0),
                5 => (ThemeRole::MdH5, 13.0),
                _ => (ThemeRole::MdH6, 13.0),
            };
            let style = SpanStyle {
                size,
                color: theme.color(role),
                weight: FontWeight::BOLD,
                slant: FontSlant::Upright,
                strike: false,
            };
            let el = render_inline_paragraph(inlines, theme, core, style, 1.3, veil, char_cursor);
            if is_first {
                el
            } else {
                rect()
                    .width(Size::fill())
                    .margin(Gaps::new(6.0, 0.0, 0.0, 0.0))
                    .child(el)
                    .into_element()
            }
        }
        MdBlock::Paragraph { inlines } => {
            let style = SpanStyle {
                size: 13.0,
                color: ctx.base_text_override.unwrap_or_else(|| theme.color(ThemeRole::Text)),
                weight: FontWeight::NORMAL,
                slant: FontSlant::Upright,
                strike: false,
            };
            render_inline_paragraph(inlines, theme, core, style, 1.48, veil, char_cursor)
        }
        MdBlock::Code { lang, text, .. } => {
            *char_cursor += text.chars().count();
            MarkdownCodeBlock {
                lang,
                code: text,
                streaming: ctx.streaming,
                key: DiffKey::None,
            }
            .into_element()
        }
        MdBlock::Quote { blocks: inner_blocks } => {
            // MD-06: Blockquote passes ThemeRole::TextMuted instead of opacity(0.88).
            let quote_ctx = RenderCtx {
                base_text_override: Some(theme.color(ThemeRole::TextMuted)),
                ..*ctx
            };
            let mut inner = rect().width(Size::fill()).spacing(6.0);
            for (i, child) in inner_blocks.into_iter().enumerate() {
                inner = inner.child(render_block(child, i == 0, &quote_ctx, char_cursor));
            }
            rect()
                .width(Size::fill())
                .overflow(Overflow::Clip)
                .padding(Gaps::new(2.0, 0.0, 2.0, 13.0))
                .child(
                    rect()
                        .position(Position::new_absolute().left(0.0).top(2.0))
                        .width(Size::px(3.0))
                        .height(Size::percent(100.0))
                        .corner_radius(CornerRadius::new_all(999.0))
                        .background(theme.color(ThemeRole::MdQuote)),
                )
                .child(inner)
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
                for (sub_i, sub) in item_blocks.into_iter().enumerate() {
                    item_col = item_col.child(render_block(sub, sub_i == 0, ctx, char_cursor));
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
        MdBlock::Table { header, align, rows } => render_table(header, align, rows, theme, core, veil, char_cursor),
    }
}

/// Computes intrinsic minimum widths for each Markdown table column so wide tables scroll sideways cleanly (`MD-05`).
fn estimate_table_col_widths(headers: &[Vec<MdInline>], rows: &[Vec<Vec<MdInline>>]) -> Vec<f32> {
    let col_count = headers.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
    let mut widths = vec![68.0f32; col_count];
    for (c, cell) in headers.iter().enumerate() {
        let len = flatten_inline_text(cell).chars().count();
        widths[c] = widths[c].max((len as f32 * 8.0 + 26.0).clamp(76.0, 240.0));
    }
    for row in rows {
        for (c, cell) in row.iter().enumerate() {
            if c < widths.len() {
                let len = flatten_inline_text(cell).chars().count();
                widths[c] = widths[c].max((len as f32 * 8.2 + 28.0).clamp(76.0, 240.0));
            }
        }
    }
    widths
}

fn render_table(
    headers: Vec<Vec<MdInline>>,
    aligns: Vec<ColumnAlign>,
    rows: Vec<Vec<Vec<MdInline>>>,
    theme: &ResolvedTheme,
    core: &crate::ui::core_view::CoreView,
    veil: &VeilTracker,
    char_cursor: &mut usize,
) -> Element {
    // MD-05: 1px MdRule borders, padding 4x8, 12px text, bold nowrap th, no zebra stripes or shaded header,
    // wrapped in a horizontal ScrollView when wider than the panel.
    let rule_col = theme.color(ThemeRole::MdRule);
    let col_widths = estimate_table_col_widths(&headers, &rows);
    let total_w: f32 = col_widths.iter().sum();

    let mut head_row = rect().width(Size::px(total_w)).direction(Direction::Horizontal);
    for (idx, cell) in headers.into_iter().enumerate() {
        let align = aligns.get(idx).copied().unwrap_or(ColumnAlign::Left);
        let cw = col_widths.get(idx).copied().unwrap_or(96.0);
        let text = flatten_inline_text(&cell);
        *char_cursor += text.chars().count();
        head_row = head_row.child(
            rect()
                .width(Size::px(cw))
                .padding(Gaps::new(4.0, 8.0, 4.0, 8.0))
                .direction(Direction::Horizontal)
                .main_align(column_main_align(align))
                .maybe_child((idx > 0).then(|| {
                    rect()
                        .position(Position::new_absolute().left(0.0).top(0.0))
                        .width(Size::px(1.0))
                        .height(Size::percent(100.0))
                        .background(rule_col)
                }))
                .child(
                    label()
                        .text(text)
                        .font_size(12.0)
                        .font_weight(FontWeight::BOLD)
                        .color(theme.color(ThemeRole::MdTableHead))
                        .max_lines(1),
                ),
        );
    }

    let mut table = rect()
        .width(Size::px(total_w))
        .min_width(Size::percent(100.0))
        .border(
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(rule_col),
        )
        .child(head_row);

    let row_count = rows.len();
    for row in rows {
        let mut r = rect().width(Size::px(total_w)).direction(Direction::Horizontal);
        for (col_idx, cell) in row.into_iter().enumerate() {
            let align = aligns.get(col_idx).copied().unwrap_or(ColumnAlign::Left);
            let cw = col_widths.get(col_idx).copied().unwrap_or(96.0);
            let style = SpanStyle {
                size: 12.0,
                color: theme.color(ThemeRole::Text),
                weight: FontWeight::NORMAL,
                slant: FontSlant::Upright,
                strike: false,
            };
            r = r.child(
                rect()
                    .width(Size::px(cw))
                    .padding(Gaps::new(4.0, 8.0, 4.0, 8.0))
                    .direction(Direction::Horizontal)
                    .main_align(column_main_align(align))
                    .maybe_child((col_idx > 0).then(|| {
                        rect()
                            .position(Position::new_absolute().left(0.0).top(0.0))
                            .width(Size::px(1.0))
                            .height(Size::percent(100.0))
                            .background(rule_col)
                    }))
                    .child(render_inline_paragraph(
                        cell,
                        theme,
                        core,
                        style,
                        1.4,
                        veil,
                        char_cursor,
                    )),
            );
        }
        table = table.child(
            rect()
                .width(Size::px(total_w))
                .height(Size::px(1.0))
                .background(rule_col),
        );
        table = table.child(r);
    }

    let est_h = ((row_count + 1) as f32 * 26.0 + 4.0).max(32.0);
    HorizontalAutoHeightScroll {
        est_height: est_h,
        child: table.into_element(),
        key: DiffKey::None,
    }
    .into_element()
}

/// Horizontal [`ScrollView`] that sizes its height to its content via `on_sized` so it never expands to fill a parent
/// vertical scroll container in Freya 0.5.4.
#[derive(Clone, PartialEq)]
struct HorizontalAutoHeightScroll {
    est_height: f32,
    child: Element,
    key: DiffKey,
}

impl KeyExt for HorizontalAutoHeightScroll {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for HorizontalAutoHeightScroll {
    fn render(&self) -> impl IntoElement {
        let mut measured_h = use_state(|| 0.0f32);
        let h = {
            let m = *measured_h.read();
            if m > 1.0 { m } else { self.est_height }
        };
        ScrollView::new()
            .width(Size::fill())
            .height(Size::px(h))
            .direction(Direction::Horizontal)
            .child(
                rect()
                    .on_sized(move |e: Event<SizedEventData>| {
                        let ch = e.area.size.height;
                        if ch > 1.0 && (ch - *measured_h.peek()).abs() > 0.5 {
                            measured_h.set(ch);
                        }
                    })
                    .child(self.child.clone()),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
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
    style: SpanStyle,
    line_height: f32,
    veil: &VeilTracker,
    char_cursor: &mut usize,
) -> Element {
    let mut links: Vec<(String, String)> = Vec::new();
    let mut code_highlights: Vec<(usize, usize)> = Vec::new();
    let mut utf16_offset = 0usize;
    let mut p = paragraph().width(Size::fill()).line_height(line_height);
    let mut ctx = SpanRenderCtx {
        theme,
        links: &mut links,
        code_highlights: &mut code_highlights,
        utf16_offset: &mut utf16_offset,
        veil,
        char_cursor,
    };
    append_spans(&mut p, &inlines, style, &mut ctx);

    if !code_highlights.is_empty() {
        p = p
            .highlight_color(theme.rgba(ThemeRole::MdCodeBackground).with_alpha(0.85).to_color())
            .highlights(Some(code_highlights));
    }

    if links.is_empty() {
        p.into_element()
    } else {
        // MD-03: Lift the 4-link cap, render all links in a scrollable strip with hover feedback & href tooltip.
        let mut link_row = rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(6.0);
        for (idx, (label_text, href)) in links.into_iter().enumerate() {
            let core_open = core.clone();
            let url = href.clone();
            link_row = link_row.child(MarkdownLinkChip {
                label: label_text,
                href,
                on_press: EventHandler::new(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    core_open.invoke_or_log(Command::TabNew {
                        url: Some(url.clone()),
                        after: None,
                        activate: true,
                    });
                }),
                key: DiffKey::from(&format!("md-link-{idx}")),
            });
        }
        rect()
            .width(Size::fill())
            .spacing(4.0)
            .child(p)
            .child(
                ScrollView::new()
                    .width(Size::fill())
                    .height(Size::px(24.0))
                    .direction(Direction::Horizontal)
                    .child(link_row),
            )
            .into_element()
    }
}

#[derive(Clone, PartialEq)]
struct MarkdownLinkChip {
    label: String,
    href: String,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for MarkdownLinkChip {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for MarkdownLinkChip {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let bg = fb.background(
            theme.rgba(ThemeRole::MdLink).with_alpha(0.14).to_color(),
            theme.rgba(ThemeRole::MdLink).with_alpha(0.24).to_color(),
            theme.rgba(ThemeRole::MdLink).with_alpha(0.32).to_color(),
        );
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let on_press = self.on_press.clone();

        KitTooltip::new(self.href.clone()).child(
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(format!("Open {}", self.href))
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(4.0)
                .padding(Gaps::new(2.0, 7.0, 2.0, 6.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(bg)
                .border(border)
                .scale(fb.scale())
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| on_press.call(e))
                .child(Icon::new("link").size(10.5).role(ThemeRole::MdLink))
                .child(
                    label()
                        .text(self.label.clone())
                        .font_size(11.0)
                        .font_weight(FontWeight::MEDIUM)
                        .color(theme.color(ThemeRole::MdLink))
                        .max_lines(1),
                ),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn apply_veil_alpha(color: Color, alpha: f32) -> Color {
    if alpha >= 0.999 {
        color
    } else {
        color.with_a(((color.a() as f32) * alpha.clamp(0.0, 1.0)).round() as u8)
    }
}

struct SpanRenderCtx<'a> {
    theme: &'a ResolvedTheme,
    links: &'a mut Vec<(String, String)>,
    code_highlights: &'a mut Vec<(usize, usize)>,
    utf16_offset: &'a mut usize,
    veil: &'a VeilTracker,
    char_cursor: &'a mut usize,
}

fn push_veiled_text_spans(
    p: &mut Paragraph,
    text: &str,
    style: SpanStyle,
    font_family: Option<&'static str>,
    underline: bool,
    ctx: &mut SpanRenderCtx<'_>,
) {
    let start_c = *ctx.char_cursor;
    let char_len = text.chars().count();
    *ctx.char_cursor += char_len;
    *ctx.utf16_offset += text.encode_utf16().count();

    if !ctx.veil.has_active_chunks() || char_len == 0 {
        let mut sp = Span::new(text.to_string())
            .font_size(style.size)
            .color(style.color)
            .font_weight(style.weight)
            .font_slant(style.slant);
        if let Some(ff) = font_family {
            sp = sp.font_family(ff);
        }
        if style.strike {
            sp = sp.text_decoration(TextDecoration::LineThrough);
        } else if underline {
            sp = sp.text_decoration(TextDecoration::Underline);
        }
        *p = std::mem::take(p).span(sp);
        return;
    }

    // Split along active veil chunk boundaries so older text stays at 100% while new chunks fade in.
    let chars: Vec<char> = text.chars().collect();
    let mut seg_start = 0usize;
    while seg_start < chars.len() {
        let a0 = ctx.veil.alpha_at(start_c + seg_start);
        let mut seg_end = seg_start + 1;
        while seg_end < chars.len() && (ctx.veil.alpha_at(start_c + seg_end) - a0).abs() < 0.02 {
            seg_end += 1;
        }
        let slice: String = chars[seg_start..seg_end].iter().collect();
        let mut sp = Span::new(slice)
            .font_size(style.size)
            .color(apply_veil_alpha(style.color, a0))
            .font_weight(style.weight)
            .font_slant(style.slant);
        if let Some(ff) = font_family {
            sp = sp.font_family(ff);
        }
        if style.strike {
            sp = sp.text_decoration(TextDecoration::LineThrough);
        } else if underline {
            sp = sp.text_decoration(TextDecoration::Underline);
        }
        *p = std::mem::take(p).span(sp);
        seg_start = seg_end;
    }
}

fn append_spans(p: &mut Paragraph, inlines: &[MdInline], style: SpanStyle, ctx: &mut SpanRenderCtx<'_>) {
    for node in inlines {
        match node {
            MdInline::Text(text) => {
                push_veiled_text_spans(p, text, style, None, false, ctx);
            }
            MdInline::Break => {
                *ctx.utf16_offset += 1;
                *ctx.char_cursor += 1;
                *p = std::mem::take(p).span(Span::new("\n").font_size(style.size));
            }
            MdInline::Strong(children) => {
                let child_style = SpanStyle {
                    color: ctx.theme.color(ThemeRole::MdStrong),
                    weight: FontWeight::BOLD,
                    ..style
                };
                append_spans(p, children, child_style, ctx);
            }
            MdInline::Em(children) => {
                let child_style = SpanStyle {
                    color: ctx.theme.color(ThemeRole::MdEm),
                    slant: FontSlant::Italic,
                    ..style
                };
                append_spans(p, children, child_style, ctx);
            }
            MdInline::Del(children) => {
                let child_style = SpanStyle {
                    color: ctx.theme.color(ThemeRole::TextMuted),
                    strike: true,
                    ..style
                };
                append_spans(p, children, child_style, ctx);
            }
            MdInline::Code(code) => {
                // MD-02: Inline code span highlighted with MdCodeBackground 85% via Paragraph::highlights.
                // Use non-breaking spaces so padding never splits onto the previous line before the code token.
                let padded = format!("\u{00A0}{code}\u{00A0}");
                let start_u16 = *ctx.utf16_offset;
                let code_style = SpanStyle {
                    size: (style.size * 0.92).max(10.5),
                    color: ctx.theme.color(ThemeRole::MdCode),
                    ..style
                };
                push_veiled_text_spans(p, &padded, code_style, Some("monospace"), false, ctx);
                ctx.code_highlights.push((start_u16, *ctx.utf16_offset));
            }
            MdInline::Link { children, href } => {
                let link_text = flatten_inline_text(children);
                if !ctx.links.iter().any(|(_, existing)| existing == href) {
                    ctx.links.push((link_text.clone(), href.clone()));
                }
                let link_style = SpanStyle {
                    color: ctx.theme.color(ThemeRole::MdLink),
                    weight: FontWeight::MEDIUM,
                    ..style
                };
                push_veiled_text_spans(p, &link_text, link_style, None, true, ctx);
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

/// Fenced code block (`MD-04`): horizontal scroll, `10 px` radius, `2 px` solid accent left bar (no outer border),
/// language label ONLY when `lang` is `Some`, and top-right hover Copy button with `120 ms` fade hidden while streaming.
#[derive(Clone, PartialEq)]
struct MarkdownCodeBlock {
    lang: Option<String>,
    code: String,
    streaming: bool,
    key: DiffKey,
}

impl KeyExt for MarkdownCodeBlock {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for MarkdownCodeBlock {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let mut copied = use_state(|| false);
        let copy_fb = use_interactive_feedback(true);

        let is_copied = *copied.read();
        let show_copy = !self.streaming && (*hovered.read() || is_copied || copy_fb.is_hovered());
        let copy_fade = use_animation_with_dependencies(&show_copy, |conf, show| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *show { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::HOVER)
                .ease(Ease::Out)
                .function(motion::css_ease_default_fn())
        });

        let bg = theme.rgba(ThemeRole::MdCodeBackground).with_alpha(0.85).to_color();
        let accent_bar = theme.color(ThemeRole::Accent);
        let lang_label = self
            .lang
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_ascii_uppercase);
        let code_for_copy = self.code.clone();
        let copy_opacity = copy_fade.get().value().clamp(0.0, 1.0);
        let copy_bg = copy_fb.background(
            theme.rgba(ThemeRole::Surface).with_alpha(0.80).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.16).to_color(),
        );

        // Compute intrinsic width and estimated height so horizontal ScrollView never wraps or expands vertically.
        let line_count = self.code.lines().count().max(1);
        let max_line_chars = self.code.lines().map(|l| l.chars().count()).max().unwrap_or(1);
        let code_min_w = (max_line_chars as f32 * 7.2 + 12.0).max(200.0);
        let code_est_h = (line_count as f32 * 17.2 + 4.0).max(20.0);

        rect()
            .width(Size::fill())
            .corner_radius(CornerRadius::new_all(10.0))
            .overflow(Overflow::Clip)
            .background(bg)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .child(
                rect()
                    .position(Position::new_absolute().left(0.0).top(0.0))
                    .width(Size::px(2.0))
                    .height(Size::percent(100.0))
                    .background(accent_bar),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(10.0, 12.0, 10.0, 14.0))
                    .spacing(6.0)
                    .maybe_child(lang_label.map(|lang| {
                        label()
                            .text(lang)
                            .font_size(10.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .font_family("monospace")
                            .color(theme.color(ThemeRole::TextMuted))
                    }))
                    .child(HorizontalAutoHeightScroll {
                        est_height: code_est_h,
                        child: rect()
                            .min_width(Size::px(code_min_w))
                            .child(
                                label()
                                    .text(self.code.clone())
                                    .font_size(12.0)
                                    .font_family("monospace")
                                    .line_height(1.4)
                                    .color(theme.color(ThemeRole::Text)),
                            )
                            .into_element(),
                        key: DiffKey::None,
                    }),
            )
            .maybe_child((copy_opacity > 0.01).then(|| {
                rect()
                    .layer(Layer::Relative(3))
                    .position(Position::new_absolute().top(6.0).right(6.0))
                    .opacity(copy_opacity)
                    .child(
                        KitTooltip::new(if is_copied { "Copied" } else { "Copy code" }).child(
                            rect()
                                .a11y_role(AccessibilityRole::Button)
                                .a11y_alt("Copy code")
                                .padding(Gaps::new_all(3.0))
                                .corner_radius(CornerRadius::new_all(6.0))
                                .background(copy_bg)
                                .cursor(CursorIcon::Pointer)
                                .on_pointer_enter(copy_fb.on_pointer_enter())
                                .on_pointer_leave(copy_fb.on_pointer_leave())
                                .on_press(move |e: Event<PressEventData>| {
                                    e.stop_propagation();
                                    copy_to_clipboard(&code_for_copy);
                                    copied.set(true);
                                    spawn(async move {
                                        timer(Duration::from_millis(1500)).await;
                                        copied.set(false);
                                    });
                                })
                                .child(Icon::new(if is_copied { "check" } else { "copy" }).size(13.0).role(
                                    if is_copied {
                                        ThemeRole::Success
                                    } else if copy_fb.is_hovered() {
                                        ThemeRole::Text
                                    } else {
                                        ThemeRole::TextMuted
                                    },
                                )),
                        ),
                    )
            }))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
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
