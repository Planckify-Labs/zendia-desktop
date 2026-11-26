/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Native interactive agent tables (`AgentTableCard`) and declared ModuleApi UI cards (`AgentDeclaredPageCard`).
//!
//! Matches `ZendiaAgentUiView` (`UV-01`, `UV-02`):
//! - Sortable columns with numeric detection, natural numeric string sorting (`natural_cmp`), and hover/focus states.
//! - Conditional search `TextField` filter when `>= 6` rows OR while the filter query is non-empty.
//! - Singular `"1 row"` vs `"{n} rows"` grammar and `Copy CSV` button with hover wash & `1.5 s` `"Copied"` state.
//! - Horizontal `ScrollView` with intrinsic column widths so wide tables never crush columns.
//! - Linked rows honor `row.tab_id` first (`Command::TabActivate`), then safe `http(s)://` `row.url` (`Command::TabNew`),
//!   with keyboard `Enter`/`Space` activation, `:focus-visible` ring, hover tint, and `1.6 s` fresh-row accent flash.

use crate::ui::agent::markdown::copy_to_clipboard;
use crate::ui::core_view::use_core;
use crate::ui::kit::{DeclaredUi, Icon, TextField, focus_ring_border, mix_color, use_interactive_feedback};
use crate::ui::theme::{ResolvedScheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::agent::{
    AgentDeclaredPage, AgentTable, AgentTableRow, TableSort, filter_and_sort_rows, is_empty_cell, is_number_column,
    table_to_csv,
};
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::TabId;

fn is_safe_http_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Estimates intrinsic minimum widths for each column so wide tables scroll horizontally cleanly (`UV-01`).
fn estimate_ui_table_col_widths(table: &AgentTable) -> Vec<f32> {
    let mut widths: Vec<f32> = table
        .columns
        .iter()
        .map(|col| (col.chars().count() as f32 * 7.5 + 34.0).clamp(84.0, 220.0))
        .collect();
    for row in &table.rows {
        for (idx, cell) in row.cells.iter().enumerate() {
            if idx < widths.len() {
                widths[idx] = widths[idx].max((cell.chars().count() as f32 * 6.8 + 22.0).clamp(84.0, 220.0));
            }
        }
    }
    widths
}

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
        let mut seen_row_count = use_state(|| self.table.rows.len());
        let mut fresh_from = use_state(|| self.table.rows.len());
        let mut table_content_h = use_state(|| 0.0f32);

        let total_count = self.table.rows.len();
        if total_count > *seen_row_count.peek() {
            let prev = *seen_row_count.peek();
            fresh_from.set(prev);
            seen_row_count.set(total_count);
        } else if total_count < *seen_row_count.peek() {
            seen_row_count.set(total_count);
            fresh_from.set(total_count);
        }

        let query = filter_query.read().clone();
        let sort = *sort_state.read();
        let visible_rows = filter_and_sort_rows(&self.table, &query, sort);
        let shown_count = visible_rows.len();
        // UV-01: Keep filter input visible while query is non-empty even if rows drop below 6.
        let show_filter = total_count >= 6 || !query.is_empty();
        let numeric_cols: Vec<bool> = (0..self.table.columns.len())
            .map(|c| is_number_column(&self.table, c))
            .collect();
        let col_widths = estimate_ui_table_col_widths(&self.table);
        let total_table_w: f32 = col_widths.iter().sum();

        let card_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(18, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(185, 255, 255, 255),
        };
        let head_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(24, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(14, 20, 24, 38),
        };
        let border_col = theme.color(ThemeRole::BorderSubtle);
        let rule_col = theme.color(ThemeRole::MdRule);

        // UV-01: Singular "1 row" vs "N rows".
        let count_label = if !query.trim().is_empty() {
            format!("{shown_count} of {total_count}")
        } else if total_count == 1 {
            "1 row".to_string()
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
                    .child(CopyCsvButton {
                        copied: is_copied,
                        on_press: EventHandler::new(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            let rows = filter_and_sort_rows(&table_for_csv, &query_for_csv, sort);
                            let csv = table_to_csv(&table_for_csv.columns, &rows);
                            copy_to_clipboard(&csv);
                            copied.set(true);
                            spawn(async move {
                                timer(Duration::from_millis(1500)).await;
                                copied.set(false);
                            });
                        }),
                        key: DiffKey::None,
                    }),
            );

        let filter_bar = show_filter.then(|| {
            rect()
                .width(Size::fill())
                .padding(Gaps::new(0.0, 10.0, 8.0, 10.0))
                .font_size(12.0)
                .child(
                    TextField::new(filter_query)
                        .placeholder("Filter rows…")
                        .width(Size::fill())
                        .height(Size::px(26.0))
                        .padding(Gaps::new(3.0, 8.0, 3.0, 8.0))
                        .corner_radius(CornerRadius::new_all(6.0))
                        .border(theme.color(ThemeRole::Border), 0.5)
                        .focus_border(theme.color(ThemeRole::Accent), 1.0)
                        .color(theme.color(ThemeRole::Text))
                        .placeholder_color(theme.color(ThemeRole::TextMuted)),
                )
        });

        let mut col_header_row = rect()
            .width(Size::px(total_table_w))
            .min_width(Size::percent(100.0))
            .direction(Direction::Horizontal)
            .background(head_bg);

        for (col_idx, col_name) in self.table.columns.iter().enumerate() {
            let is_num = numeric_cols.get(col_idx).copied().unwrap_or(false);
            let cw = col_widths.get(col_idx).copied().unwrap_or(96.0);
            let indicator = match sort {
                Some(TableSort { column, descending }) if column == col_idx => {
                    if descending {
                        " \u{25bc}"
                    } else {
                        " \u{25b2}"
                    }
                }
                _ => "",
            };
            col_header_row = col_header_row.child(SortHeaderCell {
                label: format!("{col_name}{indicator}"),
                width: cw,
                numeric: is_num,
                on_press: EventHandler::new(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    let next = TableSort::toggle(*sort_state.peek(), col_idx);
                    sort_state.set(next);
                }),
                key: DiffKey::from(&format!("sort-col-{col_idx}")),
            });
        }

        let mut rows_col = rect().width(Size::px(total_table_w)).min_width(Size::percent(100.0));
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
            let fresh_threshold = *fresh_from.read();
            for (vis_idx, row) in visible_rows.into_iter().enumerate() {
                let is_fresh = vis_idx >= fresh_threshold;
                let tab_id_opt = row.tab_id;
                let safe_url = row.url.as_ref().filter(|u| is_safe_http_url(u)).cloned();
                let core_act = core.clone();
                rows_col = rows_col
                    .child(rect().width(Size::fill()).height(Size::px(0.5)).background(rule_col))
                    .child(UiTableRowView {
                        row: row.clone(),
                        col_widths: col_widths.clone(),
                        numeric_cols: numeric_cols.clone(),
                        fresh: is_fresh,
                        on_activate: (tab_id_opt.is_some() || safe_url.is_some()).then(|| {
                            EventHandler::new(move |_| activate_table_row(&core_act, tab_id_opt, safe_url.as_deref()))
                        }),
                        key: DiffKey::from(&format!(
                            "ui-row-{vis_idx}-{}",
                            row.cells.first().map(String::as_str).unwrap_or("")
                        )),
                    });
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

        let est_table_h = ((shown_count.max(1) + 1) as f32 * 28.0 + 4.0).max(56.0);
        let scroll_table_h = {
            let m = *table_content_h.read();
            if m > 1.0 { m } else { est_table_h }
        };

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
            .child(
                ScrollView::new()
                    .width(Size::fill())
                    .height(Size::px(scroll_table_h))
                    .direction(Direction::Horizontal)
                    .child(
                        rect()
                            .width(Size::px(total_table_w))
                            .min_width(Size::percent(100.0))
                            .on_sized(move |e: Event<SizedEventData>| {
                                let ch = e.area.size.height;
                                if ch > 1.0 && (ch - *table_content_h.peek()).abs() > 0.5 {
                                    table_content_h.set(ch);
                                }
                            })
                            .child(col_header_row)
                            .child(rows_col),
                    ),
            )
            .maybe_child(note)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn activate_table_row(core: &crate::ui::core_view::CoreView, tab_id: Option<TabId>, url: Option<&str>) {
    // UV-01: Try focusTab(row.tab_id) first, then openLink(row.url) for http/https URLs.
    if let Some(tid) = tab_id {
        core.invoke_or_log(Command::TabActivate { tab_id: tid });
        return;
    }
    if let Some(u) = url
        && is_safe_http_url(u)
    {
        core.invoke_or_log(Command::TabNew {
            url: Some(u.to_string()),
            after: None,
            activate: true,
        });
    }
}

#[derive(Clone, PartialEq)]
struct CopyCsvButton {
    copied: bool,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for CopyCsvButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for CopyCsvButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let bg = fb.background(
            Color::TRANSPARENT,
            theme.rgba(ThemeRole::Border).with_alpha(0.35).to_color(),
            theme.rgba(ThemeRole::Border).with_alpha(0.50).to_color(),
        );
        let base_border = Border::new()
            .width(0.5)
            .alignment(BorderAlignment::Inner)
            .fill(theme.color(ThemeRole::Border));
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, base_border);
        let on_press = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Copy table as CSV")
            .height(Size::px(24.0))
            .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .border(border)
            .scale(fb.scale())
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| on_press.call(e))
            .child(
                label()
                    .text(if self.copied { "Copied" } else { "Copy CSV" })
                    .font_size(12.0)
                    .font_weight(FontWeight::MEDIUM)
                    .color(if self.copied {
                        theme.color(ThemeRole::Success)
                    } else {
                        theme.color(ThemeRole::Text)
                    }),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SortHeaderCell {
    label: String,
    width: f32,
    numeric: bool,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for SortHeaderCell {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SortHeaderCell {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let bg = fb.background(
            Color::TRANSPARENT,
            theme.rgba(ThemeRole::Text).with_alpha(0.06).to_color(),
            theme.rgba(ThemeRole::Text).with_alpha(0.12).to_color(),
        );
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let on_press = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Sort by {}", self.label))
            .width(Size::px(self.width))
            .padding(Gaps::new(5.0, 8.0, 5.0, 8.0))
            .background(bg)
            .border(border)
            .direction(Direction::Horizontal)
            .main_align(if self.numeric {
                Alignment::end()
            } else {
                Alignment::start()
            })
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| on_press.call(e))
            .child(
                label()
                    .text(self.label.clone())
                    .font_size(12.0)
                    .font_weight(FontWeight::BOLD)
                    .color(theme.color(ThemeRole::MdTableHead))
                    .max_lines(1),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct UiTableRowView {
    row: AgentTableRow,
    col_widths: Vec<f32>,
    numeric_cols: Vec<bool>,
    fresh: bool,
    on_activate: Option<EventHandler<()>>,
    key: DiffKey,
}

impl KeyExt for UiTableRowView {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for UiTableRowView {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let is_linked = self.on_activate.is_some();
        let fb = use_interactive_feedback(is_linked);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        // UV-01: 1.6s fresh-row accent flash when new rows arrive.
        let flash = use_animation_with_dependencies(&self.fresh, |conf, fresh| {
            if *fresh {
                conf.on_creation(OnCreation::Run);
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(1.0, 0.0)
                .duration(Duration::from_millis(1600))
                .ease(Ease::Out)
                .function(motion::css_ease_out_fn())
        });

        let flash_t = if self.fresh {
            flash.get().value().clamp(0.0, 1.0)
        } else {
            0.0
        };
        let rest_bg = if flash_t > 0.01 {
            theme.rgba(ThemeRole::Accent).with_alpha(0.28 * flash_t).to_color()
        } else {
            Color::TRANSPARENT
        };
        let hover_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.10).to_color();
        let press_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.18).to_color();
        let bg = if is_linked {
            fb.background(rest_bg, hover_bg, press_bg)
        } else {
            mix_color(Color::TRANSPARENT, rest_bg, 1.0)
        };
        let border = focus_ring_border(is_linked && matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let on_press = self.on_activate.clone();
        let on_key = self.on_activate.clone();

        let mut row_el = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(is_linked)
            .a11y_role(if is_linked {
                AccessibilityRole::Button
            } else {
                AccessibilityRole::Row
            })
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::start())
            .background(bg)
            .border(border)
            .cursor(if is_linked {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                if let Some(act) = &on_press {
                    e.stop_propagation();
                    act.call(());
                }
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if let Some(act) = &on_key
                    && (e.key == Key::Named(NamedKey::Enter) || matches!(&e.key, Key::Character(c) if c == " "))
                {
                    e.stop_propagation();
                    e.prevent_default();
                    act.call(());
                }
            });

        for (col_idx, cw) in self.col_widths.iter().copied().enumerate() {
            let raw_val = self.row.cells.get(col_idx).map(String::as_str).unwrap_or("");
            let is_num = self.numeric_cols.get(col_idx).copied().unwrap_or(false);
            let empty = is_empty_cell(raw_val);
            let display = if empty {
                "\u{2014}".to_string()
            } else {
                raw_val.to_string()
            };
            let text_col = if empty {
                theme.color(ThemeRole::TextMuted)
            } else if col_idx == 0 && is_linked {
                theme.color(ThemeRole::MdLink)
            } else {
                theme.color(ThemeRole::Text)
            };
            row_el = row_el.child(
                rect()
                    .width(Size::px(cw))
                    .padding(Gaps::new(5.0, 8.0, 5.0, 8.0))
                    .direction(Direction::Horizontal)
                    .main_align(if is_num { Alignment::end() } else { Alignment::start() })
                    .child(label().text(display).font_size(12.0).color(text_col).max_lines(2)),
            );
        }

        row_el
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
