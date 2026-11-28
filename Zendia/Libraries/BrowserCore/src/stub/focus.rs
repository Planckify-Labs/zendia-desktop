/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for phase 5 focus surfaces: explorer focus, hint labels, toolbar focus, and window mode.

use super::{StubBackend, StubState};
use crate::commands::{Command, CommandError, CommandResult};
use crate::events::{Event, ViewRequest};
use crate::focus::{
    self, FocusDirection, FocusState, FocusSurface, FocusTarget, HintMatchAction, HintOutcome, HintSession,
    LandingKind, PaneDirection,
};
use crate::model::{InputMode, SplitDirection, SplitGroupId, TabId};
use crate::tab_actions::TabAction;
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
            Command::ExplorerClose => self.handle_explorer_close(),
            Command::ExplorerMove { direction, count } => self.handle_explorer_move(direction, count),
            Command::ExplorerSelect { range } => self.handle_explorer_select(range),
            Command::ExplorerActivate => self.handle_explorer_activate(tab_source),
            Command::ExplorerPick => self.handle_explorer_pick(),
            Command::FocusHintInput { key } => self.handle_focus_hint_input(&key, tab_source),
            Command::FocusHintCancel => self.handle_focus_hint_cancel(),
            Command::ToolbarFocusOpen => self.handle_toolbar_focus_open(),
            Command::ToolbarFocusClose => self.handle_focus_close(FocusSurface::Toolbar),
            Command::ToolbarFocusMove { direction, count } => self.handle_toolbar_focus_move(direction, count),
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

    /// Resolves the last-used pane of `group_id` using `vim.tab_recency`, falling back to its first pane (`EXP-19`).
    pub(super) fn last_used_tab_in_split_group(state: &StubState, group_id: SplitGroupId) -> Option<TabId> {
        let group = state.model.split_group(group_id)?;
        state
            .vim
            .tab_recency
            .iter()
            .rev()
            .copied()
            .find(|id| group.tab_ids.contains(id))
            .or_else(|| group.tab_ids.first().copied())
    }

    /// Keeps the explorer cursor aligned with `tab_id` when a tab activates while explorer is open (`EXP-05`).
    pub(super) fn sync_explorer_cursor_on_tab_activated(&self, tab_id: Option<TabId>) {
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return;
            }
            let rows = focus::explorer_targets(&state.model);
            if let Some(id) = tab_id
                && let Some(row) = focus::explorer_row_for_tab(&state.model, id)
            {
                let idx = rows.iter().position(|r| r == &row).unwrap_or(0);
                state.model.focus.cursor = Some(row);
                state.model.focus.cursor_index = idx;
            } else if !state.model.focus.on_footer() {
                clamp_explorer_cursor(&mut state, &rows);
            }
            state.model.focus.sync_selection(&rows);
            state.model.focus.clone()
        };
        self.publish_focus(updated);
    }

    /// Clamps the explorer cursor to its remembered row index when rows are closed, deleted, or collapsed (`EXP-05`).
    pub(super) fn sync_explorer_cursor_after_mutation(&self) {
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return;
            }
            let rows = focus::explorer_targets(&state.model);
            if !state.model.focus.on_footer() {
                clamp_explorer_cursor(&mut state, &rows);
            }
            state.model.focus.sync_selection(&rows);
            state.model.focus.clone()
        };
        self.publish_focus(updated);
    }

    /// Clears any active focus surface (called when vim mode is turned off).
    pub(super) fn clear_focus_on_vim_disabled(&self) {
        let (focus_to_publish, was_focus_mode) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let prev = state.model.focus.clone();
            let was_focus_mode = matches!(
                state.model.input_mode,
                InputMode::Explorer | InputMode::Toolbar | InputMode::Window
            );
            state.model.focus.clear();
            let changed = (prev != state.model.focus).then(|| state.model.focus.clone());
            (changed, was_focus_mode)
        };
        if let Some(focus) = focus_to_publish {
            self.bus.publish(Event::FocusChanged { focus });
        }
        if was_focus_mode {
            self.leave_chrome_mode();
        }
    }

    pub(super) fn publish_focus(&self, focus: FocusState) {
        self.bus.publish(Event::FocusChanged { focus });
    }

    pub(super) fn vim_enabled(&self) -> bool {
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
        // `explorer.toggle`: entering it again leaves it immediately (`EXP-12`).
        let already_open = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.focus.surface == Some(FocusSurface::Explorer) && !state.model.focus.transient_for_hints
        };
        if already_open {
            return self.handle_focus_close(FocusSurface::Explorer);
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let visible_rows = state.model.focus.explorer_visible_rows;
            let visible_range = state.model.focus.explorer_visible_range;
            let rows = focus::explorer_targets(&state.model);
            let cursor = state
                .model
                .active_tab
                .and_then(|tab_id| focus::explorer_target_for_tab(&state.model, tab_id))
                .or_else(|| rows.first().cloned())
                .or_else(|| focus::explorer_footer_targets(&state.model).into_iter().next());
            let cursor_index = cursor
                .as_ref()
                .and_then(|c| rows.iter().position(|r| r == c))
                .unwrap_or(0);
            state.model.focus = FocusState {
                surface: Some(FocusSurface::Explorer),
                cursor,
                cursor_index,
                explorer_visible_rows: visible_rows,
                explorer_visible_range: visible_range,
                ..FocusState::default()
            };
            state.model.focus.clone()
        };
        self.set_input_mode(InputMode::Explorer);
        self.publish_focus(focus);
        Ok(())
    }

    /// `q`, `<leader>e`, and clicking outside close Explorer immediately even when rows are selected (`EXP-12`).
    pub(super) fn handle_explorer_close(&self) -> CommandResult {
        self.handle_focus_close(FocusSurface::Explorer)
    }

    /// `<Esc>` in Explorer clears any `<Tab>` or `V` selection first, and closes Explorer when none is active (`EXP-12`).
    pub(super) fn handle_explorer_escape(&self) -> CommandResult {
        let cleared_selection = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface == Some(FocusSurface::Explorer) && state.model.focus.clear_selection() {
                Some(state.model.focus.clone())
            } else {
                None
            }
        };
        if let Some(focus) = cleared_selection {
            self.publish_focus(focus);
            return Ok(());
        }
        self.handle_focus_close(FocusSurface::Explorer)
    }

    pub(super) fn handle_focus_close(&self, surface: FocusSurface) -> CommandResult {
        let (focus_after, should_reset_mode) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface == Some(surface) || state.model.focus.surface.is_none() {
                state.model.focus.clear();
            }
            let reset = matches!(
                (surface, state.model.input_mode),
                (FocusSurface::Explorer, InputMode::Explorer)
                    | (FocusSurface::Toolbar, InputMode::Toolbar)
                    | (FocusSurface::Window, InputMode::Window)
            );
            (state.model.focus.clone(), reset)
        };
        self.publish_focus(focus_after);
        if should_reset_mode {
            self.leave_chrome_mode();
        }
        Ok(())
    }

    fn handle_explorer_move(&self, direction: FocusDirection, count: Option<u32>) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }

        // `l` (`FocusDirection::Into`) on a non-footer row behaves identically to `<CR>` (`EXP-02`).
        let activate_on_into = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.focus.surface == Some(FocusSurface::Explorer)
                && direction == FocusDirection::Into
                && !state.model.focus.on_footer()
        };
        if activate_on_into {
            return self.handle_explorer_activate(self.current_tab_source());
        }

        let step_n = count.unwrap_or(1).clamp(1, 10_000) as usize;
        let (focus, updated_folder) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return Ok(());
            }
            let rows = focus::explorer_targets(&state.model);
            let foot = focus::explorer_footer_targets(&state.model);
            let mut updated_folder = None;

            if state.model.focus.on_footer() {
                let cur_fi = state
                    .model
                    .focus
                    .cursor
                    .as_ref()
                    .and_then(|c| foot.iter().position(|f| f == c))
                    .unwrap_or(state.model.focus.footer_index)
                    .min(foot.len().saturating_sub(1));
                match direction {
                    FocusDirection::Previous => {
                        if !rows.is_empty() {
                            let idx =
                                (rows.len() as isize - step_n as isize).clamp(0, rows.len() as isize - 1) as usize;
                            state.model.focus.cursor = Some(rows[idx].clone());
                            state.model.focus.cursor_index = idx;
                        }
                    }
                    FocusDirection::Next => {}
                    FocusDirection::Out => {
                        if !foot.is_empty() {
                            let next_fi = cur_fi.saturating_sub(step_n);
                            state.model.focus.footer_index = next_fi;
                            state.model.focus.cursor = Some(foot[next_fi].clone());
                        }
                    }
                    FocusDirection::Into => {
                        if !foot.is_empty() {
                            let next_fi = (cur_fi + step_n).min(foot.len() - 1);
                            state.model.focus.footer_index = next_fi;
                            state.model.focus.cursor = Some(foot[next_fi].clone());
                        }
                    }
                    FocusDirection::First => {
                        if !rows.is_empty() {
                            let idx = (count.unwrap_or(1).saturating_sub(1) as usize).min(rows.len() - 1);
                            state.model.focus.cursor = Some(rows[idx].clone());
                            state.model.focus.cursor_index = idx;
                        }
                    }
                    FocusDirection::Last => {
                        if !rows.is_empty() {
                            let idx = count
                                .map(|c| (c.saturating_sub(1) as usize).min(rows.len() - 1))
                                .unwrap_or(rows.len() - 1);
                            state.model.focus.cursor = Some(rows[idx].clone());
                            state.model.focus.cursor_index = idx;
                        }
                    }
                }
            } else if rows.is_empty() {
                if !foot.is_empty() {
                    let fi = state.model.focus.footer_index.min(foot.len() - 1);
                    state.model.focus.cursor = Some(foot[fi].clone());
                }
            } else {
                let idx = state
                    .model
                    .focus
                    .cursor
                    .as_ref()
                    .and_then(|c| rows.iter().position(|r| r == c))
                    .unwrap_or(state.model.focus.cursor_index)
                    .min(rows.len() - 1);
                match direction {
                    FocusDirection::Next => {
                        if idx + step_n >= rows.len() && state.model.focus.visual_anchor.is_none() && !foot.is_empty() {
                            state.model.focus.cursor_index = rows.len() - 1;
                            let fi = state.model.focus.footer_index.min(foot.len() - 1);
                            state.model.focus.footer_index = fi;
                            state.model.focus.cursor = Some(foot[fi].clone());
                        } else {
                            let next_idx = (idx + step_n).min(rows.len() - 1);
                            state.model.focus.cursor = Some(rows[next_idx].clone());
                            state.model.focus.cursor_index = next_idx;
                        }
                    }
                    FocusDirection::Previous => {
                        let next_idx = idx.saturating_sub(step_n);
                        state.model.focus.cursor = Some(rows[next_idx].clone());
                        state.model.focus.cursor_index = next_idx;
                    }
                    FocusDirection::First => {
                        let next_idx = (count.unwrap_or(1).saturating_sub(1) as usize).min(rows.len() - 1);
                        state.model.focus.cursor = Some(rows[next_idx].clone());
                        state.model.focus.cursor_index = next_idx;
                    }
                    FocusDirection::Last => {
                        let next_idx = count
                            .map(|c| (c.saturating_sub(1) as usize).min(rows.len() - 1))
                            .unwrap_or(rows.len() - 1);
                        state.model.focus.cursor = Some(rows[next_idx].clone());
                        state.model.focus.cursor_index = next_idx;
                    }
                    FocusDirection::Out => {
                        // `h` collapses an expanded folder or collapses a child tab's parent folder and moves to it (`EXP-03`, `EXP-16`).
                        let target_folder = match &state.model.focus.cursor {
                            Some(FocusTarget::Folder(fid)) => {
                                if state.model.folder(*fid).is_some_and(|f| !f.collapsed) {
                                    Some(*fid)
                                } else {
                                    state.model.folder_parents.get(fid).copied()
                                }
                            }
                            Some(FocusTarget::Tab(tid)) | Some(FocusTarget::PinnedTab(tid)) => {
                                state.model.tab(*tid).and_then(|t| t.folder_id)
                            }
                            _ => None,
                        };
                        if let Some(folder_id) = target_folder {
                            if let Some(folder) = state.model.folders.iter_mut().find(|f| f.id == folder_id)
                                && !folder.collapsed
                            {
                                folder.collapsed = true;
                                updated_folder = Some(folder.clone());
                            }
                            state.model.focus.cursor = Some(FocusTarget::Folder(folder_id));
                        }
                    }
                    FocusDirection::Into => {}
                }
            }

            state.model.focus.hints = None;
            let next_rows = focus::explorer_targets(&state.model);
            if let Some(ref cur) = state.model.focus.cursor
                && let Some(pos) = next_rows.iter().position(|r| r == cur)
            {
                state.model.focus.cursor_index = pos;
            }
            state.model.focus.sync_selection(&next_rows);
            (state.model.focus.clone(), updated_folder)
        };

        if let Some(folder) = updated_folder {
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
            if state.model.focus.surface != Some(FocusSurface::Explorer) || state.model.focus.on_footer() {
                return Ok(());
            }
            let Some(cursor) = state.model.focus.cursor.clone() else {
                return Ok(());
            };
            let rows = focus::explorer_targets(&state.model);
            if rows.is_empty() {
                return Ok(());
            }
            if range {
                if state.model.focus.visual_anchor.is_some() {
                    state.model.focus.visual_anchor = None;
                } else {
                    state.model.focus.visual_anchor = Some(cursor);
                }
            } else {
                if let Some(pos) = state.model.focus.picked.iter().position(|item| item == &cursor) {
                    state.model.focus.picked.remove(pos);
                } else {
                    state.model.focus.picked.push(cursor.clone());
                }
                if let Some(idx) = rows.iter().position(|r| r == &cursor) {
                    let next_idx = (idx + 1).min(rows.len() - 1);
                    state.model.focus.cursor = Some(rows[next_idx].clone());
                    state.model.focus.cursor_index = next_idx;
                }
            }
            state.model.focus.sync_selection(&rows);
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
        self.activate_explorer_target(target, true, tab_source)
    }

    pub(super) fn activate_explorer_target(
        &self,
        target: FocusTarget,
        close_on_tab: bool,
        tab_source: Option<Arc<dyn TabSource>>,
    ) -> CommandResult {
        match target {
            FocusTarget::Essential(tab_id) | FocusTarget::PinnedTab(tab_id) | FocusTarget::Tab(tab_id) => {
                if close_on_tab {
                    self.handle_focus_close(FocusSurface::Explorer)?;
                }
                if let Some(source) = tab_source {
                    source.activate_tab(tab_id)?;
                } else {
                    self.feed_tab_activated(Some(tab_id));
                }
                Ok(())
            }
            FocusTarget::SplitGroup(group_id) => {
                let target_tab = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    Self::last_used_tab_in_split_group(&state, group_id)
                };
                if close_on_tab {
                    self.handle_focus_close(FocusSurface::Explorer)?;
                }
                if let Some(tab_id) = target_tab {
                    if let Some(source) = tab_source {
                        source.activate_tab(tab_id)?;
                    } else {
                        self.feed_tab_activated(Some(tab_id));
                    }
                }
                Ok(())
            }
            FocusTarget::Folder(folder_id) => {
                self.handle_folder_toggle_collapsed(folder_id)?;
                let focus = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state.model.focus.hints = None;
                    let rows = focus::explorer_targets(&state.model);
                    state.model.focus.sync_selection(&rows);
                    state.model.focus.clone()
                };
                self.publish_focus(focus);
                Ok(())
            }
            FocusTarget::SidebarFooter(item_id) => {
                if item_id.as_str() == "builtin.new_tab" {
                    if close_on_tab {
                        self.handle_focus_close(FocusSurface::Explorer)?;
                    }
                    self.request_view(ViewRequest::CreateNewMenu);
                    return Ok(());
                }
                let command_to_run = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state
                        .model
                        .contributions
                        .toolbar_items
                        .iter()
                        .find(|item| item.id == item_id)
                        .and_then(|item| Command::from_name_and_args(&item.command, &item.command_args).ok())
                };
                if close_on_tab {
                    self.handle_focus_close(FocusSurface::Explorer)?;
                }
                if let Some(command) = command_to_run {
                    let _ = crate::BrowserCore::invoke(self, command);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn visible_explorer_hint_targets(state: &StubState) -> Vec<FocusTarget> {
        let all = focus::explorer_targets(&state.model);
        match state.model.focus.explorer_visible_range {
            Some((start, end)) if start < all.len() && end > start => all[start..end.min(all.len())].to_vec(),
            _ => all,
        }
    }

    /// `s` inside Explorer: labels visible sidebar rows and moves cursor on match without closing (`EXP-06`).
    fn handle_explorer_pick(&self) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let visible_rows = state.model.focus.explorer_visible_rows;
            let visible_range = state.model.focus.explorer_visible_range;
            let all = focus::explorer_targets(&state.model);
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                let cursor = state
                    .model
                    .active_tab
                    .and_then(|tab_id| focus::explorer_target_for_tab(&state.model, tab_id))
                    .or_else(|| all.first().cloned());
                let cursor_index = cursor
                    .as_ref()
                    .and_then(|c| all.iter().position(|r| r == c))
                    .unwrap_or(0);
                state.model.focus = FocusState {
                    surface: Some(FocusSurface::Explorer),
                    cursor,
                    cursor_index,
                    explorer_visible_rows: visible_rows,
                    explorer_visible_range: visible_range,
                    ..FocusState::default()
                };
            }
            let targets = Self::visible_explorer_hint_targets(&state);
            state.model.focus.hints = if targets.is_empty() {
                None
            } else {
                Some(HintSession::new(FocusSurface::Explorer, targets).with_action(HintMatchAction::MoveCursor))
            };
            state.model.focus.clone()
        };
        self.set_input_mode(InputMode::Explorer);
        self.publish_focus(focus);
        Ok(())
    }

    /// `<leader>bj` (`tab.pick`): transiently reveals the sidebar (if closed), labels visible rows, and activates the
    /// picked tab before closing (`NAV-04`).
    pub(super) fn handle_tab_pick_hints(&self) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let already_explorer = state.model.focus.surface == Some(FocusSurface::Explorer);
            if !already_explorer {
                let visible_rows = state.model.focus.explorer_visible_rows;
                let visible_range = state.model.focus.explorer_visible_range;
                let all = focus::explorer_targets(&state.model);
                let cursor = state
                    .model
                    .active_tab
                    .and_then(|tab_id| focus::explorer_target_for_tab(&state.model, tab_id))
                    .or_else(|| all.first().cloned());
                let cursor_index = cursor
                    .as_ref()
                    .and_then(|c| all.iter().position(|r| r == c))
                    .unwrap_or(0);
                state.model.focus = FocusState {
                    surface: Some(FocusSurface::Explorer),
                    cursor,
                    cursor_index,
                    transient_for_hints: true,
                    explorer_visible_rows: visible_rows,
                    explorer_visible_range: visible_range,
                    ..FocusState::default()
                };
            }
            let targets = Self::visible_explorer_hint_targets(&state);
            if targets.is_empty() {
                if state.model.focus.transient_for_hints {
                    state.model.focus.clear();
                }
                return Ok(());
            }
            state.model.focus.hints = Some(
                HintSession::new(FocusSurface::Explorer, targets).with_action(HintMatchAction::ActivateTabAndClose),
            );
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

        let (outcome, action, surface_before, was_transient, carry_retargeted, focus_after) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let surface_before = state.model.focus.surface;
            let was_transient = state.model.focus.transient_for_hints;
            let Some(hints) = state.model.focus.hints.as_mut() else {
                return Ok(());
            };
            let action = hints.action;
            let outcome = hints.type_char(ch);
            let mut carry_retargeted = false;
            match &outcome {
                HintOutcome::Matched(target) => {
                    state.model.focus.hints = None;
                    if surface_before == Some(FocusSurface::Window)
                        && let FocusTarget::Pane(tab_id) = target
                        && let Some(carry) = state.model.focus.window_carry.as_mut()
                    {
                        // While carrying, `f` moves the landing target onto the picked pane (`ZendiaWindowMode.sys.mjs:417-447`).
                        let idx = if carry.adding {
                            let cur_zone = carry
                                .selected_target()
                                .and_then(|t| match t.kind {
                                    LandingKind::PaneSide { zone, .. } => Some(zone),
                                    _ => None,
                                })
                                .unwrap_or(crate::split::DropZone::Right);
                            carry.targets.iter().position(|t| {
                                matches!(
                                    t.kind,
                                    LandingKind::PaneSide { target_tab, zone } if target_tab == *tab_id && zone == cur_zone
                                )
                            })
                        } else {
                            carry.targets.iter().position(|t| {
                                matches!(
                                    t.kind,
                                    LandingKind::PaneSwap { target_tab } if target_tab == *tab_id
                                )
                            })
                        };
                        if let Some(i) = idx {
                            carry.selected = i;
                        }
                        carry_retargeted = true;
                    } else {
                        state.model.focus.cursor = Some(target.clone());
                    }
                    if state.model.focus.surface == Some(FocusSurface::Explorer) {
                        let rows = focus::explorer_targets(&state.model);
                        if let Some(idx) = rows.iter().position(|r| r == target) {
                            state.model.focus.cursor_index = idx;
                        }
                        state.model.focus.sync_selection(&rows);
                    }
                }
                HintOutcome::NoMatch => {
                    state.model.focus.hints = None;
                }
                HintOutcome::Narrowed => {}
            }
            (
                outcome,
                action,
                surface_before,
                was_transient,
                carry_retargeted,
                state.model.focus.clone(),
            )
        };

        self.publish_focus(focus_after);
        match outcome {
            HintOutcome::Matched(target) => match action {
                HintMatchAction::MoveCursor => Ok(()),
                HintMatchAction::ActivateTabAndClose => {
                    let is_folder = matches!(target, FocusTarget::Folder(_));
                    self.activate_explorer_target(target, true, tab_source)?;
                    if is_folder && was_transient {
                        self.handle_focus_close(FocusSurface::Explorer)?;
                    }
                    Ok(())
                }
                HintMatchAction::ActivatePane => {
                    if carry_retargeted {
                        return Ok(());
                    }
                    if let FocusTarget::Pane(tab_id) = target {
                        if let Some(source) = tab_source {
                            source.activate_tab(tab_id)?;
                        } else {
                            self.feed_tab_activated(Some(tab_id));
                        }
                        if was_transient {
                            self.handle_focus_close(FocusSurface::Window)?;
                            self.flash_pane_chip(tab_id);
                        }
                    }
                    Ok(())
                }
            },
            HintOutcome::NoMatch if was_transient => {
                self.handle_focus_close(surface_before.unwrap_or(FocusSurface::Explorer))
            }
            _ => Ok(()),
        }
    }

    fn handle_focus_hint_cancel(&self) -> CommandResult {
        let (was_transient, surface_before, focus) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let was_transient = state.model.focus.transient_for_hints;
            let surface_before = state.model.focus.surface;
            state.model.focus.hints = None;
            (was_transient, surface_before, state.model.focus.clone())
        };
        if was_transient {
            return self.handle_focus_close(surface_before.unwrap_or(FocusSurface::Explorer));
        }
        self.publish_focus(focus);
        Ok(())
    }

    fn handle_toolbar_focus_open(&self) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        // `<leader>m` toggles Toolbar mode closed if already open (`TB-01`).
        let already_toolbar = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.focus.surface == Some(FocusSurface::Toolbar)
        };
        if already_toolbar {
            return self.handle_focus_close(FocusSurface::Toolbar);
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let from_sidebar = state.model.focus.surface == Some(FocusSurface::Explorer);
            let visible_rows = state.model.focus.explorer_visible_rows;
            let visible_range = state.model.focus.explorer_visible_range;
            state.model.focus = FocusState {
                surface: Some(FocusSurface::Toolbar),
                toolbar_from_sidebar: from_sidebar,
                explorer_visible_rows: visible_rows,
                explorer_visible_range: visible_range,
                ..FocusState::default()
            };
            let targets = focus::toolbar_targets(&state.model);
            // Cursor starts on the app menu button (`PanelUI-menu-button`) when present (`TB-01`).
            let cursor = targets
                .iter()
                .find(|target| {
                    matches!(
                        target,
                        FocusTarget::ToolbarItem(id) if id.as_str() == "builtin.app_menu"
                    )
                })
                .cloned()
                .or_else(|| targets.into_iter().next());
            state.model.focus.cursor = cursor;
            state.model.focus.clone()
        };
        self.set_input_mode(InputMode::Toolbar);
        self.publish_focus(focus);
        Ok(())
    }

    fn handle_toolbar_focus_move(&self, direction: FocusDirection, count: Option<u32>) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let step_n = count.unwrap_or(1).clamp(1, 1000) as usize;
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Toolbar) {
                return Ok(());
            }
            let targets = focus::toolbar_targets(&state.model);
            if !targets.is_empty() {
                let idx = state
                    .model
                    .focus
                    .cursor
                    .as_ref()
                    .and_then(|c| targets.iter().position(|t| t == c))
                    .unwrap_or(0);
                let next_idx = match direction {
                    FocusDirection::Next | FocusDirection::Into => (idx + step_n).min(targets.len() - 1),
                    FocusDirection::Previous | FocusDirection::Out => idx.saturating_sub(step_n),
                    FocusDirection::First => 0,
                    FocusDirection::Last => targets.len() - 1,
                };
                state.model.focus.cursor = Some(targets[next_idx].clone());
            }
            state.model.focus.hints = None;
            state.model.focus.clone()
        };
        self.publish_focus(focus);
        Ok(())
    }

    fn handle_toolbar_focus_activate(&self, _tab_source: Option<Arc<dyn TabSource>>) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let (command_to_run, is_ext_button) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Toolbar) {
                return Ok(());
            }
            let Some(FocusTarget::ToolbarItem(item_id)) = &state.model.focus.cursor else {
                return self.handle_focus_close(FocusSurface::Toolbar);
            };
            match item_id.as_str() {
                "builtin.url_bar" => (Some(Command::LocationFocus), false),
                "builtin.window_minimize" => (Some(Command::Tabs(TabAction::WindowMinimize)), false),
                "builtin.window_maximize" => (Some(Command::Tabs(TabAction::WindowToggleMaximize)), false),
                "builtin.window_close" => (Some(Command::Tabs(TabAction::WindowClose)), false),
                _ => {
                    let found = state
                        .model
                        .contributions
                        .toolbar_items
                        .iter()
                        .find(|item| &item.id == item_id);
                    let is_ext = found.is_some_and(|item| {
                        matches!(item.source, crate::contributions::ContributionSource::Extension { .. })
                            || item.command == "extension.invoke_action"
                    });
                    let cmd =
                        found.and_then(|item| Command::from_name_and_args(&item.command, &item.command_args).ok());
                    (cmd, is_ext)
                }
            }
        };

        self.handle_focus_close(FocusSurface::Toolbar)?;
        if is_ext_button {
            self.mark_extension_popup_from_keyboard();
        }
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
            let visible_rows = state.model.focus.explorer_visible_rows;
            let visible_range = state.model.focus.explorer_visible_range;
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
                explorer_visible_rows: visible_rows,
                explorer_visible_range: visible_range,
                ..FocusState::default()
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
        self.start_window_carry(None)
    }

    fn handle_window_mode_drop(&self) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        self.drop_window_carry()
    }
}

fn clamp_explorer_cursor(state: &mut StubState, rows: &[FocusTarget]) {
    if let Some(ref cur) = state.model.focus.cursor
        && let Some(pos) = rows.iter().position(|r| r == cur)
    {
        state.model.focus.cursor_index = pos;
        return;
    }
    if rows.is_empty() {
        let foot = focus::explorer_footer_targets(&state.model);
        let fi = state.model.focus.footer_index.min(foot.len().saturating_sub(1));
        state.model.focus.cursor = foot.get(fi).cloned();
        state.model.focus.cursor_index = 0;
    } else {
        let idx = state.model.focus.cursor_index.min(rows.len() - 1);
        state.model.focus.cursor = Some(rows[idx].clone());
        state.model.focus.cursor_index = idx;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrowserCore;
    use crate::chrome_actions::ChromeAction;
    use crate::events::ViewRequest;

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
    fn explorer_toggle_leaves_the_explorer_when_it_is_open() {
        let core = stub_with_vim();
        core.invoke(Command::ExplorerOpen).unwrap();
        assert_eq!(core.model().focus.surface, Some(FocusSurface::Explorer));
        core.invoke(Command::ExplorerOpen).unwrap();
        assert_eq!(core.model().focus.surface, None);
        assert_ne!(core.model().input_mode, InputMode::Explorer);
    }

    #[test]
    fn explorer_open_move_select_escape_quit_and_activate() {
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
            count: None,
        })
        .unwrap();
        core.invoke(Command::ExplorerSelect { range: false }).unwrap();
        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }
        assert_eq!(view_model.focus.selection.len(), 1);

        // `<Esc>` (`explorer.escape`) clears selection first while staying in Explorer (`EXP-12`).
        core.invoke(Command::Chrome(ChromeAction::ExplorerEscape)).unwrap();
        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }
        assert_eq!(view_model.input_mode, InputMode::Explorer);
        assert!(view_model.focus.selection.is_empty());

        // `q` / `<leader>e` (`ExplorerClose`) closes Explorer immediately even when rows are selected (`EXP-12`).
        core.invoke(Command::ExplorerSelect { range: false }).unwrap();
        assert_eq!(core.model().focus.selection.len(), 1);
        core.invoke(Command::ExplorerClose).unwrap();
        assert_eq!(core.model().focus.surface, None);
        assert_eq!(core.model().input_mode, InputMode::Normal);

        // Reopen and activate a tab with `<CR>`.
        core.invoke(Command::ExplorerOpen).unwrap();
        core.invoke(Command::ExplorerMove {
            direction: FocusDirection::First,
            count: None,
        })
        .unwrap();
        let cursor_before_activate = core.model().focus.cursor.clone();
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
    fn explorer_hints_move_cursor_without_closing_while_tab_pick_activates_and_closes() {
        let core = stub_with_vim();
        core.invoke(Command::ExplorerOpen).unwrap();
        let active_before = core.model().active_tab;
        core.invoke(Command::ExplorerPick).unwrap();
        let hints = core.model().focus.hints.expect("hints active");
        assert!(!hints.entries.is_empty());
        let first_label = hints.entries[0].label.clone();
        let first_target = hints.entries[0].target.clone();

        // `s` in Explorer moves the cursor without activating or closing (`EXP-06`).
        core.invoke(Command::FocusHintInput {
            key: first_label.clone(),
        })
        .unwrap();
        assert_eq!(core.model().focus.hints, None);
        assert_eq!(core.model().focus.surface, Some(FocusSurface::Explorer));
        assert_eq!(core.model().focus.cursor, Some(first_target.clone()));
        assert_eq!(core.model().active_tab, active_before);
        core.invoke(Command::ExplorerClose).unwrap();

        // `<leader>bj` (`tab.pick`) transiently opens sidebar hints, activates the matched tab, and closes (`NAV-04`).
        core.invoke(Command::Tabs(TabAction::TabPick)).unwrap();
        assert_eq!(core.model().focus.surface, Some(FocusSurface::Explorer));
        let pick_hints = core.model().focus.hints.expect("tab.pick hints active");
        let pick_label = pick_hints.entries[0].label.clone();
        let pick_target = pick_hints.entries[0].target.clone();
        core.invoke(Command::FocusHintInput { key: pick_label }).unwrap();
        assert_eq!(core.model().focus.surface, None);
        if let FocusTarget::Essential(tab_id) = pick_target {
            assert_eq!(core.model().active_tab, Some(tab_id));
        }
    }

    #[test]
    fn toolbar_focus_starts_on_app_menu_toggles_and_activates_item() {
        let core = stub_with_vim();

        // With vim compact defaults active (`Single` + `compact_mode`), `<leader>m` scopes to the compact top bar (`ZendiaToolbar.sys.mjs:130`).
        core.invoke(Command::ToolbarFocusOpen).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Toolbar);
        assert_eq!(
            core.model().focus.cursor,
            Some(FocusTarget::ToolbarItem(crate::contributions::ContributionId::new(
                "zendia.agents"
            )))
        );
        core.invoke(Command::ToolbarFocusClose).unwrap();

        // With non-compact toolbar visible, `<leader>m` starts on `builtin.app_menu` (`TB-01`).
        core.invoke(Command::LayoutSetCompactMode { enabled: false }).unwrap();
        let sub = core.subscribe();

        core.invoke(Command::ToolbarFocusOpen).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Toolbar);
        assert_eq!(
            core.model().focus.cursor,
            Some(FocusTarget::ToolbarItem(crate::contributions::ContributionId::new(
                "builtin.app_menu"
            )))
        );

        // Pressing `<leader>m` (`ToolbarFocusOpen`) again toggles Toolbar mode closed (`TB-01`).
        core.invoke(Command::ToolbarFocusOpen).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
        assert_eq!(core.model().focus.surface, None);

        core.invoke(Command::ToolbarFocusOpen).unwrap();
        core.invoke(Command::ToolbarFocusActivate).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
        let mut saw_app_menu = false;
        while let Ok(event) = sub.try_recv() {
            if matches!(
                event,
                Event::ViewRequested {
                    request: ViewRequest::AppMenuToggle,
                    ..
                }
            ) {
                saw_app_menu = true;
            }
        }
        assert!(saw_app_menu);
    }

    #[test]
    fn window_mode_focuses_carries_steps_and_drops_panes() {
        let core = stub_with_vim();
        core.invoke(Command::SplitTabs {
            tab_ids: vec![TabId(10), TabId(11)],
            direction: SplitDirection::Horizontal,
        })
        .unwrap();
        core.invoke(Command::TabActivate { tab_id: TabId(10) }).unwrap();

        core.invoke(Command::WindowModeOpen).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Window);
        assert_eq!(core.model().focus.cursor, Some(FocusTarget::Pane(TabId(10))));

        core.invoke(Command::WindowModeCarryToggle).unwrap();
        assert!(core.model().focus.window_carry.is_some());

        core.invoke(Command::WindowModeDrop).unwrap();
        assert!(core.model().focus.window_carry.is_none());
        assert_eq!(
            core.model().active_split_group().map(|g| g.tab_ids.clone()),
            Some(vec![TabId(11), TabId(10)])
        );

        core.invoke(Command::WindowModeFocusPane { pane: 1 }).unwrap();
        assert_eq!(core.model().active_tab, Some(TabId(11)));

        core.invoke(Command::WindowModeClose).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
        assert_eq!(core.model().focus.surface, None);
    }
}
