/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Page Surfaces gallery section: gray placeholder rectangles where Hub web views go (single, two side by side, and
//! overlay) plus live [`DebugInputOverlay`] driven by [`StubBackend`] InputServer events.

use crate::gallery::registry::GalleryCore;
use crate::ui::core_view::use_core;
use crate::ui::kit::{GlassPanel, Icon, IconButton, KeyLabel, KitChip, Pill};
use crate::ui::page::{DebugInputOverlay, PageStage};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{InputMode, WhichKeyEntry, WhichKeyState};

#[derive(Clone)]
pub struct PageSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
}

impl PartialEq for PageSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

fn apply_leader_sample(gallery_core: &Arc<GalleryCore>) {
    let stub = gallery_core.stub();
    stub.set_input_mode(InputMode::Normal);
    stub.set_pending_keys(vec!["<Space>".to_string()]);
    stub.set_which_key(Some(WhichKeyState {
        title: "Leader".to_string(),
        entries: vec![
            WhichKeyEntry {
                key: "f".to_string(),
                description: "+find".to_string(),
                is_group: true,
            },
            WhichKeyEntry {
                key: "w".to_string(),
                description: "+window".to_string(),
                is_group: true,
            },
            WhichKeyEntry {
                key: "a".to_string(),
                description: "Open agent chat".to_string(),
                is_group: false,
            },
        ],
    }));
}

fn apply_find_sample(gallery_core: &Arc<GalleryCore>) {
    let stub = gallery_core.stub();
    stub.set_input_mode(InputMode::Normal);
    stub.set_pending_keys(vec!["<Space>".to_string(), "f".to_string()]);
    stub.set_which_key(Some(WhichKeyState {
        title: "Find".to_string(),
        entries: vec![
            WhichKeyEntry {
                key: "f".to_string(),
                description: "Command palette".to_string(),
                is_group: false,
            },
            WhichKeyEntry {
                key: "t".to_string(),
                description: "Switch tab".to_string(),
                is_group: false,
            },
            WhichKeyEntry {
                key: "h".to_string(),
                description: "Browse history".to_string(),
                is_group: false,
            },
        ],
    }));
}

fn apply_mode_sample(gallery_core: &Arc<GalleryCore>, mode: InputMode, pending: Vec<String>) {
    let stub = gallery_core.stub();
    stub.set_input_mode(mode);
    stub.set_pending_keys(pending);
    stub.set_which_key(None);
}

fn placeholder_page(
    title: &str,
    url: &str,
    badge: &str,
    focused: bool,
    on_press: Option<EventHandler<Event<PressEventData>>>,
) -> Element {
    let theme = use_theme();
    let placeholder_bg = if theme.scheme.is_dark() {
        Color::from_rgb(54, 58, 74)
    } else {
        Color::from_rgb(206, 210, 222)
    };
    let inner_card_bg = if theme.scheme.is_dark() {
        Color::from_rgb(66, 71, 90)
    } else {
        Color::from_rgb(220, 224, 234)
    };
    let border = if focused {
        Border::new()
            .width(2.0)
            .alignment(BorderAlignment::Inner)
            .fill(theme.color(ThemeRole::FocusRing))
    } else {
        Border::new()
            .width(1.0)
            .alignment(BorderAlignment::Inner)
            .fill(theme.color(ThemeRole::Border))
    };

    rect()
        .width(Size::flex(1.0))
        .height(Size::fill())
        .background(placeholder_bg)
        .border(border)
        .padding(Gaps::new_all(20.0))
        .main_align(Alignment::center())
        .cross_align(Alignment::center())
        .maybe(on_press.is_some(), |el| {
            let handler = on_press.unwrap();
            el.cursor(CursorIcon::Pointer).on_press(handler)
        })
        .child(
            rect()
                .width(Size::px(340.0))
                .padding(Gaps::new_all(18.0))
                .spacing(10.0)
                .cross_align(Alignment::center())
                .corner_radius(CornerRadius::new_all(theme.radii.card))
                .background(inner_card_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::BorderSubtle)),
                )
                .child(Icon::new("page-portrait").size(24.0).role(ThemeRole::TextMuted))
                .child(
                    label()
                        .text(title.to_string())
                        .font_size(theme.font_sizes.lg)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(theme.color(ThemeRole::Text)),
                )
                .child(
                    label()
                        .text(url.to_string())
                        .font_size(theme.font_sizes.xs)
                        .font_family("monospace")
                        .color(theme.color(ThemeRole::TextMuted)),
                )
                .child(Pill::new(badge).dot(if focused {
                    ThemeRole::Accent
                } else {
                    ThemeRole::TextMuted
                })),
        )
        .into_element()
}

fn glance_card_overlay(on_close: impl FnMut(Event<PressEventData>) + 'static) -> Element {
    let theme = use_theme();
    let preview_bg = if theme.scheme.is_dark() {
        Color::from_rgb(76, 80, 98)
    } else {
        Color::from_rgb(196, 200, 214)
    };

    rect()
        .width(Size::fill())
        .height(Size::fill())
        .padding(Gaps::new(28.0, 64.0, 96.0, 64.0))
        .child(
            GlassPanel::new()
                .width(Size::fill())
                .height(Size::fill())
                .padding(Gaps::new_all(12.0))
                .spacing(10.0)
                .child(
                    rect()
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::center())
                        .main_align(Alignment::SpaceBetween)
                        .child(
                            rect()
                                .direction(Direction::Horizontal)
                                .cross_align(Alignment::center())
                                .spacing(8.0)
                                .child(Icon::new("popup").size(14.0).role(ThemeRole::Accent))
                                .child(
                                    label()
                                        .text("Glance Overlay Surface — https://ladybird.org/posts/architecture")
                                        .font_size(theme.font_sizes.sm)
                                        .font_weight(FontWeight::MEDIUM)
                                        .color(theme.color(ThemeRole::Text)),
                                ),
                        )
                        .child(IconButton::new("close", "Close Glance preview").on_press(on_close)),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::flex(1.0))
                        .corner_radius(CornerRadius::new_all(theme.radii.md))
                        .background(preview_bg)
                        .main_align(Alignment::center())
                        .cross_align(Alignment::center())
                        .spacing(6.0)
                        .child(
                            label()
                                .text("Overlay Hub View Placeholder")
                                .font_size(theme.font_sizes.lg)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(theme.color(ThemeRole::Text)),
                        )
                        .child(
                            label()
                                .text("Sits on PageOverlayLayer above underlying PageSurfaces")
                                .font_size(theme.font_sizes.xs)
                                .color(theme.color(ThemeRole::TextMuted)),
                        ),
                ),
        )
        .into_element()
}

impl Component for PageSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut active_pane = use_state(|| 0_usize);
        let mut glance_open = use_state(|| true);
        let mut seeded_for_state = use_state(String::new);

        // When switching into `overlay` or `input`, seed a sample InputServer event state so DebugInputOverlay is
        // immediately visible without requiring an extra click.
        if *seeded_for_state.peek() != self.state_id {
            seeded_for_state.set(self.state_id.clone());
            if self.state_id == "overlay" || self.state_id == "input" {
                let model = core.model().peek();
                let inactive =
                    model.input_mode == InputMode::Normal && model.pending_keys.is_empty() && model.which_key.is_none();
                if inactive {
                    apply_leader_sample(&self.gallery_core);
                }
            }
            if self.state_id == "overlay" {
                glance_open.set(true);
            }
        }

        let (current_mode, has_pending, has_which_key) = {
            let model = core.model().read();
            (
                model.input_mode,
                !model.pending_keys.is_empty(),
                model.which_key.is_some(),
            )
        };

        let cycle_core = core.clone();
        let leader_stub = Arc::clone(&self.gallery_core);
        let find_stub = Arc::clone(&self.gallery_core);
        let insert_stub = Arc::clone(&self.gallery_core);
        let visual_stub = Arc::clone(&self.gallery_core);
        let explorer_stub = Arc::clone(&self.gallery_core);
        let window_stub = Arc::clone(&self.gallery_core);
        let clear_stub = Arc::clone(&self.gallery_core);

        let control_strip = rect()
            .width(Size::fill())
            .padding(Gaps::new(10.0, 16.0, 10.0, 16.0))
            .direction(Direction::Horizontal)
            .content(Content::wrap())
            .cross_align(Alignment::center())
            .spacing(theme.spacing.sm)
            .background(theme.color(ThemeRole::SurfaceRaised))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .child(
                label()
                    .text("Stub InputServer events:")
                    .font_size(theme.font_sizes.xs)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
            .child(
                KitChip::new("Cycle (Ctrl+Alt+M)")
                    .icon("bolt")
                    .selected(true)
                    .on_press(move |_| {
                        cycle_core.invoke_or_log(Command::DebugCycleInputState);
                    }),
            )
            .child(
                KitChip::new("Leader + Which-Key")
                    .selected(current_mode == InputMode::Normal && has_which_key)
                    .on_press(move |_| apply_leader_sample(&leader_stub)),
            )
            .child(KitChip::new("Find Chord").on_press(move |_| apply_find_sample(&find_stub)))
            .child(
                KitChip::new("INSERT")
                    .selected(current_mode == InputMode::Insert)
                    .on_press(move |_| apply_mode_sample(&insert_stub, InputMode::Insert, Vec::new())),
            )
            .child(
                KitChip::new("VISUAL")
                    .selected(current_mode == InputMode::Visual)
                    .on_press(move |_| apply_mode_sample(&visual_stub, InputMode::Visual, Vec::new())),
            )
            .child(
                KitChip::new("EXPLORER + g")
                    .selected(current_mode == InputMode::Explorer && has_pending)
                    .on_press(move |_| apply_mode_sample(&explorer_stub, InputMode::Explorer, vec!["g".to_string()])),
            )
            .child(
                KitChip::new("WINDOW")
                    .selected(current_mode == InputMode::Window)
                    .on_press(move |_| apply_mode_sample(&window_stub, InputMode::Window, Vec::new())),
            )
            .child(
                KitChip::new("Clear")
                    .selected(current_mode == InputMode::Normal && !has_pending && !has_which_key)
                    .on_press(move |_| apply_mode_sample(&clear_stub, InputMode::Normal, Vec::new())),
            )
            .child(KeyLabel::new("<C-A-m>"));

        let page_content: Element = match self.state_id.as_str() {
            "split" => rect()
                .width(Size::fill())
                .height(Size::fill())
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .child(placeholder_page(
                    "Left Page Surface",
                    "https://ladybird.org/",
                    if active_pane() == 0 { "Focused Pane 1" } else { "Pane 1" },
                    active_pane() == 0,
                    Some(EventHandler::new(move |_| active_pane.set(0))),
                ))
                .child(
                    rect()
                        .width(Size::px(3.0))
                        .height(Size::fill())
                        .background(theme.color(ThemeRole::Border)),
                )
                .child(placeholder_page(
                    "Right Page Surface",
                    "https://docs.rs/freya/0.5.4",
                    if active_pane() == 1 { "Focused Pane 2" } else { "Pane 2" },
                    active_pane() == 1,
                    Some(EventHandler::new(move |_| active_pane.set(1))),
                ))
                .into_element(),
            "overlay" => placeholder_page(
                "Underlying Page Surface",
                "https://ladybird.org/",
                if glance_open() {
                    "Dimmed behind overlay"
                } else {
                    "Active Page"
                },
                !glance_open(),
                Some(EventHandler::new(move |_| {
                    if !*glance_open.peek() {
                        glance_open.set(true);
                    }
                })),
            ),
            "input" => placeholder_page(
                "Page Stage with InputServer Overlay",
                "Press Ctrl+Alt+M or use the buttons above to emit stub InputServer events",
                "PageOverlayLayer active",
                true,
                None,
            ),
            _ => placeholder_page(
                "Single Page Surface",
                "https://ladybird.org/",
                "Single View Placeholder",
                true,
                None,
            ),
        };

        let mut stage = PageStage::new().page(page_content);
        if self.state_id == "overlay" && glance_open() {
            stage = stage.overlay(glance_card_overlay(move |_| glance_open.set(false)));
        }
        stage = stage.overlay(DebugInputOverlay);

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .padding(Gaps::new_all(theme.spacing.xl))
            .spacing(theme.spacing.md)
            .child(control_strip)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .corner_radius(CornerRadius::new_all(theme.radii.card))
                    .overflow(Overflow::Clip)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.color(ThemeRole::Border)),
                    )
                    .child(stage),
            )
    }
}
