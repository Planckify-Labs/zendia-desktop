/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! What vim mode draws over a page, natively, so a page can neither hide nor fake it:
//! - The labels of a jump (`s`, `gs`, `S`, `i`, `v` and visual mode's `s`), in the old Zendia's colors and places: a
//!   control's label at its top left corner with a thin outline around it, a word's on its first letters, a
//!   paragraph's in the margin beside a bar along it, a typed match's after it with the match tinted, and the text
//!   typed so far in a prompt at the bottom (at the top when what is labeled is down there).
//! - Visual mode's cursor, a pink bar at the end of the selection that moves.
//! - The flash of what a yank copied, or of the field a put went in.

use crate::ui::core_view::use_core;
use freya::prelude::*;
use std::time::Duration;
use zendia_automation::ViewportRect;
use zendia_browser_core::model::{InputMode, TabId};
use zendia_browser_core::page_actions::{PageHintItem, PageHintSession};

/// The browser's own label color, and a blue for paragraphs.
const LABEL_BACKGROUND: Color = Color::from_rgb(0xa2, 0x57, 0x72);
const PARAGRAPH_BACKGROUND: Color = Color::from_rgb(0x3d, 0x59, 0xa1);
const LABEL_TEXT: Color = Color::from_rgb(0xee, 0xf5, 0xff);
/// The letters of a label already typed: the palette's yellow and ink.
const TYPED_BACKGROUND: Color = Color::from_rgb(0xff, 0xc7, 0x77);
const TYPED_TEXT: Color = Color::from_rgb(0x1b, 0x1d, 0x2b);
const MATCH_BACKGROUND: Color = Color::from_argb(102, 0xff, 0xc7, 0x77);
const TARGET_OUTLINE: Color = Color::from_argb(140, 0xa2, 0x57, 0x72);
/// The cursor has the pink of NORMAL.
const CURSOR_COLOR: Color = Color::from_rgb(0xfc, 0xa7, 0xea);
const FLASH_COLOR: Color = Color::from_argb(140, 0xff, 0xc7, 0x77);
const FLASH_TIME: Duration = Duration::from_millis(300);
/// How tall the band at the bottom of the page the prompt takes.
const PROMPT_BAND: f32 = 52.0;

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

        let mut layer = rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .layer(Layer::Relative(10))
            .interactive(Interactive::No);

        if let Some(flash) = flash {
            for each in &flash.rects {
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
            layer = layer.child(
                rect()
                    .position(Position::new_absolute().left(caret.x).top(caret.y))
                    .width(Size::px(2.0))
                    .height(Size::px(caret.height.max(1.0)))
                    .background(CURSOR_COLOR),
            );
        }

        if let Some(session) = hints {
            layer = draw_session(layer, session, self.viewport);
        }

        layer.into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn draw_session(mut layer: Rect, session: &PageHintSession, viewport: (f32, f32)) -> Rect {
    let typed = if session.kind.labels_at_once() {
        session.typed.chars().count()
    } else {
        0
    };
    let mut placed: Vec<(f32, f32)> = Vec::new();
    for item in session.visible_items() {
        if let Some(mark) = mark(item, session) {
            layer = layer.child(mark);
        }
        if let Some(label) = item.label.as_deref() {
            layer = layer.child(tag(item, label, typed, session, viewport, &mut placed));
        }
    }
    if session.shows_prompt() {
        let below = viewport.1 - PROMPT_BAND;
        let at_top = session.items.iter().any(|item| item.rect.y + item.rect.height > below);
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
                        .padding(Gaps::new(2.0, 10.0, 2.0, 10.0))
                        .corner_radius(CornerRadius::new_all(6.0))
                        .background(LABEL_BACKGROUND)
                        .shadow(Shadow::new().y(2.0).blur(8.0).color(Color::from_argb(102, 0, 0, 0)))
                        .child(
                            label()
                                .text(format!("\u{203a} {}", session.pattern))
                                .font_family("monospace")
                                .font_weight(FontWeight::BOLD)
                                .font_size(13.0)
                                .line_height(20.0 / 13.0)
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
fn mark(item: &PageHintItem, session: &PageHintSession) -> Option<Rect> {
    let area = item.rect;
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
        _ if session.kind == zendia_automation::JumpKind::Text => None,
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

/// A label: on a word's first letters, in a paragraph's margin, after a typed match, and at a control's top left
/// corner, moved along when another control's label is already there.
fn tag(
    item: &PageHintItem,
    label_text: &str,
    typed: usize,
    session: &PageHintSession,
    viewport: (f32, f32),
    placed: &mut Vec<(f32, f32)>,
) -> Rect {
    let area: ViewportRect = item.rect;
    let small = matches!(item.kind.as_str(), "word" | "paragraph");
    let padding = if small { 3.0 } else { 4.0 };
    let width = label_text.chars().count() as f32 * if small { 6.5 } else { 7.0 } + 8.0;
    let (mut left, mut top) = (area.x + 2.0, area.y + 2.0);
    match item.kind.as_str() {
        "paragraph" => {
            left = area.x - width - 8.0;
            top = area.y;
        }
        "word" => {
            left = area.x;
            top = area.y + ((area.height - 14.0) / 2.0).max(0.0);
        }
        "match" => {
            left = area.x + area.width;
            top = area.y;
        }
        _ if session.kind == zendia_automation::JumpKind::Text => {
            left = area.x + area.width;
            top = area.y;
        }
        _ => {
            // Two controls with one corner would show one label over the other: the later one moves along.
            let mut tries = 0;
            while tries < 4
                && placed
                    .iter()
                    .any(|(other_left, other_top)| (other_left - left).abs() < width && (other_top - top).abs() < 16.0)
            {
                left += width + 2.0;
                tries += 1;
            }
            placed.push((left, top));
        }
    }
    let left = left.min(viewport.0 - width).max(0.0);
    let top = top.min(viewport.1 - 18.0).max(0.0);
    let (font_size, line_height) = if small { (10.0, 14.0) } else { (11.0, 16.0) };
    let text = |text: String, color: Color| {
        label()
            .text(text)
            .font_family("monospace")
            .font_weight(FontWeight::BOLD)
            .font_size(font_size)
            .line_height(line_height / font_size)
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
    let mut badge = rect()
        .position(Position::new_absolute().left(left).top(top))
        .padding(Gaps::new(0.0, padding, 0.0, padding))
        .corner_radius(CornerRadius::new_all(4.0))
        .background(background)
        .shadow(Shadow::new().y(1.0).blur(3.0).color(Color::from_argb(102, 0, 0, 0)))
        .direction(Direction::Horizontal);
    if !pressed.is_empty() {
        // What is pressed stands apart from what is left to press, up to the label's edge.
        badge = badge.child(
            rect()
                .margin(Gaps::new(0.0, 0.0, 0.0, -padding))
                .padding(Gaps::new(0.0, 0.0, 0.0, padding))
                .corner_radius(CornerRadius::new(4.0, 0.0, 0.0, 4.0))
                .background(TYPED_BACKGROUND)
                .child(text(pressed.to_string(), TYPED_TEXT)),
        );
    }
    if !rest.is_empty() {
        badge = badge.child(text(rest.to_string(), LABEL_TEXT));
    }
    badge
}
