/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Browsing history filtering, grouping, formatting, clear-range calculation, and context menus for the Library's
//! History section.

use crate::contributions::MenuEntryKind;
use crate::menus::ResolvedMenuItem;
use crate::model::{HistoryEntry, HistoryEntryId};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds in one UTC day.
pub const SECS_PER_DAY: u64 = 86_400;
/// Seconds in one hour.
pub const SECS_PER_HOUR: u64 = 3_600;
/// Reference Unix timestamp used in deterministic fixtures (2026-11-26 14:30:00 UTC).
pub const SAMPLE_NOW_UNIX: u64 = 1_795_703_400;

/// Time-range filter matching Zen's "When was it visited?" exclusive filter group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HistoryWhenFilter {
    #[default]
    All,
    Today,
    ThisWeek,
    ThisMonth,
}

impl HistoryWhenFilter {
    pub const OPTIONS: [Self; 3] = [Self::Today, Self::ThisWeek, Self::ThisMonth];

    pub fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Today => "today",
            Self::ThisWeek => "week",
            Self::ThisMonth => "month",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "Any time",
            Self::Today => "Today",
            Self::ThisWeek => "This Week",
            Self::ThisMonth => "This Month",
        }
    }

    pub fn days(self) -> Option<u64> {
        match self {
            Self::All => None,
            Self::Today => Some(1),
            Self::ThisWeek => Some(7),
            Self::ThisMonth => Some(30),
        }
    }

    /// Toggles `option` in an exclusive radio group where clicking the active chip clears it back to `All`.
    pub fn toggle(self, option: Self) -> Self {
        if self == option { Self::All } else { option }
    }
}

/// Sort mode matching Zen's "Sort by" exclusive filter group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HistorySort {
    #[default]
    ByDate,
    BySite,
    MostVisited,
    LastVisited,
}

impl HistorySort {
    pub const ALL: [Self; 4] = [Self::ByDate, Self::BySite, Self::MostVisited, Self::LastVisited];

    pub fn id(self) -> &'static str {
        match self {
            Self::ByDate => "date",
            Self::BySite => "site",
            Self::MostVisited => "mostvisited",
            Self::LastVisited => "lastvisited",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ByDate => "By Date",
            Self::BySite => "By Site",
            Self::MostVisited => "By Most Visited",
            Self::LastVisited => "By Last Visited",
        }
    }
}

/// Source selector matching Zen's "What to look through" exclusive filter group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HistorySource {
    #[default]
    History,
    ClosedTabs,
}

impl HistorySource {
    pub const ALL: [Self; 2] = [Self::History, Self::ClosedTabs];

    pub fn id(self) -> &'static str {
        match self {
            Self::History => "history",
            Self::ClosedTabs => "closed",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::History => "History",
            Self::ClosedTabs => "Closed tabs",
        }
    }
}

/// Time range accepted by `history.clear`, matching Zen's History rail tab context menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HistoryClearRange {
    LastHour,
    Last12Hours,
    Today,
    LastWeek,
    LastMonth,
    #[default]
    All,
}

impl HistoryClearRange {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LastHour => "last_hour",
            Self::Last12Hours => "last_12_hours",
            Self::Today => "today",
            Self::LastWeek => "last_week",
            Self::LastMonth => "last_month",
            Self::All => "all",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "last_hour" | "hour" | "1h" => Some(Self::LastHour),
            "last_12_hours" | "12h" => Some(Self::Last12Hours),
            "today" => Some(Self::Today),
            "last_week" | "week" => Some(Self::LastWeek),
            "last_month" | "month" => Some(Self::LastMonth),
            "all" | "everything" => Some(Self::All),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::LastHour => "Last hour",
            Self::Last12Hours => "Last 12 hours",
            Self::Today => "Today",
            Self::LastWeek => "Last week",
            Self::LastMonth => "Last month",
            Self::All => "Everything",
        }
    }
}

/// Parameters controlling how browsing history is filtered, sorted, and grouped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryQuery {
    pub query: String,
    pub when: HistoryWhenFilter,
    pub sort: HistorySort,
    pub source: HistorySource,
    pub now_unix: u64,
}

impl HistoryQuery {
    pub fn new(now_unix: u64) -> Self {
        Self {
            query: String::new(),
            when: HistoryWhenFilter::All,
            sort: HistorySort::ByDate,
            source: HistorySource::History,
            now_unix,
        }
    }
}

/// A display group of history entries (a day, a site host, or an untitled flat list).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryGroup {
    pub key: String,
    /// Empty when the active sort displays a flat list without group headers.
    pub title: String,
    /// UTC day number when grouped by day, used by day-level clear actions.
    pub day_index: Option<i64>,
    pub entries: Vec<HistoryEntry>,
}

/// Current wall-clock timestamp in Unix seconds.
pub fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(SAMPLE_NOW_UNIX)
}

/// Whole UTC day index since the Unix epoch (`1970-01-01` = 0).
pub fn day_index(unix_secs: u64) -> i64 {
    (unix_secs / SECS_PER_DAY) as i64
}

/// Converts a Unix day index into `(year, month 1..=12, day 1..=31)` in the proleptic Gregorian calendar.
pub fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    // Howard Hinnant's civil-from-days algorithm.
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year, m as u32, d as u32)
}

fn weekday_name(days_since_epoch: i64) -> &'static str {
    // 1970-01-01 was a Thursday (index 4 when 0 = Sunday).
    match (days_since_epoch + 4).rem_euclid(7) {
        0 => "Sunday",
        1 => "Monday",
        2 => "Tuesday",
        3 => "Wednesday",
        4 => "Thursday",
        5 => "Friday",
        _ => "Saturday",
    }
}

fn month_short(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        _ => "Dec",
    }
}

/// Formats a day header relative to `today_day`: `"Today"`, `"Yesterday"`, weekday name within the last 6 days,
/// or a medium date (`"Nov 12, 2026"`).
pub fn format_day_label(entry_day: i64, today_day: i64) -> String {
    let days_ago = today_day - entry_day;
    match days_ago {
        ..=0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        2..=6 => weekday_name(entry_day).to_string(),
        _ => {
            let (year, month, day) = civil_from_days(entry_day);
            format!("{} {}, {}", month_short(month), day, year)
        }
    }
}

/// Formats a visit timestamp shown on row hover (e.g. `"Nov 26, 2026, 2:30 PM"`).
pub fn format_visit_timestamp(unix_secs: u64) -> String {
    let day = day_index(unix_secs);
    let (year, month, dom) = civil_from_days(day);
    let secs_of_day = unix_secs % SECS_PER_DAY;
    let hour24 = (secs_of_day / 3600) as u32;
    let minute = ((secs_of_day % 3600) / 60) as u32;
    let (hour12, meridiem) = match hour24 {
        0 => (12, "AM"),
        1..=11 => (hour24, "AM"),
        12 => (12, "PM"),
        _ => (hour24 - 12, "PM"),
    };
    format!(
        "{} {}, {}, {}:{:02} {}",
        month_short(month),
        dom,
        year,
        hour12,
        minute,
        meridiem
    )
}

/// Strips the scheme, leading `www.`, and trailing slash from a URL, matching Zen's history row subtitle.
pub fn format_display_url(url: &str) -> String {
    let trimmed = url.trim();
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let without_www = without_scheme.strip_prefix("www.").unwrap_or(without_scheme);
    without_www.strip_suffix('/').unwrap_or(without_www).to_string()
}

/// Extracts the host (without port) from an `http://` or `https://` URL.
pub fn extract_host(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let rest = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    let host = if let Some(bracketed) = host_port.strip_prefix('[') {
        bracketed.split(']').next()?
    } else {
        host_port.split(':').next()?
    };
    let host = host.trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

/// Chooses an icon name for a history entry's URL so rows render recognizable favicon tiles.
pub fn favicon_for_url(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if lower.contains("/architecture") || lower.contains("/code") || lower.contains("rust-lang.org") {
        "code"
    } else if lower.contains("/design") {
        "palette"
    } else if lower.contains("/docs") || lower.contains("book.freyaui.dev") || lower.contains("wikipedia") {
        "book"
    } else if lower.contains("/releases") || lower.contains("changelog") {
        "flame"
    } else if lower.contains("ladybird.org") {
        "compass"
    } else {
        "globe"
    }
}

/// Computes the earliest Unix timestamp removed by `range`, or `None` when `range` clears all history.
pub fn cutoff_for_clear_range(range: HistoryClearRange, now_unix: u64) -> Option<u64> {
    match range {
        HistoryClearRange::LastHour => Some(now_unix.saturating_sub(SECS_PER_HOUR)),
        HistoryClearRange::Last12Hours => Some(now_unix.saturating_sub(12 * SECS_PER_HOUR)),
        HistoryClearRange::Today => Some((now_unix / SECS_PER_DAY) * SECS_PER_DAY),
        HistoryClearRange::LastWeek => Some(now_unix.saturating_sub(7 * SECS_PER_DAY)),
        HistoryClearRange::LastMonth => Some(now_unix.saturating_sub(30 * SECS_PER_DAY)),
        HistoryClearRange::All => None,
    }
}

/// Filters, sorts, and groups history entries according to `query`.
///
/// Matches Zen's `ZenLibraryHistorySection`:
/// - `when` filters entries within the selected day window relative to `query.now_unix`.
/// - `query` matches case-insensitively against both title and URL.
/// - `ByDate` groups newest-first by UTC day ("Today", "Yesterday", weekday, date).
/// - `BySite` groups alphabetically by host with non-HTTP entries placed under `"Other"` at the end.
/// - `MostVisited` and `LastVisited` return a single flat group (with empty title) sorted accordingly.
pub fn filter_and_group_history(entries: &[HistoryEntry], query: &HistoryQuery) -> Vec<HistoryGroup> {
    let needle = query.query.trim().to_ascii_lowercase();
    let cutoff = query
        .when
        .days()
        .map(|days| query.now_unix.saturating_sub(days * SECS_PER_DAY));

    let mut filtered: Vec<HistoryEntry> = entries
        .iter()
        .filter(|entry| {
            if let Some(min_ts) = cutoff
                && entry.visited_at_unix < min_ts
            {
                return false;
            }
            if needle.is_empty() {
                return true;
            }
            entry.title.to_ascii_lowercase().contains(&needle) || entry.url.to_ascii_lowercase().contains(&needle)
        })
        .cloned()
        .collect();

    if filtered.is_empty() {
        return Vec::new();
    }

    match query.sort {
        HistorySort::ByDate => {
            filtered.sort_by(|a, b| {
                b.visited_at_unix
                    .cmp(&a.visited_at_unix)
                    .then_with(|| b.id.0.cmp(&a.id.0))
            });
            let today = day_index(query.now_unix);
            let mut groups: Vec<HistoryGroup> = Vec::new();
            for entry in filtered {
                let d = day_index(entry.visited_at_unix);
                if let Some(last) = groups.last_mut()
                    && last.day_index == Some(d)
                {
                    last.entries.push(entry);
                } else {
                    groups.push(HistoryGroup {
                        key: format!("day:{d}"),
                        title: format_day_label(d, today),
                        day_index: Some(d),
                        entries: vec![entry],
                    });
                }
            }
            groups
        }
        HistorySort::BySite => {
            filtered.sort_by(|a, b| {
                b.visited_at_unix
                    .cmp(&a.visited_at_unix)
                    .then_with(|| b.id.0.cmp(&a.id.0))
            });
            let mut by_site: BTreeMap<String, Vec<HistoryEntry>> = BTreeMap::new();
            let mut other: Vec<HistoryEntry> = Vec::new();
            for entry in filtered {
                if let Some(host) = extract_host(&entry.url) {
                    let display_host = host.strip_prefix("www.").unwrap_or(&host).to_string();
                    by_site.entry(display_host).or_default().push(entry);
                } else {
                    other.push(entry);
                }
            }
            let mut groups: Vec<HistoryGroup> = by_site
                .into_iter()
                .map(|(host, entries)| HistoryGroup {
                    key: format!("site:{host}"),
                    title: host,
                    day_index: None,
                    entries,
                })
                .collect();
            if !other.is_empty() {
                groups.push(HistoryGroup {
                    key: "site:other".to_string(),
                    title: "Other".to_string(),
                    day_index: None,
                    entries: other,
                });
            }
            groups
        }
        HistorySort::MostVisited => {
            filtered.sort_by(|a, b| {
                b.visit_count
                    .cmp(&a.visit_count)
                    .then_with(|| b.visited_at_unix.cmp(&a.visited_at_unix))
                    .then_with(|| b.id.0.cmp(&a.id.0))
            });
            vec![HistoryGroup {
                key: "flat:mostvisited".to_string(),
                title: String::new(),
                day_index: None,
                entries: filtered,
            }]
        }
        HistorySort::LastVisited => {
            filtered.sort_by(|a, b| {
                b.visited_at_unix
                    .cmp(&a.visited_at_unix)
                    .then_with(|| b.id.0.cmp(&a.id.0))
            });
            vec![HistoryGroup {
                key: "flat:lastvisited".to_string(),
                title: String::new(),
                day_index: None,
                entries: filtered,
            }]
        }
    }
}

/// Sample browsing history matching the five Today entries in the Phase 9 reference screenshots.
pub fn sample_library_history(now_unix: u64) -> Vec<HistoryEntry> {
    vec![
        HistoryEntry {
            id: HistoryEntryId(101),
            url: "http://127.0.0.1:44889/architecture".to_string(),
            title: "Architecture — Zendia".to_string(),
            visited_at_unix: now_unix.saturating_sub(120),
            visit_count: 14,
        },
        HistoryEntry {
            id: HistoryEntryId(102),
            url: "http://127.0.0.1:44889/design".to_string(),
            title: "Design Tokens — Zendia".to_string(),
            visited_at_unix: now_unix.saturating_sub(420),
            visit_count: 8,
        },
        HistoryEntry {
            id: HistoryEntryId(103),
            url: "http://127.0.0.1:44889/docs".to_string(),
            title: "Zendia Documentation".to_string(),
            visited_at_unix: now_unix.saturating_sub(900),
            visit_count: 21,
        },
        HistoryEntry {
            id: HistoryEntryId(104),
            url: "http://127.0.0.1:44889/releases".to_string(),
            title: "Releases — Zendia".to_string(),
            visited_at_unix: now_unix.saturating_sub(1_600),
            visit_count: 6,
        },
        HistoryEntry {
            id: HistoryEntryId(105),
            url: "http://127.0.0.1:44889/code".to_string(),
            title: "Repository — Zendia".to_string(),
            visited_at_unix: now_unix.saturating_sub(2_400),
            visit_count: 11,
        },
    ]
}

/// Sample recently closed tabs for the Library History "Closed tabs" source.
pub fn sample_closed_tabs(now_unix: u64) -> Vec<HistoryEntry> {
    vec![
        HistoryEntry {
            id: HistoryEntryId(100_001),
            url: "https://book.freyaui.dev/guides/animations".to_string(),
            title: "Animations — Freya Guide".to_string(),
            visited_at_unix: now_unix.saturating_sub(180),
            visit_count: 1,
        },
        HistoryEntry {
            id: HistoryEntryId(100_002),
            url: "https://doc.rust-lang.org/std/time/struct.Duration.html".to_string(),
            title: "Duration in std::time — Rust".to_string(),
            visited_at_unix: now_unix.saturating_sub(1_200),
            visit_count: 1,
        },
        HistoryEntry {
            id: HistoryEntryId(100_003),
            url: "https://ladybird.org/posts/progress-2026".to_string(),
            title: "Ladybird Progress Report".to_string(),
            visited_at_unix: now_unix.saturating_sub(SECS_PER_DAY + 600),
            visit_count: 1,
        },
    ]
}

/// Builds the context menu for a single history row, matching Zen's history menu plus open-in-tab and copy actions.
pub fn build_history_row_context_menu(entry: &HistoryEntry) -> Vec<ResolvedMenuItem> {
    let id_str = entry.id.0.to_string();
    let host = extract_host(&entry.url);

    let mut open_current = ResolvedMenuItem::action("library.history.open_current", "Reopen Page", "history.open")
        .with_arg("entry_id", id_str.clone())
        .with_arg("new_tab", "false");
    open_current.icon = Some("u-turn-to-left".to_string());

    let mut open_new_tab = ResolvedMenuItem::action("library.history.open_new_tab", "Open in New Tab", "history.open")
        .with_arg("entry_id", id_str.clone())
        .with_arg("new_tab", "true")
        .with_arg("in_background", "false");
    open_new_tab.icon = Some("new-tab".to_string());

    let mut open_background = ResolvedMenuItem::action(
        "library.history.open_background",
        "Open in Background Tab",
        "history.open",
    )
    .with_arg("entry_id", id_str.clone())
    .with_arg("new_tab", "true")
    .with_arg("in_background", "true");
    open_background.icon = Some("open".to_string());

    let mut copy_url = ResolvedMenuItem::action("library.history.copy_url", "Copy URL", "history.copy_url")
        .with_arg("entry_id", id_str.clone());
    copy_url.icon = Some("copy".to_string());

    let mut forget_page = ResolvedMenuItem::action("library.history.forget", "Forget This Page", "history.delete")
        .with_arg("entry_id", id_str);
    forget_page.icon = Some("trash".to_string());

    let mut forget_site = ResolvedMenuItem::action(
        "library.history.forget_site",
        "Forget About This Site",
        "history.forget_site",
    )
    .with_enabled(host.is_some());
    forget_site.icon = Some("trash".to_string());
    if let Some(h) = host {
        forget_site = forget_site.with_arg("host", h);
    }

    vec![
        open_current,
        open_new_tab,
        open_background,
        ResolvedMenuItem::separator("library.history.sep.1"),
        copy_url,
        ResolvedMenuItem::separator("library.history.sep.2"),
        forget_page,
        forget_site,
    ]
}

/// Builds the context menu opened by right-clicking the History section tab in the rail (Zen's `tabMenu`).
pub fn build_history_tab_context_menu() -> Vec<ResolvedMenuItem> {
    let mut clear_children = Vec::new();
    for range in [
        HistoryClearRange::LastHour,
        HistoryClearRange::Last12Hours,
        HistoryClearRange::Today,
        HistoryClearRange::LastWeek,
        HistoryClearRange::LastMonth,
    ] {
        clear_children.push(
            ResolvedMenuItem::action(
                format!("library.history.clear.{}", range.as_str()),
                range.label(),
                "history.clear",
            )
            .with_arg("range", range.as_str()),
        );
    }
    clear_children.push(ResolvedMenuItem::separator("library.history.clear.sep"));
    clear_children.push(
        ResolvedMenuItem::action(
            "library.history.clear.all",
            HistoryClearRange::All.label(),
            "history.clear",
        )
        .with_arg("range", "all"),
    );

    let mut clear_menu =
        ResolvedMenuItem::action("library.history.clear_menu", "Clear history", "").with_kind(MenuEntryKind::Submenu);
    clear_menu.icon = Some("trash".to_string());
    clear_menu.children = clear_children;
    vec![clear_menu]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, title: &str, url: &str, visited_at_unix: u64, visit_count: u32) -> HistoryEntry {
        HistoryEntry {
            id: HistoryEntryId(id),
            url: url.to_string(),
            title: title.to_string(),
            visited_at_unix,
            visit_count,
        }
    }

    #[test]
    fn day_labels_cover_today_yesterday_weekdays_and_older_dates() {
        // 2026-11-26 is a Thursday.
        let today = day_index(SAMPLE_NOW_UNIX);
        assert_eq!(civil_from_days(today), (2026, 11, 26));
        assert_eq!(format_day_label(today, today), "Today");
        assert_eq!(format_day_label(today - 1, today), "Yesterday");
        assert_eq!(format_day_label(today - 2, today), "Tuesday");
        assert_eq!(format_day_label(today - 6, today), "Friday");
        assert_eq!(format_day_label(today - 7, today), "Nov 19, 2026");
        assert_eq!(format_day_label(today - 14, today), "Nov 12, 2026");
    }

    #[test]
    fn groups_by_day_newest_first_and_filters_by_when_and_query() {
        let now = SAMPLE_NOW_UNIX;
        let items = vec![
            entry(1, "Older Docs", "https://example.org/old", now - 10 * SECS_PER_DAY, 2),
            entry(2, "Yesterday Guide", "https://example.org/y", now - SECS_PER_DAY, 4),
            entry(3, "Today Architecture", "https://zendia.local/arch", now - 60, 10),
            entry(4, "Today Tokens", "https://zendia.local/design", now - 300, 3),
        ];

        let all = filter_and_group_history(&items, &HistoryQuery::new(now));
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].title, "Today");
        assert_eq!(all[0].entries.len(), 2);
        assert_eq!(all[0].entries[0].id, HistoryEntryId(3));
        assert_eq!(all[0].entries[1].id, HistoryEntryId(4));
        assert_eq!(all[1].title, "Yesterday");
        assert_eq!(all[2].title, "Nov 16, 2026");

        let mut week = HistoryQuery::new(now);
        week.when = HistoryWhenFilter::ThisWeek;
        let week_groups = filter_and_group_history(&items, &week);
        assert_eq!(week_groups.len(), 2);

        let mut search = HistoryQuery::new(now);
        search.query = "DESIGN".to_string();
        let search_groups = filter_and_group_history(&items, &search);
        assert_eq!(search_groups.len(), 1);
        assert_eq!(search_groups[0].entries.len(), 1);
        assert_eq!(search_groups[0].entries[0].id, HistoryEntryId(4));
    }

    #[test]
    fn sorts_by_site_most_visited_and_last_visited() {
        let now = SAMPLE_NOW_UNIX;
        let items = vec![
            entry(1, "Rust", "https://www.rust-lang.org/", now - 100, 3),
            entry(2, "Internal", "about:settings", now - 50, 1),
            entry(3, "Docs", "https://book.freyaui.dev/guide", now - 200, 15),
        ];

        let mut by_site = HistoryQuery::new(now);
        by_site.sort = HistorySort::BySite;
        let site_groups = filter_and_group_history(&items, &by_site);
        let titles: Vec<&str> = site_groups.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, vec!["book.freyaui.dev", "rust-lang.org", "Other"]);

        let mut most = HistoryQuery::new(now);
        most.sort = HistorySort::MostVisited;
        let most_groups = filter_and_group_history(&items, &most);
        assert_eq!(most_groups.len(), 1);
        assert!(most_groups[0].title.is_empty());
        assert_eq!(most_groups[0].entries[0].id, HistoryEntryId(3));

        let mut last = HistoryQuery::new(now);
        last.sort = HistorySort::LastVisited;
        let last_groups = filter_and_group_history(&items, &last);
        assert_eq!(last_groups[0].entries[0].id, HistoryEntryId(2));
    }

    #[test]
    fn formats_display_urls_hosts_and_context_menus() {
        assert_eq!(format_display_url("https://www.ladybird.org/"), "ladybird.org");
        assert_eq!(
            format_display_url("http://127.0.0.1:44889/architecture"),
            "127.0.0.1:44889/architecture"
        );
        assert_eq!(
            extract_host("https://www.example.com:8080/a"),
            Some("www.example.com".to_string())
        );
        assert_eq!(extract_host("about:bookmarks"), None);

        let sample = sample_library_history(SAMPLE_NOW_UNIX);
        assert_eq!(sample.len(), 5);
        let row_menu = build_history_row_context_menu(&sample[0]);
        assert!(row_menu.iter().any(|item| item.command == "history.open"));
        assert!(row_menu.iter().any(|item| item.command == "history.delete"));
        assert!(
            row_menu
                .iter()
                .any(|item| item.command == "history.forget_site" && item.enabled)
        );

        let internal = entry(9, "Bookmarks", "about:bookmarks", SAMPLE_NOW_UNIX, 1);
        let internal_menu = build_history_row_context_menu(&internal);
        assert!(
            internal_menu
                .iter()
                .any(|item| item.command == "history.forget_site" && !item.enabled)
        );

        let tab_menu = build_history_tab_context_menu();
        assert_eq!(tab_menu.len(), 1);
        assert!(tab_menu[0].children.len() >= 6);
    }

    #[test]
    fn clear_range_cutoffs_are_monotonic() {
        let now = SAMPLE_NOW_UNIX;
        assert!(cutoff_for_clear_range(HistoryClearRange::All, now).is_none());
        let hour = cutoff_for_clear_range(HistoryClearRange::LastHour, now).unwrap();
        let twelve = cutoff_for_clear_range(HistoryClearRange::Last12Hours, now).unwrap();
        let week = cutoff_for_clear_range(HistoryClearRange::LastWeek, now).unwrap();
        let month = cutoff_for_clear_range(HistoryClearRange::LastMonth, now).unwrap();
        assert!(hour > twelve);
        assert!(twelve > week);
        assert!(week > month);
    }

    #[test]
    fn stub_handles_history_commands_bookmarks_and_library_focus() {
        use crate::BrowserCore;
        use crate::commands::Command;
        use crate::events::Event;
        use crate::model::InputMode;
        use crate::stub::StubBackend;

        let stub = StubBackend::new();
        let sub = stub.subscribe();

        // library.open section=bookmarks opens about:bookmarks in a new tab.
        let parsed =
            Command::from_name_and_args("library.open", &[("section".to_string(), "bookmarks".to_string())]).unwrap();
        assert_eq!(
            parsed,
            Command::TabNew {
                url: Some("about:bookmarks".to_string()),
                after: None,
                activate: true,
            }
        );
        stub.invoke(parsed).unwrap();
        assert_eq!(stub.model().active_tab().unwrap().url, "about:bookmarks");

        // history.open in new tab and current tab.
        let first = stub.model().history[0].clone();
        stub.invoke(Command::HistoryOpen {
            entry_id: Some(first.id),
            url: None,
            new_tab: true,
            in_background: false,
        })
        .unwrap();
        assert_eq!(stub.model().active_tab().unwrap().url, first.url);

        // Closing a tab records it in closed_tabs, and history.open reopens it.
        let active_id = stub.model().active_tab.unwrap();
        let closed_url = stub.model().active_tab().unwrap().url.clone();
        stub.invoke(Command::TabClose {
            tab_id: Some(active_id),
            force: true,
        })
        .unwrap();
        let closed_entry = stub.model().closed_tabs[0].clone();
        assert_eq!(closed_entry.url, closed_url);
        stub.invoke(Command::HistoryOpen {
            entry_id: Some(closed_entry.id),
            url: None,
            new_tab: true,
            in_background: false,
        })
        .unwrap();
        assert_eq!(stub.model().active_tab().unwrap().url, closed_url);
        assert!(!stub.model().closed_tabs.iter().any(|e| e.id == closed_entry.id));

        let second = stub.model().history[1].clone();
        stub.invoke(Command::HistoryOpen {
            entry_id: Some(second.id),
            url: None,
            new_tab: false,
            in_background: false,
        })
        .unwrap();
        assert_eq!(stub.model().active_tab().unwrap().url, second.url);

        // history.copy_url publishes ClipboardCopyRequested.
        stub.invoke(Command::HistoryCopyUrl { entry_id: first.id }).unwrap();
        let mut saw_copy = false;
        while let Ok(ev) = sub.try_recv() {
            if matches!(ev, Event::ClipboardCopyRequested { text } if text == first.url) {
                saw_copy = true;
            }
        }
        assert!(saw_copy);

        // history.delete removes entry and apply_event mirrors it.
        let mut mirror = stub.model();
        let count_before = mirror.history.len();
        stub.invoke(Command::HistoryDelete { entry_id: first.id }).unwrap();
        while let Ok(ev) = sub.try_recv() {
            mirror.apply_event(&ev);
        }
        assert_eq!(mirror.history.len(), count_before - 1);
        assert!(!mirror.history.iter().any(|h| h.id == first.id));

        // history.clear_day removes entries on that UTC day.
        let target_day = day_index(second.visited_at_unix);
        stub.invoke(Command::HistoryClearDay { day_index: target_day }).unwrap();
        assert!(
            !stub
                .model()
                .history
                .iter()
                .any(|h| day_index(h.visited_at_unix) == target_day)
        );

        // history.clear all empties history.
        stub.invoke(Command::HistoryClear {
            range: HistoryClearRange::All,
        })
        .unwrap();
        assert!(stub.model().history.is_empty());

        // library.focus enters and leaves LIBRARY mode when vim mode is enabled.
        stub.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        stub.invoke(Command::LibraryFocusSet { focused: true }).unwrap();
        assert_eq!(stub.model().input_mode, InputMode::Library);
        stub.invoke(Command::LibraryFocusSet { focused: false }).unwrap();
        assert_eq!(stub.model().input_mode, InputMode::Normal);
    }
}
