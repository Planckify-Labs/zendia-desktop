/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Named, typed browser commands and their descriptors for keymaps and the command palette.

use crate::contributions::SidePanelId;
use crate::input_state::StatuslineShow;
use crate::media::MediaControl;
use crate::model::{
    AgentId, ChatSiteId, CompactHide, ConversationId, FolderId, MediaSessionId, NotificationId, SidebarSide, SpaceId,
    SplitDirection, SplitGroupId, TabId, TabMoveTarget, ThemeOverrides, ThemeScheme, ToolbarLayout,
};
use crate::site_info::{PermissionState, SitePermissionKind};
use crate::split::{DropZone, parse_direction};
use crate::toasts::ToastId;
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
    AddressBarOpen {
        new_tab: bool,
        initial_query: Option<String>,
    },
    AddressBarSubmit {
        input: String,
        new_tab: bool,
    },
    AddressBarSwitchToTab {
        tab_id: TabId,
    },
    AddressBarCopyCurrentUrl,
    PaletteOpen {
        initial_query: Option<String>,
    },
    SitePanelToggle,
    SitePermissionSet {
        origin: Option<String>,
        permission: SitePermissionKind,
        state: PermissionState,
    },
    SiteTrackingProtectionToggle {
        origin: Option<String>,
    },
    SiteClearData {
        origin: Option<String>,
    },
    SiteBookmarkToggle {
        tab_id: Option<TabId>,
    },
    SiteScreenshot {
        tab_id: Option<TabId>,
    },
    SiteShare {
        tab_id: Option<TabId>,
    },
    SiteEditStyle {
        tab_id: Option<TabId>,
    },
    SiteReaderModeToggle {
        tab_id: Option<TabId>,
    },
    SettingsSetSearchEngine {
        name: String,
        search_url: String,
    },
    LayoutToggleCompactMode,
    SidebarToggleSide,
    FolderCollapseAll,
    FolderExpandAll,
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
    SpaceNext,
    SpacePrevious,
    SpaceClearTabs {
        space_id: Option<SpaceId>,
    },
    TabPin {
        tab_id: Option<TabId>,
    },
    TabUnpin {
        tab_id: Option<TabId>,
    },
    TabTogglePin {
        tab_id: Option<TabId>,
    },
    TabAddToEssentials {
        tab_id: Option<TabId>,
    },
    TabRemoveFromEssentials {
        tab_id: Option<TabId>,
    },
    TabToggleEssential {
        tab_id: Option<TabId>,
    },
    TabToggleMute {
        tab_id: Option<TabId>,
    },
    TabDuplicate {
        tab_id: Option<TabId>,
    },
    TabRename {
        tab_id: Option<TabId>,
        title: String,
    },
    TabStartRename {
        tab_id: Option<TabId>,
    },
    TabMove {
        tab_id: TabId,
        target: TabMoveTarget,
    },
    TabMoveToFolder {
        tab_id: Option<TabId>,
        folder_id: Option<FolderId>,
    },
    TabMoveToSpace {
        tab_id: Option<TabId>,
        space_id: SpaceId,
    },
    FolderCreate {
        name: Option<String>,
        space_id: Option<SpaceId>,
        tab_id: Option<TabId>,
    },
    FolderRename {
        folder_id: FolderId,
        name: String,
    },
    FolderStartRename {
        folder_id: FolderId,
    },
    FolderDelete {
        folder_id: FolderId,
    },
    FolderUnpack {
        folder_id: FolderId,
    },
    FolderToggleCollapsed {
        folder_id: FolderId,
    },
    NotificationDismiss {
        notification_id: NotificationId,
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
    /// Shows the given tabs side by side in one split group, taking them out of any other group.
    SplitTabs {
        tab_ids: Vec<TabId>,
        direction: SplitDirection,
    },
    /// Opens a new tab and splits it with the active tab.
    SplitNewEmpty,
    /// Opens `url` in a new tab split with `tab_id` (the active tab by default).
    SplitOpenUrl {
        tab_id: Option<TabId>,
        url: String,
    },
    /// Drops `tab_id` on one side of the pane showing `target_tab_id`.
    SplitDrop {
        tab_id: TabId,
        target_tab_id: TabId,
        zone: DropZone,
    },
    SplitRemoveTab {
        tab_id: TabId,
    },
    /// Dissolves a group (the active one by default); its tabs become separate tabs again.
    SplitUnsplit {
        group_id: Option<SplitGroupId>,
    },
    SplitSetLayout {
        group_id: Option<SplitGroupId>,
        direction: SplitDirection,
    },
    SplitResize {
        group_id: SplitGroupId,
        ratios: Vec<f32>,
    },
    GlanceOpen {
        url: String,
        parent_tab_id: Option<TabId>,
        origin: Option<(f32, f32)>,
    },
    GlanceClose,
    /// Turns the Glance page into a normal tab next to its parent and shows it.
    GlanceExpand,
    /// Splits the Glance page with its parent.
    GlanceSplit,
    ToastShow {
        message: String,
        timeout_ms: Option<u64>,
        action_label: Option<String>,
        action_command: Option<String>,
    },
    ToastDismiss {
        toast_id: ToastId,
    },
    MediaSessionControl {
        session_id: MediaSessionId,
        control: MediaControl,
    },
    LayoutSetCompactHide {
        hide: CompactHide,
    },
    LayoutCompactRevealSidebar,
    LayoutSetTabSwitchMotion {
        enabled: bool,
    },
    /// Starts a sample download so the download animation can be seen without the engine reporting downloads.
    DebugSampleDownload,
    /// Starts sample media in a background tab so the media controls can be seen without the engine reporting media.
    DebugSampleMedia,
    /// Configures how the vim statusline rests after activity settles (`always`, `shrink`, `never`).
    StatuslineSetShow {
        show: StatuslineShow,
    },
    /// Shows a short transient announcement beside the vim mode indicator.
    StatuslineAnnounce {
        message: String,
        timeout_ms: Option<u64>,
    },
    /// Clears any active statusline announcement immediately.
    StatuslineClearAnnouncement,
    /// Opens the all-keymaps which-key popup for the active vim mode (`<leader>?`).
    WhichKeyShowAll,
    /// Dismisses the active which-key popup.
    WhichKeyHide,
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
            Self::AddressBarOpen { .. } => "address_bar.open",
            Self::AddressBarSubmit { .. } => "address_bar.submit",
            Self::AddressBarSwitchToTab { .. } => "address_bar.switch_to_tab",
            Self::AddressBarCopyCurrentUrl => "address_bar.copy_current_url",
            Self::PaletteOpen { .. } => "palette.open",
            Self::SitePanelToggle => "site.panel.toggle",
            Self::SitePermissionSet { .. } => "site.permission.set",
            Self::SiteTrackingProtectionToggle { .. } => "site.tracking_protection.toggle",
            Self::SiteClearData { .. } => "site.clear_data",
            Self::SiteBookmarkToggle { .. } => "site.bookmark.toggle",
            Self::SiteScreenshot { .. } => "site.screenshot",
            Self::SiteShare { .. } => "site.share",
            Self::SiteEditStyle { .. } => "site.edit_style",
            Self::SiteReaderModeToggle { .. } => "site.reader_mode.toggle",
            Self::SettingsSetSearchEngine { .. } => "settings.set_search_engine",
            Self::LayoutToggleCompactMode => "layout.toggle_compact_mode",
            Self::SidebarToggleSide => "sidebar.toggle_side",
            Self::FolderCollapseAll => "folder.collapse_all",
            Self::FolderExpandAll => "folder.expand_all",
            Self::NavigationBack { .. } => "navigation.back",
            Self::NavigationForward { .. } => "navigation.forward",
            Self::NavigationReload { .. } => "navigation.reload",
            Self::NavigationStop { .. } => "navigation.stop",
            Self::NavigationLoadUrl { .. } => "navigation.load_url",
            Self::SpaceSwitch { .. } => "space.switch",
            Self::SpaceNext => "space.next",
            Self::SpacePrevious => "space.previous",
            Self::SpaceClearTabs { .. } => "space.clear_tabs",
            Self::TabPin { .. } => "tab.pin",
            Self::TabUnpin { .. } => "tab.unpin",
            Self::TabTogglePin { .. } => "tab.toggle_pin",
            Self::TabAddToEssentials { .. } => "tab.add_to_essentials",
            Self::TabRemoveFromEssentials { .. } => "tab.remove_from_essentials",
            Self::TabToggleEssential { .. } => "tab.toggle_essential",
            Self::TabToggleMute { .. } => "tab.toggle_mute",
            Self::TabDuplicate { .. } => "tab.duplicate",
            Self::TabRename { .. } => "tab.rename",
            Self::TabStartRename { .. } => "tab.start_rename",
            Self::TabMove { .. } => "tab.move",
            Self::TabMoveToFolder { .. } => "tab.move_to_folder",
            Self::TabMoveToSpace { .. } => "tab.move_to_space",
            Self::FolderCreate { .. } => "folder.create",
            Self::FolderRename { .. } => "folder.rename",
            Self::FolderStartRename { .. } => "folder.start_rename",
            Self::FolderDelete { .. } => "folder.delete",
            Self::FolderUnpack { .. } => "folder.unpack",
            Self::FolderToggleCollapsed { .. } => "folder.toggle_collapsed",
            Self::NotificationDismiss { .. } => "notification.dismiss",
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
            Self::SplitTabs { .. } => "split.tabs",
            Self::SplitNewEmpty => "split.new_empty",
            Self::SplitOpenUrl { .. } => "split.open_url",
            Self::SplitDrop { .. } => "split.drop",
            Self::SplitRemoveTab { .. } => "split.remove_tab",
            Self::SplitUnsplit { .. } => "split.unsplit",
            Self::SplitSetLayout { .. } => "split.set_layout",
            Self::SplitResize { .. } => "split.resize",
            Self::GlanceOpen { .. } => "glance.open",
            Self::GlanceClose => "glance.close",
            Self::GlanceExpand => "glance.expand",
            Self::GlanceSplit => "glance.split",
            Self::ToastShow { .. } => "toast.show",
            Self::ToastDismiss { .. } => "toast.dismiss",
            Self::MediaSessionControl { control, .. } => control.command_name(),
            Self::LayoutSetCompactHide { .. } => "layout.set_compact_hide",
            Self::LayoutCompactRevealSidebar => "layout.compact_reveal_sidebar",
            Self::LayoutSetTabSwitchMotion { .. } => "layout.set_tab_switch_motion",
            Self::DebugSampleDownload => "debug.sample_download",
            Self::DebugSampleMedia => "debug.sample_media",
            Self::StatuslineSetShow { .. } => "statusline.set_show",
            Self::StatuslineAnnounce { .. } => "statusline.announce",
            Self::StatuslineClearAnnouncement => "statusline.clear_announcement",
            Self::WhichKeyShowAll => "which_key.show_all",
            Self::WhichKeyHide => "which_key.hide",
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
            "address_bar.open" => Ok(Self::AddressBarOpen {
                new_tab: parse_bool_arg(name, args, "new_tab", false)?,
                initial_query: find_arg(args, "initial_query").map(str::to_string),
            }),
            "address_bar.submit" => Ok(Self::AddressBarSubmit {
                input: parse_required_string_arg(name, args, "input")?,
                new_tab: parse_bool_arg(name, args, "new_tab", false)?,
            }),
            "address_bar.switch_to_tab" => Ok(Self::AddressBarSwitchToTab {
                tab_id: TabId(parse_required_u64_arg(name, args, "tab_id")?),
            }),
            "address_bar.copy_current_url" => Ok(Self::AddressBarCopyCurrentUrl),
            "palette.open" => Ok(Self::PaletteOpen {
                initial_query: find_arg(args, "initial_query").map(str::to_string),
            }),
            "site.panel.toggle" => Ok(Self::SitePanelToggle),
            "site.permission.set" => {
                let raw_perm = parse_required_string_arg(name, args, "permission")?;
                let permission = SitePermissionKind::parse(&raw_perm).ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("unknown site permission `{raw_perm}`"),
                })?;
                let raw_state = parse_required_string_arg(name, args, "state")?;
                let state = PermissionState::parse(&raw_state).ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("unknown permission state `{raw_state}`"),
                })?;
                Ok(Self::SitePermissionSet {
                    origin: find_arg(args, "origin").map(str::to_string),
                    permission,
                    state,
                })
            }
            "site.tracking_protection.toggle" => Ok(Self::SiteTrackingProtectionToggle {
                origin: find_arg(args, "origin").map(str::to_string),
            }),
            "site.clear_data" => Ok(Self::SiteClearData {
                origin: find_arg(args, "origin").map(str::to_string),
            }),
            "site.bookmark.toggle" => Ok(Self::SiteBookmarkToggle {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "site.screenshot" => Ok(Self::SiteScreenshot {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "site.share" => Ok(Self::SiteShare {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "site.edit_style" => Ok(Self::SiteEditStyle {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "site.reader_mode.toggle" => Ok(Self::SiteReaderModeToggle {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "settings.set_search_engine" => Ok(Self::SettingsSetSearchEngine {
                name: parse_required_string_arg(name, args, "name")?,
                search_url: parse_required_string_arg(name, args, "search_url")?,
            }),
            "layout.toggle_compact_mode" => Ok(Self::LayoutToggleCompactMode),
            "sidebar.toggle_side" => Ok(Self::SidebarToggleSide),
            "folder.collapse_all" => Ok(Self::FolderCollapseAll),
            "folder.expand_all" => Ok(Self::FolderExpandAll),
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
            "space.next" => Ok(Self::SpaceNext),
            "space.previous" => Ok(Self::SpacePrevious),
            "space.clear_tabs" => Ok(Self::SpaceClearTabs {
                space_id: parse_optional_u64_arg(name, args, "space_id")?.map(SpaceId),
            }),
            "tab.pin" => Ok(Self::TabPin {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "tab.unpin" => Ok(Self::TabUnpin {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "tab.toggle_pin" => Ok(Self::TabTogglePin {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "tab.add_to_essentials" => Ok(Self::TabAddToEssentials {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "tab.remove_from_essentials" => Ok(Self::TabRemoveFromEssentials {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "tab.toggle_essential" => Ok(Self::TabToggleEssential {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "tab.toggle_mute" => Ok(Self::TabToggleMute {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "tab.duplicate" => Ok(Self::TabDuplicate {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "tab.rename" => Ok(Self::TabRename {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                title: parse_required_string_arg(name, args, "title")?,
            }),
            "tab.start_rename" => Ok(Self::TabStartRename {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "tab.move" => {
                let tab_id = TabId(parse_required_u64_arg(name, args, "tab_id")?);
                let index = parse_optional_u64_arg(name, args, "index")?.unwrap_or(0) as usize;
                let section = find_arg(args, "section").unwrap_or("normal");
                let target = match section {
                    "normal" => TabMoveTarget::Normal {
                        space_id: parse_optional_u64_arg(name, args, "space_id")?.map(SpaceId),
                        index,
                    },
                    "pinned" => TabMoveTarget::Pinned {
                        space_id: parse_optional_u64_arg(name, args, "space_id")?.map(SpaceId),
                        index,
                    },
                    "folder" => TabMoveTarget::Folder {
                        folder_id: FolderId(parse_required_u64_arg(name, args, "folder_id")?),
                        index,
                    },
                    "essential" => TabMoveTarget::Essential { index },
                    other => {
                        return Err(CommandError::InvalidArgument {
                            command: name.to_string(),
                            message: format!("unknown tab.move section `{other}`"),
                        });
                    }
                };
                Ok(Self::TabMove { tab_id, target })
            }
            "tab.move_to_folder" => Ok(Self::TabMoveToFolder {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                folder_id: parse_optional_u64_arg(name, args, "folder_id")?.map(FolderId),
            }),
            "tab.move_to_space" => Ok(Self::TabMoveToSpace {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                space_id: SpaceId(parse_required_u64_arg(name, args, "space_id")?),
            }),
            "folder.create" | "folder.new" => Ok(Self::FolderCreate {
                name: find_arg(args, "name").map(str::to_string),
                space_id: parse_optional_u64_arg(name, args, "space_id")?.map(SpaceId),
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "folder.rename" => Ok(Self::FolderRename {
                folder_id: FolderId(parse_required_u64_arg(name, args, "folder_id")?),
                name: parse_required_string_arg(name, args, "name")?,
            }),
            "folder.start_rename" => Ok(Self::FolderStartRename {
                folder_id: FolderId(parse_required_u64_arg(name, args, "folder_id")?),
            }),
            "folder.delete" => Ok(Self::FolderDelete {
                folder_id: FolderId(parse_required_u64_arg(name, args, "folder_id")?),
            }),
            "folder.unpack" => Ok(Self::FolderUnpack {
                folder_id: FolderId(parse_required_u64_arg(name, args, "folder_id")?),
            }),
            "folder.toggle_collapsed" => Ok(Self::FolderToggleCollapsed {
                folder_id: FolderId(parse_required_u64_arg(name, args, "folder_id")?),
            }),
            "notification.dismiss" => Ok(Self::NotificationDismiss {
                notification_id: NotificationId(parse_required_u64_arg(name, args, "notification_id")?),
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
            "split.tabs" => Ok(Self::SplitTabs {
                tab_ids: parse_u64_list_arg(name, args, "tab_ids")?
                    .into_iter()
                    .map(TabId)
                    .collect(),
                direction: parse_direction_arg(name, args, "direction", SplitDirection::Horizontal)?,
            }),
            "split.new_empty" => Ok(Self::SplitNewEmpty),
            "split.open_url" => Ok(Self::SplitOpenUrl {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                url: parse_required_string_arg(name, args, "url")?,
            }),
            "split.drop" => {
                let raw = parse_required_string_arg(name, args, "zone")?;
                let zone = DropZone::parse(&raw).ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("unknown drop zone `{raw}`"),
                })?;
                Ok(Self::SplitDrop {
                    tab_id: TabId(parse_required_u64_arg(name, args, "tab_id")?),
                    target_tab_id: TabId(parse_required_u64_arg(name, args, "target_tab_id")?),
                    zone,
                })
            }
            "split.remove_tab" => Ok(Self::SplitRemoveTab {
                tab_id: TabId(parse_required_u64_arg(name, args, "tab_id")?),
            }),
            "split.unsplit" => Ok(Self::SplitUnsplit {
                group_id: parse_optional_u64_arg(name, args, "group_id")?.map(SplitGroupId),
            }),
            "split.set_layout" => Ok(Self::SplitSetLayout {
                group_id: parse_optional_u64_arg(name, args, "group_id")?.map(SplitGroupId),
                direction: parse_direction_arg(name, args, "direction", SplitDirection::Grid)?,
            }),
            "split.resize" => Ok(Self::SplitResize {
                group_id: SplitGroupId(parse_required_u64_arg(name, args, "group_id")?),
                ratios: parse_f32_list_arg(name, args, "ratios")?,
            }),
            "glance.open" => {
                let x = parse_optional_f32_arg(name, args, "x")?;
                let y = parse_optional_f32_arg(name, args, "y")?;
                Ok(Self::GlanceOpen {
                    url: parse_required_string_arg(name, args, "url")?,
                    // Page menu entries pass the page's tab as `tab_id`.
                    parent_tab_id: parse_optional_u64_arg(name, args, "parent_tab_id")?
                        .or(parse_optional_u64_arg(name, args, "tab_id")?)
                        .map(TabId),
                    origin: x.zip(y),
                })
            }
            "glance.close" => Ok(Self::GlanceClose),
            "glance.expand" => Ok(Self::GlanceExpand),
            "glance.split" => Ok(Self::GlanceSplit),
            "toast.show" => Ok(Self::ToastShow {
                message: parse_required_string_arg(name, args, "message")?,
                timeout_ms: parse_optional_u64_arg(name, args, "timeout_ms")?,
                action_label: find_arg(args, "action_label").map(str::to_string),
                action_command: find_arg(args, "action_command").map(str::to_string),
            }),
            "toast.dismiss" => Ok(Self::ToastDismiss {
                toast_id: ToastId(parse_required_u64_arg(name, args, "toast_id")?),
            }),
            "media.session.toggle_play"
            | "media.session.toggle_mute"
            | "media.session.previous"
            | "media.session.next"
            | "media.session.pip"
            | "media.session.focus"
            | "media.session.dismiss"
            | "media.session.seek" => {
                let control = match name {
                    "media.session.toggle_play" => MediaControl::TogglePlay,
                    "media.session.toggle_mute" => MediaControl::ToggleMute,
                    "media.session.previous" => MediaControl::Previous,
                    "media.session.next" => MediaControl::Next,
                    "media.session.pip" => MediaControl::PictureInPicture,
                    "media.session.focus" => MediaControl::Focus,
                    "media.session.dismiss" => MediaControl::Dismiss,
                    _ => MediaControl::Seek(parse_optional_f32_arg(name, args, "position")?.ok_or_else(|| {
                        CommandError::InvalidArgument {
                            command: name.to_string(),
                            message: "missing required argument `position`".to_string(),
                        }
                    })?),
                };
                Ok(Self::MediaSessionControl {
                    session_id: MediaSessionId(parse_required_u64_arg(name, args, "session_id")?),
                    control,
                })
            }
            "layout.set_compact_hide" => {
                let raw = parse_required_string_arg(name, args, "hide")?;
                let hide = CompactHide::parse(&raw).ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("unknown compact mode target `{raw}`"),
                })?;
                Ok(Self::LayoutSetCompactHide { hide })
            }
            "layout.compact_reveal_sidebar" => Ok(Self::LayoutCompactRevealSidebar),
            "layout.set_tab_switch_motion" => Ok(Self::LayoutSetTabSwitchMotion {
                enabled: parse_bool_arg(name, args, "enabled", true)?,
            }),
            "debug.sample_download" => Ok(Self::DebugSampleDownload),
            "debug.sample_media" => Ok(Self::DebugSampleMedia),
            "statusline.set_show" => {
                let raw = parse_required_string_arg(name, args, "show")?;
                let show = StatuslineShow::parse(&raw).ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("unknown statusline.show value `{raw}` (expected always, shrink, or never)"),
                })?;
                Ok(Self::StatuslineSetShow { show })
            }
            "statusline.announce" => Ok(Self::StatuslineAnnounce {
                message: parse_required_string_arg(name, args, "message")?,
                timeout_ms: parse_optional_u64_arg(name, args, "timeout_ms")?,
            }),
            "statusline.clear_announcement" => Ok(Self::StatuslineClearAnnouncement),
            "which_key.show_all" => Ok(Self::WhichKeyShowAll),
            "which_key.hide" => Ok(Self::WhichKeyHide),
            _ => Ok(Self::Custom {
                name: name.to_string(),
                args: args.to_vec(),
            }),
        }
    }
}

fn parse_direction_arg(
    command: &str,
    args: &[(String, String)],
    key: &str,
    default: SplitDirection,
) -> Result<SplitDirection, CommandError> {
    match find_arg(args, key) {
        None => Ok(default),
        Some(raw) => parse_direction(raw).ok_or_else(|| CommandError::InvalidArgument {
            command: command.to_string(),
            message: format!("unknown split direction `{raw}`"),
        }),
    }
}

fn parse_optional_f32_arg(command: &str, args: &[(String, String)], key: &str) -> Result<Option<f32>, CommandError> {
    match find_arg(args, key) {
        None => Ok(None),
        Some(raw) => raw
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(Some)
            .ok_or_else(|| CommandError::InvalidArgument {
                command: command.to_string(),
                message: format!("argument `{key}` is not a number: `{raw}`"),
            }),
    }
}

/// Parses a comma-separated list such as `tab_ids=3,5,8`.
fn parse_u64_list_arg(command: &str, args: &[(String, String)], key: &str) -> Result<Vec<u64>, CommandError> {
    let raw = parse_required_string_arg(command, args, key)?;
    raw.split(',')
        .map(|part| {
            part.trim().parse::<u64>().map_err(|_| CommandError::InvalidArgument {
                command: command.to_string(),
                message: format!("argument `{key}` has a bad entry: `{part}`"),
            })
        })
        .collect()
}

fn parse_f32_list_arg(command: &str, args: &[(String, String)], key: &str) -> Result<Vec<f32>, CommandError> {
    let raw = parse_required_string_arg(command, args, key)?;
    raw.split(',')
        .map(|part| {
            part.trim()
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .ok_or_else(|| CommandError::InvalidArgument {
                    command: command.to_string(),
                    message: format!("argument `{key}` has a bad entry: `{part}`"),
                })
        })
        .collect()
}

/// Scalar or identifier type expected for a command argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgumentType {
    String,
    Bool,
    Integer,
    TabId,
    SpaceId,
    FolderId,
    NotificationId,
    SidePanelId,
    AgentId,
    ConversationId,
    ChatSiteId,
    SplitDirection,
    ThemeScheme,
    Float,
    ToolbarLayout,
    SidebarSide,
    SitePermission,
    PermissionState,
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
            name: "address_bar.open".to_string(),
            title: "Open Floating Address Bar".to_string(),
            description: "Open the floating address bar over the page".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "new_tab",
                    ArgumentType::Bool,
                    false,
                    "Open submitted URL or search in a new tab",
                ),
                ArgumentSchema::new(
                    "initial_query",
                    ArgumentType::String,
                    false,
                    "Optional initial query text",
                ),
            ],
        },
        CommandDescriptor {
            name: "address_bar.submit".to_string(),
            title: "Submit Address Bar".to_string(),
            description: "Navigate to a URL or search query in the current or a new tab".to_string(),
            arguments: vec![
                ArgumentSchema::new("input", ArgumentType::String, true, "URL or search query text"),
                ArgumentSchema::new(
                    "new_tab",
                    ArgumentType::Bool,
                    false,
                    "Whether to open the destination in a new tab",
                ),
            ],
        },
        CommandDescriptor {
            name: "address_bar.switch_to_tab".to_string(),
            title: "Switch to Open Tab".to_string(),
            description: "Activate an open tab across spaces from the address bar".to_string(),
            arguments: vec![ArgumentSchema::new(
                "tab_id",
                ArgumentType::TabId,
                true,
                "Tab to switch to",
            )],
        },
        CommandDescriptor {
            name: "address_bar.copy_current_url".to_string(),
            title: "Copy Current URL".to_string(),
            description: "Copy the active tab's URL to the clipboard".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "palette.open".to_string(),
            title: "Open Command Palette".to_string(),
            description: "Open the command palette in Actions mode".to_string(),
            arguments: vec![ArgumentSchema::new(
                "initial_query",
                ArgumentType::String,
                false,
                "Optional initial filter query",
            )],
        },
        CommandDescriptor {
            name: "site.panel.toggle".to_string(),
            title: "Toggle Site Data Panel".to_string(),
            description: "Open or close the site security, permissions, and extensions panel".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "site.permission.set".to_string(),
            title: "Set Site Permission".to_string(),
            description: "Allow, block, or reset a permission for a site origin".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "origin",
                    ArgumentType::String,
                    false,
                    "Target origin (defaults to active tab)",
                ),
                ArgumentSchema::new("permission", ArgumentType::SitePermission, true, "Permission category"),
                ArgumentSchema::new(
                    "state",
                    ArgumentType::PermissionState,
                    true,
                    "Permission state (allow, block, prompt)",
                ),
            ],
        },
        CommandDescriptor {
            name: "site.tracking_protection.toggle".to_string(),
            title: "Toggle Tracking Protection".to_string(),
            description: "Enable or disable tracking protection for the current site".to_string(),
            arguments: vec![ArgumentSchema::new(
                "origin",
                ArgumentType::String,
                false,
                "Target origin (defaults to active tab)",
            )],
        },
        CommandDescriptor {
            name: "site.clear_data".to_string(),
            title: "Clear Site Data".to_string(),
            description: "Clear cookies and stored site data for the current site".to_string(),
            arguments: vec![ArgumentSchema::new(
                "origin",
                ArgumentType::String,
                false,
                "Target origin (defaults to active tab)",
            )],
        },
        CommandDescriptor {
            name: "site.bookmark.toggle".to_string(),
            title: "Toggle Page Bookmark".to_string(),
            description: "Bookmark or unbookmark the active tab's page".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "site.screenshot".to_string(),
            title: "Capture Screenshot".to_string(),
            description: "Capture a screenshot of the current page".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "site.share".to_string(),
            title: "Share Page".to_string(),
            description: "Share or copy the current page link".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "site.edit_style".to_string(),
            title: "Customize Site Style".to_string(),
            description: "Open per-site appearance customization".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "site.reader_mode.toggle".to_string(),
            title: "Toggle Reader Mode".to_string(),
            description: "Toggle reader view for the active page".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "settings.set_search_engine".to_string(),
            title: "Set Default Search Engine".to_string(),
            description: "Set the default search engine used by the address bar".to_string(),
            arguments: vec![
                ArgumentSchema::new("name", ArgumentType::String, true, "Display name of the search engine"),
                ArgumentSchema::new("search_url", ArgumentType::String, true, "Search URL template with %s"),
            ],
        },
        CommandDescriptor {
            name: "layout.toggle_compact_mode".to_string(),
            title: "Toggle Compact Mode".to_string(),
            description: "Toggle compact mode in profile layout settings".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "sidebar.toggle_side".to_string(),
            title: "Toggle Tabs on Right".to_string(),
            description: "Move the vertical sidebar between left and right edges".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "folder.collapse_all".to_string(),
            title: "Collapse All Folders".to_string(),
            description: "Collapse every folder in the active space".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "folder.expand_all".to_string(),
            title: "Expand All Folders".to_string(),
            description: "Expand every folder in the active space".to_string(),
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
            name: "space.next".to_string(),
            title: "Next Space".to_string(),
            description: "Switch to the next workspace".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "space.previous".to_string(),
            title: "Previous Space".to_string(),
            description: "Switch to the previous workspace".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "space.clear_tabs".to_string(),
            title: "Clear Unpinned Tabs".to_string(),
            description: "Close all normal tabs in the active or specified space".to_string(),
            arguments: vec![ArgumentSchema::new(
                "space_id",
                ArgumentType::SpaceId,
                false,
                "Target space (defaults to active space)",
            )],
        },
        CommandDescriptor {
            name: "tab.pin".to_string(),
            title: "Pin Tab".to_string(),
            description: "Pin a tab to the current space".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "tab.unpin".to_string(),
            title: "Unpin Tab".to_string(),
            description: "Return a pinned tab to the normal tab list".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "tab.toggle_pin".to_string(),
            title: "Toggle Pin Tab".to_string(),
            description: "Pin or unpin the active or specified tab".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "tab.add_to_essentials".to_string(),
            title: "Add to Essentials".to_string(),
            description: "Promote a tab into the shared essentials grid".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "tab.remove_from_essentials".to_string(),
            title: "Remove from Essentials".to_string(),
            description: "Move an essential tab back to the active space".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "tab.toggle_essential".to_string(),
            title: "Toggle Essential Tab".to_string(),
            description: "Add or remove a tab from shared essentials".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "tab.toggle_mute".to_string(),
            title: "Mute / Unmute Tab".to_string(),
            description: "Toggle audio mute state on a tab".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "tab.duplicate".to_string(),
            title: "Duplicate Tab".to_string(),
            description: "Open a duplicate of the specified tab".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "tab.rename".to_string(),
            title: "Rename Tab".to_string(),
            description: "Assign or clear a custom title on a tab".to_string(),
            arguments: vec![
                ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab"),
                ArgumentSchema::new(
                    "title",
                    ArgumentType::String,
                    true,
                    "Custom title (empty clears the override)",
                ),
            ],
        },
        CommandDescriptor {
            name: "tab.start_rename".to_string(),
            title: "Start Renaming Tab".to_string(),
            description: "Focus the inline rename field for a tab".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "tab.move".to_string(),
            title: "Move Tab".to_string(),
            description: "Reorder a tab within normal tabs, pinned tabs, a folder, or essentials".to_string(),
            arguments: vec![
                ArgumentSchema::new("tab_id", ArgumentType::TabId, true, "Tab to move"),
                ArgumentSchema::new(
                    "section",
                    ArgumentType::String,
                    true,
                    "Destination section (normal, pinned, folder, or essential)",
                ),
                ArgumentSchema::new("index", ArgumentType::Integer, false, "Insertion index"),
                ArgumentSchema::new("space_id", ArgumentType::SpaceId, false, "Destination space"),
                ArgumentSchema::new("folder_id", ArgumentType::FolderId, false, "Destination folder"),
            ],
        },
        CommandDescriptor {
            name: "tab.move_to_folder".to_string(),
            title: "Move Tab to Folder".to_string(),
            description: "Move a tab into a folder or out of its current folder".to_string(),
            arguments: vec![
                ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab"),
                ArgumentSchema::new(
                    "folder_id",
                    ArgumentType::FolderId,
                    false,
                    "Destination folder (omit to remove from folder)",
                ),
            ],
        },
        CommandDescriptor {
            name: "tab.move_to_space".to_string(),
            title: "Move Tab to Space".to_string(),
            description: "Move a tab into another workspace".to_string(),
            arguments: vec![
                ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab"),
                ArgumentSchema::new("space_id", ArgumentType::SpaceId, true, "Destination space"),
            ],
        },
        CommandDescriptor {
            name: "folder.create".to_string(),
            title: "New Folder".to_string(),
            description: "Create a tab folder in the active or specified space".to_string(),
            arguments: vec![
                ArgumentSchema::new("name", ArgumentType::String, false, "Folder name"),
                ArgumentSchema::new("space_id", ArgumentType::SpaceId, false, "Target space"),
                ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Optional initial tab"),
            ],
        },
        CommandDescriptor {
            name: "folder.rename".to_string(),
            title: "Rename Folder".to_string(),
            description: "Change the name of a tab folder".to_string(),
            arguments: vec![
                ArgumentSchema::new("folder_id", ArgumentType::FolderId, true, "Target folder"),
                ArgumentSchema::new("name", ArgumentType::String, true, "New folder name"),
            ],
        },
        CommandDescriptor {
            name: "folder.start_rename".to_string(),
            title: "Start Renaming Folder".to_string(),
            description: "Focus the inline rename field for a folder".to_string(),
            arguments: vec![ArgumentSchema::new(
                "folder_id",
                ArgumentType::FolderId,
                true,
                "Target folder",
            )],
        },
        CommandDescriptor {
            name: "folder.delete".to_string(),
            title: "Delete Folder".to_string(),
            description: "Delete a folder and close its tabs".to_string(),
            arguments: vec![ArgumentSchema::new(
                "folder_id",
                ArgumentType::FolderId,
                true,
                "Target folder",
            )],
        },
        CommandDescriptor {
            name: "folder.unpack".to_string(),
            title: "Unpack Folder".to_string(),
            description: "Remove a folder while keeping its tabs".to_string(),
            arguments: vec![ArgumentSchema::new(
                "folder_id",
                ArgumentType::FolderId,
                true,
                "Target folder",
            )],
        },
        CommandDescriptor {
            name: "folder.toggle_collapsed".to_string(),
            title: "Toggle Folder Collapsed".to_string(),
            description: "Expand or collapse a tab folder".to_string(),
            arguments: vec![ArgumentSchema::new(
                "folder_id",
                ArgumentType::FolderId,
                true,
                "Target folder",
            )],
        },
        CommandDescriptor {
            name: "notification.dismiss".to_string(),
            title: "Dismiss Sidebar Notification".to_string(),
            description: "Dismiss a notification card from the sidebar".to_string(),
            arguments: vec![ArgumentSchema::new(
                "notification_id",
                ArgumentType::NotificationId,
                true,
                "Notification to dismiss",
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
        CommandDescriptor {
            name: "split.tabs".to_string(),
            title: "Split Tabs".to_string(),
            description: "Show tabs side by side in one split view".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "tab_ids",
                    ArgumentType::String,
                    true,
                    "Comma-separated tab ids (2 to 4)",
                ),
                ArgumentSchema::new(
                    "direction",
                    ArgumentType::SplitDirection,
                    false,
                    "horizontal, vertical or grid",
                ),
            ],
        },
        CommandDescriptor {
            name: "split.new_empty".to_string(),
            title: "New Split View".to_string(),
            description: "Open a new tab beside the current one".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "split.open_url".to_string(),
            title: "Open Link in Split View".to_string(),
            description: "Open a URL in a new tab split with the current tab".to_string(),
            arguments: vec![
                ArgumentSchema::new("url", ArgumentType::String, true, "URL to open"),
                ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Tab to split with"),
            ],
        },
        CommandDescriptor {
            name: "split.drop".to_string(),
            title: "Drop Tab Into Split View".to_string(),
            description: "Split a dragged tab onto one side of a pane".to_string(),
            arguments: vec![
                ArgumentSchema::new("tab_id", ArgumentType::TabId, true, "Dragged tab"),
                ArgumentSchema::new(
                    "target_tab_id",
                    ArgumentType::TabId,
                    true,
                    "Tab of the pane it lands on",
                ),
                ArgumentSchema::new("zone", ArgumentType::String, true, "left, right, top or bottom"),
            ],
        },
        CommandDescriptor {
            name: "split.remove_tab".to_string(),
            title: "Remove Tab From Split View".to_string(),
            description: "Take a tab out of its split view".to_string(),
            arguments: vec![ArgumentSchema::new(
                "tab_id",
                ArgumentType::TabId,
                true,
                "Tab to take out",
            )],
        },
        CommandDescriptor {
            name: "split.unsplit".to_string(),
            title: "Unsplit Tabs".to_string(),
            description: "Close the split view and keep its tabs".to_string(),
            arguments: vec![ArgumentSchema::new(
                "group_id",
                ArgumentType::Integer,
                false,
                "Split group",
            )],
        },
        CommandDescriptor {
            name: "split.set_layout".to_string(),
            title: "Change Split View Layout".to_string(),
            description: "Arrange the split view side by side, stacked or as a grid".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "direction",
                    ArgumentType::SplitDirection,
                    true,
                    "horizontal, vertical or grid",
                ),
                ArgumentSchema::new("group_id", ArgumentType::Integer, false, "Split group"),
            ],
        },
        CommandDescriptor {
            name: "split.resize".to_string(),
            title: "Resize Split View".to_string(),
            description: "Set the share of each pane".to_string(),
            arguments: vec![
                ArgumentSchema::new("group_id", ArgumentType::Integer, true, "Split group"),
                ArgumentSchema::new("ratios", ArgumentType::String, true, "Comma-separated fractions"),
            ],
        },
        CommandDescriptor {
            name: "glance.open".to_string(),
            title: "Open Link in Glance".to_string(),
            description: "Preview a page in a card over the current page".to_string(),
            arguments: vec![
                ArgumentSchema::new("url", ArgumentType::String, true, "URL to preview"),
                ArgumentSchema::new(
                    "parent_tab_id",
                    ArgumentType::TabId,
                    false,
                    "Tab the preview belongs to",
                ),
                ArgumentSchema::new(
                    "x",
                    ArgumentType::Float,
                    false,
                    "Animation origin, fraction of the page width",
                ),
                ArgumentSchema::new(
                    "y",
                    ArgumentType::Float,
                    false,
                    "Animation origin, fraction of the page height",
                ),
            ],
        },
        CommandDescriptor {
            name: "glance.close".to_string(),
            title: "Close Glance".to_string(),
            description: "Close the page preview".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "glance.expand".to_string(),
            title: "Expand Glance".to_string(),
            description: "Open the previewed page as a tab".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "glance.split".to_string(),
            title: "Split Glance".to_string(),
            description: "Show the previewed page beside the current one".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "toast.show".to_string(),
            title: "Show Toast".to_string(),
            description: "Show a short message over the page".to_string(),
            arguments: vec![
                ArgumentSchema::new("message", ArgumentType::String, true, "Text to show"),
                ArgumentSchema::new("timeout_ms", ArgumentType::Integer, false, "How long it stays up"),
                ArgumentSchema::new(
                    "action_label",
                    ArgumentType::String,
                    false,
                    "Label of the toast's button",
                ),
                ArgumentSchema::new("action_command", ArgumentType::String, false, "Command the button runs"),
            ],
        },
        CommandDescriptor {
            name: "toast.dismiss".to_string(),
            title: "Dismiss Toast".to_string(),
            description: "Hide a toast".to_string(),
            arguments: vec![ArgumentSchema::new("toast_id", ArgumentType::Integer, true, "Toast")],
        },
        media_session_descriptor(
            "media.session.toggle_play",
            "Play / Pause",
            "Play or pause background media",
        ),
        media_session_descriptor(
            "media.session.toggle_mute",
            "Mute / Unmute",
            "Mute or unmute background media",
        ),
        media_session_descriptor("media.session.previous", "Previous Track", "Skip to the previous track"),
        media_session_descriptor("media.session.next", "Next Track", "Skip to the next track"),
        media_session_descriptor(
            "media.session.pip",
            "Picture in Picture",
            "Show the media in picture-in-picture",
        ),
        media_session_descriptor(
            "media.session.focus",
            "Go to Media Tab",
            "Switch to the tab playing the media",
        ),
        media_session_descriptor(
            "media.session.dismiss",
            "Hide Media Controls",
            "Hide the controls for this media",
        ),
        CommandDescriptor {
            name: "media.session.seek".to_string(),
            title: "Seek Media".to_string(),
            description: "Move background media to a position".to_string(),
            arguments: vec![
                ArgumentSchema::new("session_id", ArgumentType::Integer, true, "Media session"),
                ArgumentSchema::new("position", ArgumentType::Float, true, "Position in seconds"),
            ],
        },
        CommandDescriptor {
            name: "layout.set_compact_hide".to_string(),
            title: "Set What Compact Mode Hides".to_string(),
            description: "Hide the sidebar, the toolbar or both in compact mode".to_string(),
            arguments: vec![ArgumentSchema::new(
                "hide",
                ArgumentType::String,
                true,
                "both, sidebar or toolbar",
            )],
        },
        CommandDescriptor {
            name: "layout.compact_reveal_sidebar".to_string(),
            title: "Show Sidebar in Compact Mode".to_string(),
            description: "Slide the hidden sidebar out, or back".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "layout.set_tab_switch_motion".to_string(),
            title: "Animate Tab Switches".to_string(),
            description: "Slide the page in when switching tabs".to_string(),
            arguments: vec![ArgumentSchema::new(
                "enabled",
                ArgumentType::Bool,
                true,
                "Whether to animate",
            )],
        },
        CommandDescriptor {
            name: "debug.sample_download".to_string(),
            title: "Start Sample Download (Debug)".to_string(),
            description: "Start a sample download to show the download animation".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "debug.sample_media".to_string(),
            title: "Play Sample Media (Debug)".to_string(),
            description: "Play sample media in a background tab to show the media controls".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "statusline.set_show".to_string(),
            title: "Set Statusline Visibility".to_string(),
            description:
                "Choose whether the vim mode indicator stays full size, shrinks to a badge, or hides when quiet"
                    .to_string(),
            arguments: vec![ArgumentSchema::new(
                "show",
                ArgumentType::String,
                true,
                "always, shrink, or never",
            )],
        },
        CommandDescriptor {
            name: "statusline.announce".to_string(),
            title: "Show Statusline Announcement".to_string(),
            description: "Display a short transient message beside the vim mode indicator".to_string(),
            arguments: vec![
                ArgumentSchema::new("message", ArgumentType::String, true, "Message to announce"),
                ArgumentSchema::new(
                    "timeout_ms",
                    ArgumentType::Integer,
                    false,
                    "Auto-clear delay in milliseconds",
                ),
            ],
        },
        CommandDescriptor {
            name: "statusline.clear_announcement".to_string(),
            title: "Clear Statusline Announcement".to_string(),
            description: "Dismiss the active statusline announcement".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "which_key.show_all".to_string(),
            title: "Show All Vim Keymaps".to_string(),
            description: "Open the which-key popup listing every mapping in the active vim mode".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "which_key.hide".to_string(),
            title: "Hide Which-Key Popup".to_string(),
            description: "Close the active which-key popup".to_string(),
            arguments: Vec::new(),
        },
    ]
}

fn media_session_descriptor(name: &str, title: &str, description: &str) -> CommandDescriptor {
    CommandDescriptor {
        name: name.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        arguments: vec![ArgumentSchema::new(
            "session_id",
            ArgumentType::Integer,
            true,
            "Media session",
        )],
    }
}
