/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for phase 4's commands: split view, Glance, toasts, media controls, downloads and compact mode.

use super::{StubBackend, StubState};
use crate::commands::{Command, CommandError, CommandResult};
use crate::events::{Event, ViewRequest};
use crate::media::{self, MediaControl};
use crate::model::{MediaSessionId, SpaceId, SplitDirection, SplitGroup, SplitGroupId, TabId};
use crate::split::{self, DropZone, Glance, MAX_SPLIT_PANES};
use crate::tab_source::TabSource;
use crate::toasts::{self, DEFAULT_TOAST_TIMEOUT_MS, Toast, ToastAction, ToastId};
use std::sync::Arc;

impl StubBackend {
    /// Handles every phase 4 command. `invoke` routes them here.
    pub(super) fn handle_page_layout_command(
        &self,
        command: Command,
        tab_source: Option<Arc<dyn TabSource>>,
    ) -> CommandResult {
        let name = command.name().to_string();
        match command {
            Command::SplitTabs { tab_ids, direction } => self.split_tabs(&name, tab_ids, direction, None),
            Command::SplitNewEmpty => {
                let anchor = self.resolve_target_tab(None, &name)?;
                let new_tab = self.open_tab_for(&name, None, anchor, tab_source.as_ref())?;
                self.split_with(&name, anchor, new_tab, DropZone::Right)?;
                self.activate(new_tab, tab_source.as_ref())?;
                self.request_view(ViewRequest::AddressBarOpen {
                    new_tab: false,
                    initial_query: None,
                });
                Ok(())
            }
            Command::SplitOpenUrl { tab_id, url } => {
                let anchor = self.resolve_target_tab(tab_id, &name)?;
                let new_tab = self.open_tab_for(&name, Some(&url), anchor, tab_source.as_ref())?;
                self.split_with(&name, anchor, new_tab, DropZone::Right)?;
                self.activate(new_tab, tab_source.as_ref())
            }
            Command::SplitDrop {
                tab_id,
                target_tab_id,
                zone,
            } => {
                self.resolve_target_tab(Some(tab_id), &name)?;
                self.resolve_target_tab(Some(target_tab_id), &name)?;
                if tab_id == target_tab_id {
                    return Err(CommandError::InvalidArgument {
                        command: name,
                        message: "cannot split a tab with itself".to_string(),
                    });
                }
                self.split_with(&name, target_tab_id, tab_id, zone)?;
                self.activate(tab_id, tab_source.as_ref())
            }
            Command::SplitRemoveTab { tab_id } => {
                let events = {
                    let mut state = self.lock();
                    detach_from_groups(&mut state, &[tab_id], None)
                };
                self.publish_all(events);
                Ok(())
            }
            Command::SplitUnsplit { group_id } => {
                let event = {
                    let mut state = self.lock();
                    let group_id = group_id
                        .or_else(|| state.model.active_split_group().map(|group| group.id))
                        .ok_or_else(|| not_found(&name, "split group"))?;
                    remove_group(&mut state, group_id).ok_or_else(|| not_found(&name, "split group"))?
                };
                self.bus.publish(event);
                Ok(())
            }
            Command::SplitSetLayout { group_id, direction } => self.update_group(&name, group_id, |group| {
                group.direction = direction;
                group.ratios = split::default_ratios(direction, group.tab_ids.len());
            }),
            Command::SplitResize { group_id, ratios } => self.update_group(&name, Some(group_id), |group| {
                group.ratios = split::normalized_ratios(group.direction, group.tab_ids.len(), &ratios);
            }),
            Command::GlanceOpen {
                url,
                parent_tab_id,
                origin,
            } => {
                if self.lock().model.glance.is_some() {
                    self.close_glance(tab_source.as_ref())?;
                }
                let parent = self.resolve_target_tab(parent_tab_id, &name).ok();
                let anchor = match parent {
                    Some(parent) => parent,
                    None => self.resolve_target_tab(None, &name)?,
                };
                let tab_id = self.open_tab_for(&name, Some(&url), anchor, tab_source.as_ref())?;
                let glance = Glance {
                    tab_id,
                    parent_tab_id: parent,
                    origin,
                };
                self.lock().model.glance = Some(glance.clone());
                self.bus.publish(Event::GlanceChanged { glance: Some(glance) });
                Ok(())
            }
            Command::GlanceClose => self.close_glance(tab_source.as_ref()),
            Command::GlanceExpand => {
                let glance = self.take_glance(&name)?;
                self.activate(glance.tab_id, tab_source.as_ref())
            }
            Command::GlanceSplit => {
                let glance = self.take_glance(&name)?;
                let parent = glance
                    .parent_tab_id
                    .ok_or_else(|| not_found(&name, "Glance parent tab"))?;
                self.split_with(&name, parent, glance.tab_id, DropZone::Right)?;
                self.activate(glance.tab_id, tab_source.as_ref())
            }
            Command::ToastShow {
                message,
                timeout_ms,
                action_label,
                action_command,
            } => {
                let action = action_label.zip(action_command).map(|(label, command)| ToastAction {
                    label,
                    command,
                    command_args: Vec::new(),
                });
                self.show_toast(message, action, timeout_ms);
                Ok(())
            }
            Command::ToastDismiss { toast_id } => {
                let removed = {
                    let mut state = self.lock();
                    let before = state.model.toasts.len();
                    state.model.toasts.retain(|toast| toast.id != toast_id);
                    state.model.toasts.len() != before
                };
                if removed {
                    self.bus.publish(Event::ToastDismissed { toast_id });
                }
                Ok(())
            }
            Command::MediaSessionControl { session_id, control } => {
                self.control_media(&name, session_id, control, tab_source.as_ref())
            }
            Command::LayoutSetCompactHide { hide } => {
                let layout = {
                    let mut state = self.lock();
                    state.model.layout.compact_hide = hide;
                    state.model.layout.normalize_compact_state();
                    state.model.layout.clone()
                };
                self.bus.publish(Event::LayoutSettingsChanged { layout });
                Ok(())
            }
            Command::LayoutCompactRevealSidebar => {
                self.request_view(ViewRequest::CompactSidebarReveal);
                Ok(())
            }
            Command::LayoutSetTabSwitchMotion { enabled } => {
                let layout = {
                    let mut state = self.lock();
                    state.model.layout.tab_switch_motion = enabled;
                    state.model.layout.clone()
                };
                self.bus.publish(Event::LayoutSettingsChanged { layout });
                Ok(())
            }
            Command::DebugSampleDownload => self.start_sample_download(),
            Command::DebugSampleMedia => {
                let sessions = {
                    let mut state = self.lock();
                    let active = state.model.active_tab;
                    let order = state.model.tabs_in_active_space_order();
                    let tab_id = order
                        .iter()
                        .copied()
                        .find(|id| Some(*id) != active)
                        .or(active)
                        .ok_or_else(|| not_found(&name, "tab to play media in"))?;
                    let id = MediaSessionId(state.model.media_sessions.iter().map(|s| s.id.0).max().unwrap_or(0) + 1);
                    state.model.media_sessions.retain(|session| session.tab_id != tab_id);
                    state.model.media_sessions.push(media::sample_session(id, tab_id));
                    state.model.media_sessions.clone()
                };
                self.bus.publish(Event::MediaSessionsChanged { sessions });
                Ok(())
            }
            other => Err(CommandError::UnknownCommand(other.name().to_string())),
        }
    }

    /// Shows a toast and publishes it, along with any toasts it pushed out.
    pub fn show_toast(&self, message: String, action: Option<ToastAction>, timeout_ms: Option<u64>) -> ToastId {
        self.show_toast_with_description(message, None, action, timeout_ms)
    }

    /// Shows a toast with an optional second-line description and publishes it (reusing an active duplicate).
    pub fn show_toast_with_description(
        &self,
        message: String,
        description: Option<String>,
        action: Option<ToastAction>,
        timeout_ms: Option<u64>,
    ) -> ToastId {
        let (toast, dropped) = {
            let mut state = self.lock();
            let id = ToastId(
                state
                    .model
                    .toasts
                    .iter()
                    .map(|toast| toast.id.0)
                    .max()
                    .unwrap_or(0)
                    .max(state.last_toast_id)
                    + 1,
            );
            state.last_toast_id = id.0;
            let toast = Toast {
                id,
                message,
                description,
                action,
                timeout_ms: timeout_ms.unwrap_or(DEFAULT_TOAST_TIMEOUT_MS),
                bounce_seq: 0,
            };
            toasts::push_or_reuse_toast(&mut state.model.toasts, toast)
        };
        let id = toast.id;
        for toast_id in dropped {
            self.bus.publish(Event::ToastDismissed { toast_id });
        }
        self.bus.publish(Event::ToastShown { toast });
        id
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, StubState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn publish_all(&self, events: Vec<Event>) {
        for event in events {
            self.bus.publish(event);
        }
    }

    fn activate(&self, tab_id: TabId, tab_source: Option<&Arc<dyn TabSource>>) -> CommandResult {
        match tab_source {
            Some(source) => source.activate_tab(tab_id),
            None => {
                self.feed_tab_activated(Some(tab_id));
                Ok(())
            }
        }
    }

    /// Opens a background tab after `anchor` and returns its id.
    ///
    /// The engine opens tabs synchronously, so the tab it feeds back during the call is the new one.
    fn open_tab_for(
        &self,
        command: &str,
        url: Option<&str>,
        anchor: TabId,
        tab_source: Option<&Arc<dyn TabSource>>,
    ) -> Result<TabId, CommandError> {
        {
            let mut state = self.lock();
            state.last_opened_tab = None;
            state.suppress_bg_tab_toast = true;
        }
        let open_result = match tab_source {
            Some(source) => source.new_tab(url, Some(anchor), false),
            None => self.handle_stub_tab_new(url.map(str::to_string), Some(anchor), false),
        };
        let mut state = self.lock();
        state.suppress_bg_tab_toast = false;
        open_result?;
        state
            .last_opened_tab
            .take()
            .ok_or_else(|| CommandError::Failed(format!("{command}: the engine opened no tab")))
    }

    /// Puts `tab_id` beside `target` on the side `zone` names, joining `target`'s group when it has one.
    fn split_with(&self, command: &str, target: TabId, tab_id: TabId, zone: DropZone) -> CommandResult {
        let target_group = self.lock().model.split_group_for_tab(target).cloned();
        let Some(mut group) = target_group else {
            let tab_ids = if zone.places_first() {
                vec![tab_id, target]
            } else {
                vec![target, tab_id]
            };
            return self.split_tabs(command, tab_ids, zone.direction(), None);
        };

        let mut tab_ids = group.tab_ids.clone();
        tab_ids.retain(|id| *id != tab_id);
        if tab_ids.len() >= MAX_SPLIT_PANES {
            self.show_toast(toasts::messages::SPLIT_VIEW_LIMIT.to_string(), None, None);
            return Err(CommandError::InvalidArgument {
                command: command.to_string(),
                message: format!("a split view holds at most {MAX_SPLIT_PANES} tabs"),
            });
        }
        let at = tab_ids.iter().position(|id| *id == target).unwrap_or(tab_ids.len());
        tab_ids.insert(if zone.places_first() { at } else { at + 1 }, tab_id);
        group.direction = if tab_ids.len() >= 3 {
            SplitDirection::Grid
        } else {
            zone.direction()
        };
        self.split_tabs(command, tab_ids, group.direction, Some(group.id))
    }

    /// Makes `tab_ids` one group (reusing `reuse` as its id when given) and takes them out of every other group.
    fn split_tabs(
        &self,
        command: &str,
        tab_ids: Vec<TabId>,
        direction: SplitDirection,
        reuse: Option<SplitGroupId>,
    ) -> CommandResult {
        let mut unique = Vec::new();
        for id in tab_ids {
            if !unique.contains(&id) {
                unique.push(id);
            }
        }
        if unique.len() < 2 {
            return Err(CommandError::InvalidArgument {
                command: command.to_string(),
                message: "a split view needs at least two tabs".to_string(),
            });
        }
        if unique.len() > MAX_SPLIT_PANES {
            self.show_toast(toasts::messages::SPLIT_VIEW_LIMIT.to_string(), None, None);
            return Err(CommandError::InvalidArgument {
                command: command.to_string(),
                message: format!("a split view holds at most {MAX_SPLIT_PANES} tabs"),
            });
        }

        let events = {
            let mut state = self.lock();
            if let Some(missing) = unique.iter().find(|id| state.model.tab(**id).is_none()) {
                return Err(not_found(command, &format!("tab {}", missing.0)));
            }
            let id = reuse.unwrap_or_else(|| {
                let id = SplitGroupId(state.next_split_id);
                state.next_split_id += 1;
                id
            });
            let mut events = detach_from_groups(&mut state, &unique, Some(id));
            let space_id = state
                .model
                .tab(unique[0])
                .and_then(|tab| tab.space_id)
                .or(state.model.active_space)
                .unwrap_or(SpaceId(1));
            let group = SplitGroup {
                id,
                space_id,
                direction,
                ratios: split::default_ratios(direction, unique.len()),
                tab_ids: unique.clone(),
            };
            for tab in &mut state.model.tabs {
                if unique.contains(&tab.id) {
                    tab.split_group_id = Some(id);
                }
            }
            match state.model.split_groups.iter_mut().find(|existing| existing.id == id) {
                Some(existing) => *existing = group.clone(),
                None => state.model.split_groups.push(group.clone()),
            }
            events.push(Event::SplitGroupUpdated { group });
            events
        };
        self.publish_all(events);
        Ok(())
    }

    fn update_group(
        &self,
        command: &str,
        group_id: Option<SplitGroupId>,
        change: impl FnOnce(&mut SplitGroup),
    ) -> CommandResult {
        let group = {
            let mut state = self.lock();
            let group_id = group_id
                .or_else(|| state.model.active_split_group().map(|group| group.id))
                .ok_or_else(|| not_found(command, "split group"))?;
            let group = state
                .model
                .split_groups
                .iter_mut()
                .find(|group| group.id == group_id)
                .ok_or_else(|| not_found(command, &format!("split group {}", group_id.0)))?;
            change(group);
            group.clone()
        };
        self.bus.publish(Event::SplitGroupUpdated { group });
        Ok(())
    }

    fn take_glance(&self, command: &str) -> Result<Glance, CommandError> {
        let glance = self
            .lock()
            .model
            .glance
            .take()
            .ok_or_else(|| not_found(command, "Glance"))?;
        self.bus.publish(Event::GlanceChanged { glance: None });
        Ok(glance)
    }

    fn close_glance(&self, tab_source: Option<&Arc<dyn TabSource>>) -> CommandResult {
        let glance = self.take_glance("glance.close")?;
        match tab_source {
            Some(source) => source.close_tab(glance.tab_id, true),
            None => self.handle_stub_tab_close(glance.tab_id),
        }
    }

    fn control_media(
        &self,
        command: &str,
        session_id: MediaSessionId,
        control: MediaControl,
        tab_source: Option<&Arc<dyn TabSource>>,
    ) -> CommandResult {
        let (changed, tab_id, sessions) = {
            let mut state = self.lock();
            let session = state
                .model
                .media_sessions
                .iter_mut()
                .find(|session| session.id == session_id)
                .ok_or_else(|| not_found(command, &format!("media session {}", session_id.0)))?;
            let changed = media::apply_control(session, control);
            let tab_id = session.tab_id;
            (changed, tab_id, state.model.media_sessions.clone())
        };
        if changed {
            self.bus.publish(Event::MediaSessionsChanged { sessions });
        }
        match control {
            MediaControl::Focus => self.activate(tab_id, tab_source),
            MediaControl::Previous | MediaControl::Next | MediaControl::PictureInPicture => {
                eprintln!(
                    "zendia: stub backend has no media engine for `{command}` (tab {})",
                    tab_id.0
                );
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

/// Takes `tab_ids` out of every group except `keep`. Groups left with one tab are removed.
fn detach_from_groups(state: &mut StubState, tab_ids: &[TabId], keep: Option<SplitGroupId>) -> Vec<Event> {
    let mut events = Vec::new();
    let mut emptied = Vec::new();
    for group in &mut state.model.split_groups {
        if Some(group.id) == keep || !group.tab_ids.iter().any(|id| tab_ids.contains(id)) {
            continue;
        }
        group.tab_ids.retain(|id| !tab_ids.contains(id));
        if group.tab_ids.len() < 2 {
            emptied.push(group.id);
        } else {
            group.ratios = split::default_ratios(group.direction, group.tab_ids.len());
            events.push(Event::SplitGroupUpdated { group: group.clone() });
        }
    }
    for tab in &mut state.model.tabs {
        if tab_ids.contains(&tab.id) && tab.split_group_id != keep {
            tab.split_group_id = None;
        }
    }
    for group_id in emptied {
        if let Some(event) = remove_group(state, group_id) {
            events.push(event);
        }
    }
    events
}

fn remove_group(state: &mut StubState, group_id: SplitGroupId) -> Option<Event> {
    let before = state.model.split_groups.len();
    state.model.split_groups.retain(|group| group.id != group_id);
    if state.model.split_groups.len() == before {
        return None;
    }
    for tab in &mut state.model.tabs {
        if tab.split_group_id == Some(group_id) {
            tab.split_group_id = None;
        }
    }
    Some(Event::SplitGroupRemoved { group_id })
}

fn not_found(command: &str, target: &str) -> CommandError {
    CommandError::TargetNotFound {
        command: command.to_string(),
        target: target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crate::BrowserCore;
    use crate::commands::Command;
    use crate::model::{CompactHide, SplitDirection, TabId};
    use crate::split::DropZone;
    use crate::stub::StubBackend;

    fn core_with_tabs(count: usize) -> (StubBackend, Vec<TabId>) {
        let core = StubBackend::new_for_external_tabs();
        for _ in 0..count {
            core.invoke(Command::TabNew {
                url: Some("https://example.org/".to_string()),
                after: None,
                activate: true,
            })
            .unwrap();
        }
        let ids = core.model().tabs.iter().map(|tab| tab.id).collect();
        (core, ids)
    }

    #[test]
    fn splitting_tabs_makes_one_group() {
        let (core, ids) = core_with_tabs(3);
        core.invoke(Command::SplitTabs {
            tab_ids: vec![ids[0], ids[1]],
            direction: SplitDirection::Horizontal,
        })
        .unwrap();
        let model = core.model();
        assert_eq!(model.split_groups.len(), 1);
        assert_eq!(
            model.split_group_for_tab(ids[1]).map(|g| g.tab_ids.clone()),
            Some(vec![ids[0], ids[1]])
        );
        assert!(model.tab(ids[0]).unwrap().split_group_id.is_some());
    }

    #[test]
    fn a_tab_leaves_its_old_group_when_split_again() {
        let (core, ids) = core_with_tabs(4);
        core.invoke(Command::SplitTabs {
            tab_ids: vec![ids[0], ids[1]],
            direction: SplitDirection::Horizontal,
        })
        .unwrap();
        core.invoke(Command::SplitTabs {
            tab_ids: vec![ids[1], ids[2]],
            direction: SplitDirection::Vertical,
        })
        .unwrap();
        let model = core.model();
        assert_eq!(
            model.split_groups.len(),
            1,
            "the first group dissolved when left with one tab"
        );
        assert!(model.split_group_for_tab(ids[0]).is_none());
        assert!(model.tab(ids[0]).unwrap().split_group_id.is_none());
    }

    #[test]
    fn dropping_on_a_grouped_pane_joins_the_group_as_a_grid() {
        let (core, ids) = core_with_tabs(3);
        core.invoke(Command::SplitTabs {
            tab_ids: vec![ids[0], ids[1]],
            direction: SplitDirection::Horizontal,
        })
        .unwrap();
        core.invoke(Command::SplitDrop {
            tab_id: ids[2],
            target_tab_id: ids[0],
            zone: DropZone::Left,
        })
        .unwrap();
        let model = core.model();
        let group = model.split_group_for_tab(ids[2]).unwrap();
        assert_eq!(group.tab_ids, vec![ids[2], ids[0], ids[1]]);
        assert_eq!(group.direction, SplitDirection::Grid);
        assert_eq!(model.active_tab, Some(ids[2]));
    }

    #[test]
    fn a_group_holds_at_most_four_tabs() {
        let (core, ids) = core_with_tabs(5);
        assert!(
            core.invoke(Command::SplitTabs {
                tab_ids: ids.clone(),
                direction: SplitDirection::Grid,
            })
            .is_err()
        );
        assert!(core.model().split_groups.is_empty());
        assert_eq!(core.model().toasts.len(), 1, "the limit shows Zen's toast");
    }

    #[test]
    fn closing_a_tab_shrinks_or_dissolves_its_group() {
        let (core, ids) = core_with_tabs(3);
        core.invoke(Command::SplitTabs {
            tab_ids: ids.clone(),
            direction: SplitDirection::Grid,
        })
        .unwrap();
        core.invoke(Command::TabClose {
            tab_id: Some(ids[2]),
            force: true,
        })
        .unwrap();
        assert_eq!(core.model().split_groups[0].tab_ids.len(), 2);
        assert_eq!(
            core.model().split_groups[0].ratios,
            vec![0.5, 0.5],
            "a two-pane grid lays out side by side"
        );
        core.invoke(Command::TabClose {
            tab_id: Some(ids[1]),
            force: true,
        })
        .unwrap();
        assert!(core.model().split_groups.is_empty());
    }

    #[test]
    fn resize_and_layout_change_the_active_group() {
        let (core, ids) = core_with_tabs(2);
        core.invoke(Command::SplitTabs {
            tab_ids: ids.clone(),
            direction: SplitDirection::Horizontal,
        })
        .unwrap();
        let group_id = core.model().split_groups[0].id;
        core.invoke(Command::SplitResize {
            group_id,
            ratios: vec![3.0, 1.0],
        })
        .unwrap();
        assert_eq!(core.model().split_groups[0].ratios, vec![0.75, 0.25]);
        core.invoke(Command::SplitSetLayout {
            group_id: None,
            direction: SplitDirection::Vertical,
        })
        .unwrap();
        assert_eq!(core.model().split_groups[0].direction, SplitDirection::Vertical);
        assert_eq!(core.model().split_groups[0].ratios, vec![0.5, 0.5]);
        core.invoke(Command::SplitUnsplit { group_id: None }).unwrap();
        assert!(core.model().split_groups.is_empty());
    }

    #[test]
    fn new_empty_split_opens_a_tab_beside_the_active_one() {
        let (core, ids) = core_with_tabs(1);
        core.invoke(Command::SplitNewEmpty).unwrap();
        let model = core.model();
        let group = model.split_group_for_tab(ids[0]).unwrap();
        assert_eq!(group.tab_ids.len(), 2);
        assert_eq!(model.active_tab, Some(group.tab_ids[1]));
    }

    #[test]
    fn glance_opens_a_hidden_tab_and_expands_into_a_normal_one() {
        let (core, ids) = core_with_tabs(1);
        core.invoke(Command::GlanceOpen {
            url: "https://example.org/glance".to_string(),
            parent_tab_id: None,
            origin: Some((0.5, 0.25)),
        })
        .unwrap();
        let model = core.model();
        let glance = model.glance.clone().unwrap();
        assert_eq!(glance.parent_tab_id, Some(ids[0]));
        assert_eq!(model.active_tab, Some(ids[0]), "Glance does not switch tabs");
        let space = model.active_space.unwrap();
        assert!(
            !model
                .normal_tabs_in_space(space)
                .iter()
                .any(|tab| tab.id == glance.tab_id)
        );

        core.invoke(Command::GlanceExpand).unwrap();
        let model = core.model();
        assert!(model.glance.is_none());
        assert_eq!(model.active_tab, Some(glance.tab_id));
        assert!(
            model
                .normal_tabs_in_space(space)
                .iter()
                .any(|tab| tab.id == glance.tab_id)
        );
    }

    #[test]
    fn glance_close_closes_its_tab_and_split_joins_the_parent() {
        let (core, ids) = core_with_tabs(1);
        let open = || {
            core.invoke(Command::GlanceOpen {
                url: "https://example.org/glance".to_string(),
                parent_tab_id: Some(ids[0]),
                origin: None,
            })
            .unwrap();
            core.model().glance.unwrap().tab_id
        };
        let first = open();
        core.invoke(Command::GlanceClose).unwrap();
        assert!(core.model().tab(first).is_none());

        let second = open();
        core.invoke(Command::GlanceSplit).unwrap();
        let model = core.model();
        assert_eq!(
            model.split_group_for_tab(ids[0]).map(|g| g.tab_ids.clone()),
            Some(vec![ids[0], second])
        );
        assert!(core.invoke(Command::GlanceClose).is_err(), "nothing left to close");
    }

    #[test]
    fn toasts_show_and_dismiss() {
        let core = StubBackend::new();
        core.invoke(Command::ToastShow {
            message: "Hello".to_string(),
            timeout_ms: None,
            action_label: Some("Undo".to_string()),
            action_command: Some("tab.reopen_closed".to_string()),
        })
        .unwrap();
        let toast = core.model().toasts[0].clone();
        assert_eq!(toast.timeout_ms, crate::toasts::DEFAULT_TOAST_TIMEOUT_MS);
        assert_eq!(toast.action.unwrap().label, "Undo");
        core.invoke(Command::ToastDismiss { toast_id: toast.id }).unwrap();
        assert!(core.model().toasts.is_empty());
    }

    #[test]
    fn copying_the_url_shows_zens_toast() {
        let core = StubBackend::new();
        core.invoke(Command::AddressBarCopyCurrentUrl).unwrap();
        assert_eq!(
            core.model().toasts.last().map(|t| t.message.clone()),
            Some(crate::toasts::messages::COPIED_CURRENT_URL.to_string())
        );
    }

    #[test]
    fn media_controls_change_the_session() {
        let (core, _) = core_with_tabs(2);
        core.invoke(Command::DebugSampleMedia).unwrap();
        let session = core.model().media_sessions[0].clone();
        assert_ne!(Some(session.tab_id), core.model().active_tab);
        let control = |control| {
            core.invoke(Command::MediaSessionControl {
                session_id: session.id,
                control,
            })
            .unwrap()
        };
        control(crate::media::MediaControl::TogglePlay);
        control(crate::media::MediaControl::ToggleMute);
        let updated = core.model().media_sessions[0].clone();
        assert!(!updated.is_playing && updated.is_muted);
        control(crate::media::MediaControl::Focus);
        assert_eq!(core.model().active_tab, Some(session.tab_id));
    }

    #[test]
    fn compact_mode_settings_round_trip() {
        let core = StubBackend::new();
        core.invoke(Command::LayoutToggleCompactMode).unwrap();
        core.invoke(Command::LayoutSetCompactHide {
            hide: CompactHide::Toolbar,
        })
        .unwrap();
        // In Single Toolbar layout there is no top toolbar, so compact mode still hides the sidebar.
        let single_layout = core.model().layout;
        assert!(
            single_layout.compact_mode
                && single_layout.compact_hides_sidebar()
                && !single_layout.compact_hides_toolbar()
        );

        // Switching to Multiple Toolbars lets CompactHide::Toolbar hide only the top toolbar.
        core.invoke(Command::LayoutSetToolbarLayout {
            layout: crate::model::ToolbarLayout::Multiple,
        })
        .unwrap();
        let multi_layout = core.model().layout;
        assert!(
            multi_layout.compact_mode && multi_layout.compact_hides_toolbar() && !multi_layout.compact_hides_sidebar()
        );
    }

    #[test]
    fn background_tab_in_compact_mode_shows_toast() {
        let core = StubBackend::new();
        core.invoke(Command::LayoutToggleCompactMode).unwrap();
        core.invoke(Command::TabNew {
            url: Some("https://example.org/bg".to_string()),
            after: None,
            activate: false,
        })
        .unwrap();
        let toasts = core.model().toasts;
        assert_eq!(
            toasts.last().map(|t| t.message.as_str()),
            Some(crate::toasts::messages::BACKGROUND_TAB_OPENED)
        );
        assert_eq!(
            toasts
                .last()
                .and_then(|t| t.action.as_ref())
                .map(|a| a.command.as_str()),
            Some("tab.activate")
        );
    }

    #[test]
    fn sample_download_is_published() {
        let core = StubBackend::new();
        let subscription = core.subscribe();
        core.invoke(Command::DebugSampleDownload).unwrap();
        let mut saw_start = false;
        while let Ok(event) = subscription.try_recv() {
            saw_start |= matches!(event, crate::events::Event::DownloadStarted { .. });
        }
        assert!(saw_start);
    }
}
