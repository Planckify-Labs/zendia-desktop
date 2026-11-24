/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Page stage and overlay layer sitting above all [`PageSurface`](crate::ui::page::PageSurface)s for panels,
//! Glance cards, hints, and InputServer overlays.

use crate::ui::core_view::use_core;
use crate::ui::kit::{GlassPanel, KeyLabel, Pill};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::model::InputMode;

/// Transparent overlay container positioned above all page surfaces.
///
/// Empty areas have no background so pointer events pass directly to the [`PageSurface`](crate::ui::page::PageSurface)
/// beneath them, while overlay children (floating panels, popovers, confirmation cards, hints) paint above every page.
#[derive(Clone, PartialEq)]
pub struct PageOverlayLayer {
    width: Size,
    height: Size,
    children: Vec<Element>,
    key: DiffKey,
}

impl Default for PageOverlayLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyExt for PageOverlayLayer {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ChildrenExt for PageOverlayLayer {
    fn children_mut(&mut self) -> &mut Vec<Element> {
        &mut self.children
    }
}

impl PageOverlayLayer {
    pub fn new() -> Self {
        Self {
            width: Size::percent(100.),
            height: Size::percent(100.),
            children: Vec::new(),
            key: DiffKey::None,
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = Size::px(width);
        self.height = Size::px(height);
        self
    }
}

impl Component for PageOverlayLayer {
    fn render(&self) -> impl IntoElement {
        rect()
            .layer(Layer::Overlay)
            .position(Position::new_absolute().top(0.).left(0.))
            .width(self.width.clone())
            .height(self.height.clone())
            .children(self.children.clone())
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Hosts the page surfaces region (single page or multi-view split) alongside a [`PageOverlayLayer`] above it.
#[derive(Clone, PartialEq)]
pub struct PageStage {
    pages: Vec<Element>,
    overlays: Vec<Element>,
    on_sized: Option<EventHandler<Event<SizedEventData>>>,
    key: DiffKey,
}

impl Default for PageStage {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyExt for PageStage {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl PageStage {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            overlays: Vec::new(),
            on_sized: None,
            key: DiffKey::None,
        }
    }

    pub fn page(mut self, element: impl IntoElement) -> Self {
        self.pages.push(element.into_element());
        self
    }

    pub fn overlay(mut self, element: impl IntoElement) -> Self {
        self.overlays.push(element.into_element());
        self
    }

    pub fn on_sized(mut self, handler: impl Into<EventHandler<Event<SizedEventData>>>) -> Self {
        self.on_sized = Some(handler.into());
        self
    }
}

impl Component for PageStage {
    fn render(&self) -> impl IntoElement {
        let mut stage_size = use_state(|| None::<(f32, f32)>);
        let on_sized_cb = self.on_sized.clone();

        let overlay = match *stage_size.read() {
            Some((width, height)) => PageOverlayLayer::new()
                .size(width, height)
                .children(self.overlays.clone()),
            None => PageOverlayLayer::new().children(self.overlays.clone()),
        };

        rect()
            .width(Size::fill())
            .height(Size::flex(1.))
            .overflow(Overflow::Clip)
            .on_sized(move |e: Event<SizedEventData>| {
                let w = e.area.width();
                let h = e.area.height();
                if w > 0.0 && h > 0.0 {
                    stage_size.set_if_modified(Some((w, h)));
                }
                if let Some(handler) = &on_sized_cb {
                    handler.call(e);
                }
            })
            .children(self.pages.clone())
            .child(overlay)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn mode_badge_label(mode: InputMode) -> (&'static str, ThemeRole) {
    match mode {
        InputMode::Normal => ("NORMAL", ThemeRole::ModeNormal),
        InputMode::Insert => ("INSERT", ThemeRole::ModeInsert),
        InputMode::Visual => ("VISUAL", ThemeRole::ModeVisual),
        InputMode::Explorer => ("EXPLORER", ThemeRole::ModeExplorer),
        InputMode::Toolbar => ("TOOLBAR", ThemeRole::ModeToolbar),
        InputMode::Library => ("LIBRARY", ThemeRole::ModeLibrary),
        InputMode::Window => ("WINDOW", ThemeRole::ModeWindow),
    }
}

/// Minimal debug overlay rendered on [`PageOverlayLayer`] when InputServer events (mode, pending keys, or which-key)
/// are active. Renders nothing in the default standard state (`Normal`, no pending keys, no which-key popup).
#[derive(Clone, PartialEq)]
pub struct DebugInputOverlay;

impl Component for DebugInputOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let (mode, pending_keys, which_key) = {
            let model = core.model().read();
            (model.input_mode, model.pending_keys.clone(), model.which_key.clone())
        };

        let active = mode != InputMode::Normal || !pending_keys.is_empty() || which_key.is_some();
        if !active {
            return rect().width(Size::px(0.)).height(Size::px(0.));
        }

        let (mode_name, mode_role) = mode_badge_label(mode);
        let pending_text = pending_keys.join("");

        rect()
            .position(Position::new_absolute().right(14.).bottom(14.))
            .interactive(Interactive::No)
            .spacing(theme.spacing.sm)
            .cross_align(Alignment::End)
            .maybe_child(which_key.map(|wk| {
                let rows = wk.entries.into_iter().enumerate().map(|(index, entry)| {
                    let desc_color = if entry.is_group {
                        theme.color(ThemeRole::WhichKeyGroup)
                    } else {
                        theme.color(ThemeRole::Text)
                    };
                    rect()
                        .key(index)
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(theme.spacing.md)
                        .child(KeyLabel::new(entry.key))
                        .child(
                            label()
                                .text(entry.description)
                                .font_size(theme.font_sizes.sm)
                                .color(desc_color),
                        )
                });

                GlassPanel::new()
                    .padding(Gaps::new_all(theme.spacing.md))
                    .spacing(theme.spacing.xs)
                    .child(
                        label()
                            .text(wk.title)
                            .font_size(theme.font_sizes.sm)
                            .font_weight(FontWeight::BOLD)
                            .color(theme.color(ThemeRole::WhichKeyKey)),
                    )
                    .children(rows)
            }))
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(theme.spacing.sm)
                    .child(Pill::new(mode_name).dot(mode_role))
                    .maybe_child((!pending_text.is_empty()).then(|| KeyLabel::new(pending_text))),
            )
    }
}
