/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! What vim mode draws over a page, natively, so a page can neither hide nor fake it:
//! - The labels of a jump (`s`, `gs`, `S`, `i`, `gc`, `v` and visual mode's `s`), matching `ZendiaJumpContent`
//!   (`JMP-06`, `JMP-07`, `CAP-03`, `CAP-09`).
//! - The keyboard cursor (`PointerPad`, `CAP-05`, `CAP-06`, `CAP-09`) matching `ZendiaPointerContent`: dashed `2 px`
//!   `rgba(255, 199, 119, 0.8)` area boxes, `1 px` `rgba(162, 87, 114, 0.55)` grid/board cells, `1 px` amber ruler
//!   lines with alternating `14 px` tag rows, `#232136` `10/13` tags with `#ffc777` typed prefix, `16 px`/`22 px`
//!   amber cursor ring + SVG arrow pointer, and `#232136` `13/20` prompt bar.
//! - Visual mode's cursor (`2 px` `#fca7ea` bar) and yank/put flashes.

use crate::ui::core_view::use_core;
use crate::ui::panels::{DockHostState, try_use_dock_state};
use crate::ui::theme::CHROME_OVERLAY_LEVEL;
use freya::prelude::*;
use std::time::Duration;
use zendia_automation::{JumpKind, ViewportRect};
use zendia_browser_core::captcha::{PadLayoutKind, PointerPadState};
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{
    BrowserModel, ConversationStatus, InputMode, PageFlashKind, ServiceId, ServiceStatus, TabId,
};
use zendia_browser_core::page_actions::{PageAction, PageHintItem, PageHintSession};

/// The browser's own label color (`#a25772`), and a blue for paragraphs (`#3d59a1`).
const LABEL_BACKGROUND: Color = Color::from_rgb(0xa2, 0x57, 0x72);
const PARAGRAPH_BACKGROUND: Color = Color::from_rgb(0x3d, 0x59, 0xa1);
const LABEL_TEXT: Color = Color::from_rgb(0xee, 0xf5, 0xff);
/// `rgba(0, 0, 0, 0.4)` shadow for badges (`0 1px 3px`) and prompts (`0 2px 8px`).
const LABEL_SHADOW: Color = Color::from_argb(102, 0, 0, 0);
/// The letters of a label already typed: `#ffc777` behind `#1b1d2b`.
const TYPED_BACKGROUND: Color = Color::from_rgb(0xff, 0xc7, 0x77);
const TYPED_TEXT: Color = Color::from_rgb(0x1b, 0x1d, 0x2b);
/// `rgba(255, 199, 119, 0.4)` tint on typed text matches.
const MATCH_BACKGROUND: Color = Color::from_argb(102, 0xff, 0xc7, 0x77);
/// `rgba(162, 87, 114, 0.55)` 1 px outline around labeled controls and cursor grid cells.
const TARGET_OUTLINE: Color = Color::from_argb(140, 0xa2, 0x57, 0x72);
/// Keyboard cursor (`PointerPad`) palette (`ZendiaPointerContent.sys.mjs:23-29`).
const PAD_BACKGROUND: Color = Color::from_rgb(0x23, 0x21, 0x36);
const PAD_FINE_BACKGROUND: Color = Color::from_argb(198, 0x23, 0x21, 0x36);
const PAD_FINE_OUTLINE: Color = Color::from_argb(96, 0xa2, 0x57, 0x72);
const PAD_TYPED_TEXT: Color = Color::from_rgb(0xff, 0xc7, 0x77);
const PAD_AREA_OUTLINE: Color = Color::from_argb(204, 0xff, 0xc7, 0x77);
const PAD_RULER_SHADOW: Color = Color::from_argb(89, 0x23, 0x21, 0x36);
const PAD_HELD_FILL: Color = Color::from_argb(115, 0xff, 0xc7, 0x77);
/// Live vertical alignment hairline (`#86e1fc`) drawn over the puzzle image while dragging a slider.
const PAD_SLIDER_GUIDE: Color = Color::from_argb(235, 0x86, 0xe1, 0xfc);
/// Numbered click-order pin colors (`#ffc777` fill, `#232136` ink).
const PAD_PIN_FILL: Color = Color::from_rgb(0xff, 0xc7, 0x77);
const PAD_PIN_INK: Color = Color::from_rgb(0x23, 0x21, 0x36);
/// Exact SVG arrow cursor from `ZendiaPointerContent.arrowCursor` (`18x24`, tip at `(0, 0)`).
const ARROW_CURSOR_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="24" viewBox="0 0 18 24"><path d="M0 0 L0 17 L4.5 13 L7.5 20 L10.5 18.6 L7.6 12 L13 12 Z" fill="#232136" stroke="#ffffff" stroke-width="1.4" stroke-linejoin="round"/></svg>"##;

/// The cursor has the pink of NORMAL (`#fca7ea`).
const CURSOR_COLOR: Color = Color::from_rgb(0xfc, 0xa7, 0xea);
/// Visual yank flash: `rgba(255, 199, 119, 0.55)`, radius `2 px` (`YNK-05`).
const YANK_FLASH_COLOR: Color = Color::from_argb(140, 0xff, 0xc7, 0x77);
/// Field put flash: `rgba(255, 199, 119, 0.45)`, radius `3 px` (`YNK-05`).
const PUT_FLASH_COLOR: Color = Color::from_argb(115, 0xff, 0xc7, 0x77);
const FLASH_TIME: Duration = Duration::from_millis(300);
/// How tall the band at the bottom of the page the prompt takes (`52 px` in the fork).
const PROMPT_BAND: f32 = 52.0;
/// Bottom offset (`56 px`) used when the closed-panel agent pill sits at `bottom: 16 px` (`JMP-07`).
const PROMPT_ABOVE_PILL_BOTTOM: f32 = 56.0;
const PROMPT_ABOVE_PILL_BAND: f32 = 92.0;

/// Exact fork label metrics (`ZendiaJumpContent.sys.mjs:1603-1664`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct LabelMetrics {
    small: bool,
    font_size: f32,
    height: f32,
    padding: f32,
    zoom: f32,
}

impl LabelMetrics {
    fn for_kind(kind: &str, zoom: f32) -> Self {
        let small = matches!(kind, "word" | "paragraph");
        let (font_size, height, padding) = if small { (10.0, 14.0, 3.0) } else { (11.0, 16.0, 4.0) };
        Self {
            small,
            font_size: font_size * zoom,
            height: height * zoom,
            padding: padding * zoom,
            zoom,
        }
    }

    /// Width estimate matching `label.length * (small ? 6.5 : 7) + 8` in `ZendiaJumpContent.sys.mjs:1623`.
    fn estimated_width(&self, label: &str) -> f32 {
        let per_char = if self.small { 6.5 } else { 7.0 };
        (label.chars().count() as f32 * per_char + 8.0) * self.zoom
    }
}

/// Returns whether the bottom closed-panel agent notice pill is currently shown so prompts clear it (`JMP-07`).
fn closed_panel_pill_visible(model: &BrowserModel, dock_state: Option<DockHostState>) -> bool {
    let server_running = model
        .services
        .iter()
        .any(|s| s.service == ServiceId::AgentServer && matches!(s.status, ServiceStatus::Running));
    if !server_running {
        return false;
    }
    let agent_open = dock_state.is_some_and(|ds| ds.is_agent_open());
    let agents_open = dock_state.is_some_and(|ds| ds.is_agents_open());
    if !agent_open
        && model
            .active_tab
            .and_then(|tid| model.active_conversation_for_tab(tid))
            .is_some_and(|c| {
                matches!(
                    c.status,
                    ConversationStatus::Working | ConversationStatus::WaitingForUser
                ) || c.unseen
            })
    {
        return true;
    }
    if !agents_open
        && model
            .conversations
            .iter()
            .any(|c| c.status == ConversationStatus::WaitingForUser)
    {
        return true;
    }
    !agent_open
        && model
            .conversations
            .iter()
            .any(|c| c.status == ConversationStatus::Working || c.unseen)
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
        let dock_state = try_use_dock_state();
        let mut flashed = use_state(|| 0u64);
        let mut scheduled_poll = use_state(|| 0u64);
        let mut scheduled_notice = use_state(|| 0u64);
        let model = core.model().read();

        let empty = || rect().width(Size::px(0.0)).height(Size::px(0.0)).into_element();
        if !model.vim_mode_enabled || self.tab_id.is_none() {
            return empty();
        }

        // Wait only for the initial scan of an empty non-prompt jump; keep existing labels up smoothly across scroll
        // and resize refreshes (`JMP-04`).
        let hints = model.page_hints.as_ref().filter(|session| {
            Some(session.tab_id) == self.tab_id
                && !(session.items.is_empty()
                    && session.cursor_pad.is_none()
                    && !session.shows_prompt()
                    && session.awaiting.as_deref() == Some(""))
        });

        // Drive staying-jump / cursor-watch polls and temporary notice expirations (`CAP-03`, `CAP-04`, `CAP-05`).
        if let Some(session) = hints {
            if let Some(delay_ms) = session.poll_delay_ms {
                let seq = session.poll_seq.max(1);
                if *scheduled_poll.peek() != seq {
                    scheduled_poll.set(seq);
                    let core_poll = core.clone();
                    let poll_seq = session.poll_seq;
                    spawn(async move {
                        timer(Duration::from_millis(delay_ms)).await;
                        core_poll.invoke_or_log(Command::Page(PageAction::PageHintPoll { seq: Some(poll_seq) }));
                    });
                }
            }
            if session.notice_duration_ms > 0
                && (session.notice.is_some() || session.cursor_pad.as_ref().is_some_and(|pad| pad.notice.is_some()))
                && *scheduled_notice.peek() != session.notice_seq
            {
                let seq = session.notice_seq;
                let ms = session.notice_duration_ms;
                scheduled_notice.set(seq);
                let core_notice = core.clone();
                spawn(async move {
                    timer(Duration::from_millis(ms)).await;
                    core_notice.invoke_or_log(Command::Page(PageAction::PageHintClearNotice { seq }));
                });
            }
        }

        let caret = (model.effective_keyed_tab() == self.tab_id && model.input_mode == InputMode::Visual)
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
        let zoom = self
            .tab_id
            .map(|id| model.zoom_for_tab(id))
            .filter(|&z| z > 0.0)
            .unwrap_or(1.0);
        let pill_visible = closed_panel_pill_visible(&model, dock_state);

        let mut layer = rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .layer(Layer::Relative(10))
            .interactive(Interactive::No);

        if let Some(flash) = flash {
            let (flash_color, flash_radius) = match flash.kind {
                PageFlashKind::Yank => (YANK_FLASH_COLOR, 2.0 * zoom),
                PageFlashKind::Put => (PUT_FLASH_COLOR, 3.0 * zoom),
            };
            for each in &flash.rects {
                let each = scaled(*each, zoom);
                layer = layer.child(
                    rect()
                        .position(Position::new_absolute().left(each.x).top(each.y))
                        .width(Size::px(each.width.max(0.0)))
                        .height(Size::px(each.height.max(0.0)))
                        .corner_radius(CornerRadius::new_all(flash_radius))
                        .background(flash_color),
                );
            }
        }

        if let Some(caret) = caret {
            let caret = scaled(caret, zoom);
            layer = layer.child(
                rect()
                    .position(Position::new_absolute().left(caret.x).top(caret.y))
                    .width(Size::px((2.0 * zoom).max(1.5)))
                    .height(Size::px(caret.height.max(1.0)))
                    .background(CURSOR_COLOR),
            );
        }

        if let Some(session) = hints {
            if let Some(pad) = session.cursor_pad.as_ref() {
                layer = draw_cursor_pad(layer, pad, self.viewport, zoom, pill_visible);
            } else {
                layer = draw_session(layer, session, self.viewport, zoom, pill_visible);
            }
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

fn draw_prompt_bar(layer: Rect, text: String, at_top: bool, bottom_offset: f32, zoom: f32, background: Color) -> Rect {
    let position = if at_top {
        Position::new_absolute().left(0.0).top(16.0 * zoom)
    } else {
        Position::new_absolute().left(0.0).bottom(bottom_offset)
    };
    let prompt_font = 13.0 * zoom;
    let prompt_line_h = 20.0 * zoom;
    layer.child(
        rect()
            .layer(Layer::OverlayLevel(CHROME_OVERLAY_LEVEL))
            .position(position)
            .width(Size::percent(100.0))
            .main_align(Alignment::center())
            .cross_align(Alignment::center())
            .interactive(Interactive::No)
            .child(
                rect()
                    .max_width(Size::percent(90.0))
                    .padding(Gaps::new(2.0 * zoom, 10.0 * zoom, 2.0 * zoom, 10.0 * zoom))
                    .corner_radius(CornerRadius::new_all(6.0 * zoom))
                    .background(background)
                    .shadow(Shadow::new().y(2.0 * zoom).blur(8.0 * zoom).color(LABEL_SHADOW))
                    .child(
                        label()
                            .text(text)
                            .font_family("monospace")
                            .font_weight(FontWeight::BOLD)
                            .font_size(prompt_font)
                            .line_height(prompt_line_h / prompt_font)
                            .color(LABEL_TEXT)
                            .max_lines(1),
                    ),
            ),
    )
}

fn draw_session(
    mut layer: Rect,
    session: &PageHintSession,
    viewport: (f32, f32),
    zoom: f32,
    pill_visible: bool,
) -> Rect {
    let typed = if session.kind.labels_at_once() {
        session.typed.chars().count()
    } else {
        0
    };
    let mut placed = Placed::default();
    for item in session.visible_items() {
        let area = scaled(item.rect, zoom);
        if let Some(mark) = mark(item, area, session, zoom) {
            layer = layer.child(mark);
        }
        if let Some(label) = item.label.as_deref() {
            layer = layer.child(tag(item, area, label, typed, session, viewport, zoom, &mut placed));
        }
    }
    if session.shows_prompt() {
        let (bottom_offset, band) = prompt_bottom_and_band(zoom, pill_visible);
        let below = viewport.1 - band;
        let at_top = session
            .items
            .iter()
            .any(|item| (item.rect.y + item.rect.height) * zoom > below);
        layer = draw_prompt_bar(
            layer,
            session.prompt_text(),
            at_top,
            bottom_offset,
            zoom,
            LABEL_BACKGROUND,
        );
    }
    layer
}

/// Draws the keyboard cursor (`PointerPad`, `ZendiaPointerContent.sys.mjs:1278-1469`, `CAP-05`, `CAP-06`, `CAP-09`).
fn draw_cursor_pad(
    mut layer: Rect,
    pad: &PointerPadState,
    viewport: (f32, f32),
    zoom: f32,
    pill_visible: bool,
) -> Rect {
    for area in &pad.areas {
        layer = draw_dashed_area(layer, scaled(*area, zoom), zoom);
    }
    let typed_chars = pad.typed.chars().count();
    if let Some(layout) = pad.shown_layout() {
        for (i, (r, lbl)) in layout.rects.iter().zip(&layout.labels).enumerate() {
            if !lbl.starts_with(&pad.typed) {
                continue;
            }
            let sr = scaled(*r, zoom);
            if layout.kind == PadLayoutKind::Ruler {
                // `1 px` amber vertical line with `1px 0 0 rgba(35, 33, 54, 0.35)` shadow and alternating `dy: 14px` tags.
                layer = layer.child(
                    rect()
                        .position(Position::new_absolute().left(sr.x).top(sr.y))
                        .width(Size::px((1.0 * zoom).max(1.0)))
                        .height(Size::px(sr.height.max(1.0)))
                        .background(PAD_AREA_OUTLINE)
                        .shadow(Shadow::new().x(1.0 * zoom).blur(0.0).color(PAD_RULER_SHADOW)),
                );
                let dy = if i % 2 == 1 { 14.0 * zoom } else { 0.0 };
                let tag_rect = ViewportRect {
                    x: sr.x - 4.0 * zoom,
                    ..sr
                };
                layer = layer.child(pad_tag(tag_rect, lbl, typed_chars, dy, zoom, false));
            } else {
                let fine = layout.kind == PadLayoutKind::FineGrid;
                layer = layer.child(
                    rect()
                        .position(Position::new_absolute().left(sr.x).top(sr.y))
                        .width(Size::px(sr.width.max(0.0)))
                        .height(Size::px(sr.height.max(0.0)))
                        .border(
                            Border::new()
                                .fill(if fine { PAD_FINE_OUTLINE } else { TARGET_OUTLINE })
                                .width((1.0 * zoom).max(1.0))
                                .alignment(BorderAlignment::Inner),
                        ),
                );
                layer = layer.child(pad_tag(sr, lbl, typed_chars, 0.0, zoom, fine));
            }
        }
    }

    // Live vertical alignment guide over the puzzle image while sliding (`slider` mode).
    if pad.holding.is_some()
        && pad.slider
        && let Some((top_y, bot_y)) = pad.ruler_y_span
    {
        let guide_x = (pad.cursor.0 + pad.slider_piece_offset_x) * zoom;
        let guide_top = top_y * zoom;
        let guide_h = ((bot_y - top_y) * zoom).max(1.0);
        layer = layer.child(
            rect()
                .position(Position::new_absolute().left(guide_x).top(guide_top))
                .width(Size::px((2.0 * zoom).max(1.5)))
                .height(Size::px(guide_h))
                .background(PAD_SLIDER_GUIDE)
                .shadow(Shadow::new().x(1.0 * zoom).blur(2.0 * zoom).color(LABEL_SHADOW)),
        );
    }

    // Numbered click-order pins (`1`, `2`, `3`, …) so multi-step click-order CAPTCHA progress stays visible.
    if !pad.clicks.is_empty() {
        let pin_size = 18.0 * zoom;
        let pin_font = 10.0 * zoom;
        for (index, &(px, py)) in pad.clicks.iter().enumerate() {
            layer = layer.child(
                rect()
                    .position(
                        Position::new_absolute()
                            .left(px * zoom - pin_size / 2.0)
                            .top(py * zoom - pin_size / 2.0),
                    )
                    .width(Size::px(pin_size))
                    .height(Size::px(pin_size))
                    .corner_radius(CornerRadius::new_all(pin_size / 2.0))
                    .background(PAD_PIN_FILL)
                    .border(
                        Border::new()
                            .fill(PAD_PIN_INK)
                            .width((1.5 * zoom).max(1.0))
                            .alignment(BorderAlignment::Inner),
                    )
                    .shadow(Shadow::new().y(1.0 * zoom).blur(3.0 * zoom).color(LABEL_SHADOW))
                    .main_align(Alignment::center())
                    .cross_align(Alignment::center())
                    .child(
                        label()
                            .text((index + 1).to_string())
                            .font_family("monospace")
                            .font_weight(FontWeight::BOLD)
                            .font_size(pin_font)
                            .line_height(1.0)
                            .color(PAD_PIN_INK)
                            .max_lines(1),
                    ),
            );
        }
    }

    // Cursor ring (`16 px`, or `22 px` filled `rgba(255,199,119,0.45)` while holding) + SVG arrow pointer at `(x, y)`.
    let cx = pad.cursor.0 * zoom;
    let cy = pad.cursor.1 * zoom;
    let ring_size = (if pad.holding.is_some() { 22.0 } else { 16.0 }) * zoom;
    let mut ring = rect()
        .position(
            Position::new_absolute()
                .left(cx - ring_size / 2.0)
                .top(cy - ring_size / 2.0),
        )
        .width(Size::px(ring_size))
        .height(Size::px(ring_size))
        .corner_radius(CornerRadius::new_all(ring_size / 2.0))
        .border(
            Border::new()
                .fill(PAD_AREA_OUTLINE)
                .width((2.0 * zoom).max(1.0))
                .alignment(BorderAlignment::Inner),
        );
    if pad.holding.is_some() {
        ring = ring.background(PAD_HELD_FILL);
    }
    layer = layer.child(ring);
    layer = layer.child(
        rect()
            .position(Position::new_absolute().left(cx).top(cy))
            .width(Size::px(18.0 * zoom))
            .height(Size::px(24.0 * zoom))
            .child(
                SvgViewer::new((("zendia-cursor-arrow", "default"), ARROW_CURSOR_SVG))
                    .width(Size::px(18.0 * zoom))
                    .height(Size::px(24.0 * zoom))
                    .show_loader(false),
            ),
    );

    let (bottom_offset, band) = prompt_bottom_and_band(zoom, pill_visible);
    let below = viewport.1 - band;
    let at_top = cy > below - 24.0 * zoom
        || pad
            .shown_layout()
            .is_some_and(|l| l.rects.iter().any(|r| (r.y + r.height) * zoom > below));
    draw_prompt_bar(layer, pad.prompt_text(), at_top, bottom_offset, zoom, PAD_BACKGROUND)
}

/// Draws a `2 px` dashed `rgba(255, 199, 119, 0.8)` box around `area` (`ZendiaPointerContent:1288`).
fn draw_dashed_area(mut layer: Rect, area: ViewportRect, zoom: f32) -> Rect {
    if area.width <= 0.0 || area.height <= 0.0 {
        return layer;
    }
    let thickness = (2.0 * zoom).max(1.0);
    let dash = (6.0 * zoom).max(3.0);
    let step = (10.0 * zoom).max(5.0);
    let mut x = 0.0;
    while x < area.width {
        let w = dash.min(area.width - x);
        layer = layer
            .child(
                rect()
                    .position(Position::new_absolute().left(area.x + x).top(area.y))
                    .width(Size::px(w))
                    .height(Size::px(thickness))
                    .background(PAD_AREA_OUTLINE),
            )
            .child(
                rect()
                    .position(
                        Position::new_absolute()
                            .left(area.x + x)
                            .top((area.y + area.height - thickness).max(area.y)),
                    )
                    .width(Size::px(w))
                    .height(Size::px(thickness))
                    .background(PAD_AREA_OUTLINE),
            );
        x += step;
    }
    let mut y = 0.0;
    while y < area.height {
        let h = dash.min(area.height - y);
        layer = layer
            .child(
                rect()
                    .position(Position::new_absolute().left(area.x).top(area.y + y))
                    .width(Size::px(thickness))
                    .height(Size::px(h))
                    .background(PAD_AREA_OUTLINE),
            )
            .child(
                rect()
                    .position(
                        Position::new_absolute()
                            .left((area.x + area.width - thickness).max(area.x))
                            .top(area.y + y),
                    )
                    .width(Size::px(thickness))
                    .height(Size::px(h))
                    .background(PAD_AREA_OUTLINE),
            );
        y += step;
    }
    layer
}

fn pad_tag(area: ViewportRect, text_str: &str, typed: usize, dy: f32, zoom: f32, fine: bool) -> Rect {
    let font_size = (if fine { 9.0 } else { 10.0 }) * zoom;
    let height = (if fine { 11.5 } else { 13.0 }) * zoom;
    let pad_x = (if fine { 1.5 } else { 2.0 }) * zoom;
    let split = text_str
        .char_indices()
        .nth(typed)
        .map_or(text_str.len(), |(idx, _)| idx);
    let (pressed, rest) = text_str.split_at(split);
    let mk_label = |s: String, color: Color| {
        label()
            .text(s)
            .font_family("monospace")
            .font_weight(FontWeight::BOLD)
            .font_size(font_size)
            .line_height(height / font_size)
            .color(color)
            .max_lines(1)
    };
    let mut badge = rect()
        .position(
            Position::new_absolute()
                .left(area.x + 1.0 * zoom)
                .top(area.y + 1.0 * zoom + dy),
        )
        .height(Size::px(height))
        .padding(Gaps::new(0.0, pad_x, 0.0, pad_x))
        .corner_radius(CornerRadius::new_all(3.0 * zoom))
        .background(if fine { PAD_FINE_BACKGROUND } else { PAD_BACKGROUND })
        .shadow(Shadow::new().y(1.0 * zoom).blur(2.0 * zoom).color(LABEL_SHADOW))
        .cross_align(Alignment::center())
        .direction(Direction::Horizontal);
    if !pressed.is_empty() {
        badge = badge.child(mk_label(pressed.to_string(), PAD_TYPED_TEXT));
    }
    if !rest.is_empty() {
        badge = badge.child(mk_label(rest.to_string(), LABEL_TEXT));
    }
    badge
}

fn prompt_bottom_and_band(zoom: f32, pill_visible: bool) -> (f32, f32) {
    if pill_visible {
        (
            PROMPT_ABOVE_PILL_BOTTOM.max(16.0 * zoom),
            PROMPT_ABOVE_PILL_BAND.max(PROMPT_BAND * zoom),
        )
    } else {
        (16.0 * zoom, PROMPT_BAND * zoom)
    }
}

/// The mark on what a label names: a bar along a paragraph, a tint on a typed match, an outline around a control.
/// A word is marked by its label alone.
fn mark(item: &PageHintItem, area: ViewportRect, session: &PageHintSession, zoom: f32) -> Option<Rect> {
    let at = Position::new_absolute().left(area.x).top(area.y);
    match item.kind.as_str() {
        "word" => None,
        "paragraph" => Some(
            rect()
                .position(Position::new_absolute().left(area.x - 6.0 * zoom).top(area.y))
                .width(Size::px(3.0 * zoom))
                .height(Size::px(area.height))
                .corner_radius(CornerRadius::new_all(2.0 * zoom))
                .background(PARAGRAPH_BACKGROUND),
        ),
        "match" => Some(
            rect()
                .position(at)
                .width(Size::px(area.width))
                .height(Size::px(area.height))
                .corner_radius(CornerRadius::new_all(3.0 * zoom))
                .background(MATCH_BACKGROUND),
        ),
        _ if session.kind == JumpKind::Text => Some(
            rect()
                .position(at)
                .width(Size::px(area.width))
                .height(Size::px(area.height))
                .corner_radius(CornerRadius::new_all(3.0 * zoom))
                .background(MATCH_BACKGROUND),
        ),
        _ => Some(
            rect()
                .position(at)
                .width(Size::px(area.width))
                .height(Size::px(area.height))
                .corner_radius(CornerRadius::new_all(3.0 * zoom))
                .border(
                    Border::new()
                        .fill(TARGET_OUTLINE)
                        .width((1.0 * zoom).max(1.0))
                        .alignment(BorderAlignment::Outer),
                ),
        ),
    }
}

/// The control labels placed so far, so the next one shifts past collisions (`ZendiaJumpContent.sys.mjs:1638-1650`).
#[derive(Default)]
struct Placed {
    controls: Vec<(f32, f32)>,
}

/// Computes the clamped `(left, top)` of a label tag in overlay logical pixels (`ZendiaJumpContent.sys.mjs:1623-1656`).
fn compute_tag_origin(
    kind: &str,
    is_text_jump: bool,
    area: ViewportRect,
    width: f32,
    viewport: (f32, f32),
    zoom: f32,
    placed: &mut Placed,
) -> (f32, f32) {
    let mut left = area.x + 2.0 * zoom;
    let mut top = area.y + 2.0 * zoom;
    if kind == "paragraph" {
        left = area.x - width - 8.0 * zoom;
        top = area.y;
    } else if kind == "word" {
        left = area.x;
        top = area.y + ((area.height - 14.0 * zoom) / 2.0).max(0.0);
    } else if kind == "match" || is_text_jump {
        left = area.x + area.width;
        top = area.y;
    } else {
        for _ in 0..4 {
            let collides = placed.controls.iter().any(|&(other_left, other_top)| {
                (other_left - left).abs() < width && (other_top - top).abs() < 16.0 * zoom
            });
            if !collides {
                break;
            }
            left += width + 2.0 * zoom;
        }
        placed.controls.push((left, top));
    }
    let clamped_left = if viewport.0 > 0.0 {
        left.min((viewport.0 - width).max(0.0)).max(0.0)
    } else {
        left.max(0.0)
    };
    let clamped_top = if viewport.1 > 0.0 {
        top.min((viewport.1 - 18.0 * zoom).max(0.0)).max(0.0)
    } else {
        top.max(0.0)
    };
    (clamped_left, clamped_top)
}

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
    let width = metrics.estimated_width(label_text);
    let is_text_jump = session.kind == JumpKind::Text;
    let (left, top) = compute_tag_origin(&item.kind, is_text_jump, area, width, viewport, zoom, placed);
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
        .shadow(Shadow::new().y(1.0 * zoom).blur(3.0 * zoom).color(LABEL_SHADOW))
        .cross_align(Alignment::center())
        .direction(Direction::Horizontal);
    if !pressed.is_empty() {
        // Typed prefix sits flush with the left edge without adding extra width (`JMP-06`).
        badge = badge.child(
            rect()
                .height(Size::fill())
                .cross_align(Alignment::center())
                .margin(Gaps::new(0.0, 0.0, 0.0, -metrics.padding))
                .padding(Gaps::new(0.0, 0.0, 0.0, metrics.padding))
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

#[cfg(test)]
mod tests {
    use super::*;
    use freya::engine::prelude::{FontMgr, svg};

    #[test]
    fn label_metrics_origins_and_cursor_arrow_svg_match_fork() {
        let ctrl = LabelMetrics::for_kind("control", 1.0);
        assert_eq!((ctrl.font_size, ctrl.height, ctrl.padding), (11.0, 16.0, 4.0));
        assert_eq!(ctrl.estimated_width("as"), 22.0);

        let word = LabelMetrics::for_kind("word", 1.0);
        assert_eq!((word.font_size, word.height, word.padding), (10.0, 14.0, 3.0));
        assert_eq!(word.estimated_width("as"), 21.0);

        let mut placed = Placed::default();
        let rect_a = ViewportRect {
            x: 100.0,
            y: 50.0,
            width: 80.0,
            height: 24.0,
        };
        assert_eq!(
            compute_tag_origin("control", false, rect_a, 15.0, (800.0, 600.0), 1.0, &mut placed),
            (102.0, 52.0)
        );
        assert_eq!(
            compute_tag_origin("control", false, rect_a, 15.0, (800.0, 600.0), 1.0, &mut placed),
            (119.0, 52.0)
        );
        assert_eq!(
            compute_tag_origin("paragraph", false, rect_a, 15.0, (800.0, 600.0), 1.0, &mut placed),
            (77.0, 50.0)
        );
        assert_eq!(
            compute_tag_origin("match", true, rect_a, 15.0, (800.0, 600.0), 1.0, &mut placed),
            (180.0, 50.0)
        );
        assert_eq!(prompt_bottom_and_band(1.0, false), (16.0, 52.0));
        assert_eq!(prompt_bottom_and_band(1.0, true), (56.0, 92.0));

        // Verify the SVG arrow cursor parses cleanly in Skia!
        let parsed = svg::Dom::from_bytes(ARROW_CURSOR_SVG, FontMgr::default());
        assert!(parsed.is_ok(), "cursor arrow SVG must parse cleanly in Skia");
    }
}
