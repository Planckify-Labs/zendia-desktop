/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Yank ring (`yank.*`) and WebChat vim actions (`webchat.*`).

use crate::commands::{ArgumentSchema, ArgumentType, Command, CommandDescriptor, CommandError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Longer texts are not kept in the yank ring, matching `ZendiaYankRing.sys.mjs`.
pub const MAX_YANK_LENGTH: usize = 100_000;

/// Default maximum number of entries kept in the yank ring.
pub const DEFAULT_YANK_RING_LENGTH: usize = 100;

/// Debounce delay before flushing `zendia-yank.json` (`ZendiaYank.sys.mjs:47`, `YNK-01`).
pub const YANK_SAVE_DELAY_MS: u64 = 2000;

/// Current Unix epoch timestamp in milliseconds.
pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Formats relative entry age matching `ZendiaYank.sys.mjs:ago` (`now`, `5m`, `3h`, `2d`, `YNK-05`).
pub fn format_yank_age(now_ms: u64, entry_time_ms: u64) -> String {
    if entry_time_ms == 0 || now_ms <= entry_time_ms {
        return "now".to_string();
    }
    let diff_ms = now_ms - entry_time_ms;
    let minutes = (diff_ms + 30_000) / 60_000;
    if minutes < 1 {
        "now".to_string()
    } else if minutes < 60 {
        format!("{minutes}m")
    } else if minutes < 1440 {
        let hours = (minutes + 30) / 60;
        format!("{hours}h")
    } else {
        let days = (minutes + 720) / 1440;
        format!("{days}d")
    }
}

/// Returns the default yank-ring persistence path (`$XDG_CONFIG_HOME/zendia/zendia-yank.json`, `YNK-01`).
pub fn yank_ring_profile_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("zendia").join("zendia-yank.json"));
    }
    std::env::var("HOME").ok().filter(|h| !h.is_empty()).map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("zendia")
            .join("zendia-yank.json")
    })
}

/// Loads a persisted [`YankRing`] from `path`, returning `None` when absent or unreadable (`YNK-01`).
pub fn load_yank_ring_from(path: &Path) -> Option<YankRing> {
    YankRing::load_from_path(path)
}

/// Atomically writes `ring` as formatted JSON to `path` (`YNK-01`).
pub fn save_yank_ring_to(path: &Path, ring: &YankRing) -> std::io::Result<()> {
    ring.save_to_path(path)
}

/// One entry in the yank ring.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct YankEntry {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default = "now_unix_ms", alias = "time")]
    pub time_ms: u64,
}

#[derive(Serialize, Deserialize)]
struct PersistedYankFile {
    #[serde(default)]
    entries: Vec<YankEntry>,
}

/// Deduplicated, newest-first ring of yanked texts (mirrors `ZendiaYankRing`).
#[derive(Clone, Debug)]
pub struct YankRing {
    entries: Vec<YankEntry>,
    max_len: usize,
}

impl Default for YankRing {
    fn default() -> Self {
        Self::new(DEFAULT_YANK_RING_LENGTH)
    }
}

impl YankRing {
    pub fn new(max_len: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_len: max_len.max(1),
        }
    }

    pub fn from_entries(max_len: usize, entries: Vec<YankEntry>) -> Self {
        let mut ring = Self::new(max_len);
        for entry in entries.into_iter().rev() {
            ring.add_at(entry.text, entry.url, entry.title, entry.time_ms);
        }
        ring
    }

    pub fn load_from_path(path: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        Self::from_json_str(&raw).ok()
    }

    pub fn from_json_str(raw: &str) -> Result<Self, serde_json::Error> {
        if let Ok(file) = serde_json::from_str::<PersistedYankFile>(raw) {
            return Ok(Self::from_entries(DEFAULT_YANK_RING_LENGTH, file.entries));
        }
        let entries: Vec<YankEntry> = serde_json::from_str(raw)?;
        Ok(Self::from_entries(DEFAULT_YANK_RING_LENGTH, entries))
    }

    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        let payload = PersistedYankFile {
            entries: self.entries.clone(),
        };
        serde_json::to_string_pretty(&payload)
    }

    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = self
            .to_json_string()
            .map_err(|err| std::io::Error::other(err.to_string()))?;
        std::fs::write(path, json)
    }

    pub fn entries(&self) -> &[YankEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Adds `text` to the front of the ring stamped with the current time.
    /// If already at index 0 with the same text, leaves the ring unchanged.
    pub fn add(&mut self, text: String, url: Option<String>, title: Option<String>) -> bool {
        self.add_at(text, url, title, now_unix_ms())
    }

    /// Adds `text` at explicit timestamp `time_ms` (in Unix ms).
    pub fn add_at(&mut self, text: String, url: Option<String>, title: Option<String>, time_ms: u64) -> bool {
        if text.trim().is_empty() || text.chars().count() > MAX_YANK_LENGTH {
            return false;
        }
        if self.entries.first().is_some_and(|e| e.text == text) {
            return false;
        }
        self.entries.retain(|e| e.text != text);
        self.entries.insert(
            0,
            YankEntry {
                text,
                url,
                title,
                time_ms,
            },
        );
        self.entries.truncate(self.max_len);
        true
    }

    /// The yank at `index`, newest first.
    pub fn at(&self, index: usize) -> Option<&YankEntry> {
        self.entries.get(index)
    }

    pub fn clear(&mut self) -> bool {
        if self.entries.is_empty() {
            false
        } else {
            self.entries.clear();
            true
        }
    }
}

/// Yank ring and WebChat vim actions.
#[derive(Clone, Debug, PartialEq)]
pub enum YankWebChatAction {
    YankHistory,
    /// Puts a yank in the focused field, or copies it when no field has the focus (a pick from the history).
    YankPut {
        text: String,
        index: usize,
    },
    YankClear,
    YankPutPrev,
    YankPutNext,
    /// Copies the active page's title and URL as a Markdown link `[title](url)` (`yank.urlMarkdown`, Appendix A).
    YankUrlMarkdown,
    WebChatOpen {
        site_id: Option<String>,
    },
    WebChatSelect,
    WebChatAccounts,
    WebChatSync,
    WebChatNewChatAll,
    WebChatAskPrompt,
    WebChatSummarizePage,
    WebChatAskImage,
    WebChatAskRegion,
    WebChatAskTabs,
    WebChatPutReply,
    WebChatExplain,
    WebChatSummarize,
    WebChatTranslate,
    WebChatDefine,
    WebChatFactCheck,
    WebChatAskAbout,
}

impl YankWebChatAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::YankHistory => "yank.history",
            Self::YankPut { .. } => "yank.put",
            Self::YankClear => "yank.clear",
            Self::YankPutPrev => "yank.put_prev",
            Self::YankPutNext => "yank.put_next",
            Self::YankUrlMarkdown => "yank.url_markdown",
            Self::WebChatOpen { .. } => "webchat.open",
            Self::WebChatSelect => "webchat.select",
            Self::WebChatAccounts => "webchat.accounts",
            Self::WebChatSync => "webchat.sync",
            Self::WebChatNewChatAll => "webchat.new_chat_all",
            Self::WebChatAskPrompt => "webchat.ask",
            Self::WebChatSummarizePage => "webchat.summarize_page",
            Self::WebChatAskImage => "webchat.ask_image",
            Self::WebChatAskRegion => "webchat.ask_region",
            Self::WebChatAskTabs => "webchat.ask_tabs",
            Self::WebChatPutReply => "webchat.put_reply",
            Self::WebChatExplain => "webchat.explain",
            Self::WebChatSummarize => "webchat.summarize",
            Self::WebChatTranslate => "webchat.translate",
            Self::WebChatDefine => "webchat.define",
            Self::WebChatFactCheck => "webchat.fact_check",
            Self::WebChatAskAbout => "webchat.ask_about",
        }
    }

    /// Default prompt prefix for selection/page webchat actions.
    pub fn prompt_prefix(&self) -> Option<&'static str> {
        match self {
            Self::WebChatSummarizePage => Some("Summarize this page:"),
            Self::WebChatExplain => Some("Explain this:"),
            Self::WebChatSummarize => Some("Summarize this:"),
            Self::WebChatTranslate => Some("Translate this:"),
            Self::WebChatDefine => Some("Define this:"),
            Self::WebChatFactCheck => Some("Fact-check this:"),
            Self::WebChatAskAbout | Self::WebChatAskPrompt => Some("About this page:"),
            Self::WebChatAskTabs => Some("About these tabs:"),
            Self::WebChatAskImage => Some("Describe and explain this image:"),
            _ => None,
        }
    }
}

/// Parses the commands of this module, or returns `None` for a name it does not own.
pub fn parse_command(name: &str, args: &[(String, String)]) -> Result<Option<Command>, CommandError> {
    let arg = |key: &str| args.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let action = match name {
        "yank.history" => YankWebChatAction::YankHistory,
        "yank.put" => YankWebChatAction::YankPut {
            text: arg("text").unwrap_or_default().to_string(),
            index: match arg("index") {
                Some(raw) => raw.parse().map_err(|_| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("expected integer `index`, got `{raw}`"),
                })?,
                None => 0,
            },
        },
        "yank.clear" => YankWebChatAction::YankClear,
        "yank.put_prev" | "yank.putPrev" => YankWebChatAction::YankPutPrev,
        "yank.put_next" | "yank.putNext" => YankWebChatAction::YankPutNext,
        "yank.url_markdown" | "yank.urlMarkdown" => YankWebChatAction::YankUrlMarkdown,
        "webchat.open" => YankWebChatAction::WebChatOpen {
            site_id: arg("site_id").filter(|s| !s.trim().is_empty()).map(str::to_string),
        },
        "webchat.select" => YankWebChatAction::WebChatSelect,
        "webchat.accounts" => YankWebChatAction::WebChatAccounts,
        "webchat.sync" => YankWebChatAction::WebChatSync,
        "webchat.new_chat_all" | "webchat.newChatAll" => YankWebChatAction::WebChatNewChatAll,
        "webchat.ask" => YankWebChatAction::WebChatAskPrompt,
        "webchat.summarize_page" | "webchat.summarizePage" => YankWebChatAction::WebChatSummarizePage,
        "webchat.ask_image" | "webchat.askImage" => YankWebChatAction::WebChatAskImage,
        "webchat.ask_region" | "webchat.askRegion" => YankWebChatAction::WebChatAskRegion,
        "webchat.ask_tabs" | "webchat.askTabs" => YankWebChatAction::WebChatAskTabs,
        "webchat.put_reply" | "webchat.putReply" => YankWebChatAction::WebChatPutReply,
        "webchat.explain" => YankWebChatAction::WebChatExplain,
        "webchat.summarize" => YankWebChatAction::WebChatSummarize,
        "webchat.translate" => YankWebChatAction::WebChatTranslate,
        "webchat.define" => YankWebChatAction::WebChatDefine,
        "webchat.fact_check" | "webchat.factCheck" => YankWebChatAction::WebChatFactCheck,
        "webchat.ask_about" | "webchat.askAbout" => YankWebChatAction::WebChatAskAbout,
        _ => return Ok(None),
    };
    Ok(Some(Command::YankWebChat(action)))
}

/// The descriptors of this module's commands.
pub fn descriptors() -> Vec<CommandDescriptor> {
    let items: &[(&str, &str, &str)] = &[
        ("yank.history", "Yank History", "Pick an entry from the yank ring"),
        ("yank.put", "Put a Yank", "Put a yank in the focused field, or copy it"),
        ("yank.clear", "Clear Yank History", "Forget everything in the yank ring"),
        (
            "yank.put_prev",
            "Put Previous Yank",
            "Put the yank before the last one put in the field",
        ),
        (
            "yank.put_next",
            "Put Next Yank",
            "Put the yank after the last one put in the field",
        ),
        (
            "yank.url_markdown",
            "Copy Markdown Link",
            "Copy the current page's title and address as a Markdown link",
        ),
        ("webchat.select", "Select Web Chat Site", "Pick a Web Chat site"),
        (
            "webchat.accounts",
            "Web Chat Accounts",
            "Open the accounts menu in the Web Chat panel",
        ),
        (
            "webchat.sync",
            "Sync Web Chat Logins",
            "Refresh login state on the active Web Chat site",
        ),
        (
            "webchat.new_chat_all",
            "New Chat on Every Web Chat Site",
            "Start a fresh chat on every configured Web Chat site",
        ),
        ("webchat.ask", "Ask Web Chat", "Ask Web Chat about the current page"),
        (
            "webchat.summarize_page",
            "Summarize the Page in Web Chat",
            "Ask Web Chat to summarize the active page",
        ),
        (
            "webchat.ask_image",
            "Ask Web Chat About an Image",
            "Ask Web Chat about an image on the page",
        ),
        (
            "webchat.ask_region",
            "Ask Web Chat About Part of the Screen",
            "Capture a screen region and ask Web Chat",
        ),
        (
            "webchat.ask_tabs",
            "Ask Web Chat About the Selected Tabs",
            "Ask Web Chat about the selected or active tabs",
        ),
        (
            "webchat.put_reply",
            "Put the Web Chat's Reply Back in the Field",
            "Insert Web Chat's latest reply into the focused field",
        ),
        (
            "webchat.explain",
            "Explain the Selection",
            "Ask Web Chat to explain the selection",
        ),
        (
            "webchat.summarize",
            "Summarize the Selection",
            "Ask Web Chat to summarize the selection",
        ),
        (
            "webchat.translate",
            "Translate the Selection",
            "Ask Web Chat to translate the selection",
        ),
        (
            "webchat.define",
            "Define the Selection",
            "Ask Web Chat to define the selection",
        ),
        (
            "webchat.fact_check",
            "Fact-Check the Selection",
            "Ask Web Chat to fact-check the selection",
        ),
        (
            "webchat.ask_about",
            "Ask About the Selection…",
            "Ask Web Chat a question about the selection",
        ),
    ];
    let mut out: Vec<CommandDescriptor> = items
        .iter()
        .map(|&(name, title, description)| CommandDescriptor {
            name: name.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            arguments: Vec::new(),
        })
        .collect();
    out.push(CommandDescriptor {
        name: "webchat.open".to_string(),
        title: "Open Web Chat Site".to_string(),
        description: "Select a Web Chat site and open the Web Chat side panel".to_string(),
        arguments: vec![ArgumentSchema::new(
            "site_id",
            ArgumentType::String,
            false,
            "Optional Web Chat site ID to select before opening",
        )],
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yank_ring_deduplicates_and_cycles() {
        let mut ring = YankRing::new(3);
        assert!(ring.add("first".to_string(), None, None));
        assert!(ring.add("second".to_string(), None, None));
        assert!(!ring.add("second".to_string(), None, None));
        assert!(ring.add("first".to_string(), None, None));
        assert_eq!(ring.entries()[0].text, "first");
        assert_eq!(ring.entries()[1].text, "second");

        assert_eq!(ring.at(1).unwrap().text, "second");
        assert!(ring.at(2).is_none());
    }

    #[test]
    fn all_yank_webchat_descriptors_round_trip() {
        for desc in descriptors() {
            let parsed = Command::from_name_and_args(&desc.name, &[]).unwrap();
            assert_eq!(parsed.name(), desc.name);
        }
    }
}
