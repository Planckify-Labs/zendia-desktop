/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Event bus for notifying views and services when browser state changes.

use crate::address_bar::SearchEngine;
use crate::agent::{AgentInstruction, AgentSelection, SidePanelPrefs};
use crate::contributions::{Contributions, SidePanelId};
use crate::focus::FocusState;
use crate::input_state::{StatuslineAnnouncement, StatuslineSettings};
use crate::library::LibrarySection;
use crate::model::{
    AgentId, Bookmark, ChatSite, ChatSiteId, Conversation, ConversationId, Download, Essential, Folder, FolderId,
    InputMode, LayoutSettings, MediaSession, NotificationId, PinnedTab, ServiceState, SidebarNotification, Space,
    SpaceId, SplitGroup, SplitGroupId, Tab, TabId, ThemeSettings, WhichKeyState, WindowId,
};
use crate::picker::{PickerRequest, PickerRequestId};
use crate::region::RegionCapture;
use crate::site_info::SiteInfo;
use crate::space_routing::{SpaceRouteId, SpaceRoutingSettings};
use crate::split::Glance;
use crate::toasts::{Toast, ToastId};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

/// Events published by the browser core to every active subscriber.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    TabOpened {
        tab: Tab,
        index: usize,
    },
    TabUpdated {
        tab: Tab,
    },
    TabClosed {
        tab_id: TabId,
    },
    TabActivated {
        tab_id: Option<TabId>,
    },
    SpaceSwitched {
        space_id: SpaceId,
    },
    SpacesUpdated {
        spaces: Vec<Space>,
    },
    SpaceRoutingChanged {
        settings: SpaceRoutingSettings,
    },
    TabsReordered {
        tabs: Vec<Tab>,
        essentials: Vec<Essential>,
        pinned_tabs: Vec<PinnedTab>,
        folders: Vec<Folder>,
    },
    EssentialsChanged {
        essentials: Vec<Essential>,
    },
    PinnedTabsChanged {
        pinned_tabs: Vec<PinnedTab>,
    },
    FolderCreated {
        folder: Folder,
    },
    FolderUpdated {
        folder: Folder,
    },
    FolderDeleted {
        folder_id: FolderId,
    },
    SidebarNotificationAdded {
        notification: SidebarNotification,
    },
    SidebarNotificationDismissed {
        notification_id: NotificationId,
    },
    EssentialsPromoChanged {
        visible: bool,
    },
    SplitGroupUpdated {
        group: SplitGroup,
    },
    SplitGroupRemoved {
        group_id: SplitGroupId,
    },
    /// Glance opened, changed or closed (`None`).
    GlanceChanged {
        glance: Option<Glance>,
    },
    ToastShown {
        toast: Toast,
    },
    ToastDismissed {
        toast_id: ToastId,
    },
    MediaSessionsChanged {
        sessions: Vec<MediaSession>,
    },
    /// A download began; views play the download animation.
    DownloadStarted {
        download: Download,
    },
    /// Profile settings for a side panel changed (layout, side, width, or fallback).
    PanelPrefsChanged {
        panel_id: SidePanelId,
        prefs: SidePanelPrefs,
    },
    /// Default agent or per-agent model/effort/context selection changed.
    AgentSettingsChanged {
        default_agent_id: Option<AgentId>,
        selections: BTreeMap<AgentId, AgentSelection>,
    },
    /// Saved agent instruction guidelines changed.
    AgentInstructionsChanged {
        instructions: Vec<AgentInstruction>,
    },
    /// Active conversation selection per tab changed.
    ActiveConversationChanged {
        tab_id: TabId,
        conversation_id: Option<ConversationId>,
    },
    /// A conversation was removed or reset.
    ConversationRemoved {
        conversation_id: ConversationId,
    },
    /// Published whenever the cross-window agents roster changes (status, confirmation, or dismissal).
    AgentRosterChanged,
    /// Published when the web chat site list, active site, pending prompt, or dictation state changes.
    WebChatChanged {
        sites: Vec<ChatSite>,
        active_site_id: Option<ChatSiteId>,
        pending_prompt: Option<(ChatSiteId, String)>,
        dictating: bool,
    },
    SearchEngineChanged {
        engine: SearchEngine,
    },
    BookmarksChanged {
        bookmarks: Vec<Bookmark>,
    },
    SiteInfoUpdated {
        info: SiteInfo,
    },
    SiteDataCleared {
        origin: String,
    },
    ModeChanged {
        mode: InputMode,
    },
    PendingKeysChanged {
        keys: Vec<String>,
    },
    PendingCountChanged {
        count: Option<u32>,
    },
    WhichKeyChanged {
        state: Option<WhichKeyState>,
    },
    StatuslineSettingsChanged {
        settings: StatuslineSettings,
    },
    StatuslineAnnouncementChanged {
        announcement: Option<StatuslineAnnouncement>,
    },
    FocusChanged {
        focus: FocusState,
    },
    ConversationUpdated {
        conversation: Conversation,
    },
    ServiceStateChanged {
        state: ServiceState,
    },
    ContributionsChanged {
        contributions: Contributions,
    },
    SettingsChanged {
        theme: ThemeSettings,
    },
    LayoutSettingsChanged {
        layout: LayoutSettings,
    },
    VimModeChanged {
        enabled: bool,
    },
    ZoomChanged {
        zoom_level: f32,
    },
    QuitRequested,
    ClipboardCopyRequested {
        text: String,
    },
    /// Published when a user or caller chooses an item in an open picker.
    PickerChosen {
        request_id: PickerRequestId,
        item_id: String,
    },
    /// Published when a page region (or the whole page) is captured for an agent or caller.
    RegionCaptured {
        capture: RegionCapture,
    },
    /// Signals subscribers to reload a fresh snapshot from `BrowserCore::model`.
    ModelReplaced,
    /// A command asked one window's view to do something that only changes view state, such as opening a panel or
    /// the palette. `window` is the window whose view invoked the command. It is `None` when the caller has no window
    /// (a key from InputServer, an agent, a plugin), and then the focused window takes the request. Every other
    /// window ignores it.
    ViewRequested {
        window: Option<WindowId>,
        request: ViewRequest,
    },
}

/// A request from a command to one window's view. It changes only that view's state, never the model.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewRequest {
    /// Start renaming a tab inline in the sidebar.
    TabRename { tab_id: TabId },
    /// Start renaming a folder inline in the sidebar.
    FolderRename { folder_id: FolderId },
    /// Slide the compact mode sidebar out or back (Zen's `toggleSidebar`).
    CompactSidebarReveal,
    /// Open a side panel if it is closed, close it if it is the one in front.
    PanelToggle { panel_id: SidePanelId },
    /// Open a side panel, focusing its primary input when `focus` is set.
    PanelOpen { panel_id: SidePanelId, focus: bool },
    /// Close a side panel.
    PanelClose { panel_id: SidePanelId },
    /// Toggle the model card on the agent panel's model chip.
    AgentModelCard,
    /// Open the instructions page inside the agent panel.
    AgentInstructions,
    /// Reload the Hub view of a web chat site.
    WebChatReload { site_id: ChatSiteId },
    /// Load a web chat site's home URL in its Hub view, for a fresh chat.
    WebChatNavigateHome { site_id: ChatSiteId },
    /// Focus the location field.
    LocationFocus,
    /// Open the floating address bar.
    AddressBarOpen {
        new_tab: bool,
        initial_query: Option<String>,
    },
    /// Open the command palette.
    PaletteOpen { initial_query: Option<String> },
    /// Toggle the site data panel.
    SitePanelToggle,
    /// Open the all-keymaps which-key popup (`<leader>?`).
    WhichKeyAll { mode: InputMode },
    /// Toggle the app menu.
    AppMenuToggle,
    /// Show the about dialog.
    AboutDialog,
    /// Show a picker modal.
    PickerOpen { request: PickerRequest },
    /// A picker modal closed.
    PickerClosed { request_id: Option<PickerRequestId> },
    /// Enter the page region picker overlay.
    RegionPick { tab_id: Option<TabId> },
    /// Region picking was cancelled.
    RegionPickCancelled,
    /// Open the library, or close it when it already shows `section` (any section when `None`).
    LibraryToggle { section: Option<LibrarySection> },
    /// Open the library on `section`, or on the section it last showed.
    LibraryOpen { section: Option<LibrarySection> },
    /// Close the library.
    LibraryClose,
    /// Start renaming a space inline in the library's Spaces section.
    SpaceRename { space_id: SpaceId },
    /// Open the icon picker for a space in the library's Spaces section.
    SpaceIconPicker { space_id: SpaceId },
    /// Open the edit form for a routing rule in the library's Space routing section.
    SpaceRoutingEdit { rule_id: SpaceRouteId },
}

/// Returned when the core's event bus has been dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubscriptionClosed;

impl fmt::Display for SubscriptionClosed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "event subscription closed")
    }
}

impl std::error::Error for SubscriptionClosed {}

/// Error returned by [`Subscription::try_recv`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TryRecvError {
    Empty,
    Closed,
}

/// Receiving end of a subscription to the core's event bus.
///
/// Works in async UI tasks (`recv`), synchronous polling (`try_recv`), and blocking threads (`recv_blocking`) without
/// tying the core to any specific async runtime.
#[derive(Clone)]
pub struct Subscription {
    receiver: async_channel::Receiver<Event>,
}

impl Subscription {
    pub async fn recv(&self) -> Result<Event, SubscriptionClosed> {
        self.receiver.recv().await.map_err(|_| SubscriptionClosed)
    }

    pub fn try_recv(&self) -> Result<Event, TryRecvError> {
        match self.receiver.try_recv() {
            Ok(event) => Ok(event),
            Err(async_channel::TryRecvError::Empty) => Err(TryRecvError::Empty),
            Err(async_channel::TryRecvError::Closed) => Err(TryRecvError::Closed),
        }
    }

    pub fn recv_blocking(&self) -> Result<Event, SubscriptionClosed> {
        self.receiver.recv_blocking().map_err(|_| SubscriptionClosed)
    }
}

/// Multi-subscriber fan-out event bus backed by unbounded channels.
#[derive(Default)]
pub struct EventBus {
    subscribers: Mutex<Vec<async_channel::Sender<Event>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&self) -> Subscription {
        let (sender, receiver) = async_channel::unbounded();
        self.subscribers
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(sender);
        Subscription { receiver }
    }

    pub fn publish(&self, event: Event) {
        let mut subscribers = self.subscribers.lock().unwrap_or_else(|error| error.into_inner());
        subscribers.retain(|sender| sender.try_send(event.clone()).is_ok());
    }
}
