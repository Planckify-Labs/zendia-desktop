/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! First-run onboarding state, seven-page catalog, choice staging, profile persistence, and command descriptors.
//!
//! All onboarding state lives in `BrowserCore` so both the Freya UI and headless/test runners drive it exclusively
//! through named commands and events.

use crate::address_bar::SearchEngine;
use crate::commands::{ArgumentSchema, ArgumentType, CommandDescriptor};
use crate::model::InputMode;
use std::path::{Path, PathBuf};

/// The seven pages of Zendia's onboarding flow in order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OnboardingPage {
    #[default]
    Start = 0,
    Keys = 1,
    Agents = 2,
    Shield = 3,
    MoveIn = 4,
    Apps = 5,
    Go = 6,
}

impl OnboardingPage {
    /// All seven pages in order, starting with the Living Emblem start page.
    pub const ALL: [Self; 7] = [
        Self::Start,
        Self::Keys,
        Self::Agents,
        Self::Shield,
        Self::MoveIn,
        Self::Apps,
        Self::Go,
    ];

    /// The six numbered setup chapters following the start page.
    pub const CHAPTERS: [Self; 6] = [
        Self::Keys,
        Self::Agents,
        Self::Shield,
        Self::MoveIn,
        Self::Apps,
        Self::Go,
    ];

    /// Canonical command/serialization slug for this page.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Keys => "vim",
            Self::Agents => "agents",
            Self::Shield => "shield",
            Self::MoveIn => "move-in",
            Self::Apps => "apps",
            Self::Go => "go",
        }
    }

    /// Parses a page slug (accepting both `vim`/`keys`, `agents`/`agent`, `move-in`/`move_in`).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "start" => Some(Self::Start),
            "vim" | "keys" => Some(Self::Keys),
            "agents" | "agent" => Some(Self::Agents),
            "shield" => Some(Self::Shield),
            "move-in" | "move_in" | "movein" => Some(Self::MoveIn),
            "apps" | "essentials" => Some(Self::Apps),
            "go" | "finish" => Some(Self::Go),
            _ => None,
        }
    }

    /// Zero-based index across all seven pages (`0..=6`).
    pub fn index(self) -> usize {
        self as usize
    }

    /// Converts a zero-based page index back to an [`OnboardingPage`].
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    /// Next page in sequence, or `None` when on `Go`.
    pub fn next(self) -> Option<Self> {
        Self::from_index(self.index() + 1)
    }

    /// Previous chapter when stepping backward inside the flow (stops at `Keys`, never jumps back to `Start`).
    pub fn prev_chapter(self) -> Option<Self> {
        match self {
            Self::Start | Self::Keys => None,
            _ => Self::from_index(self.index() - 1),
        }
    }

    /// Two-digit chapter number shown in the top stepper (`"01"`..=`"06"`), or `None` on `Start`.
    pub fn chapter_number(self) -> Option<&'static str> {
        match self {
            Self::Start => None,
            Self::Keys => Some("01"),
            Self::Agents => Some("02"),
            Self::Shield => Some("03"),
            Self::MoveIn => Some("04"),
            Self::Apps => Some("05"),
            Self::Go => Some("06"),
        }
    }

    /// Short chapter label shown in the top stepper.
    pub fn chapter_label(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::Keys => "Keys",
            Self::Agents => "Agent",
            Self::Shield => "Shield",
            Self::MoveIn => "Move in",
            Self::Apps => "Apps",
            Self::Go => "Go",
        }
    }

    /// The Voronoi mascot mode pose paired with this page.
    pub fn pose(self) -> InputMode {
        match self {
            Self::Start | Self::Keys => InputMode::Normal,
            Self::Agents => InputMode::Explorer,
            Self::Shield => InputMode::Toolbar,
            Self::MoveIn => InputMode::Library,
            Self::Apps => InputMode::Insert,
            Self::Go => InputMode::Visual,
        }
    }

    /// Mode badge name displayed above the chapter headline.
    pub fn mode_tag(self) -> &'static str {
        match self.pose() {
            InputMode::Normal | InputMode::Window => "NORMAL",
            InputMode::Insert => "INSERT",
            InputMode::Visual => "VISUAL",
            InputMode::Explorer => "EXPLORER",
            InputMode::Toolbar => "TOOLBAR",
            InputMode::Library => "LIBRARY",
        }
    }

    /// Whether this chapter's 2D stage runs a looping animation with a Pause/Play toggle.
    pub fn is_looping(self) -> bool {
        matches!(self, Self::Keys | Self::Agents | Self::Shield | Self::MoveIn)
    }

    /// Whether this chapter offers a secondary Skip action beside Next.
    pub fn has_skip(self) -> bool {
        matches!(self, Self::Apps)
    }
}

/// Which of the three groups on the Move in page is currently driving the live preview.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MoveInFocus {
    #[default]
    Import,
    Search,
    DefaultBrowser,
}

/// One profile inside an importable browser source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportProfile {
    pub id: String,
    pub name: String,
}

/// A detected browser whose bookmarks, history, and passwords can be imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportSource {
    pub key: String,
    pub name: String,
    pub icon: String,
    pub profiles: Vec<ImportProfile>,
}

/// An AI coding agent entry displayed on the Agent chapter (installed or missing with CLI install hint).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnboardingAgentInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub icon: String,
    pub accent_rgb: (u8, u8, u8),
    pub installed: bool,
    pub install_command: Option<String>,
}

/// One preset web app offered on the Apps (Essentials) chapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EssentialAppPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub icon: &'static str,
    pub color_hex: &'static str,
    pub color_rgb: (u8, u8, u8),
}

/// Built-in preset apps offered on the Apps chapter, matching the fork's order and brand colors.
pub const ESSENTIAL_APP_PRESETS: &[EssentialAppPreset] = &[
    EssentialAppPreset {
        id: "obsidian",
        name: "Obsidian",
        url: "https://obsidian.md",
        icon: "obsidian",
        color_hex: "#7c3aed",
        color_rgb: (0x7c, 0x3a, 0xed),
    },
    EssentialAppPreset {
        id: "discord",
        name: "Discord",
        url: "https://discord.com",
        icon: "discord",
        color_hex: "#5865f2",
        color_rgb: (0x58, 0x65, 0xf2),
    },
    EssentialAppPreset {
        id: "trello",
        name: "Trello",
        url: "https://trello.com",
        icon: "trello",
        color_hex: "#0052cc",
        color_rgb: (0x00, 0x52, 0xcc),
    },
    EssentialAppPreset {
        id: "slack",
        name: "Slack",
        url: "https://slack.com",
        icon: "slack",
        color_hex: "#e01e5a",
        color_rgb: (0xe0, 0x1e, 0x5a),
    },
    EssentialAppPreset {
        id: "github",
        name: "GitHub",
        url: "https://github.com",
        icon: "github",
        color_hex: "#57606a",
        color_rgb: (0x57, 0x60, 0x6a),
    },
    EssentialAppPreset {
        id: "tuta",
        name: "Tuta",
        url: "https://app.tuta.com/",
        icon: "tuta",
        color_hex: "#c00002",
        color_rgb: (0xc0, 0x00, 0x02),
    },
    EssentialAppPreset {
        id: "notion",
        name: "Notion",
        url: "https://notion.com",
        icon: "notion",
        color_hex: "#37352f",
        color_rgb: (0x37, 0x35, 0x2f),
    },
    EssentialAppPreset {
        id: "calendar",
        name: "Google Calendar",
        url: "https://calendar.google.com",
        icon: "calendar",
        color_hex: "#4285f4",
        color_rgb: (0x42, 0x85, 0xf4),
    },
    EssentialAppPreset {
        id: "figma",
        name: "Figma",
        url: "https://figma.com",
        icon: "figma",
        color_hex: "#f24e1e",
        color_rgb: (0xf2, 0x4e, 0x1e),
    },
];

/// Looks up an [`EssentialAppPreset`] by URL or short id.
pub fn find_essential_preset(url_or_id: &str) -> Option<&'static EssentialAppPreset> {
    let trimmed = url_or_id.trim();
    ESSENTIAL_APP_PRESETS
        .iter()
        .find(|app| app.url.eq_ignore_ascii_case(trimmed) || app.id.eq_ignore_ascii_case(trimmed))
}

/// The three primary search engine presets displayed on the Move in chapter.
pub fn onboarding_search_engines() -> Vec<SearchEngine> {
    let builtins = SearchEngine::builtin_engines();
    let preferred = ["Google", "Bing", "DuckDuckGo"];
    let mut engines = Vec::with_capacity(preferred.len());
    for name in preferred {
        if let Some(engine) = builtins.iter().find(|item| item.name == name) {
            engines.push(engine.clone());
        }
    }
    engines
}

/// Resolves a search engine name into a [`SearchEngine`] from Zendia's built-in presets.
pub fn resolve_search_engine_by_name(name: &str) -> Option<SearchEngine> {
    let trimmed = name.trim();
    SearchEngine::builtin_engines()
        .into_iter()
        .find(|engine| engine.name.eq_ignore_ascii_case(trimmed))
}

/// Sample detected import sources (Chrome) matching the reference onboarding state.
pub fn sample_import_sources() -> Vec<ImportSource> {
    vec![ImportSource {
        key: "chrome".to_string(),
        name: "Chrome".to_string(),
        icon: "chrome".to_string(),
        profiles: vec![ImportProfile {
            id: "Default".to_string(),
            name: "Default".to_string(),
        }],
    }]
}

/// Sample import sources with multiple profiles for testing profile sub-pills.
pub fn sample_multi_profile_import_sources() -> Vec<ImportSource> {
    vec![
        ImportSource {
            key: "chrome".to_string(),
            name: "Chrome".to_string(),
            icon: "chrome".to_string(),
            profiles: vec![
                ImportProfile {
                    id: "Default".to_string(),
                    name: "Personal".to_string(),
                },
                ImportProfile {
                    id: "Profile 1".to_string(),
                    name: "Work".to_string(),
                },
            ],
        },
        ImportSource {
            key: "firefox".to_string(),
            name: "Firefox".to_string(),
            icon: "firefox".to_string(),
            profiles: vec![ImportProfile {
                id: "default-release".to_string(),
                name: "default-release".to_string(),
            }],
        },
    ]
}

/// Sample coding agent catalog matching the reference onboarding Agent page.
pub fn sample_agent_catalog() -> Vec<OnboardingAgentInfo> {
    vec![
        OnboardingAgentInfo {
            id: "claude-code".to_string(),
            name: "Claude Code".to_string(),
            version: String::new(),
            icon: "agent-claude".to_string(),
            accent_rgb: (0xd9, 0x77, 0x57),
            installed: true,
            install_command: None,
        },
        OnboardingAgentInfo {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            version: String::new(),
            icon: "agent-codex".to_string(),
            accent_rgb: (0x10, 0xa3, 0x7f),
            installed: true,
            install_command: None,
        },
        OnboardingAgentInfo {
            id: "antigravity".to_string(),
            name: "Antigravity".to_string(),
            version: String::new(),
            icon: "agent-antigravity".to_string(),
            accent_rgb: (0x42, 0x85, 0xf4),
            installed: true,
            install_command: None,
        },
        OnboardingAgentInfo {
            id: "opencode".to_string(),
            name: "OpenCode".to_string(),
            version: String::new(),
            icon: "agent-opencode".to_string(),
            accent_rgb: (0xf5, 0xa7, 0x42),
            installed: true,
            install_command: None,
        },
        OnboardingAgentInfo {
            id: "kiro".to_string(),
            name: "Kiro".to_string(),
            version: String::new(),
            icon: "agent-kiro".to_string(),
            accent_rgb: (0x90, 0x46, 0xff),
            installed: true,
            install_command: None,
        },
        OnboardingAgentInfo {
            id: "cursor".to_string(),
            name: "Cursor".to_string(),
            version: String::new(),
            icon: "agent-cursor".to_string(),
            accent_rgb: (0xa3, 0xa3, 0xa3),
            installed: false,
            install_command: Some("curl https://cursor.com/install -fsS | bash".to_string()),
        },
    ]
}

/// Staged user choices across the onboarding chapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnboardingChoices {
    pub vim_enabled: bool,
    pub agent_id: Option<String>,
    pub shield_enabled: bool,
    pub import_source_key: Option<String>,
    pub import_profile_id: Option<String>,
    pub search_engine: String,
    pub default_browser: bool,
    pub essential_urls: Vec<String>,
}

impl Default for OnboardingChoices {
    fn default() -> Self {
        Self {
            vim_enabled: false,
            agent_id: Some("claude-code".to_string()),
            shield_enabled: true,
            import_source_key: None,
            import_profile_id: None,
            search_engine: "Google".to_string(),
            default_browser: false,
            essential_urls: Vec::new(),
        }
    }
}

/// First-run onboarding state owned by `BrowserCore` and read by views.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnboardingState {
    /// Profile setting: `true` once the user has finished or skipped onboarding.
    pub seen: bool,
    /// Whether the onboarding flow is currently open/active.
    pub active: bool,
    /// Current onboarding page.
    pub page: OnboardingPage,
    /// Highest page reached so the top chapter stepper unlocks visited chapters.
    pub reached: OnboardingPage,
    /// Last navigation direction (`1` forward, `-1` backward) for directional transitions.
    pub direction: i8,
    /// Choices staged across the six chapters.
    pub choices: OnboardingChoices,
    /// Detected browsers offered on the Move in page.
    pub import_sources: Vec<ImportSource>,
    /// Coding agents offered on the Agent page (installed and missing).
    pub agent_catalog: Vec<OnboardingAgentInfo>,
    /// True while probing the host for installed coding agent CLIs.
    pub probing_agents: bool,
    /// Last stub-logged import invocation `(source_key, profile_id)`.
    pub last_import: Option<(String, Option<String>)>,
    /// Whether `import.set_default_browser` was invoked with `true`.
    pub default_browser_applied: bool,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self::default_seen()
    }
}

impl OnboardingState {
    /// Completed state used for `sample_model()` and already-initialized profiles so older gallery sections stay
    /// unobstructed.
    pub fn default_seen() -> Self {
        Self {
            seen: true,
            active: false,
            page: OnboardingPage::Start,
            reached: OnboardingPage::Go,
            direction: 1,
            choices: OnboardingChoices::default(),
            import_sources: sample_import_sources(),
            agent_catalog: sample_agent_catalog(),
            probing_agents: false,
            last_import: None,
            default_browser_applied: false,
        }
    }

    /// Fresh first-run state where onboarding has not been seen yet and starts on the Start page.
    pub fn first_run() -> Self {
        Self {
            seen: false,
            active: true,
            page: OnboardingPage::Start,
            reached: OnboardingPage::Start,
            direction: 1,
            choices: OnboardingChoices::default(),
            import_sources: sample_import_sources(),
            agent_catalog: sample_agent_catalog(),
            probing_agents: false,
            last_import: None,
            default_browser_applied: false,
        }
    }

    /// Returns the currently selected installed agent from [`Self::agent_catalog`].
    pub fn selected_agent(&self) -> Option<&OnboardingAgentInfo> {
        let id = self.choices.agent_id.as_deref()?;
        self.agent_catalog
            .iter()
            .find(|agent| agent.installed && agent.id == id)
    }

    /// Returns the currently selected [`ImportSource`], if any.
    pub fn selected_import_source(&self) -> Option<&ImportSource> {
        let key = self.choices.import_source_key.as_deref()?;
        self.import_sources.iter().find(|source| source.key == key)
    }

    /// Returns the preset metadata for each currently selected Essential app URL, in selection order.
    pub fn selected_essential_presets(&self) -> Vec<&'static EssentialAppPreset> {
        self.choices
            .essential_urls
            .iter()
            .filter_map(|url| find_essential_preset(url))
            .collect()
    }

    /// Advances `reached` to cover `target`, updates `direction`, and moves `page` to `target`.
    pub fn navigate_to(&mut self, target: OnboardingPage) {
        self.direction = if target.index() < self.page.index() { -1 } else { 1 };
        self.page = target;
        if target > self.reached {
            self.reached = target;
        }
    }

    /// Builds a persistence record suitable for saving to the user's XDG profile directory.
    pub fn to_profile_record(&self) -> OnboardingProfileRecord {
        OnboardingProfileRecord {
            seen: self.seen,
            page: self.page,
            reached: self.reached,
            vim_enabled: self.choices.vim_enabled,
            shield_enabled: self.choices.shield_enabled,
            agent_id: self.choices.agent_id.clone(),
            search_engine: self.choices.search_engine.clone(),
            default_browser: self.choices.default_browser,
            essential_urls: self.choices.essential_urls.clone(),
        }
    }

    /// Applies a loaded [`OnboardingProfileRecord`] onto this state.
    pub fn apply_profile_record(&mut self, record: &OnboardingProfileRecord) {
        self.seen = record.seen;
        self.active = !record.seen;
        self.page = record.page;
        self.reached = record.reached.max(record.page);
        self.choices.vim_enabled = record.vim_enabled;
        self.choices.shield_enabled = record.shield_enabled;
        self.choices.agent_id = record.agent_id.clone();
        if !record.search_engine.is_empty() {
            self.choices.search_engine = record.search_engine.clone();
        }
        self.choices.default_browser = record.default_browser;
        self.choices.essential_urls = record.essential_urls.clone();
    }
}

/// Serializable profile snapshot persisted under `<XDG_CONFIG_HOME>/zendia/onboarding.conf`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnboardingProfileRecord {
    pub seen: bool,
    pub page: OnboardingPage,
    pub reached: OnboardingPage,
    pub vim_enabled: bool,
    pub shield_enabled: bool,
    pub agent_id: Option<String>,
    pub search_engine: String,
    pub default_browser: bool,
    pub essential_urls: Vec<String>,
}

impl Default for OnboardingProfileRecord {
    fn default() -> Self {
        Self {
            seen: false,
            page: OnboardingPage::Start,
            reached: OnboardingPage::Start,
            vim_enabled: false,
            shield_enabled: true,
            agent_id: Some("claude-code".to_string()),
            search_engine: "Google".to_string(),
            default_browser: false,
            essential_urls: Vec::new(),
        }
    }
}

impl OnboardingProfileRecord {
    /// Serializes this record as plain key=value lines.
    pub fn serialize(&self) -> String {
        let agent = self.agent_id.as_deref().unwrap_or("");
        let essentials = self.essential_urls.join(",");
        format!(
            "seen={}\npage={}\nreached={}\nvim_enabled={}\nshield_enabled={}\nagent_id={}\nsearch_engine={}\ndefault_browser={}\nessentials={}\n",
            self.seen,
            self.page.as_str(),
            self.reached.as_str(),
            self.vim_enabled,
            self.shield_enabled,
            agent,
            self.search_engine,
            self.default_browser,
            essentials,
        )
    }

    /// Parses a record previously written by [`Self::serialize`].
    pub fn parse(raw: &str) -> Self {
        let mut record = Self::default();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "seen" => record.seen = value.eq_ignore_ascii_case("true"),
                "page" => {
                    if let Some(page) = OnboardingPage::parse(value) {
                        record.page = page;
                    }
                }
                "reached" => {
                    if let Some(page) = OnboardingPage::parse(value) {
                        record.reached = page;
                    }
                }
                "vim_enabled" => record.vim_enabled = value.eq_ignore_ascii_case("true"),
                "shield_enabled" => record.shield_enabled = value.eq_ignore_ascii_case("true"),
                "agent_id" => {
                    record.agent_id = if value.is_empty() {
                        None
                    } else {
                        Some(value.to_string())
                    };
                }
                "search_engine" => {
                    if !value.is_empty() {
                        record.search_engine = value.to_string();
                    }
                }
                "default_browser" => record.default_browser = value.eq_ignore_ascii_case("true"),
                "essentials" => {
                    record.essential_urls = value
                        .split(',')
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(str::to_string)
                        .collect();
                }
                _ => {}
            }
        }
        record
    }
}

/// Resolves the onboarding profile file path under `$XDG_CONFIG_HOME/zendia/onboarding.conf` (falling back to
/// `$HOME/.config/zendia/onboarding.conf`).
pub fn onboarding_profile_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.trim().is_empty()
    {
        return Some(PathBuf::from(xdg).join("zendia").join("onboarding.conf"));
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.trim().is_empty()
    {
        return Some(
            PathBuf::from(home)
                .join(".config")
                .join("zendia")
                .join("onboarding.conf"),
        );
    }
    None
}

/// Reads an [`OnboardingProfileRecord`] from `path`, returning `None` when the file does not exist (fresh profile).
pub fn load_profile_record_from(path: &Path) -> Option<OnboardingProfileRecord> {
    let contents = std::fs::read_to_string(path).ok()?;
    Some(OnboardingProfileRecord::parse(&contents))
}

/// Writes `record` to `path`, creating parent directories as needed.
pub fn save_profile_record_to(path: &Path, record: &OnboardingProfileRecord) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, record.serialize())
}

/// Command descriptors for onboarding, Shield, and browser import commands.
pub fn command_descriptors() -> Vec<CommandDescriptor> {
    vec![
        CommandDescriptor {
            name: "onboarding.open".to_string(),
            title: "Open Onboarding".to_string(),
            description: "Open the first-run onboarding overlay in this window".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "onboarding.restart".to_string(),
            title: "Show Onboarding".to_string(),
            description: "Restart the first-run onboarding flow from the Start page".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "onboarding.goto".to_string(),
            title: "Go to Onboarding Page".to_string(),
            description: "Jump to a specific onboarding page (start, vim, agents, shield, move-in, apps, go)"
                .to_string(),
            arguments: vec![ArgumentSchema::new(
                "page",
                ArgumentType::String,
                true,
                "Target page slug",
            )],
        },
        CommandDescriptor {
            name: "onboarding.next".to_string(),
            title: "Onboarding Next Page".to_string(),
            description: "Commit the current onboarding page's choice and advance to the next page".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "onboarding.back".to_string(),
            title: "Onboarding Previous Page".to_string(),
            description: "Step backward to the previous onboarding chapter".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "onboarding.finish".to_string(),
            title: "Finish Onboarding".to_string(),
            description: "Apply all onboarding choices, mark onboarding seen, and enter the browser".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "onboarding.skip".to_string(),
            title: "Skip Onboarding".to_string(),
            description: "Dismiss onboarding and mark first-run setup as seen".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "onboarding.skip_apps".to_string(),
            title: "Skip Onboarding Apps".to_string(),
            description: "Clear selected essential apps and advance from the Apps chapter to Go".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "onboarding.set_vim".to_string(),
            title: "Set Onboarding Vim Choice".to_string(),
            description: "Choose whether to enable Vim mode during onboarding".to_string(),
            arguments: vec![ArgumentSchema::new(
                "enabled",
                ArgumentType::Bool,
                true,
                "Enable Vim mode",
            )],
        },
        CommandDescriptor {
            name: "onboarding.set_agent".to_string(),
            title: "Set Onboarding Agent".to_string(),
            description: "Choose the default coding agent during onboarding".to_string(),
            arguments: vec![ArgumentSchema::new(
                "agent_id",
                ArgumentType::String,
                true,
                "Agent identifier",
            )],
        },
        CommandDescriptor {
            name: "onboarding.set_shield".to_string(),
            title: "Set Onboarding Shield Choice".to_string(),
            description: "Choose whether to keep uBlock Origin ad blocking enabled during onboarding".to_string(),
            arguments: vec![ArgumentSchema::new(
                "enabled",
                ArgumentType::Bool,
                true,
                "Enable ad blocking",
            )],
        },
        CommandDescriptor {
            name: "onboarding.set_import".to_string(),
            title: "Set Onboarding Import Source".to_string(),
            description: "Select a browser source and optional profile to import from, or clear to start fresh"
                .to_string(),
            arguments: vec![
                ArgumentSchema::new("source_key", ArgumentType::String, false, "Browser migrator key"),
                ArgumentSchema::new("profile_id", ArgumentType::String, false, "Source browser profile id"),
            ],
        },
        CommandDescriptor {
            name: "onboarding.set_search_engine".to_string(),
            title: "Set Onboarding Search Engine".to_string(),
            description: "Pick a default search engine during onboarding".to_string(),
            arguments: vec![ArgumentSchema::new(
                "name",
                ArgumentType::String,
                true,
                "Search engine name",
            )],
        },
        CommandDescriptor {
            name: "onboarding.set_default_browser".to_string(),
            title: "Set Onboarding Default Browser Choice".to_string(),
            description: "Choose whether Zendia should become the system default browser".to_string(),
            arguments: vec![ArgumentSchema::new(
                "enabled",
                ArgumentType::Bool,
                true,
                "Open external links in Zendia",
            )],
        },
        CommandDescriptor {
            name: "onboarding.toggle_essential".to_string(),
            title: "Toggle Onboarding Essential App".to_string(),
            description: "Pin or unpin a preset app on the Apps chapter".to_string(),
            arguments: vec![ArgumentSchema::new(
                "url",
                ArgumentType::String,
                true,
                "Preset app URL or identifier",
            )],
        },
        CommandDescriptor {
            name: "shield.set_enabled".to_string(),
            title: "Set Shield Ad Blocking".to_string(),
            description: "Enable or disable Zendia's built-in uBlock Origin ad shield".to_string(),
            arguments: vec![ArgumentSchema::new(
                "enabled",
                ArgumentType::Bool,
                true,
                "Enable ad blocking",
            )],
        },
        CommandDescriptor {
            name: "import.start".to_string(),
            title: "Start Browser Import".to_string(),
            description: "Import bookmarks, history, and passwords from another browser".to_string(),
            arguments: vec![
                ArgumentSchema::new("source_key", ArgumentType::String, true, "Browser migrator key"),
                ArgumentSchema::new("profile_id", ArgumentType::String, false, "Source browser profile id"),
            ],
        },
        CommandDescriptor {
            name: "import.set_default_browser".to_string(),
            title: "Set Default Browser".to_string(),
            description: "Request the operating system to make Zendia the default browser".to_string(),
            arguments: vec![ArgumentSchema::new(
                "enabled",
                ArgumentType::Bool,
                true,
                "Set Zendia as default browser",
            )],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_pages_are_ordered_and_round_trip_slugs() {
        assert_eq!(OnboardingPage::ALL.len(), 7);
        assert_eq!(OnboardingPage::CHAPTERS.len(), 6);
        assert_eq!(OnboardingPage::Start.next(), Some(OnboardingPage::Keys));
        assert_eq!(OnboardingPage::Go.next(), None);
        assert_eq!(OnboardingPage::Start.prev_chapter(), None);
        assert_eq!(OnboardingPage::Keys.prev_chapter(), None);
        assert_eq!(OnboardingPage::Agents.prev_chapter(), Some(OnboardingPage::Keys));
        assert_eq!(OnboardingPage::Go.prev_chapter(), Some(OnboardingPage::Apps));

        for (index, page) in OnboardingPage::ALL.iter().enumerate() {
            assert_eq!(page.index(), index);
            assert_eq!(OnboardingPage::from_index(index), Some(*page));
            assert_eq!(OnboardingPage::parse(page.as_str()), Some(*page));
        }
        assert_eq!(OnboardingPage::parse("keys"), Some(OnboardingPage::Keys));
        assert_eq!(OnboardingPage::parse("agent"), Some(OnboardingPage::Agents));
        assert_eq!(OnboardingPage::parse("move_in"), Some(OnboardingPage::MoveIn));
        assert_eq!(OnboardingPage::parse("unknown"), None);
    }

    #[test]
    fn navigation_tracks_direction_and_furthest_page_reached() {
        let mut state = OnboardingState::first_run();
        assert!(!state.seen);
        assert_eq!(state.page, OnboardingPage::Start);
        assert_eq!(state.reached, OnboardingPage::Start);

        state.navigate_to(OnboardingPage::Keys);
        assert_eq!(state.direction, 1);
        assert_eq!(state.reached, OnboardingPage::Keys);

        state.navigate_to(OnboardingPage::Shield);
        assert_eq!(state.direction, 1);
        assert_eq!(state.reached, OnboardingPage::Shield);

        state.navigate_to(OnboardingPage::Agents);
        assert_eq!(state.direction, -1);
        assert_eq!(state.page, OnboardingPage::Agents);
        assert_eq!(state.reached, OnboardingPage::Shield);
    }

    #[test]
    fn profile_record_round_trips_serialization_and_disk_io() {
        let record = OnboardingProfileRecord {
            seen: true,
            page: OnboardingPage::Go,
            reached: OnboardingPage::Go,
            vim_enabled: true,
            shield_enabled: false,
            agent_id: Some("codex".to_string()),
            search_engine: "DuckDuckGo".to_string(),
            default_browser: true,
            essential_urls: vec!["https://github.com".to_string(), "https://discord.com".to_string()],
        };

        let serialized = record.serialize();
        let parsed = OnboardingProfileRecord::parse(&serialized);
        assert_eq!(parsed, record);

        let unique = std::env::temp_dir().join(format!("zendia-onboarding-test-{}.conf", std::process::id()));
        save_profile_record_to(&unique, &record).unwrap();
        let loaded = load_profile_record_from(&unique).unwrap();
        assert_eq!(loaded, record);
        let _ = std::fs::remove_file(&unique);
    }

    #[test]
    fn preset_catalogs_cover_expected_apps_and_engines() {
        assert_eq!(ESSENTIAL_APP_PRESETS.len(), 9);
        assert_eq!(find_essential_preset("github").unwrap().name, "GitHub");
        assert_eq!(find_essential_preset("https://obsidian.md").unwrap().name, "Obsidian");
        assert_eq!(onboarding_search_engines().len(), 3);
        assert_eq!(resolve_search_engine_by_name("DuckDuckGo").unwrap().name, "DuckDuckGo");
    }
}
