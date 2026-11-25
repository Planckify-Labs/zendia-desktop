/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Reactive Freya view over [`BrowserCore`]: one subscription turning core events into Freya state, and one hook for
//! components to read the model and invoke commands.

use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::BrowserCore;
use zendia_browser_core::commands::{Command, CommandDescriptor, CommandResult};
use zendia_browser_core::events::Event;
use zendia_browser_core::model::{BrowserModel, WindowId};

/// Context handle shared with every UI component to read reactive core state and invoke commands.
#[derive(Clone)]
pub struct CoreView {
    core: Arc<dyn BrowserCore>,
    model: State<BrowserModel>,
    window: Option<WindowId>,
}

impl PartialEq for CoreView {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.core, &other.core) && self.model == other.model && self.window == other.window
    }
}

impl CoreView {
    /// Reactive snapshot of the browser model owned by the core.
    pub fn model(&self) -> &State<BrowserModel> {
        &self.model
    }

    /// Invokes a named, typed command through the browser core on behalf of this window.
    #[allow(dead_code)]
    pub fn invoke(&self, command: Command) -> CommandResult {
        match self.window {
            Some(window) => self.core.invoke_in(window, command),
            None => self.core.invoke(command),
        }
    }

    /// Invokes a command and logs any error without surfacing it to the caller.
    pub fn invoke_or_log(&self, command: Command) -> bool {
        let name = command.name().to_string();
        match self.invoke(command) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("zendia: command `{name}` failed: {error}");
                false
            }
        }
    }

    /// Lists descriptors for all commands registered in the core.
    #[allow(dead_code)]
    pub fn commands(&self) -> Vec<CommandDescriptor> {
        self.core.commands()
    }
}

/// Installs [`CoreView`] into Freya context and starts the single event subscription for this window.
#[allow(dead_code)]
pub fn use_provide_core(core: Arc<dyn BrowserCore>) -> CoreView {
    use_provide_core_with_observer(core, |_| {})
}

/// Installs [`CoreView`] into Freya context with an optional window-level event observer (used by the window shell to
/// clean up per-view resources on tab close) while keeping a single core subscription.
///
/// This view shows the core's focused window, the only one until Freya opens more. View requests addressed to any
/// other window are dropped here, so neither the model nor the observer sees them.
pub fn use_provide_core_with_observer(
    core: Arc<dyn BrowserCore>,
    mut on_event: impl FnMut(&Event) + 'static,
) -> CoreView {
    let mut model = use_state(|| core.model());
    let window = use_hook(|| model.peek().focused_window());

    use_hook({
        let core = Arc::clone(&core);
        move || {
            let subscription = core.subscribe();
            spawn_in_window(async move {
                while let Ok(event) = subscription.recv().await {
                    match &event {
                        Event::ViewRequested { window: target, .. } => {
                            let for_this_window = match window {
                                Some(window) => model.peek().view_request_is_for(*target, window),
                                None => target.is_none(),
                            };
                            if !for_this_window {
                                continue;
                            }
                        }
                        Event::ModelReplaced => {
                            model.set(core.model());
                        }
                        _ => {
                            model.write().apply_event(&event);
                        }
                    }
                    on_event(&event);
                }
            });
        }
    });

    use_provide_context(|| CoreView { core, model, window })
}

/// Accesses the window's [`CoreView`] from any descendant component.
pub fn use_core() -> CoreView {
    use_consume::<CoreView>()
}
