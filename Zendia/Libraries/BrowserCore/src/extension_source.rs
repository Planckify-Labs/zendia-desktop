/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Adapter boundary between the browser core and the sandboxed `ExtensionServer` process.

use crate::commands::CommandResult;
use crate::modules::{ExtensionContextMenuItem, ExtensionId};

/// Receives WebExtension lifecycle, action, prompt, and menu commands from the core and forwards them to
/// `ExtensionServer`.
pub trait ExtensionSource: Send + Sync {
    /// Invokes an extension's browser/page action on the currently active tab.
    fn invoke_action(&self, extension_id: &ExtensionId) -> CommandResult;

    /// Enables or disables an installed extension.
    fn set_enabled(&self, extension_id: &ExtensionId, enabled: bool) -> CommandResult;

    /// Pins or unpins an extension's button on the browser toolbar.
    fn set_toolbar_pinned(&self, extension_id: &ExtensionId, pinned: bool) -> CommandResult;

    /// Uninstalls an extension.
    fn remove(&self, extension_id: &ExtensionId) -> CommandResult;

    /// Reloads an installed extension from disk or package store.
    fn reload(&self, extension_id: &ExtensionId) -> CommandResult;

    /// Installs an unpacked extension directory or packaged `.crx`/`.xpi`/`.zip` archive from `path`.
    fn install_path(&self, path: &str) -> CommandResult;

    /// Answers a pending user consent prompt (install, optional permissions, or uninstall).
    fn answer_prompt(&self, token: u64, accepted: bool) -> CommandResult;

    /// Synchronously queries extension-registered context menu items matching a serialized click target JSON object.
    fn context_menu_items(&self, target_json: &str) -> Vec<ExtensionContextMenuItem>;

    /// Dispatches a click on an extension-registered context menu item.
    fn context_menu_clicked(&self, extension_id: &ExtensionId, item_id: &str, target_json: &str) -> CommandResult;

    /// Invokes a manifest-declared keyboard command on an extension.
    fn invoke_command(&self, extension_id: &ExtensionId, name: &str) -> CommandResult;

    /// Restarts the `ExtensionServer` process.
    fn restart_service(&self) -> CommandResult;
}
