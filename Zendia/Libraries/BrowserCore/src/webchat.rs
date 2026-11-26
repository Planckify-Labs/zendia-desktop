/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Web chat site catalog, URL normalization, and Ask prompt helpers.
//!
//! Pure domain types and helpers live here so Freya, headless runners, and unit tests all share one source of truth.

use crate::model::{ChatSite, ChatSiteId};

/// Built-in side panel identifier for Zendia's web chat panel.
pub const WEBCHAT_PANEL_ID: &str = "zendia.webchat";

/// Default logical width of the web chat side panel (`460 px`).
pub const WEBCHAT_DEFAULT_WIDTH: f32 = 460.0;

/// Returns Zendia's built-in catalog of web chat sites matching the site strip order.
pub fn builtin_chat_sites() -> Vec<ChatSite> {
    vec![
        ChatSite {
            id: ChatSiteId::new("chatgpt"),
            name: "ChatGPT".to_string(),
            url: "https://chatgpt.com/".to_string(),
            icon: "chatgpt".to_string(),
            accent_hex: "#10a37f".to_string(),
            is_custom: false,
        },
        ChatSite {
            id: ChatSiteId::new("claude"),
            name: "Claude".to_string(),
            url: "https://claude.ai/".to_string(),
            icon: "claude".to_string(),
            accent_hex: "#d97757".to_string(),
            is_custom: false,
        },
        ChatSite {
            id: ChatSiteId::new("gemini"),
            name: "Gemini".to_string(),
            url: "https://gemini.google.com/".to_string(),
            icon: "gemini".to_string(),
            accent_hex: "#4b8bf5".to_string(),
            is_custom: false,
        },
        ChatSite {
            id: ChatSiteId::new("kimi"),
            name: "Kimi".to_string(),
            url: "https://kimi.moonshot.cn/".to_string(),
            icon: "kimi".to_string(),
            accent_hex: "#6b7280".to_string(),
            is_custom: false,
        },
        ChatSite {
            id: ChatSiteId::new("grok"),
            name: "Grok".to_string(),
            url: "https://grok.com/".to_string(),
            icon: "grok".to_string(),
            accent_hex: "#94a3b8".to_string(),
            is_custom: false,
        },
        ChatSite {
            id: ChatSiteId::new("deepseek"),
            name: "DeepSeek".to_string(),
            url: "https://chat.deepseek.com/".to_string(),
            icon: "deepseek".to_string(),
            accent_hex: "#4d6bfe".to_string(),
            is_custom: false,
        },
        ChatSite {
            id: ChatSiteId::new("perplexity"),
            name: "Perplexity".to_string(),
            url: "https://www.perplexity.ai/".to_string(),
            icon: "perplexity".to_string(),
            accent_hex: "#20b2aa".to_string(),
            is_custom: false,
        },
        ChatSite {
            id: ChatSiteId::new("copilot"),
            name: "Copilot".to_string(),
            url: "https://copilot.microsoft.com/".to_string(),
            icon: "copilot".to_string(),
            accent_hex: "#3b82f6".to_string(),
            is_custom: false,
        },
    ]
}

/// Normalizes a user-typed custom chat site URL, prepending `https://` when no scheme is present.
///
/// Returns `None` for empty inputs or malformed hostnames.
pub fn normalize_chat_site_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    let with_scheme = if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let rest = with_scheme
        .strip_prefix("https://")
        .or_else(|| with_scheme.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || (!host.contains('.') && !host.starts_with("localhost")) {
        return None;
    }
    Some(with_scheme)
}

/// Normalizes a user-typed custom chat site URL, requiring `https://` (prepending it when no scheme is given).
///
/// Rejects `http://` and other non-HTTPS schemes (`WC-03`, matching `ZendiaWebChats.sys.mjs:124-134`).
pub fn normalize_https_chat_site_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    if trimmed.contains("://") && !trimmed.starts_with("https://") {
        return None;
    }
    let with_scheme = if trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let rest = with_scheme.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#', ':']).next().unwrap_or("");
    if host.is_empty() || !host.contains('.') || host.starts_with('.') || host.ends_with('.') {
        return None;
    }
    Some(with_scheme)
}

/// Derives a readable display name and stable identifier slug for a custom chat site URL.
pub fn derive_custom_site_identity(url: &str, explicit_name: Option<&str>) -> (ChatSiteId, String) {
    let raw_host = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url)
        .split(['/', '?', '#', ':'])
        .next()
        .unwrap_or("custom")
        .to_ascii_lowercase();
    let slug_host = raw_host.strip_prefix("www.").unwrap_or(&raw_host);
    let trimmed_host = slug_host
        .strip_prefix("chat.")
        .or_else(|| slug_host.strip_prefix("app."))
        .unwrap_or(slug_host);

    let host_label = trimmed_host.split('.').next().unwrap_or("custom");
    let display_name = explicit_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            let mut chars = host_label.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => "Custom".to_string(),
            }
        });

    let slug: String = slug_host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    (ChatSiteId::new(format!("custom-{slug}")), display_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_lists_all_eight_default_sites() {
        let sites = builtin_chat_sites();
        assert_eq!(sites.len(), 8);
        assert_eq!(sites[0].id.0, "chatgpt");
        assert_eq!(sites[1].id.0, "claude");
        assert_eq!(sites[7].id.0, "copilot");
    }

    #[test]
    fn normalize_url_adds_https_and_rejects_blank_or_invalid() {
        assert_eq!(
            normalize_chat_site_url("poe.com/chat").as_deref(),
            Some("https://poe.com/chat")
        );
        assert_eq!(
            normalize_chat_site_url("http://localhost:8080").as_deref(),
            Some("http://localhost:8080")
        );
        assert_eq!(normalize_chat_site_url("   "), None);
        assert_eq!(normalize_chat_site_url("not a url"), None);
        assert_eq!(normalize_chat_site_url("singleword"), None);
    }

    #[test]
    fn derives_readable_name_and_slug_for_custom_sites() {
        let (id, name) = derive_custom_site_identity("https://www.poe.com/chat", None);
        assert_eq!(id.0, "custom-poe-com");
        assert_eq!(name, "Poe");

        let (mistral_id, mistral_name) = derive_custom_site_identity("https://chat.mistral.ai", None);
        assert_eq!(mistral_id.0, "custom-chat-mistral-ai");
        assert_eq!(mistral_name, "Mistral");

        let (app_id, app_name) = derive_custom_site_identity("https://app.openrouter.ai/chat", None);
        assert_eq!(app_id.0, "custom-app-openrouter-ai");
        assert_eq!(app_name, "Openrouter");

        let (_, explicit) = derive_custom_site_identity("https://chat.local.dev", Some("Local LLM"));
        assert_eq!(explicit, "Local LLM");
    }

    #[test]
    fn normalize_https_rejects_plain_http() {
        assert_eq!(
            normalize_https_chat_site_url("chat.mistral.ai").as_deref(),
            Some("https://chat.mistral.ai")
        );
        assert_eq!(
            normalize_https_chat_site_url("https://chat.mistral.ai/chat").as_deref(),
            Some("https://chat.mistral.ai/chat")
        );
        assert_eq!(normalize_https_chat_site_url("http://chat.mistral.ai"), None);
        assert_eq!(normalize_https_chat_site_url("ftp://example.com"), None);
        assert_eq!(normalize_https_chat_site_url("not a url"), None);
    }
}
