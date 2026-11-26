/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! What vim mode draws over a page, natively, so a page can neither hide nor fake it:
//! - The labels of a jump (`s`, `gs`, `S`, `i`, `v` and visual mode's `s`). They keep the old Zendia's colors and
//!   places: a control's label on its first line with a thin outline around it, a word's on its first letters, a
//!   paragraph's in the margin beside a bar along it, a typed match's after it with the match tinted, and the text
//!   typed so far in a prompt at the bottom (at the top when what is labeled is down there). A control's label is
//!   drawn larger than the fork's 11 pixel tag; a word's stays about as compact as the fork's, as every word on screen
//!   has one and larger tags would cover the text. All have a light ring and a deeper shadow, over a light dim of the
//!   page, so they stay readable on any page.
//! - Visual mode's cursor, a pink bar at the end of the selection that moves.
//! - The flash of what a yank copied, or of the field a put went in.
//!
//! The page answers in CSS pixels; everything here is scaled by the page's zoom, so marks stay on what they name and
//! labels grow with the page.

use crate::ui::core_view::use_core;
use freya::prelude::*;
use std::time::Duration;
use zendia_automation::{JumpKind, ViewportRect};
use zendia_browser_core::model::{InputMode, TabId};
use zendia_browser_core::page_actions::{PageHintItem, PageHintSession};

/// The browser's own label color, and a blue for paragraphs.
const LABEL_BACKGROUND: Color = Color::from_rgb(0xa2, 0x57, 0x72);
const PARAGRAPH_BACKGROUND: Color = Color::from_rgb(0x3d, 0x59, 0xa1);
const LABEL_TEXT: Color = Color::from_rgb(0xff, 0xff, 0xff);
/// A ring around each label that sets it apart from dark pages and from the label next to it.
const LABEL_RING: Color = Color::from_argb(72, 0xff, 0xff, 0xff);
const LABEL_SHADOW: Color = Color::from_argb(128, 0, 0, 0);
/// The letters of a label already typed: the palette's yellow and ink.
const TYPED_BACKGROUND: Color = Color::from_rgb(0xff, 0xc7, 0x77);
const TYPED_TEXT: Color = Color::from_rgb(0x1b, 0x1d, 0x2b);
const MATCH_BACKGROUND: Color = Color::from_argb(102, 0xff, 0xc7, 0x77);
const TARGET_OUTLINE: Color = Color::from_argb(150, 0xa2, 0x57, 0x72);
/// The page dims a little while labels are up, so they stand out from it.
const BACKDROP: Color = Color::from_argb(20, 0, 0, 0);
/// The cursor has the pink of NORMAL.
const CURSOR_COLOR: Color = Color::from_rgb(0xfc, 0xa7, 0xea);
const FLASH_COLOR: Color = Color::from_argb(140, 0xff, 0xc7, 0x77);
const FLASH_TIME: Duration = Duration::from_millis(300);
/// How tall the band at the bottom of the page the prompt takes.
const PROMPT_BAND: f32 = 52.0;
/// How far apart in height, in CSS pixels, two targets can be and still be on one row.
const ROW_TOLERANCE: f32 = 8.0;
/// How wide a character of the labels' monospace font is, as a part of its size.
const MONOSPACE_ADVANCE: f32 = 0.62;

/// The size of a label, by what it names: a control's is larger than a word's, which sits inside a line of text.
#[derive(Clone, Copy)]
struct LabelMetrics {
    font_size: f32,
    height: f32,
    padding: f32,
}

impl LabelMetrics {
    fn for_kind(kind: &str, zoom: f32) -> Self {
        let small = matches!(kind, "word" | "paragraph");
        let (font_size, height, padding) = if small { (11.0, 15.0, 3.0) } else { (14.0, 20.0, 5.0) };
        Self {
            font_size: font_size * zoom,
            height: height * zoom,
            padding: padding * zoom,
        }
    }

    fn width(&self, label: &str) -> f32 {
        label.chars().count() as f32 * self.font_size * MONOSPACE_ADVANCE + self.padding * 2.0 + 2.0
    }
}

/// Draws vim mode's marks over one page.
#[derive(Clone, PartialEq)]
pub struct PageHintsOverlay {
    pub tab_id: Option<TabId>,
    /// The page's viewport, in logical pixels.
    pub viewport: (f32, f32),
    pub key: DiffKey,
}

impl PageHintsOverlay {
    pub fn new(tab_id: Option<TabId>, viewport: (f32, f32)) -> Self {
        Self {
            tab_id,
            viewport,
            key: DiffKey::None,
        }
    }
}

impl KeyExt for PageHintsOverlay {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for PageHintsOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let mut flashed = use_state(|| 0u64);
        let model = core.model().read();

        let empty = || rect().width(Size::px(0.0)).height(Size::px(0.0)).into_element();
        if !model.vim_mode_enabled || self.tab_id.is_none() {
            return empty();
        }

        let hints = model
            .page_hints
            .as_ref()
            .filter(|session| Some(session.tab_id) == self.tab_id && session.awaiting.as_deref() != Some(""));
        let caret = (model.active_tab == self.tab_id && model.input_mode == InputMode::Visual)
            .then_some(model.visual_caret)
            .flatten();
        let flash = model
            .page_flash
            .as_ref()
            .filter(|flash| Some(flash.tab_id) == self.tab_id && flash.serial > *flashed.read());

        if let Some(flash) = flash {
            let serial = flash.serial;
            spawn(async move {
                timer(FLASH_TIME).await;
                if *flashed.peek() < serial {
                    flashed.set(serial);
                }
            });
        }

        if hints.is_none() && caret.is_none() && flash.is_none() {
            return empty();
        }
        let zoom = if model.active_tab == self.tab_id && model.zoom_level > 0.0 {
            model.zoom_level
        } else {
            1.0
        };

        let mut layer = rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .layer(Layer::Relative(10))
            .interactive(Interactive::No);

        if let Some(flash) = flash {
            for each in &flash.rects {
                let each = scaled(*each, zoom);
                layer = layer.child(
                    rect()
                        .position(Position::new_absolute().left(each.x).top(each.y))
                        .width(Size::px(each.width.max(0.0)))
                        .height(Size::px(each.height.max(0.0)))
                        .corner_radius(CornerRadius::new_all(2.0))
                        .background(FLASH_COLOR),
                );
            }
        }

        if let Some(caret) = caret {
            let caret = scaled(caret, zoom);
            layer = layer.child(
                rect()
                    .position(Position::new_absolute().left(caret.x).top(caret.y))
                    .width(Size::px(2.0))
                    .height(Size::px(caret.height.max(1.0)))
                    .background(CURSOR_COLOR),
            );
        }

        if let Some(session) = hints {
            layer = draw_session(layer, session, self.viewport, zoom);
        }

        layer.into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// A rectangle of the page, in CSS pixels, in the logical pixels of the overlay.
fn scaled(area: ViewportRect, zoom: f32) -> ViewportRect {
    ViewportRect {
        x: area.x * zoom,
        y: area.y * zoom,
        width: area.width * zoom,
        height: area.height * zoom,
    }
}

fn draw_session(mut layer: Rect, session: &PageHintSession, viewport: (f32, f32), zoom: f32) -> Rect {
    let typed = if session.kind.labels_at_once() {
        session.typed.chars().count()
    } else {
        0
    };
    // Dim the page once there are labels to read; a text jump waits for its first character before it has any.
    if session.visible_items().any(|item| item.label.is_some()) {
        layer = layer.child(
            rect()
                .position(Position::new_absolute().left(0.0).top(0.0))
                .width(Size::percent(100.0))
                .height(Size::percent(100.0))
                .background(BACKDROP),
        );
    }
    let mut placed = Placed::default();
    // Labels are placed in reading order, so of two that would collide, the one on the left keeps its place and the
    // other moves along to the right of it. Targets a few pixels apart in height (a checkbox and the link beside it)
    // are on one row.
    let mut items: Vec<&PageHintItem> = session.visible_items().collect();
    items.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y));
    let mut row_top = f32::NEG_INFINITY;
    let mut row = 0usize;
    let mut rows = Vec::with_capacity(items.len());
    for item in &items {
        if item.rect.y - row_top > ROW_TOLERANCE {
            row += 1;
            row_top = item.rect.y;
        }
        rows.push(row);
    }
    let mut items: Vec<(usize, &PageHintItem)> = rows.into_iter().zip(items).collect();
    items.sort_by(|(a_row, a), (b_row, b)| a_row.cmp(b_row).then(a.rect.x.total_cmp(&b.rect.x)));
    for (_, item) in items {
        let area = scaled(item.rect, zoom);
        if let Some(mark) = mark(item, area, session) {
            layer = layer.child(mark);
        }
        if let Some(label) = item.label.as_deref() {
            layer = layer.child(tag(item, area, label, typed, session, viewport, zoom, &mut placed));
        }
    }
    if session.shows_prompt() {
        let below = viewport.1 - PROMPT_BAND;
        let at_top = session
            .items
            .iter()
            .any(|item| (item.rect.y + item.rect.height) * zoom > below);
        let position = if at_top {
            Position::new_absolute().left(0.0).top(16.0)
        } else {
            Position::new_absolute().left(0.0).bottom(16.0)
        };
        layer = layer.child(
            rect()
                .position(position)
                .width(Size::percent(100.0))
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .child(
                    rect()
                        .padding(Gaps::new(3.0, 12.0, 3.0, 12.0))
                        .corner_radius(CornerRadius::new_all(7.0))
                        .background(LABEL_BACKGROUND)
                        .border(
                            Border::new()
                                .fill(LABEL_RING)
                                .width(1.0)
                                .alignment(BorderAlignment::Inner),
                        )
                        .shadow(Shadow::new().y(2.0).blur(10.0).color(LABEL_SHADOW))
                        .child(
                            label()
                                .text(format!("\u{203a} {}", session.pattern))
                                .font_family("monospace")
                                .font_weight(FontWeight::BOLD)
                                .font_size(14.0)
                                .line_height(20.0 / 14.0)
                                .color(LABEL_TEXT)
                                .max_lines(1),
                        ),
                ),
        );
    }
    layer
}

/// The mark on what a label names: a bar along a paragraph, a tint on a typed match, an outline around a control.
/// A word is marked by its label alone.
fn mark(item: &PageHintItem, area: ViewportRect, session: &PageHintSession) -> Option<Rect> {
    let at = Position::new_absolute().left(area.x).top(area.y);
    match item.kind.as_str() {
        "word" => None,
        "paragraph" => Some(
            rect()
                .position(Position::new_absolute().left(area.x - 6.0).top(area.y))
                .width(Size::px(3.0))
                .height(Size::px(area.height))
                .corner_radius(CornerRadius::new_all(2.0))
                .background(PARAGRAPH_BACKGROUND),
        ),
        "match" => Some(
            rect()
                .position(at)
                .width(Size::px(area.width))
                .height(Size::px(area.height))
                .corner_radius(CornerRadius::new_all(3.0))
                .background(MATCH_BACKGROUND),
        ),
        _ if session.kind == JumpKind::Text => None,
        _ => Some(
            rect()
                .position(at)
                .width(Size::px(area.width))
                .height(Size::px(area.height))
                .corner_radius(CornerRadius::new_all(3.0))
                .border(
                    Border::new()
                        .fill(TARGET_OUTLINE)
                        .width(1.0)
                        .alignment(BorderAlignment::Outer),
                ),
        ),
    }
}

/// The labels placed so far, so the next one doesn't cover them.
#[derive(Default)]
struct Placed {
    /// Each control label's left edge, top and width.
    controls: Vec<(f32, f32, f32)>,
}

/// A label: on a word's first letters, in a paragraph's margin, after a typed match, and on a control's first line
/// at its left edge, moved along when another control's label is already there.
#[allow(clippy::too_many_arguments)]
fn tag(
    item: &PageHintItem,
    area: ViewportRect,
    label_text: &str,
    typed: usize,
    session: &PageHintSession,
    viewport: (f32, f32),
    zoom: f32,
    placed: &mut Placed,
) -> Rect {
    let metrics = LabelMetrics::for_kind(&item.kind, zoom);
    let width = metrics.width(label_text);
    // Centered on the first line of what it names, which a tall control's top line is about as tall as.
    let first_line = area.height.min(28.0 * zoom);
    let centered_top = area.y + ((first_line - metrics.height) / 2.0).max(0.0);
    let (mut left, mut top) = (area.x - 2.0 * zoom, centered_top);
    match item.kind.as_str() {
        "paragraph" => {
            left = area.x - width - 10.0 * zoom;
            top = area.y;
        }
        "word" => {
            left = area.x;
            top = area.y + ((area.height - metrics.height) / 2.0).max(0.0);
        }
        "match" => {
            left = area.x + area.width;
            top = area.y + ((area.height - metrics.height) / 2.0).max(0.0);
        }
        _ if session.kind == JumpKind::Text => {
            left = area.x + area.width;
            top = area.y + ((area.height - metrics.height) / 2.0).max(0.0);
        }
        _ => {
            // Two controls close together (a checkbox and the link beside it) would show one label over the other:
            // the later one moves along just past it, so it stays on what it names.
            let mut tries = 0;
            while tries < 4 {
                let Some(right) = placed
                    .controls
                    .iter()
                    .filter(|(other_left, other_top, other_width)| {
                        left < other_left + other_width
                            && *other_left < left + width
                            && (other_top - top).abs() < metrics.height
                    })
                    .map(|(other_left, _, other_width)| other_left + other_width + zoom)
                    .reduce(f32::max)
                else {
                    break;
                };
                left = right;
                tries += 1;
            }
            placed.controls.push((left, top, width));
        }
    }
    let left = left.min(viewport.0 - width).max(0.0);
    let top = top.min(viewport.1 - metrics.height).max(0.0);
    let text = |text: String, color: Color| {
        label()
            .text(text)
            .font_family("monospace")
            .font_weight(FontWeight::BOLD)
            .font_size(metrics.font_size)
            .line_height(metrics.height / metrics.font_size)
            .color(color)
            .max_lines(1)
    };
    let background = if item.kind == "paragraph" {
        PARAGRAPH_BACKGROUND
    } else {
        LABEL_BACKGROUND
    };
    let split = label_text
        .char_indices()
        .nth(typed)
        .map_or(label_text.len(), |(index, _)| index);
    let (pressed, rest) = label_text.split_at(split);
    let radius = 4.0 * zoom;
    let mut badge = rect()
        .position(Position::new_absolute().left(left).top(top))
        .height(Size::px(metrics.height))
        .padding(Gaps::new(0.0, metrics.padding, 0.0, metrics.padding))
        .corner_radius(CornerRadius::new_all(radius))
        .background(background)
        .border(
            Border::new()
                .fill(LABEL_RING)
                .width(1.0)
                .alignment(BorderAlignment::Inner),
        )
        .shadow(Shadow::new().y(1.0).blur(4.0).color(LABEL_SHADOW))
        .cross_align(Alignment::center())
        .direction(Direction::Horizontal);
    if !pressed.is_empty() {
        // What is pressed stands apart from what is left to press, up to the label's edge.
        badge = badge.child(
            rect()
                .height(Size::fill())
                .cross_align(Alignment::center())
                .margin(Gaps::new(0.0, 0.0, 0.0, -metrics.padding))
                .padding(Gaps::new(0.0, 1.0 * zoom, 0.0, metrics.padding))
                .corner_radius(CornerRadius::new(radius, 0.0, 0.0, radius))
                .background(TYPED_BACKGROUND)
                .child(text(pressed.to_string(), TYPED_TEXT)),
        );
    }
    if !rest.is_empty() {
        badge = badge.child(text(rest.to_string(), LABEL_TEXT));
    }
    badge
}
