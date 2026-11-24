/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The few of the browser's settings that this UI reads. The Hub owns the settings and sends them as JSON, as it does
//! to the Qt UI, which reads them with `WebView::Settings`.

use serde_json::Value;

const DEFAULT_NEW_TAB_PAGE_URL: &str = "about:newtab";

// NB: As WebView::builtin_search_engines() has them.
const BUILTIN_SEARCH_ENGINES: &[(&str, &str)] = &[
    ("Bing", "https://www.bing.com/search?q=%s"),
    ("Brave", "https://search.brave.com/search?q=%s"),
    ("DuckDuckGo", "https://duckduckgo.com/?q=%s"),
    ("Ecosia", "https://ecosia.org/search?q=%s"),
    ("Google", "https://www.google.com/search?q=%s"),
    ("Kagi", "https://kagi.com/search?q=%s"),
    ("Mojeek", "https://www.mojeek.com/search?q=%s"),
    ("Startpage", "https://startpage.com/search?q=%s"),
    ("Yahoo", "https://search.yahoo.com/search?p=%s"),
    ("Yandex", "https://yandex.com/search/?text=%s"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub new_tab_page_url: String,
    /// The search URL, with %s where the query goes.
    pub search_url: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            new_tab_page_url: DEFAULT_NEW_TAB_PAGE_URL.to_string(),
            search_url: hub_protocol::location::DEFAULT_SEARCH_URL.to_string(),
        }
    }
}

impl Settings {
    pub fn from_json(json: &[u8]) -> Self {
        let mut settings = Self::default();
        let Ok(value) = serde_json::from_slice::<Value>(json) else {
            eprintln!("zendia: The settings from the Hub are not JSON");
            return settings;
        };

        if let Some(url) = value.get("newTabPageURL").and_then(Value::as_str) {
            settings.new_tab_page_url = url.to_string();
        }

        if let Some(search_engine) = value.get("searchEngine") {
            let custom_engines = search_engine.get("custom").and_then(Value::as_array);
            let find_custom = |name: &str| {
                custom_engines?.iter().find_map(|engine| {
                    (engine.get("name")?.as_str()? == name).then(|| engine.get("url")?.as_str().map(str::to_string))?
                })
            };
            if let Some(name) = search_engine.get("engine").and_then(Value::as_str) {
                let builtin = BUILTIN_SEARCH_ENGINES
                    .iter()
                    .find(|(builtin_name, _)| *builtin_name == name)
                    .map(|(_, url)| url.to_string());
                if let Some(url) = builtin.or_else(|| find_custom(name)) {
                    settings.search_url = url;
                }
            }
        }
        settings
    }
}
