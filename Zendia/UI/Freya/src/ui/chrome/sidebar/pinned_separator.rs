/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Pinned-tabs divider with Zen's inline "Clear" action that closes normal tabs in the active space.

use crate::ui::chrome::layout::ChromePalette;
use crate::ui::chrome::sidebar::state::try_use_sidebar_state;
use crate::ui::core_view::use_core;
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::SpaceId;

/// Separator between the pinned/folder section and the normal tabs section, revealing a "Clear" button when normal
/// tabs can be closed.
#[derive(Clone, PartialEq)]
pub struct PinnedSeparator {
    pub space_id: Option<SpaceId>,
    pub has_normal_tabs: bool,
    pub collapsed: bool,
}

impl Component for PinnedSeparator {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let sidebar_state = try_use_sidebar_state();
        let mut hovered = use_state(|| false);
        let mut clear_hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let force_hovered = sidebar_state.is_some_and(|s| *s.preview_hovered_separator.read());
        let show_clear =
            !self.collapsed && self.has_normal_tabs && (hovered() || force_hovered || focus() != Focus::Not);

        if self.collapsed {
            return rect()
                .width(Size::fill())
                .height(Size::px(14.0))
                .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                .center()
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(1.0))
                        .background(palette.divider),
                )
                .into_element();
        }

        let space_id = self.space_id;
        let on_clear = move |e: Event<PressEventData>| {
            e.stop_propagation();
            core.invoke_or_log(Command::SpaceClearTabs { space_id });
        };

        let clear_button = show_clear.then(|| {
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Clear unpinned tabs")
                .padding(Gaps::new(2.0, 6.0, 2.0, 6.0))
                .corner_radius(CornerRadius::new_all(5.0))
                .background(if clear_hovered() {
                    palette.surface_pill
                } else {
                    Color::TRANSPARENT
                })
                .border(if focus() == Focus::Keyboard {
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::FocusRing))
                } else {
                    Border::new().width(0.0).fill(Color::TRANSPARENT)
                })
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| clear_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| clear_hovered.set_if_modified(false))
                .on_press(on_clear)
                .child(
                    label()
                        .text("Clear")
                        .font_size(10.5)
                        .font_weight(FontWeight::MEDIUM)
                        .color(if clear_hovered() {
                            palette.text
                        } else {
                            palette.text_muted
                        }),
                )
        });

        rect()
            .width(Size::fill())
            .height(Size::px(22.0))
            .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
            .spacing(6.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .height(Size::px(1.0))
                    .background(palette.divider),
            )
            .maybe_child(clear_button)
            .into_element()
    }
}
