/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for phase 5 focus surfaces: explorer focus, hint labels, toolbar focus, and window mode.

use super::StubBackend;
use crate::commands::{Command, CommandError, CommandResult};
use crate::events::Event;
use crate::focus::{
    self, FocusDirection, FocusState, FocusSurface, FocusTarget, FolderStepEffect, HintOutcome, HintSession,
    LandingKind, PaneDirection, WindowCarryState,
};
use crate::model::{InputMode, SplitDirection};
use crate::tab_source::TabSource;
use std::sync::Arc;

impl StubBackend {
    /// Handles explorer focus, hint label, toolbar focus, and window mode commands.
    pub(super) fn handle_focus_command(
        &self,
        command: Command,
        tab_source: Option<Arc<dyn TabSource>>,
    ) -> CommandResult {
        match command {
            Command::ExplorerOpen => self.handle_explorer_open(),
            Command::ExplorerClose => self.handle_focus_close(FocusSurface::Explorer),
            Command::ExplorerMove { direction } => self.handle_explorer_move(direction),
            Command::ExplorerSelect { range } => self.handle_explorer_select(range),
            Command::ExplorerActivate => self.handle_explorer_activate(tab_source),
            Command::ExplorerPick => self.handle_explorer_pick(),
            Command::FocusHintInput { key } => self.handle_focus_hint_input(&key, tab_source),
            Command::FocusHintCancel => self.handle_focus_hint_cancel(),
            Command::ToolbarFocusOpen => self.handle_toolbar_focus_open(),
            Command::ToolbarFocusClose => self.handle_focus_close(FocusSurface::Toolbar),
            Command::ToolbarFocusMove { direction } => self.handle_toolbar_focus_move(direction),
            Command::ToolbarFocusActivate => self.handle_toolbar_focus_activate(tab_source),
            Command::WindowModeOpen => self.handle_window_mode_open(),
            Command::WindowModeClose => self.handle_focus_close(FocusSurface::Window),
            Command::WindowModeFocusPane { pane } => self.handle_window_mode_focus_pane(pane, tab_source),
            Command::WindowModeMovePane { direction } => self.handle_window_mode_move_pane(direction),
            Command::WindowModeCarryToggle => self.handle_window_mode_carry_toggle(),
            Command::WindowModeDrop => self.handle_window_mode_drop(),
            other => Err(CommandError::UnknownCommand(other.name().to_string())),
        }
    }

    /// Clears any active focus surface (called when vim mode is turned off).
    pub(super) fn clear_focus_on_vim_disabled(&self) {
        let (had_focus, was_focus_mode) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let had_focus = state.model.focus != FocusState::default();
            let was_focus_mode = matches!(
                state.model.input_mode,
                InputMode::Explorer | InputMode::Toolbar | InputMode::Window
            );
            state.model.focus.clear();
            (had_focus, was_focus_mode)
        };
        if had_focus {
            self.bus.publish(Event::FocusChanged {
                focus: FocusState::default(),
            });
        }
        if was_focus_mode {
            self.set_input_mode(InputMode::Normal);
        }
    }

    fn publish_focus(&self, focus: FocusState) {
        self.bus.publish(Event::FocusChanged { focus });
    }

    fn vim_enabled(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .model
            .vim_mode_enabled
    }

    fn handle_explorer_open(&self) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let cursor = state
                .model
                .active_tab
                .and_then(|tab_id| focus::explorer_target_for_tab(&state.model, tab_id))
                .or_else(|| focus::explorer_targets(&state.model).into_iter().next());
            state.model.focus = FocusState {
                surface: Some(FocusSurface::Explorer),
                cursor,
                selection: Vec::new(),
                visual_anchor: None,
                hints: None,
                window_carry: None,
            };
            state.model.focus.clone()
        };
        self.set_input_mode(InputMode::Explorer);
        self.publish_focus(focus);
        Ok(())
    }

    fn handle_focus_close(&self, surface: FocusSurface) -> CommandResult {
        let should_reset_mode = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface == Some(surface) || state.model.focus.surface.is_none() {
                state.model.focus.clear();
            }
            matches!(
                (surface, state.model.input_mode),
                (FocusSurface::Explorer, InputMode::Explorer)
                    | (FocusSurface::Toolbar, InputMode::Toolbar)
                    | (FocusSurface::Window, InputMode::Window)
            )
        };
        self.publish_focus(FocusState::default());
        if should_reset_mode {
            self.set_input_mode(InputMode::Normal);
        }
        Ok(())
    }

    fn handle_explorer_move(&self, direction: FocusDirection) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let (focus, folder_effect) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return Ok(());
            }
            let (next_cursor, effect) =
                focus::step_explorer(&state.model, state.model.focus.cursor.as_ref(), direction);
            let mut updated_folder = None;
            if let Some(effect) = effect {
                match effect {
                    FolderStepEffect::Expand(folder_id) => {
                        if let Some(folder) = state.model.folders.iter_mut().find(|f| f.id == folder_id) {
                            folder.collapsed = false;
                            updated_folder = Some(folder.clone());
                        }
                    }
                    FolderStepEffect::Collapse(folder_id) => {
                        if let Some(folder) = state.model.folders.iter_mut().find(|f| f.id == folder_id) {
                            folder.collapsed = true;
                            updated_folder = Some(folder.clone());
                        }
                    }
                }
            }
            state.model.focus.cursor = next_cursor.clone();
            state.model.focus.hints = None;
            if let (Some(anchor), Some(cursor)) = (&state.model.focus.visual_anchor, &next_cursor) {
                let targets = focus::explorer_targets(&state.model);
                state.model.focus.selection = focus::visual_range(&targets, anchor, cursor);
            }
            (state.model.focus.clone(), updated_folder)
        };
        if let Some(folder) = folder_effect {
            self.bus.publish(Event::FolderUpdated { folder });
        }
        self.publish_focus(focus);
        Ok(())
    }

    fn handle_explorer_select(&self, range: bool) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return Ok(());
            }
            let Some(cursor) = state.model.focus.cursor.clone() else {
                return Ok(());
            };
            if range {
                if state.model.focus.visual_anchor.is_some() {
                    state.model.focus.visual_anchor = None;
                    state.model.focus.selection.clear();
                } else {
                    state.model.focus.visual_anchor = Some(cursor.clone());
                    state.model.focus.selection = vec![cursor];
                }
            } else {
                if let Some(pos) = state.model.focus.selection.iter().position(|item| item == &cursor) {
                    state.model.focus.selection.remove(pos);
                } else {
                    state.model.focus.selection.push(cursor.clone());
                }
                let targets = focus::explorer_targets(&state.model);
                state.model.focus.cursor = focus::step_linear(&targets, Some(&cursor), FocusDirection::Next);
            }
            state.model.focus.clone()
        };
        self.publish_focus(focus);
        Ok(())
    }

    fn handle_explorer_activate(&self, tab_source: Option<Arc<dyn TabSource>>) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let target = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return Ok(());
            }
            state.model.focus.cursor.clone()
        };
        let Some(target) = target else {
            return Ok(());
        };
        self.activate_explorer_target(target, tab_source)
    }

    fn activate_explorer_target(&self, target: FocusTarget, tab_source: Option<Arc<dyn TabSource>>) -> CommandResult {
        match target {
            FocusTarget::Essential(tab_id) | FocusTarget::PinnedTab(tab_id) | FocusTarget::Tab(tab_id) => {
                if let Some(source) = tab_source {
                    source.activate_tab(tab_id)?;
                } else {
                    self.feed_tab_activated(Some(tab_id));
                }
                self.handle_focus_close(FocusSurface::Explorer)
            }
            FocusTarget::SplitGroup(group_id) => {
                let first_tab = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state
                        .model
                        .split_group(group_id)
                        .and_then(|group| group.tab_ids.first().copied())
                };
                if let Some(tab_id) = first_tab {
                    if let Some(source) = tab_source {
                        source.activate_tab(tab_id)?;
                    } else {
                        self.feed_tab_activated(Some(tab_id));
                    }
                }
                self.handle_focus_close(FocusSurface::Explorer)
            }
            FocusTarget::Folder(folder_id) => {
                self.handle_folder_toggle_collapsed(folder_id)?;
                let focus = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state.model.focus.hints = None;
                    state.model.focus.clone()
                };
                self.publish_focus(focus);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn handle_explorer_pick(&self) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let targets = focus::explorer_targets(&state.model);
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                let cursor = state
                    .model
                    .active_tab
                    .and_then(|tab_id| focus::explorer_target_for_tab(&state.model, tab_id))
                    .or_else(|| targets.first().cloned());
                state.model.focus = FocusState {
                    surface: Some(FocusSurface::Explorer),
                    cursor,
                    selection: Vec::new(),
                    visual_anchor: None,
                    hints: None,
                    window_carry: None,
                };
            }
            state.model.focus.hints = if targets.is_empty() {
                None
            } else {
                Some(HintSession::new(FocusSurface::Explorer, targets))
            };
            state.model.focus.clone()
        };
        self.set_input_mode(InputMode::Explorer);
        self.publish_focus(focus);
        Ok(())
    }

    fn handle_focus_hint_input(&self, key: &str, tab_source: Option<Arc<dyn TabSource>>) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let mut chars = key.chars();
        let Some(ch) = chars.next() else {
            return self.handle_focus_hint_cancel();
        };
        if chars.next().is_some() {
            return self.handle_focus_hint_cancel();
        }

        let (outcome, focus_after) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(hints) = state.model.focus.hints.as_mut() else {
                return Ok(());
            };
            let outcome = hints.type_char(ch);
            match &outcome {
                HintOutcome::Matched(target) => {
                    state.model.focus.cursor = Some(target.clone());
                    state.model.focus.hints = None;
                }
                HintOutcome::NoMatch => {
                    state.model.focus.hints = None;
                }
                HintOutcome::Narrowed => {}
            }
            (outcome, state.model.focus.clone())
        };

        self.publish_focus(focus_after);
        if let HintOutcome::Matched(target) = outcome {
            match target {
                FocusTarget::Essential(_)
                | FocusTarget::PinnedTab(_)
                | FocusTarget::Folder(_)
                | FocusTarget::Tab(_)
                | FocusTarget::SplitGroup(_) => {
                    return self.activate_explorer_target(target, tab_source);
                }
                FocusTarget::ToolbarItem(_) => {
                    return self.handle_toolbar_focus_activate(tab_source);
                }
                FocusTarget::Pane(tab_id) => {
                    if let Some(source) = tab_source {
                        source.activate_tab(tab_id)?;
                    } else {
                        self.feed_tab_activated(Some(tab_id));
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_focus_hint_cancel(&self) -> CommandResult {
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.focus.hints = None;
            state.model.focus.clone()
        };
        self.publish_focus(focus);
        Ok(())
    }

    fn handle_toolbar_focus_open(&self) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let targets = focus::toolbar_targets(&state.model);
            let cursor = targets
                .iter()
                .find(|target| {
                    matches!(
                        target,
                        FocusTarget::ToolbarItem(id) if id.as_str() == "builtin.url_bar"
                    )
                })
                .cloned()
                .or_else(|| targets.into_iter().next());
            state.model.focus = FocusState {
                surface: Some(FocusSurface::Toolbar),
                cursor,
                selection: Vec::new(),
                visual_anchor: None,
                hints: None,
                window_carry: None,
            };
            state.model.focus.clone()
        };
        self.set_input_mode(InputMode::Toolbar);
        self.publish_focus(focus);
        Ok(())
    }

    fn handle_toolbar_focus_move(&self, direction: FocusDirection) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Toolbar) {
                return Ok(());
            }
            let targets = focus::toolbar_targets(&state.model);
            state.model.focus.cursor = focus::step_linear(&targets, state.model.focus.cursor.as_ref(), direction);
            state.model.focus.clone()
        };
        self.publish_focus(focus);
        Ok(())
    }

    fn handle_toolbar_focus_activate(&self, _tab_source: Option<Arc<dyn TabSource>>) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let command_to_run = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Toolbar) {
                return Ok(());
            }
            let Some(FocusTarget::ToolbarItem(item_id)) = &state.model.focus.cursor else {
                return self.handle_focus_close(FocusSurface::Toolbar);
            };
            if item_id.as_str() == "builtin.url_bar" {
                let initial_query = state
                    .model
                    .active_tab()
                    .map(|tab| tab.url.clone())
                    .filter(|url| !url.is_empty() && url != "about:blank" && url != "about:newtab");
                Some(Command::AddressBarOpen {
                    new_tab: false,
                    initial_query,
                })
            } else {
                state
                    .model
                    .contributions
                    .toolbar_items
                    .iter()
                    .find(|item| &item.id == item_id)
                    .and_then(|item| Command::from_name_and_args(&item.command, &item.command_args).ok())
            }
        };

        self.handle_focus_close(FocusSurface::Toolbar)?;
        if let Some(command) = command_to_run {
            let _ = crate::BrowserCore::invoke(self, command);
        }
        Ok(())
    }

    fn handle_window_mode_open(&self) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let targets = focus::window_targets(&state.model);
            let cursor = state
                .model
                .active_tab
                .map(FocusTarget::Pane)
                .filter(|target| targets.contains(target))
                .or_else(|| targets.into_iter().next());
            state.model.focus = FocusState {
                surface: Some(FocusSurface::Window),
                cursor,
                selection: Vec::new(),
                visual_anchor: None,
                hints: None,
                window_carry: None,
            };
            state.model.focus.clone()
        };
        self.set_input_mode(InputMode::Window);
        self.publish_focus(focus);
        Ok(())
    }

    fn handle_window_mode_focus_pane(&self, pane: usize, tab_source: Option<Arc<dyn TabSource>>) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        if pane == 0 {
            return Err(CommandError::InvalidArgument {
                command: "window_mode.focus_pane".to_string(),
                message: "pane numbers start at 1".to_string(),
            });
        }
        let target_tab = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let targets = focus::window_targets(&state.model);
            match targets.get(pane - 1) {
                Some(FocusTarget::Pane(tab_id)) => Some(*tab_id),
                _ => None,
            }
        };
        let Some(tab_id) = target_tab else {
            return Ok(());
        };

        if let Some(source) = tab_source {
            source.activate_tab(tab_id)?;
        } else {
            self.feed_tab_activated(Some(tab_id));
        }

        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface == Some(FocusSurface::Window) {
                state.model.focus.cursor = Some(FocusTarget::Pane(tab_id));
                if let Some(carry) = state.model.focus.window_carry.as_mut()
                    && let Some(idx) = carry.targets.iter().position(|target| {
                        matches!(
                            target.kind,
                            LandingKind::PaneSwap { target_tab } if target_tab == tab_id
                        )
                    })
                {
                    carry.selected = idx;
                }
            }
            state.model.focus.clone()
        };
        self.publish_focus(focus);
        Ok(())
    }

    fn handle_window_mode_move_pane(&self, direction: PaneDirection) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }

        // When carrying a pane, directional keys move the landing preview across targets.
        let moved_carry = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(carry) = state.model.focus.window_carry.as_mut() {
                carry.selected = focus::step_landing_target(&carry.targets, carry.selected, direction);
                Some(state.model.focus.clone())
            } else {
                None
            }
        };
        if let Some(focus) = moved_carry {
            self.publish_focus(focus);
            return Ok(());
        }

        // Otherwise move/swap the active pane in `direction` using phase 4's split command.
        let planned: Option<(Vec<crate::model::TabId>, SplitDirection)> = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let active_tab = state.model.active_tab;
            let group = state.model.active_split_group();
            match (active_tab, group) {
                (Some(tab_id), Some(group)) => focus::move_pane_layout(group, tab_id, direction),
                _ => None,
            }
        };

        if let Some((tab_ids, split_direction)) = planned {
            crate::BrowserCore::invoke(
                self,
                Command::SplitTabs {
                    tab_ids,
                    direction: split_direction,
                },
            )?;
        }
        Ok(())
    }

    fn handle_window_mode_carry_toggle(&self) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Window) {
                return Ok(());
            }
            if state.model.focus.window_carry.is_some() {
                state.model.focus.window_carry = None;
            } else if let Some(active_tab) = state.model.active_tab {
                let targets = focus::compute_landing_targets(&state.model, active_tab);
                if !targets.is_empty() {
                    let selected = targets
                        .iter()
                        .position(|target| matches!(target.kind, LandingKind::PaneSwap { .. }))
                        .unwrap_or(0);
                    state.model.focus.window_carry = Some(WindowCarryState {
                        carried_tab: active_tab,
                        targets,
                        selected,
                    });
                }
            }
            state.model.focus.clone()
        };
        self.publish_focus(focus);
        Ok(())
    }

    fn handle_window_mode_drop(&self) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let (planned, focus) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let planned = state.model.focus.window_carry.take().and_then(|carry| {
                let target = carry.selected_target()?.clone();
                let group = state.model.active_split_group()?;
                focus::apply_landing_target(group, carry.carried_tab, &target)
            });
            (planned, state.model.focus.clone())
        };
        self.publish_focus(focus);
        if let Some((tab_ids, direction)) = planned {
            crate::BrowserCore::invoke(self, Command::SplitTabs { tab_ids, direction })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrowserCore;
    use crate::model::{SpaceId, SplitGroup, SplitGroupId, TabId};

    fn stub_with_vim() -> StubBackend {
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        core
    }

    #[test]
    fn focus_commands_are_gated_on_vim_mode() {
        let core = StubBackend::new();
        assert!(!core.model().vim_mode_enabled);
        core.invoke(Command::ExplorerOpen).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
        assert_eq!(core.model().focus.surface, None);

        core.invoke(Command::ToolbarFocusOpen).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);

        core.invoke(Command::WindowModeOpen).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
    }

    #[test]
    fn explorer_open_move_select_activate_and_close() {
        let core = stub_with_vim();
        let sub = core.subscribe();
        let mut view_model = core.model();

        core.invoke(Command::ExplorerOpen).unwrap();
        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }
        assert_eq!(view_model.input_mode, InputMode::Explorer);
        assert_eq!(view_model.focus.surface, Some(FocusSurface::Explorer));
        assert!(view_model.focus.cursor.is_some());

        core.invoke(Command::ExplorerMove {
            direction: FocusDirection::First,
        })
        .unwrap();
        core.invoke(Command::ExplorerSelect { range: false }).unwrap();
        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }
        assert_eq!(view_model.focus.selection.len(), 1);

        let cursor_before_activate = view_model.focus.cursor.clone();
        core.invoke(Command::ExplorerActivate).unwrap();
        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }
        assert_eq!(view_model.input_mode, InputMode::Normal);
        assert_eq!(view_model.focus.surface, None);
        if let Some(FocusTarget::Essential(tab_id) | FocusTarget::PinnedTab(tab_id) | FocusTarget::Tab(tab_id)) =
            cursor_before_activate
        {
            assert_eq!(view_model.active_tab, Some(tab_id));
        }
    }

    #[test]
    fn explorer_hints_pick_and_jump_to_tab() {
        let core = stub_with_vim();
        core.invoke(Command::ExplorerPick).unwrap();
        let hints = core.model().focus.hints.expect("hints active");
        assert!(!hints.entries.is_empty());
        let first_label = hints.entries[0].label.clone();
        let first_target = hints.entries[0].target.clone();

        core.invoke(Command::FocusHintInput { key: first_label }).unwrap();
        assert_eq!(core.model().focus.hints, None);
        if let FocusTarget::Essential(tab_id) = first_target {
            assert_eq!(core.model().active_tab, Some(tab_id));
        }
    }

    #[test]
    fn toolbar_focus_moves_and_activates_item() {
        let core = stub_with_vim();
        let sub = core.subscribe();

        core.invoke(Command::ToolbarFocusOpen).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Toolbar);
        assert_eq!(
            core.model().focus.cursor,
            Some(FocusTarget::ToolbarItem(crate::contributions::ContributionId::new(
                "builtin.url_bar"
            )))
        );

        core.invoke(Command::ToolbarFocusMove {
            direction: FocusDirection::First,
        })
        .unwrap();
        assert_eq!(
            core.model().focus.cursor,
            Some(FocusTarget::ToolbarItem(crate::contributions::ContributionId::new(
                "builtin.app_menu"
            )))
        );

        core.invoke(Command::ToolbarFocusActivate).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
        let mut saw_app_menu = false;
        while let Ok(event) = sub.try_recv() {
            if matches!(event, Event::AppMenuToggleRequested) {
                saw_app_menu = true;
            }
        }
        assert!(saw_app_menu);
    }

    #[test]
    fn window_mode_focuses_and_moves_panes_and_supports_carry_drop() {
        let core = stub_with_vim();
        let mut model = core.model();
        model.split_groups = vec![SplitGroup {
            id: SplitGroupId(1),
            space_id: SpaceId(1),
            direction: SplitDirection::Grid,
            tab_ids: vec![TabId(12), TabId(11), TabId(10)],
            ratios: vec![0.5, 0.5],
        }];
        model.active_tab = Some(TabId(12));
        core.replace_model(model);

        core.invoke(Command::WindowModeOpen).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Window);

        core.invoke(Command::WindowModeFocusPane { pane: 3 }).unwrap();
        assert_eq!(core.model().active_tab, Some(TabId(10)));

        core.invoke(Command::WindowModeMovePane {
            direction: PaneDirection::Left,
        })
        .unwrap();
        let group = core.model().active_split_group().cloned().expect("split group");
        assert_ne!(group.tab_ids, vec![TabId(12), TabId(11), TabId(10)]);

        core.invoke(Command::WindowModeCarryToggle).unwrap();
        assert!(core.model().focus.window_carry.is_some());
        core.invoke(Command::WindowModeDrop).unwrap();
        assert!(core.model().focus.window_carry.is_none());

        core.invoke(Command::WindowModeClose).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
    }

    #[test]
    fn disabling_vim_mode_clears_active_focus() {
        let core = stub_with_vim();
        core.invoke(Command::ExplorerOpen).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Explorer);

        core.invoke(Command::VimSetEnabled { enabled: false }).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
        assert_eq!(core.model().focus.surface, None);
    }
}
