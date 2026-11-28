/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for phase 5's picker and region capture commands.

use super::StubBackend;
use crate::BrowserCore;
use crate::commands::{Command, CommandError, CommandResult};
use crate::events::{Event, ViewRequest};
use crate::model::TabId;
use crate::picker::{PickerRequest, PickerRequestId, build_builtin_picker};
use crate::region::{RegionCapture, RegionRect};

impl StubBackend {
    /// Registers a custom picker request so any command, plugin, or agent can open it by `request_id`.
    pub fn register_picker(&self, request: PickerRequest) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.registered_pickers.insert(request.id.clone(), request);
    }

    /// Returns the currently active picker request, if one is open in the stub backend.
    pub fn active_picker(&self) -> Option<PickerRequest> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .active_picker
            .clone()
    }

    /// Returns the most recent region capture result published through `region.capture`.
    pub fn last_region_capture(&self) -> Option<RegionCapture> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .last_region_capture
            .clone()
    }

    /// Closes any active picker or region picker when Vim mode is turned off.
    pub(super) fn on_vim_mode_disabled(&self) {
        let closed_id = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.active_picker.take().map(|request| request.id)
        };
        if closed_id.is_some() {
            self.request_view(ViewRequest::PickerClosed { request_id: closed_id });
        }
        self.request_view(ViewRequest::RegionPickCancelled);
    }

    /// Handles `picker.*` and `region.*` commands routed from [`StubBackend::invoke`].
    pub(super) fn handle_picker_or_region_command(&self, command: Command) -> CommandResult {
        match command {
            Command::PickerOpen { name, request_id } => self.handle_picker_open(name, request_id),
            Command::PickerChoose { request_id, item_id } => self.handle_picker_choose(request_id, item_id),
            Command::PickerClose { request_id } => self.handle_picker_close(request_id),
            Command::RegionPick { tab_id } => self.handle_region_pick(tab_id),
            Command::RegionCapture {
                tab_id,
                rect,
                whole_page,
            } => self.handle_region_capture(tab_id, rect, whole_page),
            Command::RegionCancel => self.handle_region_cancel(),
            other => Err(CommandError::UnknownCommand(other.name().to_string())),
        }
    }

    fn require_vim_enabled(&self, command: &str) -> Result<(), CommandError> {
        let enabled = self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .model
            .vim_mode_enabled;
        if enabled {
            Ok(())
        } else {
            Err(CommandError::Failed(format!(
                "`{command}` requires vim mode to be enabled"
            )))
        }
    }

    fn handle_picker_open(&self, name: Option<String>, request_id: Option<PickerRequestId>) -> CommandResult {
        self.require_vim_enabled("picker.open")?;

        let request = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let resolved = if let Some(ref req_id) = request_id {
                state
                    .registered_pickers
                    .get(req_id)
                    .cloned()
                    .or_else(|| build_builtin_picker(req_id.as_str(), &state.model))
                    .ok_or_else(|| CommandError::TargetNotFound {
                        command: "picker.open".to_string(),
                        target: format!("picker `{}`", req_id.as_str()),
                    })?
            } else {
                let builtin_name = name.as_deref().unwrap_or("tabs");
                let custom_id = PickerRequestId::new(builtin_name);
                state
                    .registered_pickers
                    .get(&custom_id)
                    .cloned()
                    .or_else(|| build_builtin_picker(builtin_name, &state.model))
                    .ok_or_else(|| CommandError::TargetNotFound {
                        command: "picker.open".to_string(),
                        target: format!("picker `{builtin_name}`"),
                    })?
            };
            state.active_picker = Some(resolved.clone());
            resolved
        };

        self.request_view(ViewRequest::PickerOpen { request });
        Ok(())
    }

    fn handle_picker_choose(&self, request_id: Option<PickerRequestId>, item_id: String) -> CommandResult {
        let (resolved_id, item) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let active = state
                .active_picker
                .as_ref()
                .ok_or_else(|| CommandError::TargetNotFound {
                    command: "picker.choose".to_string(),
                    target: "open picker".to_string(),
                })?;
            if let Some(ref expected) = request_id
                && active.id != *expected
            {
                return Err(CommandError::TargetNotFound {
                    command: "picker.choose".to_string(),
                    target: format!("picker `{}`", expected.as_str()),
                });
            }
            let item = active
                .find_item(&item_id)
                .cloned()
                .ok_or_else(|| CommandError::TargetNotFound {
                    command: "picker.choose".to_string(),
                    target: format!("picker item `{item_id}`"),
                })?;
            if !item.enabled {
                return Err(CommandError::Failed(format!(
                    "picker item `{}` is disabled",
                    item.title
                )));
            }
            let resolved_id = active.id.clone();
            state.active_picker = None;
            (resolved_id, item)
        };

        self.bus.publish(Event::PickerChosen {
            request_id: resolved_id.clone(),
            item_id,
        });
        self.request_view(ViewRequest::PickerClosed {
            request_id: Some(resolved_id),
        });

        if !item.command.is_empty() {
            let parsed = Command::from_name_and_args(&item.command, &item.command_args)?;
            self.invoke(parsed)?;
        }
        Ok(())
    }

    fn handle_picker_close(&self, request_id: Option<PickerRequestId>) -> CommandResult {
        let closed_id = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(ref expected) = request_id {
                if state
                    .active_picker
                    .as_ref()
                    .is_some_and(|active| active.id == *expected)
                {
                    state.active_picker = None;
                }
                Some(expected.clone())
            } else {
                state.active_picker.take().map(|active| active.id)
            }
        };
        self.request_view(ViewRequest::PickerClosed { request_id: closed_id });
        Ok(())
    }

    fn handle_region_pick(&self, tab_id: Option<TabId>) -> CommandResult {
        self.require_vim_enabled("region.pick")?;
        let target_tab = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(id) = tab_id {
                if state.model.tab(id).is_none() {
                    return Err(CommandError::TargetNotFound {
                        command: "region.pick".to_string(),
                        target: format!("tab {}", id.0),
                    });
                }
                Some(id)
            } else {
                state.model.active_tab
            }
        };
        self.request_view(ViewRequest::RegionPick { tab_id: target_tab });
        Ok(())
    }

    fn handle_region_capture(
        &self,
        tab_id: Option<TabId>,
        rect: Option<RegionRect>,
        whole_page: bool,
    ) -> CommandResult {
        if !whole_page
            && let Some(r) = rect
            && !r.is_valid_drag()
        {
            return Err(CommandError::InvalidArgument {
                command: "region.capture".to_string(),
                message: "captured region is smaller than minimum size".to_string(),
            });
        }
        let resolved_tab = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let resolved = tab_id.or(state.model.active_tab);
            let capture = RegionCapture {
                tab_id: resolved,
                rect,
                whole_page,
            };
            state.last_region_capture = Some(capture);
            resolved
        };
        let capture = RegionCapture {
            tab_id: resolved_tab,
            rect,
            whole_page,
        };
        eprintln!(
            "zendia: stub region.capture (tab={:?}, whole_page={}, rect={:?}) — agent handoff not connected yet",
            capture.tab_id, capture.whole_page, capture.rect
        );
        self.bus.publish(Event::RegionCaptured { capture });
        Ok(())
    }

    fn handle_region_cancel(&self) -> CommandResult {
        self.request_view(ViewRequest::RegionPickCancelled);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::BrowserCore;
    use crate::commands::Command;
    use crate::events::{Event, ViewRequest};
    use crate::model::TabId;
    use crate::picker::{PickerItem, PickerLayout, PickerRequest, PickerRequestId};
    use crate::region::RegionRect;
    use crate::stub::StubBackend;

    #[test]
    fn picker_and_region_are_gated_on_vim_mode() {
        let core = StubBackend::new();
        assert!(!core.model().vim_mode_enabled);

        assert!(
            core.invoke(Command::PickerOpen {
                name: Some("tabs".to_string()),
                request_id: None,
            })
            .is_err()
        );
        assert!(core.invoke(Command::RegionPick { tab_id: None }).is_err());

        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        core.invoke(Command::PickerOpen {
            name: Some("tabs".to_string()),
            request_id: None,
        })
        .unwrap();
        assert!(core.active_picker().is_some());

        // Turning vim mode off closes the open picker and cancels region capture.
        let sub = core.subscribe();
        core.invoke(Command::VimSetEnabled { enabled: false }).unwrap();
        assert!(core.active_picker().is_none());
        let mut saw_closed = false;
        let mut saw_region_cancel = false;
        while let Ok(ev) = sub.try_recv() {
            match ev {
                Event::ViewRequested {
                    request: ViewRequest::PickerClosed { .. },
                    ..
                } => saw_closed = true,
                Event::ViewRequested {
                    request: ViewRequest::RegionPickCancelled,
                    ..
                } => saw_region_cancel = true,
                _ => {}
            }
        }
        assert!(saw_closed);
        assert!(saw_region_cancel);
    }

    #[test]
    fn tabs_picker_switches_active_tab_via_choose() {
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        let sub = core.subscribe();

        core.invoke(Command::PickerOpen {
            name: Some("tabs".to_string()),
            request_id: None,
        })
        .unwrap();
        let open_request = core.active_picker().expect("tabs picker should be active");
        let target_item = open_request
            .items
            .iter()
            .find(|item| item.id == "tab.11")
            .expect("Architecture tab should be in tabs picker")
            .clone();

        core.invoke(Command::PickerChoose {
            request_id: Some(open_request.id.clone()),
            item_id: target_item.id.clone(),
        })
        .unwrap();

        assert_eq!(core.model().active_tab, Some(TabId(11)));
        assert!(core.active_picker().is_none());

        let mut saw_open = false;
        let mut saw_chosen = false;
        let mut saw_closed = false;
        while let Ok(ev) = sub.try_recv() {
            match ev {
                Event::ViewRequested {
                    request: ViewRequest::PickerOpen { .. },
                    ..
                } => saw_open = true,
                Event::PickerChosen { item_id, .. } if item_id == "tab.11" => saw_chosen = true,
                Event::ViewRequested {
                    request: ViewRequest::PickerClosed { .. },
                    ..
                } => saw_closed = true,
                _ => {}
            }
        }
        assert!(saw_open && saw_chosen && saw_closed);
    }

    #[test]
    fn agents_picker_rejects_dimmed_disabled_item_and_custom_picker_works() {
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();

        core.invoke(Command::PickerOpen {
            name: Some("agents".to_string()),
            request_id: None,
        })
        .unwrap();
        assert!(
            core.invoke(Command::PickerChoose {
                request_id: None,
                item_id: "agent.cursor".to_string(),
            })
            .is_err(),
            "disabled uninstalled agent cannot be chosen"
        );
        core.invoke(Command::PickerClose { request_id: None }).unwrap();
        assert!(core.active_picker().is_none());

        // Custom picker registered by a plugin or agent.
        let custom = PickerRequest::new(
            "plugin.demo",
            "Demo Picker",
            PickerLayout::List,
            vec![PickerItem::new(
                "demo.one",
                "Toggle Compact",
                "layout.toggle_compact_mode",
            )],
        );
        core.register_picker(custom);
        core.invoke(Command::PickerOpen {
            name: None,
            request_id: Some(PickerRequestId::new("plugin.demo")),
        })
        .unwrap();
        assert!(core.model().layout.compact_mode);
        core.invoke(Command::PickerChoose {
            request_id: Some(PickerRequestId::new("plugin.demo")),
            item_id: "demo.one".to_string(),
        })
        .unwrap();
        assert!(!core.model().layout.compact_mode);
    }

    #[test]
    fn region_pick_capture_and_cancel_publish_events() {
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        let sub = core.subscribe();

        core.invoke(Command::RegionPick { tab_id: None }).unwrap();
        let rect = RegionRect::new(40.0, 60.0, 320.0, 200.0);
        core.invoke(Command::RegionCapture {
            tab_id: None,
            rect: Some(rect),
            whole_page: false,
        })
        .unwrap();
        assert_eq!(core.last_region_capture().unwrap().rect, Some(rect));
        assert!(!core.last_region_capture().unwrap().whole_page);

        core.invoke(Command::RegionCapture {
            tab_id: None,
            rect: None,
            whole_page: true,
        })
        .unwrap();
        assert!(core.last_region_capture().unwrap().whole_page);

        core.invoke(Command::RegionCancel).unwrap();

        let mut saw_pick = false;
        let mut capture_count = 0usize;
        let mut saw_cancel = false;
        while let Ok(ev) = sub.try_recv() {
            match ev {
                Event::ViewRequested {
                    request: ViewRequest::RegionPick { .. },
                    ..
                } => saw_pick = true,
                Event::RegionCaptured { .. } => capture_count += 1,
                Event::ViewRequested {
                    request: ViewRequest::RegionPickCancelled,
                    ..
                } => saw_cancel = true,
                _ => {}
            }
        }
        assert!(saw_pick);
        assert_eq!(capture_count, 2);
        assert!(saw_cancel);
    }
}
