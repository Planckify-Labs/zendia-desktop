/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Named, typed browser commands and their descriptors for keymaps and the command palette.

use crate::agent::{InstructionAttachment, InstructionId};
use crate::contributions::{SidePanelId, SidePanelLayout, SidePanelSide};
use crate::focus::{FocusDirection, PaneDirection};
use crate::history::HistoryClearRange;
use crate::input_state::StatuslineShow;
use crate::library::LibrarySection;
use crate::media::MediaControl;
use crate::model::{
    AgentId, ChatSiteId, CompactHide, ConversationId, DownloadId, FolderId, HistoryEntryId, LibraryMediaId,
    MediaSessionId, ModelEffort, ModelId, NotificationId, SidebarSide, SpaceId, SplitDirection, SplitGroupId, StepId,
    TabId, TabMoveTarget, ThemeOverrides, ThemeScheme, ToolbarLayout,
};
use crate::picker::PickerRequestId;
use crate::region::RegionRect;
use crate::site_info::{PermissionState, SitePermissionKind};
use crate::space_appearance::{SpaceAppearance, SpaceColorScheme, TextureKind};
use crate::space_routing::{RouteMatchType, RouteTarget, SpaceRouteId};
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
    PanelOpen {
        panel_id: SidePanelId,
        focus: bool,
    },
    PanelClose {
        panel_id: SidePanelId,
    },
    PanelSetLayout {
        panel_id: SidePanelId,
        layout: SidePanelLayout,
    },
    PanelSetSide {
        panel_id: SidePanelId,
        side: SidePanelSide,
    },
    PanelSetWidth {
        panel_id: SidePanelId,
        width: f32,
    },
    PanelResetWidth {
        panel_id: SidePanelId,
    },
    AgentToggle,
    AgentAsk,
    AgentSend {
        conversation_id: Option<ConversationId>,
        agent_id: Option<AgentId>,
        prompt: String,
    },
    AgentSendWithFiles {
        conversation_id: Option<ConversationId>,
        agent_id: Option<AgentId>,
        prompt: String,
        files: Vec<String>,
    },
    AgentStop {
        conversation_id: Option<ConversationId>,
    },
    AgentNew {
        tab_id: Option<TabId>,
    },
    AgentSelect {
        agent_id: Option<AgentId>,
    },
    AgentCycle {
        direction: i8,
    },
    AgentModel {
        agent_id: Option<AgentId>,
        model_id: Option<ModelId>,
    },
    AgentSetEffort {
        agent_id: Option<AgentId>,
        effort: Option<ModelEffort>,
    },
    AgentSetContext {
        agent_id: Option<AgentId>,
        context_id: String,
    },
    AgentConfirm {
        conversation_id: ConversationId,
        step_id: StepId,
        allow: bool,
    },
    AgentSwitchConversation {
        conversation_id: ConversationId,
    },
    AgentMarkSeen {
        conversation_id: ConversationId,
    },
    AgentOpenInstructions,
    AgentInstructionAdd,
    AgentInstructionUpdate {
        id: InstructionId,
        enabled: Option<bool>,
        title: Option<String>,
        text: Option<String>,
    },
    AgentInstructionRemove {
        id: InstructionId,
    },
    AgentInstructionSetFile {
        id: InstructionId,
        file: Option<InstructionAttachment>,
    },
    AgentSummarizeCaptcha {
        tab_id: Option<TabId>,
    },
    AgentLog {
        tab_id: Option<TabId>,
    },
    AgentList,
    /// Dismisses a conversation from the cross-window Agents roster.
    AgentDismiss {
        conversation_id: ConversationId,
    },
    /// Populates a rich sample agent conversation on the active tab for UI inspection.
    DebugSampleAgent,
    /// Toggles AgentServer between Running and Stopped so the service-down state can be inspected.
    DebugToggleAgentService,
    WebChatToggle,
    WebChatSelectSite {
        site_id: ChatSiteId,
    },
    WebChatAddSite {
        name: Option<String>,
        url: String,
    },
    WebChatRemoveSite {
        site_id: ChatSiteId,
    },
    WebChatReload {
        site_id: Option<ChatSiteId>,
    },
    WebChatNewChat {
        site_id: Option<ChatSiteId>,
    },
    WebChatOpenInTab {
        site_id: Option<ChatSiteId>,
    },
    WebChatSetDictating {
        listening: bool,
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
    /// Focuses the sidebar explorer in vim mode.
    ExplorerOpen,
    ExplorerClose,
    ExplorerMove {
        direction: FocusDirection,
    },
    ExplorerSelect {
        range: bool,
    },
    ExplorerActivate,
    ExplorerPick,
    FocusHintInput {
        key: String,
    },
    FocusHintCancel,
    /// Focuses toolbar buttons and the address bar in vim mode.
    ToolbarFocusOpen,
    ToolbarFocusClose,
    ToolbarFocusMove {
        direction: FocusDirection,
    },
    ToolbarFocusActivate,
    /// Enters window mode over split panes in vim mode.
    WindowModeOpen,
    WindowModeClose,
    WindowModeFocusPane {
        pane: usize,
    },
    WindowModeMovePane {
        direction: PaneDirection,
    },
    WindowModeCarryToggle,
    WindowModeDrop,
    /// Opens a Zendia picker modal by built-in name (e.g. `"tabs"`, `"agents"`) or registered request id.
    PickerOpen {
        name: Option<String>,
        request_id: Option<PickerRequestId>,
    },
    /// Chooses an item in the active picker and runs its associated command.
    PickerChoose {
        request_id: Option<PickerRequestId>,
        item_id: String,
    },
    /// Dismisses the active picker modal without choosing an item.
    PickerClose {
        request_id: Option<PickerRequestId>,
    },
    /// Opens the library, or closes it when it is open on `section` (or on any section when `section` is `None`).
    LibraryToggle {
        section: Option<LibrarySection>,
    },
    /// Opens the library on `section`, or on the section it last showed.
    LibraryOpen {
        section: Option<LibrarySection>,
    },
    /// Closes the library.
    LibraryClose,
    /// Reports whether the library panel holds keyboard focus, switching Vim mode between LIBRARY and NORMAL.
    LibraryFocusSet {
        focused: bool,
    },
    /// Opens a history visit in the current tab or a new (foreground or background) tab.
    HistoryOpen {
        entry_id: Option<HistoryEntryId>,
        url: Option<String>,
        new_tab: bool,
        in_background: bool,
    },
    /// Removes a single entry from browsing history.
    HistoryDelete {
        entry_id: HistoryEntryId,
    },
    /// Forgets all history entries and site data for a host.
    HistoryForgetSite {
        host: String,
    },
    /// Clears browsing history entries within `range` (defaults to all history).
    HistoryClear {
        range: HistoryClearRange,
    },
    /// Clears browsing history entries recorded on a single UTC day index.
    HistoryClearDay {
        day_index: i64,
    },
    /// Copies a history entry's URL to the clipboard.
    HistoryCopyUrl {
        entry_id: HistoryEntryId,
    },
    /// Creates a new workspace with an optional name and icon.
    SpaceCreate {
        name: Option<String>,
        icon: Option<String>,
    },
    /// Renames a workspace.
    SpaceRename {
        space_id: SpaceId,
        name: String,
    },
    /// Starts inline renaming of a workspace card in the library.
    SpaceStartRename {
        space_id: SpaceId,
    },
    /// Updates or clears a workspace's icon.
    SpaceSetIcon {
        space_id: SpaceId,
        icon: String,
    },
    /// Opens the icon picker for a workspace in the library.
    SpaceStartIconPicker {
        space_id: SpaceId,
    },
    /// Opens the theme picker for a workspace (placeholder until Phase 11).
    SpaceOpenThemePicker {
        space_id: SpaceId,
    },
    /// Reorders a workspace to another index.
    SpaceReorder {
        space_id: SpaceId,
        index: usize,
    },
    /// Deletes a workspace and moves its tabs to another space.
    SpaceDelete {
        space_id: SpaceId,
    },
    /// Unloads tabs in a workspace.
    SpaceUnload {
        space_id: SpaceId,
    },
    /// Unloads tabs in every other workspace.
    SpaceUnloadOthers {
        space_id: SpaceId,
    },
    /// Switches to a tab's workspace, activates the tab, and closes the library.
    SpaceOpenTab {
        tab_id: TabId,
    },
    /// Adds a URL space routing rule.
    SpaceRoutingAdd {
        match_type: RouteMatchType,
        pattern: String,
        target: RouteTarget,
    },
    /// Updates an existing URL space routing rule.
    SpaceRoutingUpdate {
        rule_id: SpaceRouteId,
        match_type: RouteMatchType,
        pattern: String,
        target: RouteTarget,
    },
    /// Deletes a URL space routing rule.
    SpaceRoutingDelete {
        rule_id: SpaceRouteId,
    },
    /// Reorders a URL space routing rule to another priority index.
    SpaceRoutingReorder {
        rule_id: SpaceRouteId,
        index: usize,
    },
    /// Sets the default destination for externally opened links.
    SpaceRoutingSetDefaultExternal {
        target: RouteTarget,
    },
    /// Opens the edit form for a space routing rule in the library.
    SpaceRoutingStartEdit {
        rule_id: SpaceRouteId,
    },
    /// Creates a space routing rule for a tab's domain and opens Space routing.
    SpaceRoutingAddDomain {
        tab_id: Option<TabId>,
    },
    /// Opens a completed download (or toggles open-when-done if still in progress).
    DownloadOpen {
        download_id: DownloadId,
    },
    /// Reveals a completed download in its containing folder.
    DownloadShowInFolder {
        download_id: DownloadId,
    },
    /// Pauses an in-progress download.
    DownloadPause {
        download_id: DownloadId,
    },
    /// Resumes a paused download.
    DownloadResume {
        download_id: DownloadId,
    },
    /// Retries a failed or cancelled download.
    DownloadRetry {
        download_id: DownloadId,
    },
    /// Cancels an active or paused download.
    DownloadCancel {
        download_id: DownloadId,
    },
    /// Removes one download entry from the library list.
    DownloadRemove {
        download_id: DownloadId,
    },
    /// Clears all finished (completed, failed, and cancelled) downloads from the library list.
    DownloadClearFinished,
    /// Copies a download's source URL to the clipboard.
    DownloadCopyLink {
        download_id: DownloadId,
    },
    /// Toggles opening an in-progress download as soon as it finishes.
    DownloadToggleOpenWhenDone {
        download_id: DownloadId,
    },
    /// Advances simulated stub downloads by one tick.
    DownloadTick,
    /// Turns the library's Media scan on or off (Zen's media opt-in).
    LibraryMediaSetEnabled {
        enabled: bool,
    },
    /// Rescans the library's Media items from folders and completed downloads.
    LibraryMediaRefresh,
    /// Opens the full-size media preview overlay on `media_id`.
    LibraryMediaPreviewOpen {
        media_id: LibraryMediaId,
    },
    /// Closes the full-size media preview overlay.
    LibraryMediaPreviewClose,
    /// Moves the open media preview by `delta` (`-1` previous, `+1` next).
    LibraryMediaPreviewStep {
        delta: i32,
    },
    /// Opens a media item in its external viewer.
    LibraryMediaOpen {
        media_id: LibraryMediaId,
    },
    /// Reveals a media item in its containing folder.
    LibraryMediaShowInFolder {
        media_id: LibraryMediaId,
    },
    /// Copies a media item to the clipboard.
    LibraryMediaCopy {
        media_id: LibraryMediaId,
    },
    /// Starts the page region picker overlay on the active or specified tab.
    RegionPick {
        tab_id: Option<TabId>,
    },
    /// Captures a chosen rectangle in page coordinates or the whole page.
    RegionCapture {
        tab_id: Option<TabId>,
        rect: Option<RegionRect>,
        whole_page: bool,
    },
    /// Cancels an active region pick without capturing.
    RegionCancel,
    /// Opens the sidebar space creation form.
    SpaceCreateFormOpen,
    /// Submits the sidebar space creation form with name, optional icon, profile, and appearance.
    SpaceCreateFormSubmit {
        name: String,
        icon: Option<String>,
        profile: Option<String>,
        appearance: Option<SpaceAppearance>,
    },
    /// Cancels and closes the sidebar space creation form.
    SpaceCreateFormCancel,
    /// Opens the gradient generator theme picker for `space_id` (or the active space).
    ThemePickerOpen {
        space_id: Option<SpaceId>,
    },
    /// Closes the gradient generator theme picker.
    ThemePickerClose,
    /// Updates a space's full gradient appearance configuration.
    SpaceAppearanceSet {
        space_id: Option<SpaceId>,
        appearance: SpaceAppearance,
    },
    /// Applies a predefined swatch preset `(page, index)` to a space.
    SpaceAppearanceApplyPreset {
        space_id: Option<SpaceId>,
        page: usize,
        index: usize,
    },
    /// Sets auto, light, or dark appearance scheme on a space and updates window scheme.
    SpaceAppearanceSetScheme {
        space_id: Option<SpaceId>,
        scheme: SpaceColorScheme,
    },
    /// Sets a space's gradient opacity intensity.
    SpaceAppearanceSetOpacity {
        space_id: Option<SpaceId>,
        opacity: f32,
    },
    /// Sets a space's texture kind and dial intensity.
    SpaceAppearanceSetTexture {
        space_id: Option<SpaceId>,
        kind: TextureKind,
        amount: f32,
    },
    /// Cycles the color harmony of a multi-dot space gradient.
    SpaceAppearanceCycleHarmony {
        space_id: Option<SpaceId>,
    },
    /// Resets a space's custom gradient dots back to the default theme.
    SpaceAppearanceReset {
        space_id: Option<SpaceId>,
    },
    /// Opens the share confirmation dialog for the current page, space, or folder.
    ShareOpen {
        tab_id: Option<TabId>,
        space_id: Option<SpaceId>,
        folder_id: Option<FolderId>,
    },
    /// Confirms sharing and copies the stub share link.
    ShareConfirm {
        display_name: String,
        dont_ask_again: bool,
        url: Option<String>,
        title: Option<String>,
    },
    /// Cancels and dismisses the share confirmation dialog.
    ShareCancel,
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

fn parse_optional_library_section_arg(
    command: &str,
    args: &[(String, String)],
) -> Result<Option<LibrarySection>, CommandError> {
    find_arg(args, "section")
        .map(|id| {
            LibrarySection::from_id(id).ok_or_else(|| CommandError::InvalidArgument {
                command: command.to_string(),
                message: format!("unknown library section `{id}`"),
            })
        })
        .transpose()
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
            Self::PanelOpen { .. } => "panel.open",
            Self::PanelClose { .. } => "panel.close",
            Self::PanelSetLayout { .. } => "panel.set_layout",
            Self::PanelSetSide { .. } => "panel.set_side",
            Self::PanelSetWidth { .. } => "panel.set_width",
            Self::PanelResetWidth { .. } => "panel.reset_width",
            Self::AgentToggle => "agent.toggle",
            Self::AgentAsk => "agent.ask",
            Self::AgentSend { .. } => "agent.send",
            Self::AgentSendWithFiles { .. } => "agent.send_with_files",
            Self::AgentStop { .. } => "agent.stop",
            Self::AgentNew { .. } => "agent.new",
            Self::AgentSelect { .. } => "agent.select",
            Self::AgentCycle { .. } => "agent.cycle",
            Self::AgentModel { .. } => "agent.model",
            Self::AgentSetEffort { .. } => "agent.set_effort",
            Self::AgentSetContext { .. } => "agent.set_context",
            Self::AgentConfirm { .. } => "agent.confirm",
            Self::AgentSwitchConversation { .. } => "agent.switch_conversation",
            Self::AgentMarkSeen { .. } => "agent.mark_seen",
            Self::AgentOpenInstructions => "agent.instructions.open",
            Self::AgentInstructionAdd => "agent.instructions.add",
            Self::AgentInstructionUpdate { .. } => "agent.instructions.update",
            Self::AgentInstructionRemove { .. } => "agent.instructions.remove",
            Self::AgentInstructionSetFile { .. } => "agent.instructions.set_file",
            Self::AgentSummarizeCaptcha { .. } => "agent.summarize_captcha",
            Self::AgentLog { .. } => "agent.log",
            Self::AgentList => "agent.list",
            Self::AgentDismiss { .. } => "agent.dismiss",
            Self::DebugSampleAgent => "debug.sample_agent",
            Self::DebugToggleAgentService => "debug.toggle_agent_service",
            Self::WebChatToggle => "webchat.toggle",
            Self::WebChatSelectSite { .. } => "webchat.select_site",
            Self::WebChatAddSite { .. } => "webchat.add_site",
            Self::WebChatRemoveSite { .. } => "webchat.remove_site",
            Self::WebChatReload { .. } => "webchat.reload",
            Self::WebChatNewChat { .. } => "webchat.new_chat",
            Self::WebChatOpenInTab { .. } => "webchat.open_in_tab",
            Self::WebChatSetDictating { .. } => "webchat.set_dictating",
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
            Self::ExplorerOpen => "explorer.toggle",
            Self::ExplorerClose => "explorer.close",
            Self::ExplorerMove { .. } => "explorer.move",
            Self::ExplorerSelect { .. } => "explorer.select",
            Self::ExplorerActivate => "explorer.activate",
            Self::ExplorerPick => "explorer.pick",
            Self::FocusHintInput { .. } => "focus.hint_input",
            Self::FocusHintCancel => "focus.hint_cancel",
            Self::ToolbarFocusOpen => "toolbar.open",
            Self::ToolbarFocusClose => "toolbar.focus_close",
            Self::ToolbarFocusMove { .. } => "toolbar.focus_move",
            Self::ToolbarFocusActivate => "toolbar.focus_activate",
            Self::WindowModeOpen => "window.mode",
            Self::WindowModeClose => "window_mode.close",
            Self::WindowModeFocusPane { .. } => "window_mode.focus_pane",
            Self::WindowModeMovePane { .. } => "window_mode.move_pane",
            Self::WindowModeCarryToggle => "window_mode.carry_toggle",
            Self::WindowModeDrop => "window_mode.drop",
            Self::PickerOpen { .. } => "picker.open",
            Self::PickerChoose { .. } => "picker.choose",
            Self::PickerClose { .. } => "picker.close",
            Self::LibraryToggle { .. } => "library.toggle",
            Self::LibraryOpen { .. } => "library.open",
            Self::LibraryClose => "library.close",
            Self::LibraryFocusSet { .. } => "library.focus",
            Self::HistoryOpen { .. } => "history.open",
            Self::HistoryDelete { .. } => "history.delete",
            Self::HistoryForgetSite { .. } => "history.forget_site",
            Self::HistoryClear { .. } => "history.clear",
            Self::HistoryClearDay { .. } => "history.clear_day",
            Self::HistoryCopyUrl { .. } => "history.copy_url",
            Self::SpaceCreate { .. } => "space.create",
            Self::SpaceRename { .. } => "space.rename",
            Self::SpaceStartRename { .. } => "space.start_rename",
            Self::SpaceSetIcon { .. } => "space.set_icon",
            Self::SpaceStartIconPicker { .. } => "space.start_icon_picker",
            Self::SpaceOpenThemePicker { .. } => "space.open_theme_picker",
            Self::SpaceReorder { .. } => "space.reorder",
            Self::SpaceDelete { .. } => "space.delete",
            Self::SpaceUnload { .. } => "space.unload",
            Self::SpaceUnloadOthers { .. } => "space.unload_others",
            Self::SpaceOpenTab { .. } => "space.open_tab",
            Self::SpaceRoutingAdd { .. } => "space_routing.add",
            Self::SpaceRoutingUpdate { .. } => "space_routing.update",
            Self::SpaceRoutingDelete { .. } => "space_routing.delete",
            Self::SpaceRoutingReorder { .. } => "space_routing.reorder",
            Self::SpaceRoutingSetDefaultExternal { .. } => "space_routing.set_default_external",
            Self::SpaceRoutingStartEdit { .. } => "space_routing.start_edit",
            Self::SpaceRoutingAddDomain { .. } => "space_routing.add_domain",
            Self::DownloadOpen { .. } => "download.open",
            Self::DownloadShowInFolder { .. } => "download.show_in_folder",
            Self::DownloadPause { .. } => "download.pause",
            Self::DownloadResume { .. } => "download.resume",
            Self::DownloadRetry { .. } => "download.retry",
            Self::DownloadCancel { .. } => "download.cancel",
            Self::DownloadRemove { .. } => "download.remove",
            Self::DownloadClearFinished => "download.clear_finished",
            Self::DownloadCopyLink { .. } => "download.copy_link",
            Self::DownloadToggleOpenWhenDone { .. } => "download.toggle_open_when_done",
            Self::DownloadTick => "download.tick",
            Self::LibraryMediaSetEnabled { .. } => "library.media.set_enabled",
            Self::LibraryMediaRefresh => "library.media.refresh",
            Self::LibraryMediaPreviewOpen { .. } => "library.media.preview.open",
            Self::LibraryMediaPreviewClose => "library.media.preview.close",
            Self::LibraryMediaPreviewStep { .. } => "library.media.preview.step",
            Self::LibraryMediaOpen { .. } => "library.media.open",
            Self::LibraryMediaShowInFolder { .. } => "library.media.show_in_folder",
            Self::LibraryMediaCopy { .. } => "library.media.copy",
            Self::RegionPick { .. } => "region.pick",
            Self::RegionCapture { .. } => "region.capture",
            Self::RegionCancel => "region.cancel",
            Self::SpaceCreateFormOpen => "space.create_form.open",
            Self::SpaceCreateFormSubmit { .. } => "space.create_form.submit",
            Self::SpaceCreateFormCancel => "space.create_form.cancel",
            Self::ThemePickerOpen { .. } => "theme.picker.open",
            Self::ThemePickerClose => "theme.picker.close",
            Self::SpaceAppearanceSet { .. } => "space.appearance.set",
            Self::SpaceAppearanceApplyPreset { .. } => "space.appearance.apply_preset",
            Self::SpaceAppearanceSetScheme { .. } => "space.appearance.set_scheme",
            Self::SpaceAppearanceSetOpacity { .. } => "space.appearance.set_opacity",
            Self::SpaceAppearanceSetTexture { .. } => "space.appearance.set_texture",
            Self::SpaceAppearanceCycleHarmony { .. } => "space.appearance.cycle_harmony",
            Self::SpaceAppearanceReset { .. } => "space.appearance.reset",
            Self::ShareOpen { .. } => "share.open",
            Self::ShareConfirm { .. } => "share.confirm",
            Self::ShareCancel => "share.cancel",
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
            "panel.open" => Ok(Self::PanelOpen {
                panel_id: SidePanelId::new(parse_required_string_arg(name, args, "panel_id")?),
                focus: parse_bool_arg(name, args, "focus", true)?,
            }),
            "panel.close" => Ok(Self::PanelClose {
                panel_id: SidePanelId::new(parse_required_string_arg(name, args, "panel_id")?),
            }),
            "panel.set_layout" => {
                let raw = parse_required_string_arg(name, args, "layout")?;
                let layout = match raw.trim().to_ascii_lowercase().as_str() {
                    "float" => SidePanelLayout::Float,
                    "slide" => SidePanelLayout::Slide,
                    "split" => SidePanelLayout::Split,
                    other => {
                        return Err(CommandError::InvalidArgument {
                            command: name.to_string(),
                            message: format!("unknown side panel layout `{other}`"),
                        });
                    }
                };
                Ok(Self::PanelSetLayout {
                    panel_id: SidePanelId::new(parse_required_string_arg(name, args, "panel_id")?),
                    layout,
                })
            }
            "panel.set_side" => {
                let raw = parse_required_string_arg(name, args, "side")?;
                let side = match raw.trim().to_ascii_lowercase().as_str() {
                    "left" => SidePanelSide::Left,
                    "right" => SidePanelSide::Right,
                    other => {
                        return Err(CommandError::InvalidArgument {
                            command: name.to_string(),
                            message: format!("unknown side panel side `{other}`"),
                        });
                    }
                };
                Ok(Self::PanelSetSide {
                    panel_id: SidePanelId::new(parse_required_string_arg(name, args, "panel_id")?),
                    side,
                })
            }
            "panel.set_width" => {
                let raw = parse_required_string_arg(name, args, "width")?;
                let width = raw.parse::<f32>().map_err(|_| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("argument `width` expected number, got `{raw}`"),
                })?;
                Ok(Self::PanelSetWidth {
                    panel_id: SidePanelId::new(parse_required_string_arg(name, args, "panel_id")?),
                    width,
                })
            }
            "panel.reset_width" => Ok(Self::PanelResetWidth {
                panel_id: SidePanelId::new(parse_required_string_arg(name, args, "panel_id")?),
            }),
            "agent.toggle" | "zendia.chat.toggle" => Ok(Self::AgentToggle),
            "agent.ask" => Ok(Self::AgentAsk),
            "agent.send" => Ok(Self::AgentSend {
                conversation_id: parse_optional_u64_arg(name, args, "conversation_id")?.map(ConversationId),
                agent_id: find_arg(args, "agent_id").map(AgentId::new),
                prompt: parse_required_string_arg(name, args, "prompt")?,
            }),
            "agent.send_with_files" => {
                let files = find_arg(args, "files")
                    .map(|raw| {
                        raw.split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(Self::AgentSendWithFiles {
                    conversation_id: parse_optional_u64_arg(name, args, "conversation_id")?.map(ConversationId),
                    agent_id: find_arg(args, "agent_id").map(AgentId::new),
                    prompt: find_arg(args, "prompt").unwrap_or("").to_string(),
                    files,
                })
            }
            "agent.stop" => Ok(Self::AgentStop {
                conversation_id: parse_optional_u64_arg(name, args, "conversation_id")?.map(ConversationId),
            }),
            "agent.new" => Ok(Self::AgentNew {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "agent.select" => Ok(Self::AgentSelect {
                agent_id: find_arg(args, "agent_id").map(AgentId::new),
            }),
            "agent.cycle" => {
                let direction = match find_arg(args, "direction") {
                    Some("previous") | Some("-1") => -1,
                    _ => 1,
                };
                Ok(Self::AgentCycle { direction })
            }
            "agent.model" | "agent.set_model" => Ok(Self::AgentModel {
                agent_id: find_arg(args, "agent_id").map(AgentId::new),
                model_id: find_arg(args, "model_id").filter(|s| !s.is_empty()).map(ModelId::new),
            }),
            "agent.set_effort" => {
                let effort = match find_arg(args, "effort") {
                    None | Some("") | Some("default") => None,
                    Some(raw) => Some(ModelEffort::parse(raw).ok_or_else(|| CommandError::InvalidArgument {
                        command: name.to_string(),
                        message: format!("unknown effort level `{raw}`"),
                    })?),
                };
                Ok(Self::AgentSetEffort {
                    agent_id: find_arg(args, "agent_id").map(AgentId::new),
                    effort,
                })
            }
            "agent.set_context" => Ok(Self::AgentSetContext {
                agent_id: find_arg(args, "agent_id").map(AgentId::new),
                context_id: parse_required_string_arg(name, args, "context_id")?,
            }),
            "agent.confirm" => Ok(Self::AgentConfirm {
                conversation_id: ConversationId(parse_required_u64_arg(name, args, "conversation_id")?),
                step_id: StepId(parse_required_u64_arg(name, args, "step_id")?),
                allow: parse_bool_arg(name, args, "allow", false)?,
            }),
            "agent.switch_conversation" => Ok(Self::AgentSwitchConversation {
                conversation_id: ConversationId(parse_required_u64_arg(name, args, "conversation_id")?),
            }),
            "agent.mark_seen" => Ok(Self::AgentMarkSeen {
                conversation_id: ConversationId(parse_required_u64_arg(name, args, "conversation_id")?),
            }),
            "agent.instructions.open" => Ok(Self::AgentOpenInstructions),
            "agent.instructions.add" => Ok(Self::AgentInstructionAdd),
            "agent.instructions.update" => Ok(Self::AgentInstructionUpdate {
                id: InstructionId(parse_required_u64_arg(name, args, "id")?),
                enabled: find_arg(args, "enabled").map(|v| v == "true"),
                title: find_arg(args, "title").map(str::to_string),
                text: find_arg(args, "text").map(str::to_string),
            }),
            "agent.instructions.remove" => Ok(Self::AgentInstructionRemove {
                id: InstructionId(parse_required_u64_arg(name, args, "id")?),
            }),
            "agent.instructions.set_file" => Ok(Self::AgentInstructionSetFile {
                id: InstructionId(parse_required_u64_arg(name, args, "id")?),
                file: find_arg(args, "name").map(|file_name| InstructionAttachment {
                    name: file_name.to_string(),
                    path: find_arg(args, "path").unwrap_or(file_name).to_string(),
                    size_bytes: find_arg(args, "size_bytes")
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0),
                    kind: crate::agent::AttachmentKind::from_filename(file_name),
                }),
            }),
            "agent.summarize_captcha" => Ok(Self::AgentSummarizeCaptcha {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "agent.log" => Ok(Self::AgentLog {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "agent.list" | "zendia.agents.toggle" => Ok(Self::AgentList),
            "agent.dismiss" => Ok(Self::AgentDismiss {
                conversation_id: ConversationId(parse_required_u64_arg(name, args, "conversation_id")?),
            }),
            "debug.sample_agent" => Ok(Self::DebugSampleAgent),
            "debug.toggle_agent_service" => Ok(Self::DebugToggleAgentService),
            "webchat.toggle" | "zendia.webchat.toggle" => Ok(Self::WebChatToggle),
            "webchat.select_site" => Ok(Self::WebChatSelectSite {
                site_id: ChatSiteId::new(parse_required_string_arg(name, args, "site_id")?),
            }),
            "webchat.add_site" => Ok(Self::WebChatAddSite {
                name: find_arg(args, "name").map(ToOwned::to_owned),
                url: parse_required_string_arg(name, args, "url")?,
            }),
            "webchat.remove_site" => Ok(Self::WebChatRemoveSite {
                site_id: ChatSiteId::new(parse_required_string_arg(name, args, "site_id")?),
            }),
            "webchat.reload" => Ok(Self::WebChatReload {
                site_id: find_arg(args, "site_id").map(ChatSiteId::new),
            }),
            "webchat.new_chat" => Ok(Self::WebChatNewChat {
                site_id: find_arg(args, "site_id").map(ChatSiteId::new),
            }),
            "webchat.open_in_tab" => Ok(Self::WebChatOpenInTab {
                site_id: find_arg(args, "site_id").map(ChatSiteId::new),
            }),
            "webchat.set_dictating" => Ok(Self::WebChatSetDictating {
                listening: find_arg(args, "listening")
                    .is_some_and(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes")),
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
            // Entering the focus surfaces uses the names of the vim profile (Base/res/keyboard/vim.toml).
            "explorer.toggle" => Ok(Self::ExplorerOpen),
            "explorer.close" => Ok(Self::ExplorerClose),
            "explorer.move" => {
                let raw = parse_required_string_arg(name, args, "direction")?;
                let direction = FocusDirection::parse(&raw).ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("unknown focus direction `{raw}`"),
                })?;
                Ok(Self::ExplorerMove { direction })
            }
            "explorer.select" => Ok(Self::ExplorerSelect {
                range: parse_bool_arg(name, args, "range", false)?,
            }),
            "explorer.activate" => Ok(Self::ExplorerActivate),
            "explorer.pick" => Ok(Self::ExplorerPick),
            "focus.hint_input" => Ok(Self::FocusHintInput {
                key: parse_required_string_arg(name, args, "key")?,
            }),
            "focus.hint_cancel" => Ok(Self::FocusHintCancel),
            "toolbar.open" | "toolbar.focus_open" => Ok(Self::ToolbarFocusOpen),
            "toolbar.focus_close" => Ok(Self::ToolbarFocusClose),
            "toolbar.focus_move" => {
                let raw = parse_required_string_arg(name, args, "direction")?;
                let direction = FocusDirection::parse(&raw).ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("unknown focus direction `{raw}`"),
                })?;
                Ok(Self::ToolbarFocusMove { direction })
            }
            "toolbar.focus_activate" => Ok(Self::ToolbarFocusActivate),
            "window.mode" | "window_mode.open" => Ok(Self::WindowModeOpen),
            "window_mode.close" => Ok(Self::WindowModeClose),
            "window_mode.focus_pane" => Ok(Self::WindowModeFocusPane {
                pane: parse_required_u64_arg(name, args, "pane")? as usize,
            }),
            "window_mode.move_pane" => {
                let raw = parse_required_string_arg(name, args, "direction")?;
                let direction = PaneDirection::parse(&raw).ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("unknown pane direction `{raw}`"),
                })?;
                Ok(Self::WindowModeMovePane { direction })
            }
            "window_mode.carry_toggle" => Ok(Self::WindowModeCarryToggle),
            "window_mode.drop" => Ok(Self::WindowModeDrop),
            "picker.open" => Ok(Self::PickerOpen {
                name: find_arg(args, "name").map(str::to_string),
                request_id: find_arg(args, "request_id").map(PickerRequestId::new),
            }),
            "picker.choose" => Ok(Self::PickerChoose {
                request_id: find_arg(args, "request_id").map(PickerRequestId::new),
                item_id: parse_required_string_arg(name, args, "item_id")?,
            }),
            "picker.close" => Ok(Self::PickerClose {
                request_id: find_arg(args, "request_id").map(PickerRequestId::new),
            }),
            "library.toggle" => Ok(Self::LibraryToggle {
                section: parse_optional_library_section_arg(name, args)?,
            }),
            "library.open" => {
                if find_arg(args, "section") == Some("bookmarks") {
                    Ok(Self::TabNew {
                        url: Some("about:bookmarks".to_string()),
                        after: None,
                        activate: true,
                    })
                } else {
                    Ok(Self::LibraryOpen {
                        section: parse_optional_library_section_arg(name, args)?,
                    })
                }
            }
            "library.close" => Ok(Self::LibraryClose),
            "library.focus" => Ok(Self::LibraryFocusSet {
                focused: parse_bool_arg(name, args, "focused", true)?,
            }),
            "history.open" => Ok(Self::HistoryOpen {
                entry_id: parse_optional_u64_arg(name, args, "entry_id")?.map(HistoryEntryId),
                url: find_arg(args, "url").map(str::to_string),
                new_tab: parse_bool_arg(name, args, "new_tab", false)?,
                in_background: parse_bool_arg(name, args, "in_background", false)?,
            }),
            "history.delete" => Ok(Self::HistoryDelete {
                entry_id: HistoryEntryId(parse_required_u64_arg(name, args, "entry_id")?),
            }),
            "history.forget_site" => Ok(Self::HistoryForgetSite {
                host: parse_required_string_arg(name, args, "host")?,
            }),
            "history.clear" => {
                let range = match find_arg(args, "range") {
                    None => HistoryClearRange::All,
                    Some(raw) => HistoryClearRange::parse(raw).ok_or_else(|| CommandError::InvalidArgument {
                        command: name.to_string(),
                        message: format!("unknown history clear range `{raw}`"),
                    })?,
                };
                Ok(Self::HistoryClear { range })
            }
            "history.clear_day" => {
                let raw = parse_required_string_arg(name, args, "day_index")?;
                let day_index = raw.parse::<i64>().map_err(|_| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("argument `day_index` expected integer, got `{raw}`"),
                })?;
                Ok(Self::HistoryClearDay { day_index })
            }
            "history.copy_url" => Ok(Self::HistoryCopyUrl {
                entry_id: HistoryEntryId(parse_required_u64_arg(name, args, "entry_id")?),
            }),
            "download.open" => Ok(Self::DownloadOpen {
                download_id: DownloadId(parse_required_u64_arg(name, args, "download_id")?),
            }),
            "download.show_in_folder" => Ok(Self::DownloadShowInFolder {
                download_id: DownloadId(parse_required_u64_arg(name, args, "download_id")?),
            }),
            "download.pause" => Ok(Self::DownloadPause {
                download_id: DownloadId(parse_required_u64_arg(name, args, "download_id")?),
            }),
            "download.resume" => Ok(Self::DownloadResume {
                download_id: DownloadId(parse_required_u64_arg(name, args, "download_id")?),
            }),
            "download.retry" => Ok(Self::DownloadRetry {
                download_id: DownloadId(parse_required_u64_arg(name, args, "download_id")?),
            }),
            "download.cancel" => Ok(Self::DownloadCancel {
                download_id: DownloadId(parse_required_u64_arg(name, args, "download_id")?),
            }),
            "download.remove" => Ok(Self::DownloadRemove {
                download_id: DownloadId(parse_required_u64_arg(name, args, "download_id")?),
            }),
            "download.clear_finished" => Ok(Self::DownloadClearFinished),
            "download.copy_link" => Ok(Self::DownloadCopyLink {
                download_id: DownloadId(parse_required_u64_arg(name, args, "download_id")?),
            }),
            "download.toggle_open_when_done" => Ok(Self::DownloadToggleOpenWhenDone {
                download_id: DownloadId(parse_required_u64_arg(name, args, "download_id")?),
            }),
            "download.tick" => Ok(Self::DownloadTick),
            "library.media.set_enabled" => Ok(Self::LibraryMediaSetEnabled {
                enabled: parse_bool_arg(name, args, "enabled", true)?,
            }),
            "library.media.refresh" => Ok(Self::LibraryMediaRefresh),
            "library.media.preview.open" => Ok(Self::LibraryMediaPreviewOpen {
                media_id: LibraryMediaId(parse_required_u64_arg(name, args, "media_id")?),
            }),
            "library.media.preview.close" => Ok(Self::LibraryMediaPreviewClose),
            "library.media.preview.step" => {
                let raw = find_arg(args, "delta").ok_or_else(|| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: "missing required argument `delta`".to_string(),
                })?;
                let delta = raw.parse::<i32>().map_err(|_| CommandError::InvalidArgument {
                    command: name.to_string(),
                    message: format!("argument `delta` expected i32, got `{raw}`"),
                })?;
                Ok(Self::LibraryMediaPreviewStep { delta })
            }
            "library.media.open" => Ok(Self::LibraryMediaOpen {
                media_id: LibraryMediaId(parse_required_u64_arg(name, args, "media_id")?),
            }),
            "library.media.show_in_folder" => Ok(Self::LibraryMediaShowInFolder {
                media_id: LibraryMediaId(parse_required_u64_arg(name, args, "media_id")?),
            }),
            "library.media.copy" => Ok(Self::LibraryMediaCopy {
                media_id: LibraryMediaId(parse_required_u64_arg(name, args, "media_id")?),
            }),
            "region.pick" => Ok(Self::RegionPick {
                tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
            }),
            "region.capture" => {
                let x = parse_optional_f32_arg(name, args, "x")?;
                let y = parse_optional_f32_arg(name, args, "y")?;
                let width = parse_optional_f32_arg(name, args, "width")?;
                let height = parse_optional_f32_arg(name, args, "height")?;
                let rect = match (x, y, width, height) {
                    (Some(x), Some(y), Some(width), Some(height)) => Some(RegionRect::new(x, y, width, height)),
                    (None, None, None, None) => None,
                    _ => {
                        return Err(CommandError::InvalidArgument {
                            command: name.to_string(),
                            message: "region.capture requires all of x, y, width, height when specifying a rect"
                                .to_string(),
                        });
                    }
                };
                let whole_page = parse_bool_arg(name, args, "whole_page", rect.is_none())?;
                Ok(Self::RegionCapture {
                    tab_id: parse_optional_u64_arg(name, args, "tab_id")?.map(TabId),
                    rect,
                    whole_page,
                })
            }
            "region.cancel" => Ok(Self::RegionCancel),
            _ => {
                if let Some(cmd) = crate::space_routing::parse_command(name, args)? {
                    return Ok(cmd);
                }
                if let Some(cmd) = crate::space_appearance::parse_command(name, args)? {
                    return Ok(cmd);
                }
                if let Some(cmd) = crate::share::parse_command(name, args)? {
                    return Ok(cmd);
                }
                Ok(Self::vim_profile_alias(name).unwrap_or_else(|| Self::Custom {
                    name: name.to_string(),
                    args: args.to_vec(),
                }))
            }
        }
    }

    /// The vim profile, ported from the fork, names some focus actions its own way. Those names parse to the same
    /// typed commands so keys from either source reach one implementation.
    fn vim_profile_alias(name: &str) -> Option<Self> {
        use crate::focus::FocusDirection as Dir;
        let explorer = |direction| Self::ExplorerMove { direction };
        let toolbar = |direction| Self::ToolbarFocusMove { direction };
        Some(match name {
            "explorer.open" => Self::ExplorerActivate,
            "explorer.down" => explorer(Dir::Next),
            "explorer.up" => explorer(Dir::Previous),
            "explorer.first" => explorer(Dir::First),
            "explorer.last" => explorer(Dir::Last),
            "explorer.right" => explorer(Dir::Into),
            "explorer.collapse" => explorer(Dir::Out),
            "explorer.quit" => Self::ExplorerClose,
            "explorer.jump" => Self::ExplorerPick,
            "explorer.visual" => Self::ExplorerSelect { range: true },
            "explorer.toggle_select" => Self::ExplorerSelect { range: false },
            "toolbar.left" => toolbar(Dir::Previous),
            "toolbar.right" => toolbar(Dir::Next),
            "toolbar.first" => toolbar(Dir::First),
            "toolbar.last" => toolbar(Dir::Last),
            "toolbar.press" => Self::ToolbarFocusActivate,
            "toolbar.quit" => Self::ToolbarFocusClose,
            "window.quit" | "window.escape" => Self::WindowModeClose,
            "window.drop" => Self::WindowModeDrop,
            _ => return None,
        })
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
    let mut descriptors = vec![
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
            name: "agent.dismiss".to_string(),
            title: "Dismiss Agent Run".to_string(),
            description: "Remove a conversation from the Agents roster".to_string(),
            arguments: vec![ArgumentSchema::new(
                "conversation_id",
                ArgumentType::ConversationId,
                true,
                "Conversation to dismiss",
            )],
        },
        CommandDescriptor {
            name: "webchat.toggle".to_string(),
            title: "Toggle Web Chat Panel".to_string(),
            description: "Open or close the web chat side panel".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "webchat.select_site".to_string(),
            title: "Select Web Chat Site".to_string(),
            description: "Switch the active chat site in the web chat panel".to_string(),
            arguments: vec![ArgumentSchema::new(
                "site_id",
                ArgumentType::ChatSiteId,
                true,
                "Chat site to activate",
            )],
        },
        CommandDescriptor {
            name: "webchat.add_site".to_string(),
            title: "Add Web Chat Site".to_string(),
            description: "Add a custom web chat URL to the site strip".to_string(),
            arguments: vec![
                ArgumentSchema::new("url", ArgumentType::String, true, "Site URL"),
                ArgumentSchema::new("name", ArgumentType::String, false, "Optional display name"),
            ],
        },
        CommandDescriptor {
            name: "webchat.remove_site".to_string(),
            title: "Remove Web Chat Site".to_string(),
            description: "Remove a custom chat site from the web chat strip".to_string(),
            arguments: vec![ArgumentSchema::new(
                "site_id",
                ArgumentType::ChatSiteId,
                true,
                "Chat site to remove",
            )],
        },
        CommandDescriptor {
            name: "webchat.reload".to_string(),
            title: "Reload Web Chat Site".to_string(),
            description: "Reload the active web chat site view".to_string(),
            arguments: vec![ArgumentSchema::new(
                "site_id",
                ArgumentType::ChatSiteId,
                false,
                "Optional site to reload (defaults to active)",
            )],
        },
        CommandDescriptor {
            name: "webchat.new_chat".to_string(),
            title: "New Web Chat".to_string(),
            description: "Navigate the active web chat site back to its home URL".to_string(),
            arguments: vec![ArgumentSchema::new(
                "site_id",
                ArgumentType::ChatSiteId,
                false,
                "Optional site to reset (defaults to active)",
            )],
        },
        CommandDescriptor {
            name: "webchat.open_in_tab".to_string(),
            title: "Open Web Chat in Tab".to_string(),
            description: "Open the active web chat site in a regular browser tab".to_string(),
            arguments: vec![ArgumentSchema::new(
                "site_id",
                ArgumentType::ChatSiteId,
                false,
                "Optional site to open (defaults to active)",
            )],
        },
        CommandDescriptor {
            name: "webchat.set_dictating".to_string(),
            title: "Set Web Chat Dictation State".to_string(),
            description: "Toggle the web chat voice dictation listening indicator".to_string(),
            arguments: vec![ArgumentSchema::new(
                "listening",
                ArgumentType::Bool,
                true,
                "Whether dictation is listening",
            )],
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
            name: "library.toggle".to_string(),
            title: "Toggle Library".to_string(),
            description: "Open or close the library".to_string(),
            arguments: vec![ArgumentSchema::new(
                "section",
                ArgumentType::String,
                false,
                "Section to open (history, downloads, media, spaces, space-routing)",
            )],
        },
        CommandDescriptor {
            name: "library.open".to_string(),
            title: "Open Library Section".to_string(),
            description: "Open the library on a section".to_string(),
            arguments: vec![ArgumentSchema::new(
                "section",
                ArgumentType::String,
                false,
                "Section to open (history, downloads, media, spaces, space-routing)",
            )],
        },
        CommandDescriptor {
            name: "library.close".to_string(),
            title: "Close Library".to_string(),
            description: "Close the library".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "library.focus".to_string(),
            title: "Set Library Keyboard Focus".to_string(),
            description: "Enter or leave LIBRARY input mode as library keyboard focus changes".to_string(),
            arguments: vec![ArgumentSchema::new(
                "focused",
                ArgumentType::Bool,
                false,
                "Whether the library holds keyboard focus",
            )],
        },
        CommandDescriptor {
            name: "history.open".to_string(),
            title: "Open History Entry".to_string(),
            description: "Open a history visit in the current tab or a new tab".to_string(),
            arguments: vec![
                ArgumentSchema::new("entry_id", ArgumentType::Integer, false, "History entry id"),
                ArgumentSchema::new("url", ArgumentType::String, false, "Direct URL to open"),
                ArgumentSchema::new("new_tab", ArgumentType::Bool, false, "Open in a new tab"),
                ArgumentSchema::new(
                    "in_background",
                    ArgumentType::Bool,
                    false,
                    "Keep the new tab in the background and leave the library open",
                ),
            ],
        },
        CommandDescriptor {
            name: "history.delete".to_string(),
            title: "Forget History Entry".to_string(),
            description: "Remove a single visit from browsing history".to_string(),
            arguments: vec![ArgumentSchema::new(
                "entry_id",
                ArgumentType::Integer,
                true,
                "History entry id",
            )],
        },
        CommandDescriptor {
            name: "history.forget_site".to_string(),
            title: "Forget About This Site".to_string(),
            description: "Remove history entries and site data for a host".to_string(),
            arguments: vec![ArgumentSchema::new(
                "host",
                ArgumentType::String,
                true,
                "Hostname to forget",
            )],
        },
        CommandDescriptor {
            name: "history.clear".to_string(),
            title: "Clear History".to_string(),
            description: "Clear browsing history for a time range or everything".to_string(),
            arguments: vec![ArgumentSchema::new(
                "range",
                ArgumentType::String,
                false,
                "Time range (last_hour, last_12_hours, today, last_week, last_month, all)",
            )],
        },
        CommandDescriptor {
            name: "history.clear_day".to_string(),
            title: "Clear History Day".to_string(),
            description: "Clear browsing history entries for a single day".to_string(),
            arguments: vec![ArgumentSchema::new(
                "day_index",
                ArgumentType::Integer,
                true,
                "UTC day index since epoch",
            )],
        },
        CommandDescriptor {
            name: "history.copy_url".to_string(),
            title: "Copy History URL".to_string(),
            description: "Copy a history entry's URL to the clipboard".to_string(),
            arguments: vec![ArgumentSchema::new(
                "entry_id",
                ArgumentType::Integer,
                true,
                "History entry id",
            )],
        },
        CommandDescriptor {
            name: "download.open".to_string(),
            title: "Open Download".to_string(),
            description: "Open a completed download or toggle open-when-done for an active one".to_string(),
            arguments: vec![ArgumentSchema::new(
                "download_id",
                ArgumentType::Integer,
                true,
                "Target download id",
            )],
        },
        CommandDescriptor {
            name: "download.show_in_folder".to_string(),
            title: "Show Download in Folder".to_string(),
            description: "Reveal a completed download in its containing folder".to_string(),
            arguments: vec![ArgumentSchema::new(
                "download_id",
                ArgumentType::Integer,
                true,
                "Target download id",
            )],
        },
        CommandDescriptor {
            name: "download.pause".to_string(),
            title: "Pause Download".to_string(),
            description: "Pause an in-progress download".to_string(),
            arguments: vec![ArgumentSchema::new(
                "download_id",
                ArgumentType::Integer,
                true,
                "Target download id",
            )],
        },
        CommandDescriptor {
            name: "download.resume".to_string(),
            title: "Resume Download".to_string(),
            description: "Resume a paused download".to_string(),
            arguments: vec![ArgumentSchema::new(
                "download_id",
                ArgumentType::Integer,
                true,
                "Target download id",
            )],
        },
        CommandDescriptor {
            name: "download.retry".to_string(),
            title: "Retry Download".to_string(),
            description: "Restart a failed or cancelled download".to_string(),
            arguments: vec![ArgumentSchema::new(
                "download_id",
                ArgumentType::Integer,
                true,
                "Target download id",
            )],
        },
        CommandDescriptor {
            name: "download.cancel".to_string(),
            title: "Cancel Download".to_string(),
            description: "Cancel an active or paused download".to_string(),
            arguments: vec![ArgumentSchema::new(
                "download_id",
                ArgumentType::Integer,
                true,
                "Target download id",
            )],
        },
        CommandDescriptor {
            name: "download.remove".to_string(),
            title: "Remove Download".to_string(),
            description: "Remove a download entry from the library list".to_string(),
            arguments: vec![ArgumentSchema::new(
                "download_id",
                ArgumentType::Integer,
                true,
                "Target download id",
            )],
        },
        CommandDescriptor {
            name: "download.clear_finished".to_string(),
            title: "Clear Finished Downloads".to_string(),
            description: "Remove all completed, failed, and cancelled downloads from the library".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "download.copy_link".to_string(),
            title: "Copy Download Link".to_string(),
            description: "Copy a download's source URL to the clipboard".to_string(),
            arguments: vec![ArgumentSchema::new(
                "download_id",
                ArgumentType::Integer,
                true,
                "Target download id",
            )],
        },
        CommandDescriptor {
            name: "download.toggle_open_when_done".to_string(),
            title: "Toggle Open Download When Done".to_string(),
            description: "Toggle opening an in-progress download automatically when it finishes".to_string(),
            arguments: vec![ArgumentSchema::new(
                "download_id",
                ArgumentType::Integer,
                true,
                "Target download id",
            )],
        },
        CommandDescriptor {
            name: "download.tick".to_string(),
            title: "Tick Download Progress".to_string(),
            description: "Advance active stub transfers by one progress step".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "library.media.set_enabled".to_string(),
            title: "Toggle Library Media".to_string(),
            description: "Enable or disable showing local media in the library".to_string(),
            arguments: vec![ArgumentSchema::new(
                "enabled",
                ArgumentType::Bool,
                false,
                "Whether to enable Media in the library (default true)",
            )],
        },
        CommandDescriptor {
            name: "library.media.refresh".to_string(),
            title: "Refresh Library Media".to_string(),
            description: "Rescan media files from folders and completed downloads".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "library.media.preview.open".to_string(),
            title: "Open Media Preview".to_string(),
            description: "Open the full-size preview for a media item".to_string(),
            arguments: vec![ArgumentSchema::new(
                "media_id",
                ArgumentType::Integer,
                true,
                "Target media item id",
            )],
        },
        CommandDescriptor {
            name: "library.media.preview.close".to_string(),
            title: "Close Media Preview".to_string(),
            description: "Close the full-size media preview overlay".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "library.media.preview.step".to_string(),
            title: "Step Media Preview".to_string(),
            description: "Move the open media preview to the previous or next item".to_string(),
            arguments: vec![ArgumentSchema::new(
                "delta",
                ArgumentType::Integer,
                true,
                "Step offset (-1 for previous, +1 for next)",
            )],
        },
        CommandDescriptor {
            name: "library.media.open".to_string(),
            title: "Open Media File".to_string(),
            description: "Open a media file in its default viewer".to_string(),
            arguments: vec![ArgumentSchema::new(
                "media_id",
                ArgumentType::Integer,
                true,
                "Target media item id",
            )],
        },
        CommandDescriptor {
            name: "library.media.show_in_folder".to_string(),
            title: "Show Media in Folder".to_string(),
            description: "Reveal a media file in its containing folder".to_string(),
            arguments: vec![ArgumentSchema::new(
                "media_id",
                ArgumentType::Integer,
                true,
                "Target media item id",
            )],
        },
        CommandDescriptor {
            name: "library.media.copy".to_string(),
            title: "Copy Media".to_string(),
            description: "Copy a media item to the clipboard".to_string(),
            arguments: vec![ArgumentSchema::new(
                "media_id",
                ArgumentType::Integer,
                true,
                "Target media item id",
            )],
        },
        CommandDescriptor {
            name: "picker.open".to_string(),
            title: "Open Picker".to_string(),
            description: "Open a built-in (tabs, agents) or registered Zendia picker modal".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "name",
                    ArgumentType::String,
                    false,
                    "Built-in picker name (tabs or agents)",
                ),
                ArgumentSchema::new(
                    "request_id",
                    ArgumentType::String,
                    false,
                    "Registered custom picker request id",
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
        CommandDescriptor {
            name: "explorer.toggle".to_string(),
            title: "Open Explorer Focus".to_string(),
            description: "Focus sidebar rows in vim explorer mode".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "explorer.close".to_string(),
            title: "Close Explorer Focus".to_string(),
            description: "Leave vim explorer mode".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "explorer.move".to_string(),
            title: "Move Explorer Cursor".to_string(),
            description: "Move the explorer cursor over sidebar rows".to_string(),
            arguments: vec![ArgumentSchema::new(
                "direction",
                ArgumentType::String,
                true,
                "Direction to move (next, previous, first, last, into, out)",
            )],
        },
        CommandDescriptor {
            name: "explorer.select".to_string(),
            title: "Select Explorer Row".to_string(),
            description: "Toggle selection on the focused sidebar row or start visual range selection".to_string(),
            arguments: vec![ArgumentSchema::new(
                "range",
                ArgumentType::Bool,
                false,
                "Toggle visual range selection instead of picking one row",
            )],
        },
        CommandDescriptor {
            name: "explorer.activate".to_string(),
            title: "Activate Explorer Row".to_string(),
            description: "Activate the focused tab or toggle the focused folder".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "explorer.pick".to_string(),
            title: "Show Sidebar Hint Labels".to_string(),
            description: "Display jump hint labels over visible sidebar rows".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "focus.hint_input".to_string(),
            title: "Type Hint Label Key".to_string(),
            description: "Narrow or jump to an active hint label".to_string(),
            arguments: vec![ArgumentSchema::new(
                "key",
                ArgumentType::String,
                true,
                "Hint character typed",
            )],
        },
        CommandDescriptor {
            name: "focus.hint_cancel".to_string(),
            title: "Cancel Hint Labels".to_string(),
            description: "Dismiss active hint labels".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "toolbar.open".to_string(),
            title: "Open Toolbar Focus".to_string(),
            description: "Focus toolbar buttons and the address bar in vim toolbar mode".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "toolbar.focus_close".to_string(),
            title: "Close Toolbar Focus".to_string(),
            description: "Leave vim toolbar mode".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "toolbar.focus_move".to_string(),
            title: "Move Toolbar Cursor".to_string(),
            description: "Move the toolbar cursor across toolbar buttons and the address bar".to_string(),
            arguments: vec![ArgumentSchema::new(
                "direction",
                ArgumentType::String,
                true,
                "Direction to move (next, previous, first, last)",
            )],
        },
        CommandDescriptor {
            name: "toolbar.focus_activate".to_string(),
            title: "Activate Toolbar Item".to_string(),
            description: "Press the focused toolbar button or open the address bar".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "window.mode".to_string(),
            title: "Open Window Mode".to_string(),
            description: "Enter vim window mode over split panes".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "window_mode.close".to_string(),
            title: "Close Window Mode".to_string(),
            description: "Leave vim window mode".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "window_mode.focus_pane".to_string(),
            title: "Focus Split Pane by Number".to_string(),
            description: "Focus a numbered pane in the active split group".to_string(),
            arguments: vec![ArgumentSchema::new(
                "pane",
                ArgumentType::Integer,
                true,
                "1-based pane number",
            )],
        },
        CommandDescriptor {
            name: "window_mode.move_pane".to_string(),
            title: "Move Split Pane".to_string(),
            description: "Move the active pane or carry landing target in a direction".to_string(),
            arguments: vec![ArgumentSchema::new(
                "direction",
                ArgumentType::String,
                true,
                "Direction to move (left, right, up, down)",
            )],
        },
        CommandDescriptor {
            name: "window_mode.carry_toggle".to_string(),
            title: "Toggle Window Mode Carry".to_string(),
            description: "Pick up or release the active pane to show landing labels".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "window_mode.drop".to_string(),
            title: "Drop Carried Pane".to_string(),
            description: "Land the carried pane at the selected target".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "picker.choose".to_string(),
            title: "Choose Picker Item".to_string(),
            description: "Select an item from the active picker and invoke its command".to_string(),
            arguments: vec![
                ArgumentSchema::new("request_id", ArgumentType::String, false, "Picker request id"),
                ArgumentSchema::new("item_id", ArgumentType::String, true, "Item id to choose"),
            ],
        },
        CommandDescriptor {
            name: "picker.close".to_string(),
            title: "Close Picker".to_string(),
            description: "Dismiss the active Zendia picker modal".to_string(),
            arguments: vec![ArgumentSchema::new(
                "request_id",
                ArgumentType::String,
                false,
                "Picker request id",
            )],
        },
        CommandDescriptor {
            name: "region.pick".to_string(),
            title: "Pick Page Region".to_string(),
            description: "Start the page region selection overlay".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "region.capture".to_string(),
            title: "Capture Page Region".to_string(),
            description: "Capture a page rectangle or the whole page view".to_string(),
            arguments: vec![
                ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab"),
                ArgumentSchema::new("x", ArgumentType::Float, false, "Left edge in page CSS pixels"),
                ArgumentSchema::new("y", ArgumentType::Float, false, "Top edge in page CSS pixels"),
                ArgumentSchema::new("width", ArgumentType::Float, false, "Width in page CSS pixels"),
                ArgumentSchema::new("height", ArgumentType::Float, false, "Height in page CSS pixels"),
                ArgumentSchema::new("whole_page", ArgumentType::Bool, false, "Capture the whole page view"),
            ],
        },
        CommandDescriptor {
            name: "region.cancel".to_string(),
            title: "Cancel Page Region Pick".to_string(),
            description: "Cancel an active page region selection".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "panel.open".to_string(),
            title: "Open Side Panel".to_string(),
            description: "Open a registered side panel in the dock host".to_string(),
            arguments: vec![
                ArgumentSchema::new("panel_id", ArgumentType::String, true, "Side panel identifier"),
                ArgumentSchema::new("focus", ArgumentType::Bool, false, "Focus the panel input"),
            ],
        },
        CommandDescriptor {
            name: "panel.close".to_string(),
            title: "Close Side Panel".to_string(),
            description: "Close a registered side panel in the dock host".to_string(),
            arguments: vec![ArgumentSchema::new(
                "panel_id",
                ArgumentType::String,
                true,
                "Side panel identifier",
            )],
        },
        CommandDescriptor {
            name: "panel.set_layout".to_string(),
            title: "Set Side Panel Layout".to_string(),
            description: "Switch a side panel between float, slide, and split layouts".to_string(),
            arguments: vec![
                ArgumentSchema::new("panel_id", ArgumentType::String, true, "Side panel identifier"),
                ArgumentSchema::new("layout", ArgumentType::String, true, "float, slide, or split"),
            ],
        },
        CommandDescriptor {
            name: "panel.set_side".to_string(),
            title: "Move Side Panel Edge".to_string(),
            description: "Dock a side panel on the left or right edge".to_string(),
            arguments: vec![
                ArgumentSchema::new("panel_id", ArgumentType::String, true, "Side panel identifier"),
                ArgumentSchema::new("side", ArgumentType::String, true, "left or right"),
            ],
        },
        CommandDescriptor {
            name: "panel.set_width".to_string(),
            title: "Resize Side Panel".to_string(),
            description: "Set a side panel's logical width in pixels".to_string(),
            arguments: vec![
                ArgumentSchema::new("panel_id", ArgumentType::String, true, "Side panel identifier"),
                ArgumentSchema::new("width", ArgumentType::Float, true, "Width in logical pixels"),
            ],
        },
        CommandDescriptor {
            name: "panel.reset_width".to_string(),
            title: "Reset Side Panel Width".to_string(),
            description: "Reset a side panel to its default narrowest width".to_string(),
            arguments: vec![ArgumentSchema::new(
                "panel_id",
                ArgumentType::String,
                true,
                "Side panel identifier",
            )],
        },
        CommandDescriptor {
            name: "agent.toggle".to_string(),
            title: "Toggle Agent Panel".to_string(),
            description: "Open or close the AI agent side panel".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "agent.ask".to_string(),
            title: "Ask Agent About This Tab".to_string(),
            description: "Open the agent side panel and focus its composer".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "agent.send_with_files".to_string(),
            title: "Send Agent Prompt with Files".to_string(),
            description: "Send a prompt and attached filenames to the active agent conversation".to_string(),
            arguments: vec![
                ArgumentSchema::new("conversation_id", ArgumentType::Integer, false, "Target conversation"),
                ArgumentSchema::new("agent_id", ArgumentType::String, false, "Target agent"),
                ArgumentSchema::new("prompt", ArgumentType::String, false, "Prompt text"),
                ArgumentSchema::new("files", ArgumentType::String, false, "Comma-separated filenames"),
            ],
        },
        CommandDescriptor {
            name: "agent.stop".to_string(),
            title: "Stop Agent Run".to_string(),
            description: "Stop the active agent turn while keeping its trace".to_string(),
            arguments: vec![ArgumentSchema::new(
                "conversation_id",
                ArgumentType::Integer,
                false,
                "Target conversation",
            )],
        },
        CommandDescriptor {
            name: "agent.new".to_string(),
            title: "New Agent Chat".to_string(),
            description: "Start a fresh agent conversation for the active tab".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "agent.select".to_string(),
            title: "Select Agent".to_string(),
            description: "Switch to a specific agent or open the agents picker".to_string(),
            arguments: vec![ArgumentSchema::new("agent_id", ArgumentType::String, false, "Agent id")],
        },
        CommandDescriptor {
            name: "agent.cycle".to_string(),
            title: "Cycle Agent".to_string(),
            description: "Cycle to the next or previous available agent".to_string(),
            arguments: vec![ArgumentSchema::new(
                "direction",
                ArgumentType::String,
                false,
                "next or previous",
            )],
        },
        CommandDescriptor {
            name: "agent.model".to_string(),
            title: "Choose Agent Model".to_string(),
            description: "Open the model card or select a model for the active agent".to_string(),
            arguments: vec![
                ArgumentSchema::new("agent_id", ArgumentType::String, false, "Target agent"),
                ArgumentSchema::new("model_id", ArgumentType::String, false, "Model id (empty for Default)"),
            ],
        },
        CommandDescriptor {
            name: "agent.set_effort".to_string(),
            title: "Set Model Reasoning Effort".to_string(),
            description: "Set reasoning effort for the active agent (or default)".to_string(),
            arguments: vec![
                ArgumentSchema::new("agent_id", ArgumentType::String, false, "Target agent"),
                ArgumentSchema::new("effort", ArgumentType::String, false, "Effort level or default"),
            ],
        },
        CommandDescriptor {
            name: "agent.set_context".to_string(),
            title: "Set Model Context Window".to_string(),
            description: "Choose a context window size for the active agent".to_string(),
            arguments: vec![
                ArgumentSchema::new("agent_id", ArgumentType::String, false, "Target agent"),
                ArgumentSchema::new("context_id", ArgumentType::String, true, "Context choice id"),
            ],
        },
        CommandDescriptor {
            name: "agent.confirm".to_string(),
            title: "Answer Agent Permission Prompt".to_string(),
            description: "Allow or deny a pending agent confirmation card".to_string(),
            arguments: vec![
                ArgumentSchema::new("conversation_id", ArgumentType::Integer, true, "Conversation id"),
                ArgumentSchema::new("step_id", ArgumentType::Integer, true, "Confirmation step id"),
                ArgumentSchema::new("allow", ArgumentType::Bool, true, "Whether to allow the action"),
            ],
        },
        CommandDescriptor {
            name: "agent.switch_conversation".to_string(),
            title: "Switch Tab Agent Chat".to_string(),
            description: "Switch to another agent conversation on the active tab".to_string(),
            arguments: vec![ArgumentSchema::new(
                "conversation_id",
                ArgumentType::Integer,
                true,
                "Conversation id",
            )],
        },
        CommandDescriptor {
            name: "agent.mark_seen".to_string(),
            title: "Mark Agent Chat Seen".to_string(),
            description: "Clear the unseen indicator for an agent conversation".to_string(),
            arguments: vec![ArgumentSchema::new(
                "conversation_id",
                ArgumentType::Integer,
                true,
                "Conversation id",
            )],
        },
        CommandDescriptor {
            name: "agent.instructions.open".to_string(),
            title: "Open Agent Instructions".to_string(),
            description: "Open the agent guidelines page over the agent panel".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "agent.instructions.add".to_string(),
            title: "Add Agent Instruction".to_string(),
            description: "Create a new agent instruction card".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "agent.instructions.update".to_string(),
            title: "Update Agent Instruction".to_string(),
            description: "Edit an agent instruction card's enabled state, title, or text".to_string(),
            arguments: vec![
                ArgumentSchema::new("id", ArgumentType::Integer, true, "Instruction id"),
                ArgumentSchema::new("enabled", ArgumentType::Bool, false, "Enabled state"),
                ArgumentSchema::new("title", ArgumentType::String, false, "Instruction title"),
                ArgumentSchema::new("text", ArgumentType::String, false, "Instruction text"),
            ],
        },
        CommandDescriptor {
            name: "agent.instructions.remove".to_string(),
            title: "Delete Agent Instruction".to_string(),
            description: "Delete a saved agent instruction card".to_string(),
            arguments: vec![ArgumentSchema::new("id", ArgumentType::Integer, true, "Instruction id")],
        },
        CommandDescriptor {
            name: "agent.instructions.set_file".to_string(),
            title: "Attach File to Instruction".to_string(),
            description: "Attach or remove the file on an instruction card".to_string(),
            arguments: vec![
                ArgumentSchema::new("id", ArgumentType::Integer, true, "Instruction id"),
                ArgumentSchema::new("name", ArgumentType::String, false, "Filename (omit to clear)"),
            ],
        },
        CommandDescriptor {
            name: "agent.summarize_captcha".to_string(),
            title: "Summarize CAPTCHA Finding".to_string(),
            description: "Summarize the CAPTCHA detected on the active tab".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "agent.log".to_string(),
            title: "Open Agent Trace Log".to_string(),
            description: "Open the conversation log for the active tab".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Target tab")],
        },
        CommandDescriptor {
            name: "agent.list".to_string(),
            title: "Toggle Agents Roster Panel".to_string(),
            description: "Open or close the cross-window agents panel".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "debug.sample_agent".to_string(),
            title: "Load Sample Agent Conversation".to_string(),
            description: "Populate the active tab with a sample Markdown, table, and confirmation agent run"
                .to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "debug.toggle_agent_service".to_string(),
            title: "Toggle AgentServer Health (Debug)".to_string(),
            description: "Toggle AgentServer between Running and Stopped to preview the service-down state".to_string(),
            arguments: Vec::new(),
        },
    ];
    descriptors.extend(crate::space_routing::command_descriptors());
    descriptors.extend(crate::space_appearance::command_descriptors());
    descriptors.extend(crate::share::command_descriptors());
    descriptors
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The vim profile ported from the fork, shipped with the browser.
    const VIM_PROFILE: &str = include_str!("../../../Base/res/keyboard/vim.toml");

    /// Vim profile commands for phase 5's surfaces that have no implementation yet. Each one is a later feature;
    /// remove it here when it lands.
    const NOT_YET: &[&str] = &[
        "explorer.create",
        "explorer.find",
        "explorer.fold_close",
        "explorer.fold_open",
        "explorer.fold_toggle",
        "explorer.half_page_down",
        "explorer.half_page_up",
        "explorer.menu",
        "explorer.move_down",
        "explorer.move_out_of_folder",
        "explorer.move_up",
        "explorer.preview",
        "explorer.split_horizontal",
        "explorer.split_vertical",
        "explorer.yank",
        "explorer.yank_markdown",
        "toolbar.jump",
        "toolbar.menu",
        "window.close",
        "window.minimize",
        "window.toggle_maximize",
    ];

    #[test]
    fn vim_profile_focus_commands_reach_typed_commands() {
        let namespaces = [
            "explorer.",
            "toolbar.",
            "window.",
            "picker.",
            "region.",
            "statusline.",
            "which_key.",
            "agent.",
        ];
        let mut missing = Vec::new();
        for line in VIM_PROFILE.lines() {
            let Some(name) = line
                .trim()
                .strip_prefix("command = \"")
                .and_then(|rest| rest.strip_suffix('"'))
            else {
                continue;
            };
            if !namespaces.iter().any(|prefix| name.starts_with(prefix)) || NOT_YET.contains(&name) {
                continue;
            }
            let parsed = Command::from_name_and_args(name, &[]);
            let typed = match parsed {
                Ok(Command::Custom { .. }) => false,
                Ok(_) => true,
                // A typed command that needs arguments the profile line gives separately still exists.
                Err(CommandError::InvalidArgument { .. }) => true,
                Err(_) => false,
            };
            if !typed && !missing.contains(&name) {
                missing.push(name);
            }
        }
        assert!(
            missing.is_empty(),
            "vim.toml commands with no typed command: {missing:?}"
        );
    }

    #[test]
    fn entering_focus_surfaces_uses_the_vim_profile_names() {
        assert_eq!(Command::ExplorerOpen.name(), "explorer.toggle");
        assert_eq!(Command::ToolbarFocusOpen.name(), "toolbar.open");
        assert_eq!(Command::WindowModeOpen.name(), "window.mode");
        assert_eq!(
            Command::from_name_and_args("explorer.open", &[]).unwrap(),
            Command::ExplorerActivate,
            "in the vim profile, explorer.open opens the item under the cursor"
        );
        assert_eq!(
            Command::from_name_and_args("window_mode.open", &[]).unwrap(),
            Command::WindowModeOpen
        );
    }
}
