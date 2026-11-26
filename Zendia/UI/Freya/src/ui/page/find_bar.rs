/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The find bar over the page (`/`, `page.find`): typing highlights the text's matches in the page and goes to the
//! first, `<CR>` goes to the next and `<S-CR>` to the one before, `<Esc>` closes it. `n` and `N` search the last text
//! again with the bar closed. What was searched and the matches are the core's ([`zendia_browser_core::model::PageFind`]);
//! whether the bar is open and its text are this window's.

use crate::ui::core_view::use_core;
use crate::ui::kit::text_field::TextField;
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::tab_actions::TabAction;

/// Whether the find bar is open, and its text.
#[derive(Clone, Copy, PartialEq)]
pub struct FindBarState {
    pub open: State<bool>,
    pub query: State<String>,
    pub field: AccessibilityId,
    /// Grows each time the bar opens, so its text is selected for typing over.
    pub opened: State<u64>,
}

impl FindBarState {
    /// Opens the bar with the last text searched, selected.
    pub fn open(&self, last_query: &str) {
        let mut query = self.query;
        if query.peek().is_empty() {
            query.set(last_query.to_string());
        }
        let mut opened = self.opened;
        let next = *opened.peek() + 1;
        opened.set(next);
        let mut open = self.open;
        open.set(true);
        self.field.request_focus();
    }

    pub fn close(&self) {
        let mut open = self.open;
        open.set_if_modified(false);
    }

    pub fn is_open(&self) -> bool {
        *self.open.peek()
    }
}

/// Installs [`FindBarState`] for the current window.
pub fn use_provide_find_bar() -> FindBarState {
    let open = use_state(|| false);
    let query = use_state(String::new);
    let opened = use_state(|| 0u64);
    let field = use_hook(AccessibilityId::new_unique);
    use_provide_context(|| FindBarState {
        open,
        query,
        field,
        opened,
    })
}

/// The find bar, at the bottom of the page.
#[derive(Clone, PartialEq, Default)]
pub struct FindBar {
    pub key: DiffKey,
}

impl KeyExt for FindBar {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for FindBar {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let Some(state) = try_consume_context::<FindBarState>() else {
            return rect().into_element();
        };
        if !*state.open.read() {
            return rect().into_element();
        }
        let find = core.model().read().page_find.clone();

        let invoke = {
            let core = core.clone();
            move |action: TabAction| {
                core.invoke_or_log(Command::Tabs(action));
            }
        };
        let on_change = {
            let invoke = invoke.clone();
            move |query: String| invoke(TabAction::PageFindQuery { query })
        };
        let on_submit = {
            let invoke = invoke.clone();
            move |_: String| invoke(TabAction::SearchNext)
        };
        let on_pre_key_down = {
            let invoke = invoke.clone();
            move |e: Event<KeyboardEventData>| -> bool {
                match &e.key {
                    Key::Named(NamedKey::Escape) => {
                        invoke(TabAction::PageFindClose);
                        state.close();
                        false
                    }
                    Key::Named(NamedKey::Enter) if e.modifiers.contains(Modifiers::SHIFT) => {
                        invoke(TabAction::SearchPrev);
                        false
                    }
                    _ => true,
                }
            }
        };
        let on_close = {
            let invoke = invoke.clone();
            move |_: Event<MouseEventData>| {
                invoke(TabAction::PageFindClose);
                state.close();
            }
        };

        let status = if state.query.read().is_empty() {
            String::new()
        } else {
            match (find.current, find.total) {
                (_, Some(0)) => "Phrase not found".to_string(),
                (Some(0), Some(1)) => "1 of 1 match".to_string(),
                (Some(current), Some(total)) => format!("{} of {total} matches", current + 1),
                _ => String::new(),
            }
        };

        rect()
            .position(Position::new_absolute().left(0.0).bottom(12.0))
            .width(Size::percent(100.0))
            .main_align(Alignment::center())
            .cross_align(Alignment::center())
            .layer(Layer::Relative(12))
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .padding(Gaps::new(6.0, 8.0, 6.0, 10.0))
                    .corner_radius(CornerRadius::new_all(10.0))
                    .background(theme.color(ThemeRole::SurfaceRaised))
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.rgba(ThemeRole::BorderSubtle).to_color()),
                    )
                    .shadow(Shadow::new().y(4.0).blur(16.0).color(Color::from_argb(56, 0, 0, 0)))
                    .child(
                        TextField::new(state.query)
                            .a11y_id(state.field)
                            .auto_focus(true)
                            .select_all_seq(*state.opened.read())
                            .placeholder("Find in page")
                            .width(Size::px(260.0))
                            .padding(Gaps::new(4.0, 6.0, 4.0, 6.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .background(theme.color(ThemeRole::Surface))
                            .focus_background(theme.color(ThemeRole::Surface))
                            .color(theme.color(ThemeRole::Text))
                            .placeholder_color(theme.color(ThemeRole::TextMuted))
                            .on_change(on_change)
                            .on_submit(on_submit)
                            .on_pre_key_down(on_pre_key_down),
                    )
                    .child(
                        label()
                            .text(status)
                            .font_size(12.0)
                            .color(theme.color(ThemeRole::TextMuted))
                            .max_lines(1),
                    )
                    .child(
                        rect()
                            .padding(Gaps::new(2.0, 6.0, 2.0, 6.0))
                            .corner_radius(CornerRadius::new_all(4.0))
                            .on_mouse_up(on_close)
                            .child(
                                label()
                                    .text("\u{2715}")
                                    .font_size(12.0)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                    ),
            )
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
