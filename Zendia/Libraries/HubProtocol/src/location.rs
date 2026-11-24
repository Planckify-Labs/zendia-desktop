/*
 * Copyright (c) 2026-present, the Ladybird developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! What the user typed in the location bar, as the URL to load. This follows `WebView::sanitize_url()` closely enough
//! for typing addresses and searching. The URLs it makes are ones the WHATWG basic URL parser accepts, which is what
//! the Hub parses them with again.

use crate::ipc_types::URL_URL;
use url::Url;

/// The search a query goes to when what was typed is not an address.
pub const DEFAULT_SEARCH_URL: &str = "https://duckduckgo.com/?q=%s";

const SCHEMES_THAT_NEED_NO_HOST: &[&str] = &["about", "data", "file", "javascript", "blob", "view-source"];

pub fn url_for_location(location: &str, search_url: &str) -> Option<URL_URL> {
    let location = location.trim();
    if location.is_empty() {
        return None;
    }

    if let Ok(url) = Url::parse(location) {
        let scheme = url.scheme();
        let looks_like_host_and_port = !url.has_host() && !SCHEMES_THAT_NEED_NO_HOST.contains(&scheme);
        // NB: "localhost:8000" parses as a URL whose scheme is "localhost", which is not what anyone means by it.
        if !looks_like_host_and_port || scheme == "ladybird" {
            return Some(URL_URL::from_serialization(String::from(url)));
        }
    }

    if location.starts_with('/')
        && let Ok(url) = Url::from_file_path(location)
    {
        return Some(URL_URL::from_serialization(String::from(url)));
    }

    if looks_like_address(location) {
        let scheme = if is_local_host(location) { "http" } else { "https" };
        if let Ok(url) = Url::parse(&format!("{scheme}://{location}")) {
            return Some(URL_URL::from_serialization(String::from(url)));
        }
    }

    let query: String = url::form_urlencoded::byte_serialize(location.as_bytes()).collect();
    Url::parse(&search_url.replace("%s", &query))
        .ok()
        .map(|url| URL_URL::from_serialization(String::from(url)))
}

fn host_part(location: &str) -> &str {
    let end = location.find(['/', '?', '#']).unwrap_or(location.len());
    let authority = &location[..end];
    // NB: An IPv6 address has colons of its own, inside its brackets.
    if authority.starts_with('[') {
        return authority.find(']').map(|end| &authority[..=end]).unwrap_or(authority);
    }
    authority.rsplit_once(':').map(|(host, _)| host).unwrap_or(authority)
}

fn is_local_host(location: &str) -> bool {
    let host = host_part(location);
    host == "localhost"
        || host.ends_with(".localhost")
        || host.parse::<std::net::Ipv4Addr>().is_ok()
        || host.starts_with('[')
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
    // NB: A host with a dot in it, not at either end, is taken to be a domain, as in "ladybird.org/news".
    host.contains('.') && !host.starts_with('.') && !host.ends_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(location: &str) -> String {
        url_for_location(location, DEFAULT_SEARCH_URL)
            .unwrap()
            .as_str()
            .to_string()
    }

    #[test]
    fn typed_locations() {
        assert_eq!(url("https://ladybird.org"), "https://ladybird.org/");
        assert_eq!(url("ladybird.org"), "https://ladybird.org/");
        assert_eq!(url("ladybird.org/news?x=1"), "https://ladybird.org/news?x=1");
        assert_eq!(url("localhost:8000"), "http://localhost:8000/");
        assert_eq!(url("127.0.0.1:8080/a"), "http://127.0.0.1:8080/a");
        assert_eq!(url("about:blank"), "about:blank");
        assert_eq!(url("/etc/hosts"), "file:///etc/hosts");
        assert_eq!(url("hello world"), "https://duckduckgo.com/?q=hello+world");
        assert_eq!(url("rust"), "https://duckduckgo.com/?q=rust");
        assert!(url_for_location("   ", DEFAULT_SEARCH_URL).is_none());
    }
}
