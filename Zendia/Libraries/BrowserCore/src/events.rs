/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Event bus for notifying views and services when browser state changes.

use crate::address_bar::SearchEngine;
use crate::contributions::{Contributions, SidePanelId};
use crate::focus::FocusState;
use crate::input_state::{StatuslineAnnouncement, StatuslineSettings};
use crate::model::{
    Bookmark, Conversation, Download, Essential, Folder, FolderId, InputMode, LayoutSettings, MediaSession,
    NotificationId, PinnedTab, ServiceState, SidebarNotification, Space, SpaceId, SplitGroup, SplitGroupId, Tab, TabId,
    ThemeSettings, WhichKeyState,
};
use crate::picker::{PickerRequest, PickerRequestId};
use crate::region::RegionCapture;
use crate::site_info::SiteInfo;
use crate::split::Glance;
use crate::toasts::{Toast, ToastId};
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
    TabRenameRequested {
        tab_id: TabId,
    },
    FolderRenameRequested {
        folder_id: FolderId,
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
    /// View-level request to slide the compact mode sidebar out or back (Zen's `toggleSidebar`).
    CompactSidebarRevealRequested,
    PanelToggled {
        panel_id: SidePanelId,
    },
    LocationFocusRequested,
    AddressBarOpenRequested {
        new_tab: bool,
        initial_query: Option<String>,
    },
    PaletteOpenRequested {
        initial_query: Option<String>,
    },
    SitePanelToggleRequested,
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
    /// View-level request to open the all-keymaps which-key popup (`<leader>?`).
    WhichKeyAllRequested {
        mode: InputMode,
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
    AppMenuToggleRequested,
    AboutDialogRequested,
    QuitRequested,
    ClipboardCopyRequested {
        text: String,
    },
    /// View-level request to show a Zendia picker modal in the window.
    PickerOpenRequested {
        request: PickerRequest,
    },
    /// Published when a user or caller chooses an item in an open picker.
    PickerChosen {
        request_id: PickerRequestId,
        item_id: String,
    },
    /// View-level notification that a picker modal closed.
    PickerClosed {
        request_id: Option<PickerRequestId>,
    },
    /// View-level request to enter the page region picker overlay.
    RegionPickRequested {
        tab_id: Option<TabId>,
    },
    /// Published when a page region (or the whole page) is captured for an agent or caller.
    RegionCaptured {
        capture: RegionCapture,
    },
    /// View-level notification that region picking was cancelled.
    RegionPickCancelled,
    /// Signals subscribers to reload a fresh snapshot from `BrowserCore::model`.
    ModelReplaced,
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
