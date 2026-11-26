/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Address bar URL-or-search classification, result providers and ranking, and command palette fuzzy matching.
//!
//! All result computation lives in `BrowserCore` so both the Freya UI and `UI/Headless` share identical behavior.

use crate::commands::CommandDescriptor;
use crate::contributions::{ContributionId, ContributionSource, PaletteEntry};
use crate::model::{BookmarkId, BrowserModel, FolderId, HistoryEntryId, SpaceId, TabId, TabKind, ThemeScheme};
use std::collections::HashSet;
use std::net::Ipv4Addr;
use zendia_keymap::KeyTable;

/// Default search URL template when no engine is configured.
pub const DEFAULT_SEARCH_ENGINE_NAME: &str = "Google";
pub const DEFAULT_SEARCH_ENGINE_URL: &str = "https://www.google.com/search?q=%s";

/// Known built-in search engines matching Ladybird's settings.
const BUILTIN_SEARCH_ENGINES: &[(&str, &str)] = &[
    ("Google", "https://www.google.com/search?q=%s"),
    ("DuckDuckGo", "https://duckduckgo.com/?q=%s"),
    ("Kagi", "https://kagi.com/search?q=%s"),
    ("Bing", "https://www.bing.com/search?q=%s"),
    ("Brave", "https://search.brave.com/search?q=%s"),
    ("Ecosia", "https://ecosia.org/search?q=%s"),
    ("Mojeek", "https://www.mojeek.com/search?q=%s"),
    ("Startpage", "https://startpage.com/search?q=%s"),
    ("Yahoo", "https://search.yahoo.com/search?p=%s"),
    ("Yandex", "https://yandex.com/search/?text=%s"),
];

/// Schemes accepted without an authority/host component.
const SCHEMES_WITHOUT_HOST: &[&str] = &["about", "data", "file", "javascript", "blob", "view-source", "mailto"];

/// Known explicit URL schemes with `scheme://` authority.
const SCHEMES_WITH_AUTHORITY: &[&str] = &[
    "http",
    "https",
    "ftp",
    "ftps",
    "file",
    "ws",
    "wss",
    "ladybird",
    "chrome-extension",
    "moz-extension",
    "plugin",
];

// Fuzzy scoring constants matching Zen's action provider ranking.
const EXACT_MATCH_SCORE: i32 = 1_000_000;
const PREFIX_MATCH_SCORE: i32 = 100_000;
const INLINE_ACTION_MIN_SCORE: i32 = 92;
const PALETTE_MIN_SCORE: i32 = 30;

/// Profile setting describing the active default search engine owned by `BrowserCore`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchEngine {
    pub name: String,
    /// Search URL template containing `%s` where the encoded query is substituted.
    pub search_url: String,
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self {
            name: DEFAULT_SEARCH_ENGINE_NAME.to_string(),
            search_url: DEFAULT_SEARCH_ENGINE_URL.to_string(),
        }
    }
}

impl SearchEngine {
    pub fn new(name: impl Into<String>, search_url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            search_url: search_url.into(),
        }
    }

    /// Infers a [`SearchEngine`] name from a configured search URL template (such as Ladybird's settings).
    pub fn from_url(search_url: &str) -> Self {
        let trimmed = search_url.trim();
        if trimmed.is_empty() {
            return Self::default();
        }
        let lower = trimmed.to_ascii_lowercase();
        for &(name, template) in BUILTIN_SEARCH_ENGINES {
            if lower == template.to_ascii_lowercase() || lower.contains(&format!("{}.", name.to_ascii_lowercase())) {
                return Self::new(name, trimmed);
            }
        }
        Self::new("Web", trimmed)
    }

    /// Lists Zendia's built-in search engine presets.
    pub fn builtin_engines() -> Vec<Self> {
        BUILTIN_SEARCH_ENGINES
            .iter()
            .map(|&(name, url)| Self::new(name, url))
            .collect()
    }

    /// Builds a navigable search URL by percent-encoding `query` into `search_url`.
    pub fn search_url_for(&self, query: &str) -> String {
        let encoded = form_url_encode(query.trim());
        if self.search_url.contains("%s") {
            self.search_url.replace("%s", &encoded)
        } else {
            format!("{}{encoded}", self.search_url)
        }
    }
}

/// Classification of raw text entered into the address bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocationInputKind {
    Url {
        normalized_url: String,
        display_url: String,
    },
    Search {
        query: String,
        search_url: String,
    },
}

/// Percent-encodes a query string in `application/x-www-form-urlencoded` format (` ` -> `+`).
fn form_url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    for &byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b' ' => out.push('+'),
            other => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                out.push('%');
                out.push(HEX[(other >> 4) as usize] as char);
                out.push(HEX[(other & 0x0F) as usize] as char);
            }
        }
    }
    out
}

fn host_part(location: &str) -> &str {
    let end = location.find(['/', '?', '#']).unwrap_or(location.len());
    let authority = &location[..end];
    if authority.starts_with('[') {
        return authority.find(']').map(|idx| &authority[..=idx]).unwrap_or(authority);
    }
    if let Some((host, port)) = authority.rsplit_once(':')
        && !port.is_empty()
        && port.bytes().all(|b| b.is_ascii_digit())
    {
        return host;
    }
    authority
}

fn is_local_host(location: &str) -> bool {
    let host = host_part(location);
    let lower = host.to_ascii_lowercase();
    lower == "localhost"
        || lower.ends_with(".localhost")
        || host.parse::<Ipv4Addr>().is_ok()
        || (host.starts_with('[') && host.ends_with(']') && host.contains(':'))
}

fn looks_like_address(location: &str) -> bool {
    if location.contains(char::is_whitespace) {
        return false;
    }
    let host = host_part(location);
    if host.is_empty() {
        return false;
    }
    if is_local_host(location) {
        return true;
    }
    if !host.contains('.') || host.starts_with('.') || host.ends_with('.') || host.contains("..") {
        return false;
    }
    // Reject invalid host characters while permitting standard DNS labels and punycode/IDN non-ASCII chars.
    host.chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '.' || !c.is_ascii())
}

fn parse_explicit_scheme(location: &str) -> Option<&str> {
    let (scheme, rest) = location.split_once(':')?;
    if scheme.is_empty() {
        return None;
    }
    let mut chars = scheme.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        return None;
    }
    let lower = scheme.to_ascii_lowercase();
    if rest.starts_with("//") && SCHEMES_WITH_AUTHORITY.contains(&lower.as_str()) {
        return Some(scheme);
    }
    if SCHEMES_WITHOUT_HOST.contains(&lower.as_str()) || lower == "ladybird" {
        return Some(scheme);
    }
    None
}

/// Decides whether `input` should navigate directly to a URL or search using `engine`.
pub fn classify_location_input(input: &str, engine: &SearchEngine) -> Option<LocationInputKind> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(scheme) = parse_explicit_scheme(trimmed) {
        let lower_scheme = scheme.to_ascii_lowercase();
        let rest = &trimmed[scheme.len()..];
        let normalized_url = format!("{lower_scheme}{rest}");
        let display_url = trimmed.to_string();
        return Some(LocationInputKind::Url {
            normalized_url,
            display_url,
        });
    }

    if trimmed.starts_with('/') && !trimmed.contains(char::is_whitespace) {
        let normalized_url = format!("file://{trimmed}");
        return Some(LocationInputKind::Url {
            normalized_url: normalized_url.clone(),
            display_url: normalized_url,
        });
    }

    if looks_like_address(trimmed) {
        let scheme = if is_local_host(trimmed) { "http" } else { "https" };
        let normalized_url = format!("{scheme}://{trimmed}");
        return Some(LocationInputKind::Url {
            normalized_url,
            display_url: trimmed.to_string(),
        });
    }

    Some(LocationInputKind::Search {
        query: trimmed.to_string(),
        search_url: engine.search_url_for(trimmed),
    })
}

/// Resolves typed address bar text into the final URL to load using `engine`.
pub fn resolve_navigation_url(input: &str, engine: &SearchEngine) -> Option<String> {
    match classify_location_input(input, engine)? {
        LocationInputKind::Url { normalized_url, .. } => Some(normalized_url),
        LocationInputKind::Search { search_url, .. } => Some(search_url),
    }
}

/// Formats a URL for concise display inside the in-sidebar address bar (Zen's domain-only single-toolbar style).
pub fn display_url_for_sidebar(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed == "about:blank" || trimmed == "about:newtab" {
        return String::new();
    }
    let without_slash = trimmed.strip_suffix('/').unwrap_or(trimmed);
    if let Some(rest) = without_slash.strip_prefix("https://") {
        let domain = rest.split('/').next().unwrap_or(rest);
        return domain.strip_prefix("www.").unwrap_or(domain).to_string();
    }
    without_slash.to_string()
}

/// Formats a URL for secondary result row display (e.g. `https://www.wikipedia.org/` -> `wikipedia.org`).
pub fn display_url_for_result(url: &str) -> String {
    let trimmed = url.trim();
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let without_www = without_scheme.strip_prefix("www.").unwrap_or(without_scheme);
    without_www.strip_suffix('/').unwrap_or(without_www).to_string()
}

/// Semantic kind of one row returned by the address bar result providers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddressBarResultKind {
    Search {
        query: String,
        engine_name: String,
        search_url: String,
    },
    NavigateUrl {
        url: String,
        display_url: String,
    },
    SwitchToTab {
        tab_id: TabId,
        space_id: Option<SpaceId>,
        space_name: Option<String>,
        url: String,
    },
    History {
        entry_id: HistoryEntryId,
        url: String,
    },
    Bookmark {
        bookmark_id: BookmarkId,
        url: String,
    },
    Folder {
        folder_id: FolderId,
        space_id: SpaceId,
        space_name: String,
    },
    Action {
        command: String,
        command_args: Vec<(String, String)>,
        shortcut_hint: Option<String>,
    },
}

/// One ranked result row computed by `BrowserCore` for the address bar dropdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddressBarResult {
    pub id: String,
    pub kind: AddressBarResultKind,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub score: i32,
    /// True for the primary heuristic top row (search or direct URL) that receives Zen's accent selection highlight.
    pub heuristic: bool,
    pub shortcut_hint: Option<String>,
    /// Trailing action chip text (such as `"Switch to Tab"` or `"Open"`).
    pub badge_text: Option<String>,
    /// Optional accent RGB tint for the trailing action pill (used for workspace switch pills).
    pub badge_accent: Option<(u8, u8, u8)>,
}

/// One filtered and ranked item computed by `BrowserCore` for the command palette / Actions mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedPaletteItem {
    pub id: String,
    pub title: String,
    pub detail: Option<String>,
    pub category: String,
    pub icon: Option<String>,
    pub command: String,
    pub command_args: Vec<(String, String)>,
    pub shortcut_hint: Option<String>,
    pub badge_text: Option<String>,
    pub badge_accent: Option<(u8, u8, u8)>,
    pub score: i32,
}

/// VS Code / Zen-style fuzzy match scorer.
///
/// Exact case-insensitive matches score `1_000_000`, prefix matches score `100_000 + query.len()`, and ordered
/// subsequence matches earn bonuses for word boundaries and consecutive runs. Returns `0` when `query` is not a
/// subsequence of `target`.
pub fn fuzzy_score(target: &str, query: &str) -> i32 {
    let target_trim = target.trim();
    let query_trim = query.trim();
    if target_trim.is_empty() || query_trim.is_empty() {
        return 0;
    }
    let target_chars: Vec<char> = target_trim.to_lowercase().chars().collect();
    let query_chars: Vec<char> = query_trim.to_lowercase().chars().collect();
    if query_chars.len() > target_chars.len() {
        return 0;
    }
    if target_chars == query_chars {
        return EXACT_MATCH_SCORE;
    }
    if target_chars.starts_with(&query_chars) {
        return PREFIX_MATCH_SCORE + query_chars.len() as i32;
    }

    let mut score: i32 = 0;
    let mut query_idx = 0usize;
    let mut last_match: Option<usize> = None;
    let mut consecutive = 0i32;

    for (target_idx, &ch) in target_chars.iter().enumerate() {
        if query_idx < query_chars.len() && ch == query_chars[query_idx] {
            let mut bonus = 10;
            if target_idx == 0 || matches!(target_chars[target_idx - 1], ' ' | '-' | '_' | '.' | '/' | ':') {
                bonus += 15;
            }
            if last_match.is_some_and(|prev| prev + 1 == target_idx) {
                consecutive += 1;
                bonus += 20 * consecutive;
            } else {
                consecutive = 0;
            }
            if let Some(prev) = last_match {
                let distance = (target_idx - prev) as i32;
                bonus -= (distance - 1).min(10);
            }
            score += bonus;
            last_match = Some(target_idx);
            query_idx += 1;
        }
    }

    if query_idx == query_chars.len() {
        score.max(1)
    } else {
        0
    }
}

fn token_match_score(tokens: &[String], title: &str, url: &str) -> i32 {
    if tokens.is_empty() {
        return 0;
    }
    let title_lower = title.to_lowercase();
    let url_lower = url.to_lowercase();
    let combined = format!("{title_lower} {url_lower}");
    if !tokens.iter().all(|tok| combined.contains(tok.as_str())) {
        return 0;
    }
    let mut score = 200;
    let joined = tokens.join(" ");
    if title_lower == joined {
        score += 400;
    } else if title_lower.starts_with(&joined) {
        score += 250;
    } else if title_lower.contains(&joined) {
        score += 140;
    }
    let display = display_url_for_result(url).to_lowercase();
    if display.starts_with(&joined) {
        score += 220;
    } else if url_lower.contains(&joined) {
        score += 90;
    }
    score
}

fn normalize_url_key(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn shortcut_display_rank(sequence: &zendia_keymap::KeySequence) -> u8 {
    let Some(first) = sequence.strokes().first() else {
        return 100;
    };
    let is_super = first.modifiers.contains(zendia_keymap::KeyModifiers::SUPER);
    let is_fn_key = matches!(first.key, zendia_keymap::KeyCode::F(_));
    match (is_super, is_fn_key) {
        (false, false) => 0,
        (false, true) => 1,
        (true, false) => 2,
        (true, true) => 3,
    }
}

/// Resolves a human-readable shortcut for `(command, args)` from `key_table`, preferring bindings whose arguments
/// match `args` when a command has multiple bindings (such as `navigation.reload` with and without `bypass_cache`).
pub fn shortcut_for_command_with_args(
    key_table: &KeyTable,
    command: &str,
    args: &[(String, String)],
) -> Option<String> {
    let mut best_exact: Option<(u8, String)> = None;
    let mut best_fallback: Option<(u8, String)> = None;
    for binding in key_table.bindings() {
        if binding.invocation.command != command {
            continue;
        }
        let binding_args = binding.invocation.args_as_pairs();
        let rank = shortcut_display_rank(&binding.keys);
        let display = binding.keys.display_human();
        if binding_args == args || (args.is_empty() && binding_args.is_empty()) {
            if best_exact.as_ref().is_none_or(|(best_rank, _)| rank < *best_rank) {
                best_exact = Some((rank, display));
            }
        } else if args.is_empty() && best_fallback.as_ref().is_none_or(|(best_rank, _)| rank < *best_rank) {
            best_fallback = Some((rank, display));
        }
    }
    best_exact.or(best_fallback).map(|(_, display)| display)
}

/// Computes ranked address bar results for `query` from the browser core model (open tabs across all spaces,
/// bookmarks, browsing history, default search engine, URL classification, and inline quick actions).
pub fn query_address_bar(
    model: &BrowserModel,
    query: &str,
    key_table: Option<&KeyTable>,
    max_results: usize,
) -> Vec<AddressBarResult> {
    if max_results == 0 {
        return Vec::new();
    }

    let trimmed = query.trim();

    // Empty query: show top sites / bookmarks and most-visited history entries (as Zen's floating empty bar does).
    if trimmed.is_empty() {
        let mut results = Vec::new();
        let mut seen_urls = HashSet::new();

        for bookmark in &model.bookmarks {
            let key = normalize_url_key(&bookmark.url);
            if !seen_urls.insert(key) {
                continue;
            }
            results.push(AddressBarResult {
                id: format!("bookmark:{}", bookmark.id.0),
                kind: AddressBarResultKind::Bookmark {
                    bookmark_id: bookmark.id,
                    url: bookmark.url.clone(),
                },
                title: bookmark.title.clone(),
                subtitle: Some(display_url_for_result(&bookmark.url)),
                icon: bookmark.favicon.clone().or_else(|| Some("bookmark".to_string())),
                score: 500,
                heuristic: false,
                shortcut_hint: None,
                badge_text: None,
                badge_accent: None,
            });
            if results.len() >= max_results {
                return results;
            }
        }

        let mut history: Vec<_> = model.history.iter().collect();
        history.sort_by(|a, b| {
            b.visit_count
                .cmp(&a.visit_count)
                .then_with(|| b.visited_at_unix.cmp(&a.visited_at_unix))
        });

        for entry in history {
            let key = normalize_url_key(&entry.url);
            if !seen_urls.insert(key) {
                continue;
            }
            results.push(AddressBarResult {
                id: format!("history:{}", entry.id.0),
                kind: AddressBarResultKind::History {
                    entry_id: entry.id,
                    url: entry.url.clone(),
                },
                title: entry.title.clone(),
                subtitle: Some(display_url_for_result(&entry.url)),
                icon: Some("history".to_string()),
                score: 300 + entry.visit_count as i32,
                heuristic: false,
                shortcut_hint: None,
                badge_text: None,
                badge_accent: None,
            });
            if results.len() >= max_results {
                break;
            }
        }

        return results;
    }

    let mut results = Vec::new();
    let mut seen_urls = HashSet::new();

    // 1. Primary heuristic row: direct URL navigation or default search engine query.
    if let Some(classified) = classify_location_input(trimmed, &model.search_engine) {
        match classified {
            LocationInputKind::Url {
                normalized_url,
                display_url,
            } => {
                seen_urls.insert(normalize_url_key(&normalized_url));
                results.push(AddressBarResult {
                    id: format!("url:{normalized_url}"),
                    kind: AddressBarResultKind::NavigateUrl {
                        url: normalized_url,
                        display_url: display_url.clone(),
                    },
                    title: display_url,
                    subtitle: Some("Visit".to_string()),
                    icon: Some("globe".to_string()),
                    score: EXACT_MATCH_SCORE,
                    heuristic: true,
                    shortcut_hint: None,
                    badge_text: None,
                    badge_accent: None,
                });
            }
            LocationInputKind::Search { query, search_url } => {
                results.push(AddressBarResult {
                    id: format!("search:{query}"),
                    kind: AddressBarResultKind::Search {
                        query: query.clone(),
                        engine_name: model.search_engine.name.clone(),
                        search_url,
                    },
                    title: query,
                    subtitle: Some(format!("Search with {}", model.search_engine.name)),
                    icon: Some("search-glass".to_string()),
                    score: EXACT_MATCH_SCORE,
                    heuristic: true,
                    shortcut_hint: None,
                    badge_text: None,
                    badge_accent: None,
                });
            }
        }
    }

    let tokens: Vec<String> = trimmed.split_whitespace().map(|part| part.to_lowercase()).collect();

    let mut secondary = Vec::new();

    // 2. Open tabs across ALL spaces ("Switch to Tab").
    let active_tab_id = model.active_tab;
    let active_space_id = model.active_space;
    let mut seen_tab_urls = HashSet::new();

    // Order active-space tabs before other spaces so active-space matches win deduplication.
    let mut candidate_tabs: Vec<_> = model.tabs.iter().collect();
    candidate_tabs.sort_by_key(|tab| {
        let same_space = tab.space_id == active_space_id || tab.space_id.is_none();
        (!same_space, tab.id.0)
    });

    for tab in candidate_tabs {
        if Some(tab.id) == active_tab_id {
            continue;
        }
        if tab.url.is_empty() || tab.url == "about:blank" || tab.url == "about:newtab" {
            continue;
        }
        let title = tab.display_title();
        let match_score = token_match_score(&tokens, &title, &tab.url);
        if match_score == 0 {
            continue;
        }
        let url_key = normalize_url_key(&tab.url);
        if !seen_tab_urls.insert(url_key.clone()) {
            continue;
        }
        seen_urls.insert(url_key);
        let space_name = tab
            .space_id
            .filter(|&sid| Some(sid) != active_space_id)
            .and_then(|sid| model.space(sid))
            .map(|space| space.name.clone());
        let space_bonus = if tab.space_id == active_space_id || tab.space_id.is_none() {
            60
        } else {
            0
        };
        secondary.push(AddressBarResult {
            id: format!("tab:{}", tab.id.0),
            kind: AddressBarResultKind::SwitchToTab {
                tab_id: tab.id,
                space_id: tab.space_id,
                space_name: space_name.clone(),
                url: tab.url.clone(),
            },
            title,
            subtitle: space_name,
            icon: tab.favicon.clone().or_else(|| Some("globe".to_string())),
            score: 800 + match_score + space_bonus,
            heuristic: false,
            shortcut_hint: None,
            badge_text: Some("Switch to Tab".to_string()),
            badge_accent: None,
        });
    }

    // 2b. Sidebar folders (matching ZenUBSidebarProvider.#addFolders).
    for folder in &model.folders {
        let space_name = model
            .space(folder.space_id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "Space".to_string());
        let match_score = token_match_score(&tokens, &folder.name, &space_name);
        if match_score == 0 {
            continue;
        }
        let space_accent = model
            .space(folder.space_id)
            .and_then(|s| s.gradient.stops.first().copied());
        secondary.push(AddressBarResult {
            id: format!("folder:{}", folder.id.0),
            kind: AddressBarResultKind::Folder {
                folder_id: folder.id,
                space_id: folder.space_id,
                space_name: space_name.clone(),
            },
            title: folder.name.clone(),
            subtitle: Some(format!("{space_name} / Folder")),
            icon: Some(if folder.icon.is_empty() {
                "folder".to_string()
            } else {
                folder.icon.clone()
            }),
            score: 650 + match_score,
            heuristic: false,
            shortcut_hint: None,
            badge_text: Some("Open".to_string()),
            badge_accent: space_accent,
        });
    }

    // 3. Bookmarks.
    for bookmark in &model.bookmarks {
        let match_score = token_match_score(&tokens, &bookmark.title, &bookmark.url);
        if match_score == 0 {
            continue;
        }
        let url_key = normalize_url_key(&bookmark.url);
        if !seen_urls.insert(url_key) {
            continue;
        }
        secondary.push(AddressBarResult {
            id: format!("bookmark:{}", bookmark.id.0),
            kind: AddressBarResultKind::Bookmark {
                bookmark_id: bookmark.id,
                url: bookmark.url.clone(),
            },
            title: bookmark.title.clone(),
            subtitle: Some(display_url_for_result(&bookmark.url)),
            icon: bookmark.favicon.clone().or_else(|| Some("bookmark".to_string())),
            score: 600 + match_score,
            heuristic: false,
            shortcut_hint: None,
            badge_text: None,
            badge_accent: None,
        });
    }

    // 4. Browsing history.
    for entry in &model.history {
        let match_score = token_match_score(&tokens, &entry.title, &entry.url);
        if match_score == 0 {
            continue;
        }
        let url_key = normalize_url_key(&entry.url);
        if !seen_urls.insert(url_key) {
            continue;
        }
        let visit_bonus = (entry.visit_count as i32 * 4).min(120);
        secondary.push(AddressBarResult {
            id: format!("history:{}", entry.id.0),
            kind: AddressBarResultKind::History {
                entry_id: entry.id,
                url: entry.url.clone(),
            },
            title: entry.title.clone(),
            subtitle: Some(display_url_for_result(&entry.url)),
            icon: Some("history".to_string()),
            score: 400 + match_score + visit_bonus,
            heuristic: false,
            shortcut_hint: None,
            badge_text: None,
            badge_accent: None,
        });
    }

    // 5. Inline quick actions (matching ZenUBActionsProvider: >= 3 chars, non-protocol, high score),
    //    including dynamic workspace-switch ("Focus on <Space>") and extension actions.
    if trimmed.len() >= 3 && parse_explicit_scheme(trimmed).is_none() {
        let mut action_matches: Vec<(i32, AddressBarResult)> = Vec::new();
        for entry in &model.contributions.palette_entries {
            if !is_palette_entry_available(model, entry) {
                continue;
            }
            let score = fuzzy_score(&entry.title, trimmed);
            if score > INLINE_ACTION_MIN_SCORE {
                let shortcut_hint = key_table
                    .and_then(|table| shortcut_for_command_with_args(table, &entry.command, &entry.command_args))
                    .or_else(|| entry.shortcut_hint.clone());
                action_matches.push((
                    score,
                    AddressBarResult {
                        id: format!("action:{}", entry.id.as_str()),
                        kind: AddressBarResultKind::Action {
                            command: entry.command.clone(),
                            command_args: entry.command_args.clone(),
                            shortcut_hint: shortcut_hint.clone(),
                        },
                        title: entry.title.clone(),
                        subtitle: Some("Action".to_string()),
                        icon: entry.icon.clone().or_else(|| Some("sparkles".to_string())),
                        score: 350 + score.min(300),
                        heuristic: false,
                        shortcut_hint,
                        badge_text: None,
                        badge_accent: None,
                    },
                ));
            }
        }
        for dyn_item in dynamic_space_and_extension_actions(model) {
            let score = fuzzy_score(&dyn_item.title, trimmed);
            if score > INLINE_ACTION_MIN_SCORE {
                action_matches.push((
                    score,
                    AddressBarResult {
                        id: format!("action:{}", dyn_item.id),
                        kind: AddressBarResultKind::Action {
                            command: dyn_item.command,
                            command_args: dyn_item.command_args,
                            shortcut_hint: None,
                        },
                        title: dyn_item.title,
                        subtitle: dyn_item.detail,
                        icon: dyn_item.icon,
                        score: 360 + score.min(300),
                        heuristic: false,
                        shortcut_hint: None,
                        badge_text: dyn_item.badge_text,
                        badge_accent: dyn_item.badge_accent,
                    },
                ));
            }
        }
        action_matches.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.title.cmp(&b.1.title)));
        for (_, item) in action_matches.into_iter().take(2) {
            secondary.push(item);
        }
    }

    secondary.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    let remaining = max_results.saturating_sub(results.len());
    results.extend(secondary.into_iter().take(remaining));
    results
}

fn palette_entry(
    id: &str,
    title: &str,
    detail: &str,
    category: &str,
    icon: &str,
    command: &str,
    command_args: Vec<(String, String)>,
) -> PaletteEntry {
    PaletteEntry {
        id: ContributionId::new(id),
        title: title.to_string(),
        detail: Some(detail.to_string()),
        category: category.to_string(),
        icon: Some(icon.to_string()),
        command: command.to_string(),
        command_args,
        shortcut_hint: None,
        source: ContributionSource::BuiltIn,
    }
}

/// Built-in palette contributions covering Zen's global actions (in Zen's top order) and Zendia panels.
pub fn builtin_palette_entries() -> Vec<PaletteEntry> {
    vec![
        palette_entry(
            "zen.action.toggle_compact_mode",
            "Toggle Compact Mode",
            "Hide or reveal toolbars and sidebar chrome",
            "View",
            "sidebar",
            "layout.toggle_compact_mode",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.open_theme_picker",
            "Open Theme Picker",
            "Customize the active space's gradient and theme",
            "Appearance",
            "paintbrush-fill",
            "theme.picker.open",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.open_library",
            "Open Library",
            "Browse spaces, downloads, history, and media",
            "Library",
            "library",
            "library.toggle",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.new_split_view",
            "New Split View",
            "Open a new split pane alongside the current page",
            "Layout",
            "split",
            "split.new_empty",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.unsplit_view",
            "Expand Current View",
            "Unsplit the active split view into separate tabs",
            "Layout",
            "split",
            "split.unsplit",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.new_space",
            "New Space",
            "Create a new workspace",
            "Spaces",
            "plus",
            "space.create_form.open",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.share_page",
            "Share Link…",
            "Create a share link for the current page",
            "Share",
            "share",
            "share.open",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.new_folder",
            "New Folder",
            "Create a tab folder in the active space",
            "Tabs",
            "folder",
            "folder.create",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.collapse_all_folders",
            "Collapse All Folders",
            "Collapse every tab folder in the active space",
            "Tabs",
            "arrows-collapse",
            "folder.collapse_all",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.expand_all_folders",
            "Expand All Folders",
            "Expand every tab folder in the active space",
            "Tabs",
            "arrows-expand",
            "folder.expand_all",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.copy_current_url",
            "Copy Current URL",
            "Copy the active tab's address to the clipboard",
            "Navigation",
            "link",
            "address_bar.copy_current_url",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.settings",
            "Settings",
            "Open Zendia browser settings",
            "Application",
            "settings",
            "settings.open",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.open_private_window",
            "Open Private Window",
            "Open a new private browsing window",
            "Window",
            "private-window",
            "window.new_private",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.open_new_window",
            "Open New Window",
            "Open a new browser window",
            "Window",
            "window",
            "window.new",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.new_blank_window",
            "New Blank Window",
            "Open a new unsynced blank window",
            "Window",
            "window",
            "window.new_blank",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.pin_tab",
            "Pin Tab",
            "Pin the active tab to the current space",
            "Tabs",
            "pin",
            "tab.pin",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.unpin_tab",
            "Unpin Tab",
            "Unpin the active tab back to normal tabs",
            "Tabs",
            "unpin",
            "tab.unpin",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.open_space_routing",
            "Open Space Routing",
            "Configure automatic URL routing rules for spaces",
            "Spaces",
            "airplane",
            "space_routing.open",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.next_space",
            "Next Space",
            "Switch to the next workspace",
            "Spaces",
            "forward",
            "space.next",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.previous_space",
            "Previous Space",
            "Switch to the previous workspace",
            "Spaces",
            "back",
            "space.previous",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.close_tab",
            "Close Tab",
            "Close the active tab",
            "Tabs",
            "close",
            "tab.close",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.reopen_closed_tab",
            "Reopen Closed Tab",
            "Restore the most recently closed tab",
            "Tabs",
            "history",
            "tab.reopen_closed",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.duplicate_tab",
            "Duplicate Current Tab",
            "Open a duplicate of the active tab",
            "Tabs",
            "duplicate-tab",
            "tab.duplicate",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.reload_tab",
            "Reload Tab",
            "Reload the active tab",
            "Navigation",
            "reload",
            "navigation.reload",
            vec![("bypass_cache".to_string(), "false".to_string())],
        ),
        palette_entry(
            "zen.action.reload_tab_without_cache",
            "Reload Tab Without Cache",
            "Reload the active tab ignoring cached resources",
            "Navigation",
            "reload",
            "navigation.reload",
            vec![("bypass_cache".to_string(), "true".to_string())],
        ),
        palette_entry(
            "zen.action.next_tab",
            "Next Tab",
            "Switch to the next tab",
            "Tabs",
            "forward",
            "tab.next",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.previous_tab",
            "Previous Tab",
            "Switch to the previous tab",
            "Tabs",
            "back",
            "tab.previous",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.capture_screenshot",
            "Capture Screenshot",
            "Capture a screenshot of the current page",
            "Page",
            "screenshot",
            "site.screenshot",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.toggle_tabs_on_right",
            "Toggle Tabs on Right",
            "Move the vertical sidebar between left and right",
            "View",
            "sidebars-right",
            "sidebar.toggle_side",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.add_to_essentials",
            "Add to Essentials",
            "Pin the active tab to the essentials grid",
            "Tabs",
            "essential-add",
            "tab.add_to_essentials",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.remove_from_essentials",
            "Remove from Essentials",
            "Remove the active tab from the essentials grid",
            "Tabs",
            "essential-remove",
            "tab.remove_from_essentials",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.find_in_page",
            "Find in Page",
            "Search text in the current page",
            "Page",
            "search-page",
            "page.find",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.manage_extensions",
            "Manage Extensions",
            "Open installed extensions and add-ons",
            "Extensions",
            "extension",
            "extensions.open",
            Vec::new(),
        ),
        palette_entry(
            "zen.action.theme_auto",
            "Switch to Automatic Appearance",
            "Follow the operating system color scheme",
            "Appearance",
            "sparkles",
            "theme.set",
            vec![("scheme".to_string(), "system".to_string())],
        ),
        palette_entry(
            "zen.action.theme_light",
            "Switch to Light Mode",
            "Use the Tokyonight Day light theme",
            "Appearance",
            "face-sun",
            "theme.set",
            vec![("scheme".to_string(), "light".to_string())],
        ),
        palette_entry(
            "zen.action.theme_dark",
            "Switch to Dark Mode",
            "Use the Tokyonight Moon dark theme",
            "Appearance",
            "moon-stars",
            "theme.set",
            vec![("scheme".to_string(), "dark".to_string())],
        ),
        palette_entry(
            "zen.action.print",
            "Print",
            "Print the current page",
            "Page",
            "print",
            "page.print",
            Vec::new(),
        ),
        palette_entry(
            "zendia.palette.new_tab",
            "New Tab",
            "Open a new tab in the current space",
            "Tabs",
            "plus",
            "tab.new",
            Vec::new(),
        ),
        palette_entry(
            "zendia.palette.toggle_chat",
            "Toggle Agent Panel",
            "Open or close the active agent conversation",
            "Agents",
            "chat",
            "agent.toggle",
            Vec::new(),
        ),
        palette_entry(
            "zendia.palette.toggle_agents",
            "Toggle Agents Panel",
            "Open or close the cross-window agents roster panel",
            "Agents",
            "agents",
            "agents.toggle",
            Vec::new(),
        ),
        palette_entry(
            "zendia.palette.toggle_webchat",
            "Toggle Web Chat Panel",
            "Open ChatGPT, Gemini, or another chat site",
            "Web chat",
            "webchat",
            "webchat.toggle",
            Vec::new(),
        ),
        palette_entry(
            "zendia.palette.show_onboarding",
            "Show onboarding",
            "Open the first-run onboarding walkthrough",
            "General",
            "sparkles",
            "onboarding.restart",
            Vec::new(),
        ),
    ]
}

fn default_icon_for_command(name: &str) -> &'static str {
    if name.starts_with("tab.") {
        "tab"
    } else if name.starts_with("navigation.") {
        "globe"
    } else if name.starts_with("space.") {
        "layers"
    } else if name.starts_with("folder.") {
        "folder"
    } else if name.starts_with("site.") {
        "sliders"
    } else if name.starts_with("sidebar.") || name.starts_with("layout.") {
        "sidebar"
    } else if name.starts_with("theme.") {
        "palette"
    } else if name.starts_with("zoom.") {
        "search-glass"
    } else if name.starts_with("vim.") {
        "terminal"
    } else if name.starts_with("address_bar.") || name.starts_with("location.") {
        "search-glass"
    } else if name.starts_with("palette.") {
        "sparkles"
    } else {
        "bolt"
    }
}

/// Evaluates Zen's context-aware `isAvailable` rules against the current browser model.
pub fn is_palette_entry_available(model: &BrowserModel, entry: &PaletteEntry) -> bool {
    let active_tab = model.active_tab();
    let has_active_tab = active_tab.is_some();
    let active_folders = model
        .active_space
        .map(|sid| model.folders_in_space(sid))
        .unwrap_or_default();

    match entry.id.as_str() {
        "zen.action.unsplit_view" => model.active_split_group().is_some(),
        "zen.action.collapse_all_folders" => active_folders.iter().any(|f| !f.collapsed),
        "zen.action.expand_all_folders" => active_folders.iter().any(|f| f.collapsed),
        "zen.action.pin_tab" => active_tab.is_some_and(|t| t.kind == TabKind::Normal),
        "zen.action.unpin_tab" => active_tab.is_some_and(|t| t.kind == TabKind::Pinned),
        "zen.action.add_to_essentials" => active_tab.is_some_and(|t| t.kind != TabKind::Essential),
        "zen.action.remove_from_essentials" => active_tab.is_some_and(|t| t.kind == TabKind::Essential),
        "zen.action.next_space" | "zen.action.previous_space" => model.spaces.len() > 1,
        "zen.action.theme_auto" => model.theme.scheme != ThemeScheme::System,
        "zen.action.theme_light" => model.theme.scheme != ThemeScheme::Light,
        "zen.action.theme_dark" => model.theme.scheme != ThemeScheme::Dark,
        "zen.action.close_tab"
        | "zen.action.duplicate_tab"
        | "zen.action.capture_screenshot"
        | "zen.action.find_in_page"
        | "zen.action.print" => has_active_tab,
        _ => true,
    }
}

/// Synthesizes dynamic workspace-switch (`Focus on <Space>`) and enabled extension (`Extension <Name>`) actions,
/// matching Zen's `ZenUBActionsProvider`.
fn dynamic_space_and_extension_actions(model: &BrowserModel) -> Vec<ResolvedPaletteItem> {
    let mut out = Vec::new();
    for space in &model.spaces {
        if Some(space.id) == model.active_space {
            continue;
        }
        let accent = space.gradient.stops.first().copied();
        out.push(ResolvedPaletteItem {
            id: format!("zen.action.space.{}", space.id.0),
            title: format!("Focus on {}", space.name),
            detail: Some(format!("Switch to workspace {}", space.name)),
            category: "Spaces".to_string(),
            icon: Some(space.icon.clone()),
            command: "space.switch".to_string(),
            command_args: vec![("space_id".to_string(), space.id.0.to_string())],
            shortcut_hint: None,
            badge_text: Some(space.name.clone()),
            badge_accent: accent,
            score: 0,
        });
    }
    for ext in &model.extensions {
        if !ext.enabled {
            continue;
        }
        out.push(ResolvedPaletteItem {
            id: format!("zen.action.extension.{}", ext.id.0),
            title: format!("Extension {}", ext.name),
            detail: Some(format!("Run {} extension action", ext.name)),
            category: "Extensions".to_string(),
            icon: Some(ext.icon.clone()),
            command: "extension.invoke_action".to_string(),
            command_args: vec![("extension_id".to_string(), ext.id.0.clone())],
            shortcut_hint: None,
            badge_text: Some("Extension".to_string()),
            badge_accent: None,
            score: 0,
        });
    }
    out
}

/// Builds and fuzzy-filters command palette items from contributed palette entries and all registered core commands,
/// resolving each item's shortcut from `key_table`.
pub fn query_palette(
    model: &BrowserModel,
    commands: &[CommandDescriptor],
    query: &str,
    key_table: Option<&KeyTable>,
) -> Vec<ResolvedPaletteItem> {
    let mut items = Vec::new();
    let mut covered_commands: HashSet<String> = HashSet::new();

    // 1. Contributed palette entries (Zen global actions, plugin/extension palette contributions) in curated order.
    covered_commands.insert("tab.toggle_pin".to_string());
    covered_commands.insert("tab.toggle_essential".to_string());
    for entry in &model.contributions.palette_entries {
        if entry.command_args.is_empty() {
            covered_commands.insert(entry.command.clone());
        }
        if !is_palette_entry_available(model, entry) {
            continue;
        }
        let shortcut_hint = key_table
            .and_then(|table| shortcut_for_command_with_args(table, &entry.command, &entry.command_args))
            .or_else(|| entry.shortcut_hint.clone());
        items.push(ResolvedPaletteItem {
            id: entry.id.as_str().to_string(),
            title: entry.title.clone(),
            detail: entry.detail.clone(),
            category: entry.category.clone(),
            icon: entry.icon.clone(),
            command: entry.command.clone(),
            command_args: entry.command_args.clone(),
            shortcut_hint,
            badge_text: None,
            badge_accent: None,
            score: 0,
        });
    }

    // 1b. Dynamic workspace-switch and extension actions.
    items.extend(dynamic_space_and_extension_actions(model));

    // 2. Remaining registered commands from the registry so every zero-arg/defaultable command is reachable.
    for desc in commands {
        if covered_commands.contains(&desc.name) {
            continue;
        }
        // Skip commands that require caller-specific IDs/text (like `tab.rename` or `page.replace_misspelling`)
        // unless no curated entry covers them and all arguments are optional.
        let has_required_arg = desc.arguments.iter().any(|arg| arg.required);
        if has_required_arg {
            continue;
        }
        let shortcut_hint = key_table.and_then(|table| shortcut_for_command_with_args(table, &desc.name, &[]));
        let category = desc
            .name
            .split('.')
            .next()
            .map(|c| {
                let mut chars = c.chars();
                match chars.next() {
                    Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                    None => "Command".to_string(),
                }
            })
            .unwrap_or_else(|| "Command".to_string());
        items.push(ResolvedPaletteItem {
            id: format!("cmd:{}", desc.name),
            title: desc.title.clone(),
            detail: Some(desc.description.clone()),
            category,
            icon: Some(default_icon_for_command(&desc.name).to_string()),
            command: desc.name.clone(),
            command_args: Vec::new(),
            shortcut_hint,
            badge_text: None,
            badge_accent: None,
            score: 0,
        });
    }

    let trimmed = query.trim();
    if trimmed.is_empty() {
        return items;
    }

    let mut filtered = Vec::new();
    for (idx, mut item) in items.into_iter().enumerate() {
        let title_score = fuzzy_score(&item.title, trimmed);
        let cmd_score = fuzzy_score(&item.command, trimmed) / 2;
        let detail_score = item.detail.as_deref().map(|d| fuzzy_score(d, trimmed) / 3).unwrap_or(0);
        let best = title_score.max(cmd_score).max(detail_score);
        if best > PALETTE_MIN_SCORE {
            item.score = best;
            filtered.push((best, idx, item));
        }
    }

    filtered.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    filtered.into_iter().map(|(_, _, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::builtin_command_descriptors;
    use crate::stub::sample_model;

    #[test]
    fn palette_lists_each_title_once() {
        let model = sample_model(true);
        let items = query_palette(&model, &builtin_command_descriptors(), "", None);
        let mut seen = std::collections::HashSet::new();
        for item in &items {
            assert!(seen.insert(item.title.clone()), "`{}` is listed twice", item.title);
        }
    }

    #[test]
    fn classifies_urls_and_searches() {
        let engine = SearchEngine::default();

        assert_eq!(
            resolve_navigation_url("https://ladybird.org/docs", &engine),
            Some("https://ladybird.org/docs".to_string())
        );
        assert_eq!(
            resolve_navigation_url("ladybird.org", &engine),
            Some("https://ladybird.org".to_string())
        );
        assert_eq!(
            resolve_navigation_url("ladybird.org/news?x=1", &engine),
            Some("https://ladybird.org/news?x=1".to_string())
        );
        assert_eq!(
            resolve_navigation_url("localhost:8000", &engine),
            Some("http://localhost:8000".to_string())
        );
        assert_eq!(
            resolve_navigation_url("127.0.0.1:8080/page", &engine),
            Some("http://127.0.0.1:8080/page".to_string())
        );
        assert_eq!(
            resolve_navigation_url("[::1]:3000", &engine),
            Some("http://[::1]:3000".to_string())
        );
        assert_eq!(
            resolve_navigation_url("about:settings", &engine),
            Some("about:settings".to_string())
        );
        assert_eq!(
            resolve_navigation_url("data:text/html,<h1>Hi</h1>", &engine),
            Some("data:text/html,<h1>Hi</h1>".to_string())
        );
        assert_eq!(
            resolve_navigation_url("file:///tmp/test.html", &engine),
            Some("file:///tmp/test.html".to_string())
        );
        assert_eq!(
            resolve_navigation_url("/etc/hosts", &engine),
            Some("file:///etc/hosts".to_string())
        );
        assert_eq!(
            resolve_navigation_url("Architecture", &engine),
            Some("https://www.google.com/search?q=Architecture".to_string())
        );
        assert_eq!(
            resolve_navigation_url("hello world", &engine),
            Some("https://www.google.com/search?q=hello+world".to_string())
        );
        assert_eq!(resolve_navigation_url("   ", &engine), None);

        let ddg = SearchEngine::from_url("https://duckduckgo.com/?q=%s");
        assert_eq!(ddg.name, "DuckDuckGo");
        assert_eq!(
            resolve_navigation_url("rust lang", &ddg),
            Some("https://duckduckgo.com/?q=rust+lang".to_string())
        );
    }

    #[test]
    fn address_bar_provider_ranks_heuristic_tabs_bookmarks_history_and_actions() {
        let model = sample_model(true);
        let key_table = KeyTable::standard();

        // Empty query returns top sites / bookmarks and history.
        let empty = query_address_bar(&model, "", Some(&key_table), 6);
        assert!(!empty.is_empty());
        assert!(empty.iter().all(|r| !r.heuristic));

        // Query "Architecture": primary heuristic Search row first, then Switch to Tab row.
        let arch = query_address_bar(&model, "Architecture", Some(&key_table), 8);
        assert!(arch.len() >= 2);
        assert!(arch[0].heuristic);
        assert!(matches!(arch[0].kind, AddressBarResultKind::Search { .. }));
        assert_eq!(arch[0].subtitle.as_deref(), Some("Search with Google"));
        assert!(
            arch.iter()
                .any(|r| matches!(r.kind, AddressBarResultKind::SwitchToTab { .. })
                    && r.badge_text.as_deref() == Some("Switch to Tab"))
        );

        // Domain query produces a NavigateUrl heuristic row first.
        let url_rows = query_address_bar(&model, "zendia.local", Some(&key_table), 8);
        assert!(matches!(url_rows[0].kind, AddressBarResultKind::NavigateUrl { .. }));

        // Cross-space tab search finds tabs in Work space too.
        let cross_space = query_address_bar(&model, "HTML Living", Some(&key_table), 8);
        assert!(
            cross_space.iter().any(|r| matches!(
                &r.kind,
                AddressBarResultKind::SwitchToTab {
                    space_name: Some(name),
                    ..
                } if name == "Work"
            )),
            "expected Work space tab in cross-space results: {cross_space:?}"
        );
    }

    #[test]
    fn palette_filters_and_resolves_shortcuts_from_active_keymap() {
        let model = sample_model(true);
        let commands = builtin_command_descriptors();
        let key_table = KeyTable::standard();

        let all = query_palette(&model, &commands, "", Some(&key_table));
        assert!(all.len() >= 30);
        assert_eq!(all[0].title, "Toggle Compact Mode");
        assert_eq!(all[1].title, "Open Theme Picker");
        assert_eq!(all[2].title, "Open Library");
        assert_eq!(all[3].title, "New Split View");
        assert_eq!(all[4].title, "New Space");

        // Verify shortcuts come from the active keymap table.
        let close_tab = all.iter().find(|item| item.command == "tab.close").unwrap();
        assert_eq!(close_tab.shortcut_hint.as_deref(), Some("Ctrl+W"));

        let reload_hard = all
            .iter()
            .find(|item| item.title == "Reload Tab Without Cache")
            .unwrap();
        assert_eq!(reload_hard.shortcut_hint.as_deref(), Some("Ctrl+Shift+R"));

        // Exact & prefix matches outrank fuzzy matches.
        let filtered = query_palette(&model, &commands, "close tab", Some(&key_table));
        assert!(!filtered.is_empty());
        assert_eq!(filtered[0].title, "Close Tab");

        // No match returns empty.
        let none = query_palette(&model, &commands, "zzzznotacommand", Some(&key_table));
        assert!(none.is_empty());
    }

    #[test]
    fn fuzzy_scorer_prioritizes_exact_and_prefix_over_subsequence() {
        assert!(fuzzy_score("Close Tab", "close tab") > fuzzy_score("Reopen Closed Tab", "close tab"));
        assert!(fuzzy_score("Toggle Compact Mode", "toggle") > fuzzy_score("Sidebar Toggle", "toggle"));
        assert_eq!(fuzzy_score("New Space", "xyz"), 0);
    }

    #[test]
    fn address_bar_matches_folders_workspaces_and_extensions() {
        let model = sample_model(true);
        let key_table = KeyTable::standard();

        // Folder search (AR-04).
        let folder_name = model.folders[0].name.clone();
        let folder_hits = query_address_bar(&model, &folder_name, Some(&key_table), 8);
        assert!(
            folder_hits
                .iter()
                .any(|r| matches!(r.kind, AddressBarResultKind::Folder { .. })
                    && r.badge_text.as_deref() == Some("Open")),
            "expected folder match for `{folder_name}`: {folder_hits:?}"
        );

        // Workspace switch action (AR-05).
        let space_hits = query_address_bar(&model, "Focus on Work", Some(&key_table), 8);
        assert!(
            space_hits
                .iter()
                .any(|r| matches!(&r.kind, AddressBarResultKind::Action { command, .. } if command == "space.switch")),
            "expected space.switch inline action: {space_hits:?}"
        );

        // Extension action (AR-05).
        let ext_hits = query_address_bar(&model, "Extension uBlock", Some(&key_table), 8);
        assert!(
            ext_hits.iter().any(|r| matches!(
                &r.kind,
                AddressBarResultKind::Action { command, .. } if command == "extension.invoke_action"
            )),
            "expected extension.invoke_action inline action: {ext_hits:?}"
        );
    }

    #[test]
    fn palette_filters_context_aware_actions() {
        let mut model = sample_model(true);
        let commands = builtin_command_descriptors();
        let key_table = KeyTable::standard();

        // Default model: no active split, active tab is normal, theme is Dark.
        model.theme.scheme = ThemeScheme::Dark;
        let items = query_palette(&model, &commands, "", Some(&key_table));
        assert!(!items.iter().any(|i| i.id == "zen.action.unsplit_view"));
        assert!(items.iter().any(|i| i.title == "Pin Tab"));
        assert!(!items.iter().any(|i| i.title == "Unpin Tab"));
        assert!(items.iter().any(|i| i.title == "Add to Essentials"));
        assert!(!items.iter().any(|i| i.title == "Remove from Essentials"));
        assert!(!items.iter().any(|i| i.id == "zen.action.theme_dark"));
        assert!(items.iter().any(|i| i.id == "zen.action.theme_light"));
        assert!(items.iter().any(|i| i.title == "Focus on Work"));
    }
}
