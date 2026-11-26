/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Centered modal dialog for WebExtension install, permission, and removal consent prompts.

use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;

/// Window-root overlay showing any pending [`ExtensionPrompt`](zendia_browser_core::modules::ExtensionPrompt) from
/// `ExtensionServer`.
#[derive(Clone, PartialEq)]
pub struct ExtensionPromptOverlay;

impl Component for ExtensionPromptOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let prompt = core.model().read().pending_extension_prompt.clone();

        let Some(prompt) = prompt else {
            return rect().into_element();
        };

        let token = prompt.token;
        let core_cancel = core.clone();
        let core_accept = core.clone();
        let core_esc = core.clone();

        let detail_rows: Vec<Element> = prompt
            .details
            .iter()
            .map(|detail| {
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::Start)
                    .spacing(8.0)
                    .child(
                        label()
                            .text("•")
                            .font_size(12.5)
                            .color(theme.color(ThemeRole::TextMuted)),
                    )
                    .child(
                        label()
                            .width(Size::flex(1.0))
                            .text(detail.clone())
                            .font_size(12.5)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .into_element()
            })
            .collect();

        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .background(Color::from_argb(128, 0, 0, 0))
            .center()
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if e.code == Code::Escape {
                    e.stop_propagation();
                    e.prevent_default();
                    core_esc.invoke_or_log(Command::ExtensionPromptAnswer { token, accepted: false });
                }
            })
            .child(
                rect()
                    .width(Size::px(420.0))
                    .max_width(Size::percent(90.0))
                    .padding(Gaps::new_all(20.0))
                    .spacing(14.0)
                    .corner_radius(CornerRadius::new_all(12.0))
                    .background(theme.color(ThemeRole::Surface))
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.color(ThemeRole::Border)),
                    )
                    .shadow(theme.shadows.float_drop.to_shadow())
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::Center)
                            .spacing(10.0)
                            .child(
                                rect()
                                    .width(Size::px(30.0))
                                    .height(Size::px(30.0))
                                    .corner_radius(CornerRadius::new_all(8.0))
                                    .background(theme.rgba(ThemeRole::Accent).with_alpha(0.15).to_color())
                                    .center()
                                    .child(Icon::new("extension").size(16.0).role(ThemeRole::Accent)),
                            )
                            .child(
                                label()
                                    .width(Size::flex(1.0))
                                    .text(prompt.title.clone())
                                    .font_size(15.0)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(theme.color(ThemeRole::Text)),
                            ),
                    )
                    .maybe_child((!prompt.message.is_empty()).then(|| {
                        label()
                            .width(Size::fill())
                            .text(prompt.message.clone())
                            .font_size(13.0)
                            .color(theme.color(ThemeRole::TextMuted))
                    }))
                    .maybe_child((!detail_rows.is_empty()).then(|| {
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(10.0, 12.0, 10.0, 12.0))
                            .spacing(6.0)
                            .corner_radius(CornerRadius::new_all(8.0))
                            .background(theme.color(ThemeRole::SurfaceRaised))
                            .children(detail_rows)
                    }))
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .main_align(Alignment::End)
                            .spacing(8.0)
                            .child(
                                rect()
                                    .padding(Gaps::new(7.0, 14.0, 7.0, 14.0))
                                    .corner_radius(CornerRadius::new_all(8.0))
                                    .background(theme.color(ThemeRole::SurfaceRaised))
                                    .border(
                                        Border::new()
                                            .width(1.0)
                                            .alignment(BorderAlignment::Inner)
                                            .fill(theme.color(ThemeRole::Border)),
                                    )
                                    .cursor(CursorIcon::Pointer)
                                    .on_press(move |e: Event<PressEventData>| {
                                        e.stop_propagation();
                                        core_cancel
                                            .invoke_or_log(Command::ExtensionPromptAnswer { token, accepted: false });
                                    })
                                    .child(
                                        label()
                                            .text("Cancel")
                                            .font_size(13.0)
                                            .font_weight(FontWeight::MEDIUM)
                                            .color(theme.color(ThemeRole::Text)),
                                    ),
                            )
                            .child(
                                rect()
                                    .padding(Gaps::new(7.0, 14.0, 7.0, 14.0))
                                    .corner_radius(CornerRadius::new_all(8.0))
                                    .background(theme.color(ThemeRole::Accent))
                                    .cursor(CursorIcon::Pointer)
                                    .on_press(move |e: Event<PressEventData>| {
                                        e.stop_propagation();
                                        core_accept
                                            .invoke_or_log(Command::ExtensionPromptAnswer { token, accepted: true });
                                    })
                                    .child(
                                        label()
                                            .text(prompt.accept_label.clone())
                                            .font_size(13.0)
                                            .font_weight(FontWeight::SEMI_BOLD)
                                            .color(theme.color(ThemeRole::TextOnAccent)),
                                    ),
                            ),
                    ),
            )
            .into_element()
    }
}
