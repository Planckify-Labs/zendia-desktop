/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The library's History section: search and filter bar, day/site grouped visit list, hover actions, and row context
//! menu (Zen's `ZenLibraryHistorySection`).

use crate::ui::chrome::menus::{OpenContextMenu, try_use_menu_state};
use crate::ui::chrome::sidebar::state::favicon_badge_colors;
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::kit::icons::prepared_icon_bytes;
use crate::ui::library::nav::{LibraryCursorRing, LibraryNavLayout, LibraryNavRow, use_library_nav};
use crate::ui::library::search::{LibraryFilterChip, LibraryFilterGroup, LibrarySearchBar};
use crate::ui::library::try_use_library_state;
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::history::{
    HistoryQuery, HistorySort, HistorySource, HistoryWhenFilter, build_history_row_context_menu, current_unix_secs,
    favicon_for_url, filter_and_group_history, format_display_url, format_visit_timestamp,
};
use zendia_browser_core::library::LibrarySection;
use zendia_browser_core::model::HistoryEntry;

/// Width of this section's body next to the rail (Zen's `418px - 110px`).
pub const WIDTH: f32 = 308.0;

#[derive(Clone, PartialEq)]
pub struct HistorySection;

impl Component for HistorySection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let library = try_use_library_state();

        let mut query = use_state(String::new);
        let mut when_filter = use_state(|| HistoryWhenFilter::All);
        let mut sort_mode = use_state(|| HistorySort::ByDate);
        let mut source_mode = use_state(|| HistorySource::History);
        let mut filters_open = use_state(|| false);
        let mut applied_query_override = use_state(|| None::<String>);
        let mut applied_filters_override = use_state(|| None::<bool>);

        // Sync gallery fixture overrides without fighting live user typing.
        if let Some(lib) = library {
            let next_q = lib.history_query_override.read().clone();
            if *applied_query_override.peek() != next_q {
                applied_query_override.set(next_q.clone());
                if let Some(q) = next_q {
                    query.set_if_modified(q);
                }
            }
            let next_f = *lib.history_filters_open_override.read();
            if *applied_filters_override.peek() != next_f {
                applied_filters_override.set(next_f);
                if let Some(open) = next_f {
                    filters_open.set_if_modified(open);
                }
            }
        }

        let entries = core.model().read().history.clone();
        let current_query = query.read().clone();
        let current_when = *when_filter.read();
        let current_sort = *sort_mode.read();
        let current_source = *source_mode.read();

        let source_entries: &[HistoryEntry] = match current_source {
            HistorySource::History => &entries,
            HistorySource::ClosedTabs => &[],
        };

        // Anchor "Today" to the newest entry when fixture timestamps are ahead of or behind the clock,
        // otherwise use the real clock.
        let now_unix = source_entries
            .iter()
            .map(|e| e.visited_at_unix)
            .max()
            .map(|latest| latest.max(current_unix_secs()))
            .unwrap_or_else(current_unix_secs);

        let history_query = HistoryQuery {
            query: current_query.clone(),
            when: current_when,
            sort: current_sort,
            source: current_source,
            now_unix,
        };
        let groups = filter_and_group_history(source_entries, &history_query);

        let mut nav_rows = Vec::new();
        for group in &groups {
            for entry in &group.entries {
                nav_rows.push(LibraryNavRow::new(
                    format!("history:{}", entry.id.0),
                    Some(Command::HistoryOpen {
                        entry_id: Some(entry.id),
                        url: None,
                        new_tab: true,
                        in_background: false,
                    }),
                ));
            }
        }
        let cursor_key = use_library_nav(LibrarySection::History, LibraryNavLayout::List, nav_rows);

        let showing_closed = current_source == HistorySource::ClosedTabs;
        let filter_groups = vec![
            LibraryFilterGroup::new(
                "when",
                "When was it visited?",
                HistoryWhenFilter::OPTIONS
                    .into_iter()
                    .map(|opt| LibraryFilterChip::new(opt.id(), opt.label(), current_when == opt))
                    .collect(),
            ),
            LibraryFilterGroup::new(
                "sort",
                "Sort by",
                HistorySort::ALL
                    .into_iter()
                    .map(|opt| {
                        LibraryFilterChip::new(opt.id(), opt.label(), current_sort == opt)
                            .disabled(showing_closed && opt == HistorySort::MostVisited)
                    })
                    .collect(),
            ),
            LibraryFilterGroup::new(
                "source",
                "What to look through",
                HistorySource::ALL
                    .into_iter()
                    .map(|opt| LibraryFilterChip::new(opt.id(), opt.label(), current_source == opt))
                    .collect(),
            ),
        ];

        let force_search_ring = library.is_some_and(|lib| *lib.force_search_focus_ring.read());
        let search_bar = LibrarySearchBar::new("Search History…", query)
            .filter_title("Filter History…")
            .filter_groups(filter_groups)
            .filters_open(filters_open)
            .auto_focus(!*filters_open.read())
            .force_focused_ring(force_search_ring)
            .on_toggle_filter(move |(group_id, option_id): (String, String)| match group_id.as_str() {
                "when" => {
                    let chosen = match option_id.as_str() {
                        "today" => HistoryWhenFilter::Today,
                        "week" => HistoryWhenFilter::ThisWeek,
                        "month" => HistoryWhenFilter::ThisMonth,
                        _ => HistoryWhenFilter::All,
                    };
                    let next = when_filter.peek().toggle(chosen);
                    when_filter.set(next);
                }
                "sort" => {
                    let chosen = match option_id.as_str() {
                        "site" => HistorySort::BySite,
                        "mostvisited" => HistorySort::MostVisited,
                        "lastvisited" => HistorySort::LastVisited,
                        _ => HistorySort::ByDate,
                    };
                    sort_mode.set_if_modified(chosen);
                }
                "source" => {
                    let chosen = match option_id.as_str() {
                        "closed" => HistorySource::ClosedTabs,
                        _ => HistorySource::History,
                    };
                    source_mode.set_if_modified(chosen);
                    if chosen == HistorySource::ClosedTabs && *sort_mode.peek() == HistorySort::MostVisited {
                        sort_mode.set(HistorySort::ByDate);
                    }
                }
                _ => {}
            });

        let has_active_search_or_filter = !current_query.trim().is_empty() || current_when != HistoryWhenFilter::All;
        let list_body: Element = if groups.is_empty() {
            let empty_text = if showing_closed {
                "No closed tabs"
            } else if has_active_search_or_filter && !entries.is_empty() {
                "No matching history found"
            } else {
                "No history found"
            };
            rect()
                .width(Size::fill())
                .padding(Gaps::new(28.0, 12.0, 28.0, 12.0))
                .cross_align(Alignment::center())
                .spacing(10.0)
                .child(
                    label()
                        .text(empty_text)
                        .font_size(13.5)
                        .color(theme.color(ThemeRole::Text))
                        .opacity(0.6),
                )
                .maybe_child((has_active_search_or_filter && !entries.is_empty()).then(|| {
                    let chip_bg = match theme.scheme {
                        ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.09).to_color(),
                        ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.07).to_color(),
                    };
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("Clear search and filters")
                        .padding(Gaps::new(6.0, 12.0, 6.0, 12.0))
                        .corner_radius(CornerRadius::new_all(10.0))
                        .background(chip_bg)
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |_| {
                            query.set(String::new());
                            when_filter.set(HistoryWhenFilter::All);
                        })
                        .child(
                            label()
                                .text("Clear filters")
                                .font_size(12.5)
                                .font_weight(FontWeight::MEDIUM)
                                .color(theme.color(ThemeRole::Text)),
                        )
                }))
                .into_element()
        } else {
            let mut list = rect()
                .width(Size::fill())
                .padding(Gaps::new(0.0, 10.0, 12.0, 10.0))
                .spacing(2.0);
            for group in groups {
                let mut group_box = rect().key(group.key.clone()).width(Size::fill()).spacing(2.0);
                if !group.title.is_empty() {
                    group_box = group_box.child(
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(10.0, 8.0, 5.0, 8.0))
                            .child(
                                label()
                                    .text(group.title)
                                    .font_size(12.0)
                                    .font_weight(FontWeight::BOLD)
                                    .color(theme.color(ThemeRole::Text))
                                    .opacity(0.6),
                            ),
                    );
                }
                for entry in group.entries {
                    let row_key = format!("history:{}", entry.id.0);
                    let focused = cursor_key.as_deref() == Some(row_key.as_str());
                    group_box = group_box.child(HistoryRow {
                        entry,
                        focused,
                        key: DiffKey::from(&row_key),
                    });
                }
                list = list.child(group_box);
            }
            list.into_element()
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .child(search_bar)
            .child(
                ScrollView::new()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .child(list_body),
            )
    }
}

#[derive(Clone, PartialEq)]
struct HistoryRow {
    entry: HistoryEntry,
    focused: bool,
    key: DiffKey,
}

impl Component for HistoryRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = try_use_menu_state();
        let library = try_use_library_state();
        let mut hovered = use_state(|| false);

        let is_hovered = *hovered.read();
        let hover_bg = if is_hovered && !self.focused {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.09).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.07).to_color(),
            }
        } else {
            Color::TRANSPARENT
        };

        let icon_name = favicon_for_url(&self.entry.url);
        let (badge_bg, badge_fg) = favicon_badge_colors(Some(icon_name), &theme);
        let favicon_svg: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(icon_name) {
            SvgViewer::new((("zendia-icon", canonical), bytes))
                .width(Size::px(11.5))
                .height(Size::px(11.5))
                .color(badge_fg)
                .show_loader(false)
                .into_element()
        } else {
            Icon::new("globe").size(11.5).role(ThemeRole::TextMuted).into_element()
        };
        let favicon_tile = rect()
            .width(Size::px(18.0))
            .height(Size::px(18.0))
            .corner_radius(CornerRadius::new_all(4.5))
            .background(badge_bg)
            .main_align(Alignment::center())
            .cross_align(Alignment::center())
            .child(favicon_svg);

        let title = if self.entry.title.trim().is_empty() {
            self.entry.url.clone()
        } else {
            self.entry.title.clone()
        };
        let subtitle = if is_hovered {
            format_visit_timestamp(self.entry.visited_at_unix)
        } else {
            format_display_url(&self.entry.url)
        };

        let entry_id = self.entry.id;
        let entry_for_menu = self.entry.clone();
        let row_key = format!("history:{}", entry_id.0);
        let click_core = core.clone();
        let mid_core = core.clone();
        let del_core = core.clone();
        let reopen_core = core.clone();

        let actions = is_hovered.then(|| {
            let btn_bg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.10).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.08).to_color(),
            };
            rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(4.0)
                .child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("Remove from history")
                        .width(Size::px(28.0))
                        .height(Size::px(28.0))
                        .corner_radius(CornerRadius::new_all(8.0))
                        .background(btn_bg)
                        .main_align(Alignment::center())
                        .cross_align(Alignment::center())
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            del_core.invoke_or_log(Command::HistoryDelete { entry_id });
                        })
                        .child(Icon::new("trash").size(14.0).role(ThemeRole::Text)),
                )
                .child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("Reopen page")
                        .width(Size::px(28.0))
                        .height(Size::px(28.0))
                        .corner_radius(CornerRadius::new_all(8.0))
                        .background(btn_bg)
                        .main_align(Alignment::center())
                        .cross_align(Alignment::center())
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            reopen_core.invoke_or_log(Command::HistoryOpen {
                                entry_id: Some(entry_id),
                                url: None,
                                new_tab: true,
                                in_background: false,
                            });
                        })
                        .child(Icon::new("u-turn-to-left").size(14.0).role(ThemeRole::Text)),
                )
        });

        let row = rect()
            .a11y_role(AccessibilityRole::ListItem)
            .a11y_alt(title.clone())
            .width(Size::fill())
            .padding(Gaps::new(9.0, 12.0, 9.0, 12.0))
            .corner_radius(CornerRadius::new_all(14.0))
            .background(hover_bg)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(12.0)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_mouse_down(move |e: Event<MouseEventData>| {
                if e.button == Some(MouseButton::Right) {
                    e.stop_propagation();
                    if let Some(lib) = library {
                        let mut cursor = lib.nav.cursor;
                        cursor.set_if_modified(Some(row_key.clone()));
                    }
                    if let Some(ms) = menu_state {
                        ms.open_context_menu(OpenContextMenu {
                            position: (e.global_location.x as f32, e.global_location.y as f32),
                            items: build_history_row_context_menu(&entry_for_menu),
                            tab_id: None,
                            open_submenu: None,
                        });
                    }
                } else if e.button == Some(MouseButton::Middle) {
                    e.stop_propagation();
                    mid_core.invoke_or_log(Command::HistoryOpen {
                        entry_id: Some(entry_id),
                        url: None,
                        new_tab: true,
                        in_background: true,
                    });
                }
            })
            .on_press(move |_| {
                click_core.invoke_or_log(Command::HistoryOpen {
                    entry_id: Some(entry_id),
                    url: None,
                    new_tab: true,
                    in_background: false,
                });
            })
            .child(favicon_tile)
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .overflow(Overflow::Clip)
                    .spacing(2.0)
                    .child(
                        label()
                            .text(title)
                            .font_size(13.5)
                            .font_weight(FontWeight::MEDIUM)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .child(
                        label()
                            .text(subtitle)
                            .font_size(11.5)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                            .color(theme.color(ThemeRole::Text))
                            .opacity(0.6),
                    ),
            )
            .maybe_child(actions);

        LibraryCursorRing::new(self.focused, row).corner_radius(14.0)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
