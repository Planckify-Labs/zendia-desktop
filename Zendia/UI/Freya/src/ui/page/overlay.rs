/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Page stage and overlay layer sitting above all [`PageSurface`](crate::ui::page::PageSurface)s for panels,
//! Glance cards, hints, and vim surfaces.

use freya::prelude::*;

pub use crate::ui::vim::VimOverlayLayer;

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
