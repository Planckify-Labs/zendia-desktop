/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Event bus for notifying views and services when browser state changes.

use crate::contributions::{Contributions, SidePanelId};
use crate::model::{
    Conversation, InputMode, LayoutSettings, ServiceState, SpaceId, SplitGroup, Tab, TabId, ThemeSettings,
    WhichKeyState,
};
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
    SplitGroupUpdated {
        group: SplitGroup,
    },
    PanelToggled {
        panel_id: SidePanelId,
    },
    LocationFocusRequested,
    ModeChanged {
        mode: InputMode,
    },
    PendingKeysChanged {
        keys: Vec<String>,
    },
    WhichKeyChanged {
        state: Option<WhichKeyState>,
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
