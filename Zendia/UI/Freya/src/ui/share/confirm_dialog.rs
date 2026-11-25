/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Share link confirmation modal dialog (`zen-share-confirm.inc.xhtml`).

use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::share::state::try_use_share_state;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::share::SharePayload;

const DIALOG_WIDTH: f32 = 452.0;

/// Window-root overlay rendering [`ShareConfirmDialog`] centered over a dimmed backdrop when open.
#[derive(Clone, PartialEq)]
pub struct ShareConfirmOverlay;

impl Component for ShareConfirmOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let share_state = try_use_share_state();
        let payload = share_state.and_then(|s| s.payload.read().clone());

        let Some(payload) = payload else {
            return rect().into_element();
        };
        let share_state = share_state.expect("checked above");

        let scrim_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(0, 0, 0, 0.34).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.14).to_color(),
        };

        let esc_core = core.clone();
        let scrim_core = core.clone();

        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .background(scrim_bg)
            .center()
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Escape)) {
                    e.stop_propagation();
                    esc_core.invoke_or_log(Command::ShareCancel);
                }
            })
            .on_mouse_down(move |_| {
                scrim_core.invoke_or_log(Command::ShareCancel);
            })
            .child(ShareConfirmDialog {
                payload,
                name_draft: share_state.name_draft,
                dont_ask: share_state.dont_ask,
                auto_focus: true,
                key: DiffKey::from(&"share-confirm-modal"),
            })
            .into_element()
    }
}

/// Share link confirmation modal card, reusable in both [`ShareConfirmOverlay`] and the UI gallery.
#[derive(Clone, PartialEq)]
pub struct ShareConfirmDialog {
    pub payload: SharePayload,
    pub name_draft: State<String>,
    pub dont_ask: State<bool>,
    pub auto_focus: bool,
    pub key: DiffKey,
}

impl KeyExt for ShareConfirmDialog {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ShareConfirmDialog {
    pub fn new(payload: SharePayload, name_draft: State<String>, dont_ask: State<bool>) -> Self {
        Self {
            payload,
            name_draft,
            dont_ask,
            auto_focus: false,
            key: DiffKey::None,
        }
    }
}

impl Component for ShareConfirmDialog {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let input_a11y_id = use_a11y();
        let mut cancel_hovered = use_state(|| false);
        let mut share_hovered = use_state(|| false);
        let mut check_hovered = use_state(|| false);

        let is_dark = theme.scheme == ResolvedScheme::Dark;
        let (
            card_bg,
            card_border,
            title_color,
            body_color,
            muted_color,
            input_bg,
            input_border,
            checkbox_bg,
            checkbox_border,
            cancel_bg,
            cancel_hover_bg,
            cancel_fg,
            share_bg,
            share_hover_bg,
            share_fg,
        ) = if is_dark {
            (
                Color::from_rgb(28, 28, 31),
                Rgba::from_rgb_alpha(255, 255, 255, 0.14).to_color(),
                Color::from_rgb(245, 245, 247),
                Color::from_argb(230, 238, 238, 242),
                Color::from_argb(145, 210, 210, 220),
                Color::from_rgb(38, 38, 43),
                Color::from_rgb(142, 134, 168),
                Color::from_rgb(48, 48, 54),
                Rgba::from_rgb_alpha(255, 255, 255, 0.18).to_color(),
                Color::from_rgb(56, 56, 61),
                Color::from_rgb(68, 68, 74),
                Color::from_rgb(238, 238, 242),
                Color::from_rgb(228, 228, 232),
                Color::from_rgb(246, 246, 250),
                Color::from_rgb(24, 24, 28),
            )
        } else {
            (
                Color::from_rgb(255, 255, 255),
                Rgba::from_rgb_alpha(0, 0, 0, 0.14).to_color(),
                Color::from_rgb(20, 20, 24),
                Color::from_rgb(32, 34, 40),
                Color::from_rgb(108, 110, 120),
                Color::from_rgb(255, 255, 255),
                Color::from_rgb(42, 46, 58),
                Color::from_rgb(245, 245, 247),
                Rgba::from_rgb_alpha(0, 0, 0, 0.24).to_color(),
                Color::from_rgb(228, 228, 232),
                Color::from_rgb(214, 214, 220),
                Color::from_rgb(24, 24, 28),
                Color::from_rgb(28, 28, 32),
                Color::from_rgb(46, 46, 52),
                Color::from_rgb(255, 255, 255),
            )
        };

        let name_draft = self.name_draft;
        let mut dont_ask = self.dont_ask;
        let checked = *dont_ask.read();
        let share_url = self.payload.url.clone();
        let share_title = self.payload.title.clone();
        let share_summary = self.payload.summary.clone();

        let confirm_core = core.clone();
        let confirm_url = share_url.clone();
        let confirm_title = share_title.clone();
        let do_confirm = move || {
            let display_name = name_draft.peek().clone();
            let skip = *dont_ask.peek();
            confirm_core.invoke_or_log(Command::ShareConfirm {
                display_name,
                dont_ask_again: skip,
                url: Some(confirm_url.clone()),
                title: Some(confirm_title.clone()),
            });
        };

        let cancel_core = core.clone();
        let do_cancel = move || {
            cancel_core.invoke_or_log(Command::ShareCancel);
        };

        let key_confirm = do_confirm.clone();
        let key_cancel = do_cancel.clone();
        let on_input_key = move |e: Event<KeyboardEventData>| match e.key {
            Key::Named(NamedKey::Enter) => {
                e.stop_propagation();
                e.prevent_default();
                key_confirm();
                false
            }
            Key::Named(NamedKey::Escape) => {
                e.stop_propagation();
                e.prevent_default();
                key_cancel();
                false
            }
            _ => true,
        };

        let btn_cancel = do_cancel;
        let btn_confirm = do_confirm;

        let target_row = (!share_summary.is_empty()).then(|| {
            rect()
                .width(Size::fill())
                .padding(Gaps::new(6.0, 10.0, 6.0, 10.0))
                .spacing(6.0)
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .corner_radius(CornerRadius::new_all(8.0))
                .background(if is_dark {
                    Rgba::from_rgb_alpha(255, 255, 255, 0.05).to_color()
                } else {
                    Rgba::from_rgb_alpha(0, 0, 0, 0.045).to_color()
                })
                .child(Icon::new("link").size(12.5).role(ThemeRole::TextMuted))
                .child(
                    label()
                        .text(share_summary)
                        .font_size(11.5)
                        .color(muted_color)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
        });

        rect()
            .a11y_role(AccessibilityRole::Dialog)
            .a11y_alt("Share link confirmation")
            .width(Size::px(DIALOG_WIDTH))
            .padding(Gaps::new(24.0, 24.0, 20.0, 24.0))
            .spacing(14.0)
            .corner_radius(CornerRadius::new_all(16.0))
            .background(card_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(card_border),
            )
            .shadow(
                Shadow::new()
                    .x(0.0)
                    .y(14.0)
                    .blur(36.0)
                    .color(Color::from_argb(if is_dark { 130 } else { 58 }, 0, 0, 0)),
            )
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .child(
                label()
                    .text("Share link")
                    .font_size(19.0)
                    .font_weight(FontWeight::BOLD)
                    .color(title_color),
            )
            .child(
                label()
                    .text("Zendia uploads a copy of these tabs to the share server and copies a link anyone can open. The link expires after 30 days.")
                    .font_size(13.0)
                    .color(body_color),
            )
            .child(
                label()
                    .text("Sharing is anonymous unless you add a name.")
                    .font_size(13.0)
                    .color(body_color),
            )
            .maybe_child(target_row)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(38.0))
                    .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .corner_radius(CornerRadius::new_all(8.0))
                    .background(input_bg)
                    .border(
                        Border::new()
                            .width(1.5)
                            .alignment(BorderAlignment::Inner)
                            .fill(input_border),
                    )
                    .child(
                        Input::new(name_draft)
                            .a11y_id(input_a11y_id)
                            .auto_focus(self.auto_focus)
                            .flat()
                            .compact()
                            .placeholder("Your name (optional)")
                            .width(Size::fill())
                            .background(Color::TRANSPARENT)
                            .focus_background(Color::TRANSPARENT)
                            .border_fill(Color::TRANSPARENT)
                            .focus_border_fill(Color::TRANSPARENT)
                            .color(title_color)
                            .placeholder_color(muted_color)
                            .padding(Gaps::new(0.0, 2.0, 0.0, 2.0))
                            .corner_radius(CornerRadius::new_all(0.0))
                            .on_pre_key_down(on_input_key),
                    ),
            )
            .child(
                rect()
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::CheckBox)
                    .a11y_alt("Don't ask me again")
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| check_hovered.set_if_modified(true))
                    .on_pointer_leave(move |_| check_hovered.set_if_modified(false))
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        let next = !*dont_ask.peek();
                        dont_ask.set(next);
                    })
                    .child(
                        rect()
                            .width(Size::px(16.0))
                            .height(Size::px(16.0))
                            .corner_radius(CornerRadius::new_all(4.0))
                            .background(if checked {
                                theme.color(ThemeRole::Accent)
                            } else if check_hovered() {
                                cancel_hover_bg
                            } else {
                                checkbox_bg
                            })
                            .border(
                                Border::new()
                                    .width(1.0)
                                    .alignment(BorderAlignment::Inner)
                                    .fill(if checked {
                                        theme.color(ThemeRole::Accent)
                                    } else {
                                        checkbox_border
                                    }),
                            )
                            .center()
                            .maybe_child(checked.then(|| Icon::new("check").size(11.0).role(ThemeRole::Text))),
                    )
                    .child(
                        label()
                            .text("Don't ask me again")
                            .font_size(13.0)
                            .color(body_color),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(4.0, 0.0, 0.0, 0.0))
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::End)
                    .cross_align(Alignment::center())
                    .spacing(10.0)
                    .child(
                        rect()
                            .a11y_focusable(true)
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Cancel sharing")
                            .height(Size::px(36.0))
                            .padding(Gaps::new(0.0, 20.0, 0.0, 20.0))
                            .corner_radius(CornerRadius::new_all(9.0))
                            .background(if cancel_hovered() { cancel_hover_bg } else { cancel_bg })
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_pointer_enter(move |_| cancel_hovered.set_if_modified(true))
                            .on_pointer_leave(move |_| cancel_hovered.set_if_modified(false))
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                btn_cancel();
                            })
                            .child(
                                label()
                                    .text("Cancel")
                                    .font_size(13.5)
                                    .font_weight(FontWeight::MEDIUM)
                                    .color(cancel_fg)
                                    .max_lines(1),
                            ),
                    )
                    .child(
                        rect()
                            .a11y_focusable(true)
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Confirm and share link")
                            .height(Size::px(36.0))
                            .padding(Gaps::new(0.0, 22.0, 0.0, 22.0))
                            .corner_radius(CornerRadius::new_all(9.0))
                            .background(if share_hovered() { share_hover_bg } else { share_bg })
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_pointer_enter(move |_| share_hovered.set_if_modified(true))
                            .on_pointer_leave(move |_| share_hovered.set_if_modified(false))
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                btn_confirm();
                            })
                            .child(
                                label()
                                    .text("Share")
                                    .font_size(13.5)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(share_fg)
                                    .max_lines(1),
                            ),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
