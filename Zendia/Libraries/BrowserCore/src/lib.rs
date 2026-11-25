/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The browser core, which runs in the UI process.
//!
//! It holds the command registry, through which every caller (the window, InputServer, PluginHost, ExtensionServer and
//! AgentServer) drives the browser; the browser model (windows and tabs); the event bus that tells subscribers what
//! changed; and the profile data (history, bookmarks, sessions). It runs with or without a window, so agents can work
//! headless.
//!
//! See `Zendia/Documentation/Architecture.md`, "The command layer".

pub mod address_bar;
pub mod agent;
pub mod commands;
pub mod downloads;
pub mod events;
pub mod focus;
pub mod history;
pub mod input_state;
pub mod library;
pub mod library_media;
pub mod markdown;
pub mod media;
pub mod menus;
pub mod model;
pub mod onboarding;
pub mod picker;
pub mod region;
pub mod share;
pub mod site_info;
pub mod space_appearance;
pub mod space_routing;
pub mod split;
pub mod stub;
pub mod tab_source;
pub mod toasts;
pub mod webchat;

pub use zendia_module_api::contributions;
pub use zendia_module_api::ui;

use crate::commands::{Command, CommandDescriptor, CommandResult};
use crate::events::Subscription;
use crate::model::{BrowserModel, WindowId};

/// Public facade over the browser core used by Freya, headless runners, and service adapters.
pub trait BrowserCore: Send + Sync {
    /// Returns a snapshot of the current browser model.
    fn model(&self) -> BrowserModel;

    /// Subscribes to subsequent events published by the core.
    fn subscribe(&self) -> Subscription;

    /// Invokes a named, typed browser command for a caller with no window, such as an agent or a plugin. Requests it
    /// makes of a view go to the focused window.
    fn invoke(&self, command: Command) -> CommandResult;

    /// Invokes a command on behalf of `window`'s view, so requests it makes of a view (open a panel, show the palette)
    /// reach that window and no other.
    fn invoke_in(&self, window: WindowId, command: Command) -> CommandResult;

    /// Lists descriptors for all commands registered in the core.
    fn commands(&self) -> Vec<CommandDescriptor>;
}
