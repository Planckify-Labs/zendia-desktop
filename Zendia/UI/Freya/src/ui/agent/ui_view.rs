/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Native interactive agent tables (`AgentTableCard`) and declared ModuleApi UI cards (`AgentDeclaredPageCard`).
//!
//! Replaces the legacy fork's HTML/sandboxed UI blocks (`ZendiaAgentUiView`) with native Freya widgets:
//! - Sortable columns with numeric detection & right-alignment (`is_number_column`, `filter_and_sort_rows`).
//! - Conditional search filter input when a table has `>= 6` rows.
//! - One-click `Copy CSV` button (`table_to_csv`) with `1.5s` `"Copied"` confirmation.
//! - Clickable linked rows opening the row's URL in a new tab.
//! - Native [`DeclaredUi`] cards for declarative `zendia-module-api::ui::UiNode` pages.

use crate::ui::agent::agent_input_colors;
use crate::ui::agent::markdown::copy_to_clipboard;
use crate::ui::core_view::use_core;
use crate::ui::kit::{DeclaredUi, Icon};
use crate::ui::theme::{ResolvedScheme, ThemeRole, use_theme};
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::agent::{
    AgentDeclaredPage, AgentTable, TableSort, filter_and_sort_rows, is_empty_cell, is_number_column, table_to_csv,
};
use zendia_browser_core::commands::Command;

/// Native interactive sortable/filterable agent table card.
#[derive(Clone, PartialEq)]
pub struct AgentTableCard {
    pub table: AgentTable,
    key: DiffKey,
}

impl KeyExt for AgentTableCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl AgentTableCard {
    pub fn new(table: AgentTable) -> Self {
        Self {
            table,
            key: DiffKey::None,
        }
    }
}

impl Component for AgentTableCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let filter_query = use_state(String::new);
        let mut sort_state = use_state(|| None::<TableSort>);
        let mut copied = use_state(|| false);
        let mut hovered_row = use_state(|| None::<usize>);

        let query = filter_query.read().clone();
        let sort = *sort_state.read();
        let visible_rows = filter_and_sort_rows(&self.table, &query, sort);
        let total_count = self.table.rows.len();
        let shown_count = visible_rows.len();
        let show_filter = total_count >= 6;
        let numeric_cols: Vec<bool> = (0..self.table.columns.len())
            .map(|c| is_number_column(&self.table, c))
            .collect();

        let card_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(18, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(185, 255, 255, 255),
        };
        let head_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(24, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(14, 20, 24, 38),
        };
        let row_hover_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(26, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(16, 20, 24, 38),
        };
        let border_col = theme.color(ThemeRole::BorderSubtle);

        let count_label = if !query.trim().is_empty() {
            format!("{shown_count} of {total_count}")
        } else {
            format!("{total_count} rows")
        };

        let is_copied = *copied.read();
        let table_for_csv = self.table.clone();
        let query_for_csv = query.clone();

        let header = rect()
            .width(Size::fill())
            .padding(Gaps::new(8.0, 10.0, 8.0, 10.0))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .main_align(Alignment::SpaceBetween)
            .spacing(8.0)
            .child(
                label()
                    .text(self.table.title.clone())
                    .font_size(12.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(theme.color(ThemeRole::Text))
                    .max_lines(1),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .child(
                        label()
                            .text(count_label)
                            .font_size(11.0)
                            .color(theme.color(ThemeRole::TextMuted))
                            .max_lines(1),
                    )
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Copy table as CSV")
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(4.0)
                            .height(Size::px(24.0))
                            .padding(Gaps::new(0.0, 9.0, 0.0, 9.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .background(Color::TRANSPARENT)
                            .border(
                                Border::new()
                                    .width(1.0)
                                    .alignment(BorderAlignment::Inner)
                                    .fill(border_col),
                            )
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                let rows = filter_and_sort_rows(&table_for_csv, &query_for_csv, sort);
                                let csv = table_to_csv(&table_for_csv.columns, &rows);
                                copy_to_clipboard(&csv);
                                copied.set(true);
                                spawn(async move {
                                    timer(Duration::from_millis(1500)).await;
                                    copied.set(false);
                                });
                            })
                            .child(
                                label()
                                    .text(if is_copied { "Copied" } else { "Copy CSV" })
                                    .font_size(11.0)
                                    .font_weight(FontWeight::MEDIUM)
                                    .color(if is_copied {
                                        theme.color(ThemeRole::Success)
                                    } else {
                                        theme.color(ThemeRole::Text)
                                    }),
                            ),
                    ),
            );

        let filter_bar = show_filter.then(|| {
            rect()
                .width(Size::fill())
                .padding(Gaps::new(0.0, 10.0, 8.0, 10.0))
                .child(
                    Input::new(filter_query)
                        .placeholder("Filter rows…")
                        .width(Size::fill())
                        .theme_colors(agent_input_colors(&theme, false)),
                )
        });

        let mut col_header_row = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .background(head_bg)
            .padding(Gaps::new(6.0, 10.0, 6.0, 10.0))
            .spacing(8.0);

        for (col_idx, col_name) in self.table.columns.iter().enumerate() {
            let is_num = numeric_cols.get(col_idx).copied().unwrap_or(false);
            let indicator = match sort {
                Some(TableSort { column, descending }) if column == col_idx => {
                    if descending {
                        " ▼"
                    } else {
                        " ▲"
                    }
                }
                _ => "",
            };
            col_header_row = col_header_row.child(
                rect()
                    .width(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .main_align(if is_num { Alignment::end() } else { Alignment::start() })
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        let next = match *sort_state.peek() {
                            Some(TableSort {
                                column,
                                descending: false,
                            }) if column == col_idx => Some(TableSort {
                                column: col_idx,
                                descending: true,
                            }),
                            Some(TableSort {
                                column,
                                descending: true,
                            }) if column == col_idx => None,
                            _ => Some(TableSort {
                                column: col_idx,
                                descending: false,
                            }),
                        };
                        sort_state.set(next);
                    })
                    .child(
                        label()
                            .text(format!("{col_name}{indicator}"))
                            .font_size(11.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(theme.color(ThemeRole::MdTableHead))
                            .max_lines(1),
                    ),
            );
        }

        let mut rows_col = rect().width(Size::fill());
        if visible_rows.is_empty() {
            rows_col = rows_col.child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(12.0, 10.0, 12.0, 10.0))
                    .center()
                    .child(
                        label()
                            .text("No matching rows")
                            .font_size(11.5)
                            .color(theme.color(ThemeRole::TextMuted)),
                    ),
            );
        } else {
            for (vis_idx, row) in visible_rows.into_iter().enumerate() {
                let is_linked = row.url.is_some();
                let is_hovered = *hovered_row.read() == Some(vis_idx);
                let url_opt = row.url.clone();
                let core_open = core.clone();

                let mut row_el = rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .padding(Gaps::new(6.0, 10.0, 6.0, 10.0))
                    .spacing(8.0)
                    .background(if is_hovered { row_hover_bg } else { Color::TRANSPARENT })
                    .cursor(if is_linked {
                        CursorIcon::Pointer
                    } else {
                        CursorIcon::Default
                    })
                    .on_pointer_enter(move |_| hovered_row.set_if_modified(Some(vis_idx)))
                    .on_pointer_leave(move |_| {
                        if *hovered_row.peek() == Some(vis_idx) {
                            hovered_row.set(None);
                        }
                    })
                    .on_press(move |e: Event<PressEventData>| {
                        if let Some(url) = &url_opt {
                            e.stop_propagation();
                            core_open.invoke_or_log(Command::TabNew {
                                url: Some(url.clone()),
                                after: None,
                                activate: true,
                            });
                        }
                    });

                for col_idx in 0..self.table.columns.len() {
                    let raw_val = row.cells.get(col_idx).map(String::as_str).unwrap_or("");
                    let is_num = numeric_cols.get(col_idx).copied().unwrap_or(false);
                    let empty = is_empty_cell(raw_val);
                    let display = if empty { "—".to_string() } else { raw_val.to_string() };
                    let text_col = if empty {
                        theme.color(ThemeRole::TextMuted)
                    } else if col_idx == 0 && is_linked {
                        theme.color(ThemeRole::MdLink)
                    } else {
                        theme.color(ThemeRole::Text)
                    };
                    row_el = row_el.child(
                        rect()
                            .width(Size::flex(1.0))
                            .direction(Direction::Horizontal)
                            .main_align(if is_num { Alignment::end() } else { Alignment::start() })
                            .child(label().text(display).font_size(12.0).color(text_col).max_lines(2)),
                    );
                }

                rows_col = rows_col
                    .child(rect().width(Size::fill()).height(Size::px(1.0)).background(border_col))
                    .child(row_el);
            }
        }

        let note = self.table.note.as_ref().map(|n| {
            rect()
                .width(Size::fill())
                .padding(Gaps::new(6.0, 10.0, 8.0, 10.0))
                .child(
                    label()
                        .text(n.clone())
                        .font_size(11.0)
                        .color(theme.color(ThemeRole::TextMuted)),
                )
        });

        rect()
            .width(Size::fill())
            .corner_radius(CornerRadius::new_all(12.0))
            .overflow(Overflow::Clip)
            .background(card_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_col),
            )
            .child(header)
            .maybe_child(filter_bar)
            .child(rect().width(Size::fill()).height(Size::px(1.0)).background(border_col))
            .child(col_header_row)
            .child(rows_col)
            .maybe_child(note)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Native card rendering an agent-published declarative [`zendia_module_api::ui::UiNode`] tree via [`DeclaredUi`].
#[derive(Clone, PartialEq)]
pub struct AgentDeclaredPageCard {
    pub page: AgentDeclaredPage,
    key: DiffKey,
}

impl KeyExt for AgentDeclaredPageCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl AgentDeclaredPageCard {
    pub fn new(page: AgentDeclaredPage) -> Self {
        Self {
            page,
            key: DiffKey::None,
        }
    }
}

impl Component for AgentDeclaredPageCard {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let card_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(18, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(190, 255, 255, 255),
        };
        let border_col = theme.color(ThemeRole::BorderSubtle);

        rect()
            .width(Size::fill())
            .corner_radius(CornerRadius::new_all(12.0))
            .overflow(Overflow::Clip)
            .background(card_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_col),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(8.0, 10.0, 8.0, 10.0))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .main_align(Alignment::SpaceBetween)
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(6.0)
                            .child(Icon::new("sparkles").size(12.0).role(ThemeRole::Accent))
                            .child(
                                label()
                                    .text(self.page.title.clone())
                                    .font_size(12.5)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(theme.color(ThemeRole::Text))
                                    .max_lines(1),
                            ),
                    )
                    .maybe_child(self.page.draft.then(|| {
                        label()
                            .text("building…")
                            .font_size(11.0)
                            .color(theme.color(ThemeRole::TextMuted))
                    })),
            )
            .child(rect().width(Size::fill()).height(Size::px(1.0)).background(border_col))
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(10.0, 10.0, 10.0, 10.0))
                    .child(DeclaredUi::new(self.page.root.clone())),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
