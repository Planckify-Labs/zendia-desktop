/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Yank ring (`yank.*`) and WebChat vim actions (`webchat.*`).

use crate::commands::{Command, CommandDescriptor, CommandError};

/// Longer texts are not kept in the yank ring, matching `ZendiaYankRing.sys.mjs`.
pub const MAX_YANK_LENGTH: usize = 100_000;

/// Default maximum number of entries kept in the yank ring.
pub const DEFAULT_YANK_RING_LENGTH: usize = 100;

/// One entry in the yank ring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YankEntry {
    pub text: String,
    pub url: Option<String>,
    pub title: Option<String>,
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

    pub fn entries(&self) -> &[YankEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Adds `text` to the front of the ring. If already present, moves it to the front.
    /// Returns `true` if the ring changed.
    pub fn add(&mut self, text: String, url: Option<String>, title: Option<String>) -> bool {
        if text.trim().is_empty() || text.len() > MAX_YANK_LENGTH {
            return false;
        }
        if self.entries.first().is_some_and(|e| e.text == text) {
            return false;
        }
        self.entries.retain(|e| e.text != text);
        self.entries.insert(0, YankEntry { text, url, title });
        self.entries.truncate(self.max_len);
        true
    }

    /// The yank at `index`, newest first.
    pub fn at(&self, index: usize) -> Option<&YankEntry> {
        self.entries.get(index)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
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
    WebChatSelect,
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
            Self::WebChatSelect => "webchat.select",
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
        "yank.put_prev" => YankWebChatAction::YankPutPrev,
        "yank.put_next" => YankWebChatAction::YankPutNext,
        "webchat.select" => YankWebChatAction::WebChatSelect,
        "webchat.new_chat_all" => YankWebChatAction::WebChatNewChatAll,
        "webchat.summarize_page" => YankWebChatAction::WebChatSummarizePage,
        "webchat.ask_image" => YankWebChatAction::WebChatAskImage,
        "webchat.ask_region" => YankWebChatAction::WebChatAskRegion,
        "webchat.ask_tabs" => YankWebChatAction::WebChatAskTabs,
        "webchat.put_reply" => YankWebChatAction::WebChatPutReply,
        "webchat.explain" => YankWebChatAction::WebChatExplain,
        "webchat.summarize" => YankWebChatAction::WebChatSummarize,
        "webchat.translate" => YankWebChatAction::WebChatTranslate,
        "webchat.define" => YankWebChatAction::WebChatDefine,
        "webchat.fact_check" => YankWebChatAction::WebChatFactCheck,
        "webchat.ask_about" => YankWebChatAction::WebChatAskAbout,
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
        ("webchat.select", "Select Web Chat Site", "Pick a Web Chat site"),
        (
            "webchat.new_chat_all",
            "New Chat on Every Web Chat Site",
            "Start a fresh chat on every configured Web Chat site",
        ),
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
            "Explain Selection",
            "Ask Web Chat to explain the selection",
        ),
        (
            "webchat.summarize",
            "Summarize Selection",
            "Ask Web Chat to summarize the selection",
        ),
        (
            "webchat.translate",
            "Translate Selection",
            "Ask Web Chat to translate the selection",
        ),
        (
            "webchat.define",
            "Define Selection",
            "Ask Web Chat to define the selection",
        ),
        (
            "webchat.fact_check",
            "Fact Check Selection",
            "Ask Web Chat to fact-check the selection",
        ),
        (
            "webchat.ask_about",
            "Ask About Selection",
            "Ask Web Chat a question about the selection",
        ),
    ];
    items
        .iter()
        .map(|&(name, title, description)| CommandDescriptor {
            name: name.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            arguments: Vec::new(),
        })
        .collect()
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
