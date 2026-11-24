/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Named, typed browser commands and their descriptors for keymaps and the command palette.

use crate::contributions::SidePanelId;
use crate::model::{
    AgentId, ChatSiteId, ConversationId, SidebarSide, SpaceId, SplitDirection, TabId, ThemeOverrides, ThemeScheme,
    ToolbarLayout,
};
use std::fmt;

/// Context-menu action applied to an HTML audio or video element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaAction {
    TogglePlay,
    ToggleMute,
    ToggleControls,
    ToggleLoop,
    TogglePictureInPicture,
    ToggleFullscreen,
}

impl MediaAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TogglePlay => "toggle_play",
            Self::ToggleMute => "toggle_mute",
            Self::ToggleControls => "toggle_controls",
            Self::ToggleLoop => "toggle_loop",
            Self::TogglePictureInPicture => "toggle_pip",
            Self::ToggleFullscreen => "toggle_fullscreen",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "toggle_play" => Some(Self::TogglePlay),
            "toggle_mute" => Some(Self::ToggleMute),
            "toggle_controls" => Some(Self::ToggleControls),
            "toggle_loop" => Some(Self::ToggleLoop),
            "toggle_pip" => Some(Self::TogglePictureInPicture),
            "toggle_fullscreen" => Some(Self::ToggleFullscreen),
            _ => None,
        }
    }
}

/// Every action a view, keymap, palette, plugin, extension, or agent asks the browser core to perform.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    TabNew {
        url: Option<String>,
        after: Option<TabId>,
        activate: bool,
    },
    TabClose {
        tab_id: Option<TabId>,
        /// When true, close immediately without waiting for page confirmation.
        force: bool,
    },
    TabActivate {
        tab_id: TabId,
    },
    TabNext,
    TabPrevious,
    LocationFocus,
    NavigationBack {
        tab_id: Option<TabId>,
    },
    NavigationForward {
        tab_id: Option<TabId>,
    },
    NavigationReload {
        tab_id: Option<TabId>,
        bypass_cache: bool,
    },
    NavigationStop {
        tab_id: Option<TabId>,
    },
    NavigationLoadUrl {
        tab_id: Option<TabId>,
        input: String,
    },
    SpaceSwitch {
        space_id: SpaceId,
    },
    SplitAdd {
        tab_id: TabId,
        direction: SplitDirection,
    },
    PanelToggle {
        panel_id: SidePanelId,
    },
    AgentSend {
        conversation_id: Option<ConversationId>,
        agent_id: Option<AgentId>,
        prompt: String,
    },
    WebChatAsk {
        site_id: ChatSiteId,
        prompt: String,
    },
    ThemeSet {
        scheme: Option<ThemeScheme>,
        overrides: Option<ThemeOverrides>,
    },
    LayoutSetToolbarLayout {
        layout: ToolbarLayout,
    },
    LayoutSetCompactMode {
        enabled: bool,
    },
    SidebarToggleExpanded,
    SidebarSetExpanded {
        expanded: bool,
    },
    SidebarSetSide {
        side: SidebarSide,
    },
    SidebarSetWidth {
        width: f32,
    },
    /// Cycles sample InputServer events (mode, pending keys, which-key) in the stub backend.
    DebugCycleInputState,
    // Menu, vim mode, zoom, dialog, and page context menu commands.
    VimSetEnabled {
        enabled: bool,
    },
    ZoomIn {
        tab_id: Option<TabId>,
    },
    ZoomOut {
        tab_id: Option<TabId>,
    },
    ZoomReset {
        tab_id: Option<TabId>,
    },
    MenuAppToggle,
    AppAbout,
    AppQuit,
    PageInspectElement {
        tab_id: Option<TabId>,
    },
    PageCut {
        tab_id: Option<TabId>,
    },
    PageCopy {
        tab_id: Option<TabId>,
    },
    PagePaste {
        tab_id: Option<TabId>,
        match_style: bool,
    },
    PageSelectAll {
        tab_id: Option<TabId>,
    },
    PageCopyLink {
        url: String,
    },
    PageOpenLinkInNewTab {
        tab_id: Option<TabId>,
        url: String,
        activate: bool,
    },
    PageViewSource {
        tab_id: Option<TabId>,
    },
    PageReplaceMisspelling {
        tab_id: Option<TabId>,
        replacement: String,
    },
    PageIgnoreMisspelling {
        tab_id: Option<TabId>,
        word: String,
    },
    PageLearnMisspelling {
        tab_id: Option<TabId>,
        word: String,
    },
    MediaContextAction {
        tab_id: Option<TabId>,
        action: MediaAction,
    },
    PageSelectDropdownChoose {
        tab_id: Option<TabId>,
        item_id: Option<u32>,
    },
    /// Extension point for commands registered by modules or not yet promoted to a typed variant.
    Custom {
        name: String,
        args: Vec<(String, String)>,
    },
}

fn find_arg<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

fn parse_bool_arg(command: &str, args: &[(String, String)], key: &str, default: bool) -> Result<bool, CommandError> {
    match find_arg(args, key) {
        None => Ok(default),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(other) => Err(CommandError::InvalidArgument {
            command: command.to_string(),
            message: format!("argument `{key}` expected bool, got `{other}`"),
        }),
    }
}

fn parse_optional_u64_arg(command: &str, args: &[(String, String)], key: &str) -> Result<Option<u64>, CommandError> {
    match find_arg(args, key) {
        None => Ok(None),
        Some(value) => value
            .parse::<u64>()
            .map(Some)
            .map_err(|_| CommandError::InvalidArgument {
                command: command.to_string(),
                message: format!("argument `{key}` expected integer id, got `{value}`"),
            }),
    }
}

fn parse_required_u64_arg(command: &str, args: &[(String, String)], key: &str) -> Result<u64, CommandError> {
    parse_optional_u64_arg(command, args, key)?.ok_or_else(|| CommandError::InvalidArgument {
        command: command.to_string(),
        message: format!("missing required argument `{key}`"),
    })
}

fn parse_required_string_arg(command: &str, args: &[(String, String)], key: &str) -> Result<String, CommandError> {
    find_arg(args, key)
        .map(str::to_string)
        .ok_or_else(|| CommandError::InvalidArgument {
            command: command.to_string(),
            message: format!("missing required argument `{key}`"),
        })
}

impl Command {
    /// Dotted command name used by keymaps, the palette, and audit traces.
    pub fn name(&self) -> &str {
        match self {
            Self::TabNew { .. } => "tab.new",
            Self::TabClose { .. } => "tab.close",
            Self::TabActivate { .. } => "tab.activate",
            Self::TabNext => "tab.next",
            Self::TabPrevious => "tab.previous",
            Self::LocationFocus => "location.focus",
            Self::NavigationBack { .. } => "navigation.back",
            Self::NavigationForward { .. } => "navigation.forward",
            Self::NavigationReload { .. } => "navigation.reload",
            Self::NavigationStop { .. } => "navigation.stop",
            Self::NavigationLoadUrl { .. } => "navigation.load_url",
            Self::SpaceSwitch { .. } => "space.switch",
            Self::SplitAdd { .. } => "split.add",
            Self::PanelToggle { .. } => "panel.toggle",
            Self::AgentSend { .. } => "agent.send",
            Self::WebChatAsk { .. } => "webchat.ask",
            Self::ThemeSet { .. } => "theme.set",
            Self::LayoutSetToolbarLayout { .. } => "layout.set_toolbar_layout",
            Self::LayoutSetCompactMode { .. } => "layout.set_compact_mode",
            Self::SidebarToggleExpanded => "sidebar.toggle_expanded",
            Self::SidebarSetExpanded { .. } => "sidebar.set_expanded",
            Self::SidebarSetSide { .. } => "sidebar.set_side",
            Self::SidebarSetWidth { .. } => "sidebar.set_width",
            Self::DebugCycleInputState => "debug.cycle_input_state",
            Self::VimSetEnabled { .. } => "vim.set_enabled",
            Self::ZoomIn { .. } => "zoom.in",
            Self::ZoomOut { .. } => "zoom.out",
            Self::ZoomReset { .. } => "zoom.reset",
            Self::MenuAppToggle => "menu.app.toggle",
            Self::AppAbout => "app.about",
            Self::AppQuit => "app.quit",
            Self::PageInspectElement { .. } => "page.inspect_element",
            Self::PageCut { .. } => "page.cut",
            Self::PageCopy { .. } => "page.copy",
            Self::PagePaste { match_style, .. } => {
                if *match_style {
                    "page.paste_and_match_style"
                } else {
                    "page.paste"
                }
            }
            Self::PageSelectAll { .. } => "page.select_all",
            Self::PageCopyLink { .. } => "page.copy_link",
            Self::PageOpenLinkInNewTab { .. } => "page.open_link_in_new_tab",
            Self::PageViewSource { .. } => "page.view_source",
            Self::PageReplaceMisspelling { .. } => "page.replace_misspelling",
            Self::PageIgnoreMisspelling { .. } => "page.ignore_misspelling",
            Self::PageLearnMisspelling { .. } => "page.learn_misspelling",
            Self::MediaContextAction { action, .. } => match action {
                MediaAction::TogglePlay => "media.toggle_play",
                MediaAction::ToggleMute => "media.toggle_mute",
                MediaAction::ToggleControls => "media.toggle_controls",
                MediaAction::ToggleLoop => "media.toggle_loop",
                MediaAction::TogglePictureInPicture => "media.toggle_pip",
                MediaAction::ToggleFullscreen => "media.toggle_fullscreen",
            },
            Self::PageSelectDropdownChoose { .. } => "page.select_dropdown_choose",
            Self::Custom { name, .. } => name.as_str(),
        }
    }

    /// Parses a dotted command name and key/value string arguments (from a keymap table, palette, or module) into a
    /// typed [`Command`]. Unknown command names become [`Command::Custom`].
    pub fn from_name_and_args(name: &str, args: &[(String, String)]) -> Result<Self, CommandError> {
        match name {
            "tab.new" => Ok(Self::TabNew {
                url: find_arg(args, "url").map(str::to_string),
                after: parse_optional_u64_arg(name, args, "after")?.map(TabId),
                activate: parse_bool_arg(name, args, "activate", true)?,
            }),
            "tab.close" => Ok(Self::TabClose {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                force: parse_bool_arg(name, args, "force", false)?,
            }),
            "tab.activate" => Ok(Self::TabActivate {
                tab_id: TabId(parse_required_u64_arg(name, args, "tab_id")?),
            }),
            "tab.next" => Ok(Self::TabNext),
            "tab.previous" => Ok(Self::TabPrevious),
            "location.focus" => Ok(Self::LocationFocus),
            "navigation.back" => Ok(Self::NavigationBack {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "navigation.forward" => Ok(Self::NavigationForward {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "navigation.reload" => Ok(Self::NavigationReload {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                bypass_cache: parse_bool_arg(name, args, "bypass_cache", false)?,
            }),
            "navigation.stop" => Ok(Self::NavigationStop {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "navigation.load_url" => Ok(Self::NavigationLoadUrl {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                input: parse_required_string_arg(name, args, "input")?,
            }),
            "space.switch" => Ok(Self::SpaceSwitch {
                space_id: SpaceId(parse_required_u64_arg(name, args, "space_id")?),
            }),
            "split.add" => {
                let tab_id = TabId(parse_required_u64_arg(name, args, "tab_id")?);
                let direction = match find_arg(args, "direction").unwrap_or("horizontal") {
                    "horizontal" => SplitDirection::Horizontal,
                    "vertical" => SplitDirection::Vertical,
                    "grid" => SplitDirection::Grid,
                    other => {
                        return Err(CommandError::InvalidArgument {
                            command: name.to_string(),
                            message: format!("unknown split direction `{other}`"),
                        });
                    }
                };
                Ok(Self::SplitAdd { tab_id, direction })
            }
            "panel.toggle" => Ok(Self::PanelToggle {
                panel_id: SidePanelId::new(parse_required_string_arg(name, args, "panel_id")?),
            }),
            "agent.send" => Ok(Self::AgentSend {
                conversation_id: parse_optional_u64_arg(name, args, "conversation_id")?.map(ConversationId),
                agent_id: find_arg(args, "agent_id").map(AgentId::new),
                prompt: parse_required_string_arg(name, args, "prompt")?,
            }),
            "webchat.ask" => Ok(Self::WebChatAsk {
                site_id: ChatSiteId::new(parse_required_string_arg(name, args, "site_id")?),
                prompt: parse_required_string_arg(name, args, "prompt")?,
            }),
            "theme.set" => {
                let scheme = match find_arg(args, "scheme") {
                    None => None,
                    Some("system") => Some(ThemeScheme::System),
                    Some("light") => Some(ThemeScheme::Light),
                    Some("dark") => Some(ThemeScheme::Dark),
                    Some(other) => {
                        return Err(CommandError::InvalidArgument {
                            command: name.to_string(),
                            message: format!("unknown theme scheme `{other}`"),
                        });
                    }
                };
                Ok(Self::ThemeSet {
                    scheme,
                    overrides: None,
                })
            }
            "layout.set_toolbar_layout" => {
                let raw = parse_required_string_arg(name, args, "layout")?;
                let layout = match raw.as_str() {
                    "single" => ToolbarLayout::Single,
                    "multiple" => ToolbarLayout::Multiple,
                    "collapsed" => ToolbarLayout::Collapsed,
                    other => {
                        return Err(CommandError::InvalidArgument {
                            command: name.to_string(),
                            message: format!("unknown toolbar layout `{other}`"),
                        });
                    }
                };
                Ok(Self::LayoutSetToolbarLayout { layout })
            }
            "layout.set_compact_mode" => Ok(Self::LayoutSetCompactMode {
                enabled: parse_bool_arg(name, args, "enabled", false)?,
            }),
            "sidebar.toggle_expanded" => Ok(Self::SidebarToggleExpanded),
            "sidebar.set_expanded" => Ok(Self::SidebarSetExpanded {
                expanded: parse_bool_arg(name, args, "expanded", true)?,
            }),
            "sidebar.set_side" => {
                let raw = parse_required_string_arg(name, args, "side")?;
                let side = match raw.as_str() {
                    "left" => SidebarSide::Left,
                    "right" => SidebarSide::Right,
                    other => {
                        return Err(CommandError::InvalidArgument {
                            command: name.to_string(),
                            message: format!("unknown sidebar side `{other}`"),
                        });
                    }
                };
                Ok(Self::SidebarSetSide { side })
            }
            "sidebar.set_width" => {
                let raw = parse_required_string_arg(name, args, "width")?;
                let width = raw.parse::<f32>().map_err(|_| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("argument `width` expected number, got `{raw}`"),
                })?;
                Ok(Self::SidebarSetWidth { width })
            }
            "debug.cycle_input_state" => Ok(Self::DebugCycleInputState),
            "vim.set_enabled" => Ok(Self::VimSetEnabled {
                enabled: parse_bool_arg(name, args, "enabled", true)?,
            }),
            "zoom.in" => Ok(Self::ZoomIn {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "zoom.out" => Ok(Self::ZoomOut {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "zoom.reset" => Ok(Self::ZoomReset {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "menu.app.toggle" => Ok(Self::MenuAppToggle),
            "app.about" => Ok(Self::AppAbout),
            "app.quit" => Ok(Self::AppQuit),
            "page.inspect_element" => Ok(Self::PageInspectElement {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "page.cut" => Ok(Self::PageCut {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "page.copy" => Ok(Self::PageCopy {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "page.paste" => Ok(Self::PagePaste {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                match_style: parse_bool_arg(name, args, "match_style", false)?,
            }),
            "page.paste_and_match_style" => Ok(Self::PagePaste {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                match_style: true,
            }),
            "page.select_all" => Ok(Self::PageSelectAll {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "page.copy_link" | "page.copy_image_address" | "page.copy_media_address" => Ok(Self::PageCopyLink {
                url: parse_required_string_arg(name, args, "url")?,
            }),
            "page.open_link_in_new_tab" | "page.open_image_in_new_tab" | "page.open_media_in_new_tab" => {
                Ok(Self::PageOpenLinkInNewTab {
                    tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                    url: parse_required_string_arg(name, args, "url")?,
                    activate: parse_bool_arg(name, args, "activate", false)?,
                })
            }
            "page.view_source" => Ok(Self::PageViewSource {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "page.replace_misspelling" => Ok(Self::PageReplaceMisspelling {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                replacement: parse_required_string_arg(name, args, "replacement")?,
            }),
            "page.ignore_misspelling" => Ok(Self::PageIgnoreMisspelling {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                word: parse_required_string_arg(name, args, "word")?,
            }),
            "page.learn_misspelling" => Ok(Self::PageLearnMisspelling {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                word: parse_required_string_arg(name, args, "word")?,
            }),
            "media.toggle_play" => Ok(Self::MediaContextAction {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                action: MediaAction::TogglePlay,
            }),
            "media.toggle_mute" => Ok(Self::MediaContextAction {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                action: MediaAction::ToggleMute,
            }),
            "media.toggle_controls" => Ok(Self::MediaContextAction {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                action: MediaAction::ToggleControls,
            }),
            "media.toggle_loop" => Ok(Self::MediaContextAction {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                action: MediaAction::ToggleLoop,
            }),
            "media.toggle_pip" => Ok(Self::MediaContextAction {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                action: MediaAction::TogglePictureInPicture,
            }),
            "media.toggle_fullscreen" => Ok(Self::MediaContextAction {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                action: MediaAction::ToggleFullscreen,
            }),
            "media.action" => {
                let raw = parse_required_string_arg(name, args, "action")?;
                let action = MediaAction::parse(&raw).ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("unknown media action `{raw}`"),
                })?;
                Ok(Self::MediaContextAction {
                    tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                    action,
                })
            }
            "page.select_dropdown_choose" => {
                let item_id = match parse_optional_u64_arg(name, args, "item_id")? {
                    Some(v) => Some(u32::try_from(v).map_err(|_| CommandError::InvalidArgument {
                        command: name.to_string(),
                        message: format!("argument `item_id` out of u32 range: `{v}`"),
                    })?),
                    None => None,
                };
                Ok(Self::PageSelectDropdownChoose {
                    tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                    item_id,
                })
            }
            _ => Ok(Self::Custom {
                name: name.to_string(),
                args: args.to_vec(),
            }),
        }
    }
}

/// Scalar or identifier type expected for a command argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgumentType {
    String,
    Bool,
    Integer,
    TabId,
    SpaceId,
    SidePanelId,
    AgentId,
    ConversationId,
    ChatSiteId,
    SplitDirection,
    ThemeScheme,
    Float,
    ToolbarLayout,
    SidebarSide,
}

/// Schema entry describing one argument of a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgumentSchema {
    pub name: String,
    pub arg_type: ArgumentType,
    pub required: bool,
    pub description: String,
}

impl ArgumentSchema {
    pub fn new(
        name: impl Into<String>,
        arg_type: ArgumentType,
        required: bool,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            arg_type,
            required,
            description: description.into(),
        }
    }
}

/// Metadata describing a registered command for the command palette and keymap validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandDescriptor {
    pub name: String,
    pub title: String,
    pub description: String,
    pub arguments: Vec<ArgumentSchema>,
}

/// Errors returned when invoking a command fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    UnknownCommand(String),
    InvalidArgument { command: String, message: String },
    TargetNotFound { command: String, target: String },
    BackendUnavailable(String),
    Failed(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCommand(name) => write!(f, "unknown command `{name}`"),
            Self::InvalidArgument { command, message } => {
                write!(f, "invalid argument for `{command}`: {message}")
            }
            Self::TargetNotFound { command, target } => {
                write!(f, "target `{target}` not found for `{command}`")
            }
            Self::BackendUnavailable(message) => write!(f, "backend unavailable: {message}"),
            Self::Failed(message) => write!(f, "command failed: {message}"),
        }
    }
}

impl std::error::Error for CommandError {}

pub type CommandResult = Result<(), CommandError>;

/// Descriptors for the core's built-in commands.
pub fn builtin_command_descriptors() -> Vec<CommandDescriptor> {
    vec![
        CommandDescriptor {
            name: "tab.new".to_string(),
            title: "New Tab".to_string(),
            description: "Open a new browser tab".to_string(),
            arguments: vec![
                ArgumentSchema::new("url", ArgumentType::String, false, "Initial URL to load"),
                ArgumentSchema::new("after", ArgumentType::TabId, false, "Insert after this tab"),
                ArgumentSchema::new("activate", ArgumentType::Bool, false, "Focus the new tab"),
            ],
        },
        CommandDescriptor {
            name: "tab.close".to_string(),
            title: "Close Tab".to_string(),
            description: "Close the active or specified tab".to_string(),
            arguments: vec![
                ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Tab to close"),
                ArgumentSchema::new("force", ArgumentType::Bool, false, "Close without waiting for page"),
            ],
        },
        CommandDescriptor {
            name: "tab.activate".to_string(),
            title: "Activate Tab".to_string(),
            description: "Switch focus to a tab".to_string(),
            arguments: vec![ArgumentSchema::new(
                "tab_id",
                ArgumentType::TabId,
                true,
                "Tab to activate",
            )],
        },
        CommandDescriptor {
            name: "tab.next".to_string(),
            title: "Next Tab".to_string(),
            description: "Switch to the next tab in the strip".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "tab.previous".to_string(),
            title: "Previous Tab".to_string(),
            description: "Switch to the previous tab in the strip".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "location.focus".to_string(),
            title: "Focus Location Bar".to_string(),
            description: "Move keyboard focus to the address bar".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "navigation.back".to_string(),
            title: "Go Back".to_string(),
            description: "Navigate one step back in session history".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "navigation.forward".to_string(),
            title: "Go Forward".to_string(),
            description: "Navigate one step forward in session history".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "navigation.reload".to_string(),
            title: "Reload Page".to_string(),
            description: "Reload the current page".to_string(),
            arguments: vec![
                ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab"),
                ArgumentSchema::new("bypass_cache", ArgumentType::Bool, false, "Ignore cached resources"),
            ],
        },
        CommandDescriptor {
            name: "navigation.stop".to_string(),
            title: "Stop Loading".to_string(),
            description: "Stop loading the current page".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "navigation.load_url".to_string(),
            title: "Go to Address".to_string(),
            description: "Load a URL or search query in a tab".to_string(),
            arguments: vec![
                ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab"),
                ArgumentSchema::new("input", ArgumentType::String, true, "URL or search text"),
            ],
        },
        CommandDescriptor {
            name: "space.switch".to_string(),
            title: "Switch Space".to_string(),
            description: "Switch the active workspace".to_string(),
            arguments: vec![ArgumentSchema::new(
                "space_id",
                ArgumentType::SpaceId,
                true,
                "Destination space",
            )],
        },
        CommandDescriptor {
            name: "split.add".to_string(),
            title: "Add to Split View".to_string(),
            description: "Show a tab alongside the active tab in a split group".to_string(),
            arguments: vec![
                ArgumentSchema::new("tab_id", ArgumentType::TabId, true, "Tab to add to the split"),
                ArgumentSchema::new("direction", ArgumentType::SplitDirection, true, "Split orientation"),
            ],
        },
        CommandDescriptor {
            name: "panel.toggle".to_string(),
            title: "Toggle Side Panel".to_string(),
            description: "Open or close a side panel".to_string(),
            arguments: vec![ArgumentSchema::new(
                "panel_id",
                ArgumentType::SidePanelId,
                true,
                "Side panel identifier",
            )],
        },
        CommandDescriptor {
            name: "agent.send".to_string(),
            title: "Send Agent Prompt".to_string(),
            description: "Send a prompt to an AI agent conversation".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "conversation_id",
                    ArgumentType::ConversationId,
                    false,
                    "Existing conversation to continue",
                ),
                ArgumentSchema::new(
                    "agent_id",
                    ArgumentType::AgentId,
                    false,
                    "Agent to use for a new conversation",
                ),
                ArgumentSchema::new("prompt", ArgumentType::String, true, "Prompt text"),
            ],
        },
        CommandDescriptor {
            name: "webchat.ask".to_string(),
            title: "Ask Web Chat".to_string(),
            description: "Send a prompt to a configured web chat site".to_string(),
            arguments: vec![
                ArgumentSchema::new("site_id", ArgumentType::ChatSiteId, true, "Target chat site"),
                ArgumentSchema::new("prompt", ArgumentType::String, true, "Prompt text"),
            ],
        },
        CommandDescriptor {
            name: "theme.set".to_string(),
            title: "Set Theme".to_string(),
            description: "Update the color scheme or theme overrides".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "scheme",
                    ArgumentType::ThemeScheme,
                    false,
                    "Color scheme (system, light, or dark)",
                ),
                ArgumentSchema::new(
                    "overrides",
                    ArgumentType::String,
                    false,
                    "Theme palette, role, glass, and agent color overrides",
                ),
            ],
        },
        CommandDescriptor {
            name: "layout.set_toolbar_layout".to_string(),
            title: "Set Toolbar Layout".to_string(),
            description: "Switch between single toolbar, multiple toolbars, and collapsed toolbar".to_string(),
            arguments: vec![ArgumentSchema::new(
                "layout",
                ArgumentType::ToolbarLayout,
                true,
                "Toolbar layout (single, multiple, or collapsed)",
            )],
        },
        CommandDescriptor {
            name: "layout.set_compact_mode".to_string(),
            title: "Set Compact Mode".to_string(),
            description: "Enable or disable compact mode in profile layout settings".to_string(),
            arguments: vec![ArgumentSchema::new(
                "enabled",
                ArgumentType::Bool,
                true,
                "Whether compact mode is enabled",
            )],
        },
        CommandDescriptor {
            name: "sidebar.toggle_expanded".to_string(),
            title: "Toggle Sidebar Expanded".to_string(),
            description: "Toggle the vertical sidebar between expanded and collapsed".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "sidebar.set_expanded".to_string(),
            title: "Set Sidebar Expanded".to_string(),
            description: "Expand or collapse the vertical tab sidebar".to_string(),
            arguments: vec![ArgumentSchema::new(
                "expanded",
                ArgumentType::Bool,
                true,
                "Whether the sidebar should be expanded",
            )],
        },
        CommandDescriptor {
            name: "sidebar.set_side".to_string(),
            title: "Set Sidebar Side".to_string(),
            description: "Place the vertical sidebar on the left or right edge of the window".to_string(),
            arguments: vec![ArgumentSchema::new(
                "side",
                ArgumentType::SidebarSide,
                true,
                "Sidebar edge (left or right)",
            )],
        },
        CommandDescriptor {
            name: "sidebar.set_width".to_string(),
            title: "Set Sidebar Width".to_string(),
            description: "Resize the expanded sidebar width in logical pixels".to_string(),
            arguments: vec![ArgumentSchema::new(
                "width",
                ArgumentType::Float,
                true,
                "Width in logical pixels",
            )],
        },
        CommandDescriptor {
            name: "debug.cycle_input_state".to_string(),
            title: "Cycle Sample Vim State".to_string(),
            description: "Emit sample InputServer mode, pending keys, and which-key events".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "vim.set_enabled".to_string(),
            title: "Set Vim Mode".to_string(),
            description: "Enable or disable Neovim-style keyboard control".to_string(),
            arguments: vec![ArgumentSchema::new(
                "enabled",
                ArgumentType::Bool,
                true,
                "Whether vim mode should be enabled",
            )],
        },
        CommandDescriptor {
            name: "zoom.in".to_string(),
            title: "Zoom In".to_string(),
            description: "Increase page zoom level".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "zoom.out".to_string(),
            title: "Zoom Out".to_string(),
            description: "Decrease page zoom level".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "zoom.reset".to_string(),
            title: "Reset Zoom".to_string(),
            description: "Reset page zoom level to 100%".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "menu.app.toggle".to_string(),
            title: "Toggle Application Menu".to_string(),
            description: "Open or close the main application menu".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "app.about".to_string(),
            title: "About Zendia".to_string(),
            description: "Show version and license information for Zendia".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "app.quit".to_string(),
            title: "Quit".to_string(),
            description: "Close all windows and exit Zendia".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "page.inspect_element".to_string(),
            title: "Inspect Element".to_string(),
            description: "Open developer tools on the right-clicked page element".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "page.cut".to_string(),
            title: "Cut".to_string(),
            description: "Cut the selected editable text".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "page.copy".to_string(),
            title: "Copy".to_string(),
            description: "Copy the selected text to the clipboard".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "page.paste".to_string(),
            title: "Paste".to_string(),
            description: "Paste clipboard contents into the focused field".to_string(),
            arguments: vec![
                ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab"),
                ArgumentSchema::new("match_style", ArgumentType::Bool, false, "Paste as plain text"),
            ],
        },
        CommandDescriptor {
            name: "page.select_all".to_string(),
            title: "Select All".to_string(),
            description: "Select all text in the active field or page".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "page.copy_link".to_string(),
            title: "Copy Link".to_string(),
            description: "Copy a link or resource address to the clipboard".to_string(),
            arguments: vec![ArgumentSchema::new(
                "url",
                ArgumentType::String,
                true,
                "Address to copy",
            )],
        },
        CommandDescriptor {
            name: "page.open_link_in_new_tab".to_string(),
            title: "Open Link in New Tab".to_string(),
            description: "Open a URL in a new tab".to_string(),
            arguments: vec![
                ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Source tab"),
                ArgumentSchema::new("url", ArgumentType::String, true, "URL to open"),
                ArgumentSchema::new("activate", ArgumentType::Bool, false, "Focus the new tab"),
            ],
        },
        CommandDescriptor {
            name: "page.view_source".to_string(),
            title: "View Page Source".to_string(),
            description: "Open the HTML source of the current page".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "media.toggle_play".to_string(),
            title: "Play / Pause Media".to_string(),
            description: "Toggle playback on the context-menu media element".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "media.toggle_mute".to_string(),
            title: "Mute / Unmute Media".to_string(),
            description: "Toggle mute on the context-menu media element".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "media.toggle_controls".to_string(),
            title: "Show / Hide Media Controls".to_string(),
            description: "Toggle built-in controls on the context-menu media element".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "media.toggle_loop".to_string(),
            title: "Loop Media".to_string(),
            description: "Toggle looping on the context-menu media element".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "media.toggle_pip".to_string(),
            title: "Toggle Picture in Picture".to_string(),
            description: "Enter or exit picture-in-picture for the context-menu video".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "media.toggle_fullscreen".to_string(),
            title: "Toggle Media Full Screen".to_string(),
            description: "Enter or exit fullscreen for the context-menu video".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
    ]
}
