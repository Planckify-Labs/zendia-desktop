/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Top-level window shell: space gradient backdrop, optional top toolbar, sidebar frame, resize handle, and rounded
//! inset page card.

use crate::ui::chrome::layout::{
    BackdropEndpoints, ChromePalette, PAGE_CARD_RADIUS, SPACE_GRADIENT_CROSSFADE, ToolbarSlots, page_card_margins,
    window_backdrop_endpoints,
};
use crate::ui::chrome::sidebar::{SidebarFrame, SidebarResizeHandle};
use crate::ui::chrome::toolbar::TopToolbar;
use crate::ui::core_view::use_core;
use crate::ui::theme::{ResolvedScheme, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::model::{SidebarSide, SpaceId};

/// Composes Zendia's complete browser window shell around a caller-supplied page element (real [`PageStage`] in the
/// live window, or a placeholder card in the UI gallery).
#[derive(Clone, PartialEq)]
pub struct WindowShell {
    location: State<String>,
    location_a11y_id: AccessibilityId,
    page: Element,
    interactive_window_controls: bool,
    key: DiffKey,
}

impl KeyExt for WindowShell {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl WindowShell {
    pub fn new(location: State<String>, location_a11y_id: AccessibilityId, page: impl IntoElement) -> Self {
        Self {
            location,
            location_a11y_id,
            page: page.into_element(),
            interactive_window_controls: true,
            key: DiffKey::None,
        }
    }

    /// Disables OS window minimize/maximize/close actions (used when previewing the shell inside `zendia-gallery`).
    pub fn interactive_window_controls(mut self, interactive: bool) -> Self {
        self.interactive_window_controls = interactive;
        self
    }
}

impl Component for WindowShell {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let is_resizing = use_state(|| false);

        let (layout, active_space, toolbar_items) = {
            let model = core.model().read();
            (
                model.layout.clone(),
                model.active_space().cloned(),
                model.contributions.toolbar_items.clone(),
            )
        };

        let target_endpoints = window_backdrop_endpoints(active_space.as_ref(), theme.scheme);
        let gradient_key: (Option<SpaceId>, ResolvedScheme) = (active_space.as_ref().map(|s| s.id), theme.scheme);

        let mut from_endpoints = use_state(|| target_endpoints);
        let mut to_endpoints = use_state(|| target_endpoints);
        let mut last_gradient_key = use_state(|| gradient_key);

        if *last_gradient_key.peek() != gradient_key {
            let prev = *to_endpoints.peek();
            from_endpoints.set(prev);
            to_endpoints.set(target_endpoints);
            last_gradient_key.set(gradient_key);
        }

        let crossfade = use_animation_with_dependencies(&gradient_key, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(SPACE_GRADIENT_CROSSFADE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let t = crossfade.get().value().clamp(0.0, 1.0);
        let backdrop = BackdropEndpoints::lerp(&from_endpoints.read(), &to_endpoints.read(), t).to_gradient();
        let slots = ToolbarSlots::resolve(&toolbar_items, &layout);
        let (card_top, card_right, card_bottom, card_left) = page_card_margins(&layout);

        let top_bar = layout.has_top_toolbar().then(|| TopToolbar {
            slots: slots.clone(),
            location: self.location,
            location_a11y_id: self.location_a11y_id,
            interactive_window_controls: self.interactive_window_controls,
        });

        let sidebar = SidebarFrame {
            slots,
            location: self.location,
            location_a11y_id: self.location_a11y_id,
            interactive_window_controls: self.interactive_window_controls,
            is_resizing,
        };

        let resize_handle = SidebarResizeHandle {
            side: layout.sidebar_side,
            expanded: layout.sidebar_expanded,
            current_width: layout.sidebar_width,
            is_resizing,
        };

        let card_shadow = match theme.scheme {
            ResolvedScheme::Dark => Shadow::new()
                .x(0.0)
                .y(3.0)
                .blur(10.0)
                .color(Color::from_argb(72, 0, 0, 0)),
            ResolvedScheme::Light => Shadow::new()
                .x(0.0)
                .y(2.0)
                .blur(8.0)
                .color(Color::from_argb(36, 0, 0, 0)),
        };

        let page_card = rect()
            .width(Size::flex(1.0))
            .height(Size::fill())
            .padding(Gaps::new(card_top, card_right, card_bottom, card_left))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .corner_radius(CornerRadius::new_all(PAGE_CARD_RADIUS))
                    .overflow(Overflow::Clip)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(palette.page_card_border),
                    )
                    .shadow(card_shadow)
                    .child(self.page.clone()),
            );

        let body = match layout.sidebar_side {
            SidebarSide::Left => rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .child(sidebar)
                .child(resize_handle)
                .child(page_card),
            SidebarSide::Right => rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .child(page_card)
                .child(resize_handle)
                .child(sidebar),
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .background(backdrop)
            .color(palette.text)
            .maybe_child(top_bar)
            .child(body)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
