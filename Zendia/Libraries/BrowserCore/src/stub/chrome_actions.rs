/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for [`crate::chrome_actions`].

use super::StubBackend;
use crate::BrowserCore;
use crate::address_bar::AddressBarFilter;
use crate::chrome_actions::{ChromeAction, HALF_PAGE_STEP};
use crate::commands::{Command, CommandResult};
use crate::events::{Event, ViewRequest};
use crate::focus::{self, FocusSurface, FocusTarget, HintMatchAction, HintSession};
use crate::model::{FolderId, InputMode, SplitDirection, TabId, TabKind, TabMoveTarget};
use crate::picker::{PickerItem, PickerLayout, PickerRequest};

impl StubBackend {
    pub(super) fn handle_chrome_actions(&self, action: ChromeAction) -> CommandResult {
        match action {
            ChromeAction::ExplorerHalfPageDown { count } => self.handle_explorer_half_page(true, count),
            ChromeAction::ExplorerHalfPageUp { count } => self.handle_explorer_half_page(false, count),
            ChromeAction::ExplorerPreview => self.handle_explorer_preview(),
            ChromeAction::ExplorerFoldToggle => self.handle_explorer_fold(None),
            ChromeAction::ExplorerFoldOpen => self.handle_explorer_fold(Some(false)),
            ChromeAction::ExplorerFoldClose => self.handle_explorer_fold(Some(true)),
            ChromeAction::ExplorerFind => {
                self.handle_explorer_close()?;
                self.request_view(ViewRequest::AddressBarOpen {
                    new_tab: false,
                    initial_query: None,
                    filter: AddressBarFilter::Tabs,
                });
                Ok(())
            }
            ChromeAction::ExplorerEscape => self.handle_explorer_escape(),
            ChromeAction::ExplorerAdd => {
                self.handle_explorer_close()?;
                self.request_view(ViewRequest::AddressBarOpen {
                    new_tab: true,
                    initial_query: None,
                    filter: AddressBarFilter::All,
                });
                Ok(())
            }
            ChromeAction::ExplorerAddFolder => self.handle_explorer_add_folder(),
            ChromeAction::ExplorerDelete => self.handle_explorer_delete(),
            ChromeAction::ExplorerRename => self.handle_explorer_rename(),
            ChromeAction::ExplorerDuplicate => self.handle_explorer_duplicate(),
            ChromeAction::ExplorerTogglePin => self.handle_explorer_toggle_pin(),
            ChromeAction::ExplorerToggleEssential => self.handle_explorer_toggle_essential(),
            ChromeAction::ExplorerReload => self.handle_explorer_reload(),
            ChromeAction::ExplorerUnload => self.handle_explorer_unload(),
            ChromeAction::ExplorerToggleMute => self.handle_explorer_toggle_mute(),
            ChromeAction::ExplorerMoveToFolderPick => self.handle_move_to_folder_pick(true),
            ChromeAction::ExplorerMoveToFolderApply {
                folder_id,
                create_new,
                tab_ids,
                rename_new,
            } => self.handle_move_to_folder_apply(folder_id, create_new, tab_ids, rename_new),
            ChromeAction::ExplorerYank { markdown } => self.handle_explorer_yank(markdown),
            ChromeAction::ExplorerMoveOutOfFolder => self.handle_explorer_move_out_of_folder(),
            ChromeAction::ExplorerMoveDown => self.handle_explorer_reorder(1),
            ChromeAction::ExplorerMoveUp => self.handle_explorer_reorder(-1),
            ChromeAction::ExplorerSplitVertical => self.handle_explorer_split(SplitDirection::Horizontal),
            ChromeAction::ExplorerSplitHorizontal => self.handle_explorer_split(SplitDirection::Vertical),
            ChromeAction::ExplorerMenu => self.handle_explorer_menu(),
            ChromeAction::ExplorerCreate => self.handle_explorer_create(),
            ChromeAction::ToolbarJump => self.handle_toolbar_jump(),
            ChromeAction::ToolbarMenu => self.handle_toolbar_menu(),
            ChromeAction::ToolbarUrlbar => {
                self.handle_focus_close(FocusSurface::Toolbar)?;
                self.invoke(Command::LocationFocus)
            }
            ChromeAction::LibraryQuit => {
                self.handle_library_vim_action(crate::chrome_actions::LibraryVimAction::Quit, None)
            }
            ChromeAction::LibraryGoto(section) => {
                self.handle_library_vim_action(crate::chrome_actions::LibraryVimAction::GotoSection(section), None)
            }
            ChromeAction::Library { action, count } => self.handle_library_vim_action(action, count),
        }
    }

    /// Forwards library-mode vim actions to the open library view (`ZendiaLibrary.sys.mjs` `inLibrary` / `inSpaces`).
    ///
    /// Library space actions (`r`, `t`, `I`, `<A-h>`, `<A-l>`, `a`) act only on the Spaces/Routing section's cursor
    /// inside the open library, never on `active_space` when the library is closed (`LIB-01`).
    fn handle_library_vim_action(
        &self,
        action: crate::chrome_actions::LibraryVimAction,
        count: Option<u32>,
    ) -> CommandResult {
        if self.model().input_mode != InputMode::Library {
            return Ok(());
        }
        self.request_view(ViewRequest::LibraryVimAction {
            action,
            count: count.unwrap_or(1).clamp(1, crate::vim_keys::MAX_REPEAT),
        });
        Ok(())
    }

    /// Resolves multi-tab explorer targets (`selection` if non-empty, else `[cursor]`, expanding split groups) and
    /// clears any active `<Tab>` / `V` selection (`EXP-01`, `EXP-14`, `EXP-17`, `EXP-19`).
    fn explorer_target_tabs_and_clear(&self) -> (Vec<TabId>, Option<FolderId>) {
        let (tabs, folder_without_selection, cleared_focus) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return (Vec::new(), None);
            }
            let had_selection = !state.model.focus.selection.is_empty();
            let targets: Vec<FocusTarget> = if had_selection {
                state.model.focus.selection.clone()
            } else {
                state.model.focus.cursor.clone().into_iter().collect()
            };
            let folder_only = if !had_selection {
                match &state.model.focus.cursor {
                    Some(FocusTarget::Folder(fid)) => Some(*fid),
                    _ => None,
                }
            } else {
                None
            };
            let mut tabs = Vec::new();
            for target in targets {
                match target {
                    FocusTarget::Essential(id) | FocusTarget::PinnedTab(id) | FocusTarget::Tab(id) => {
                        if !tabs.contains(&id) {
                            tabs.push(id);
                        }
                    }
                    FocusTarget::SplitGroup(group_id) => {
                        if let Some(group) = state.model.split_group(group_id) {
                            for &id in &group.tab_ids {
                                if !tabs.contains(&id) {
                                    tabs.push(id);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            let cleared = state.model.focus.clear_selection().then(|| state.model.focus.clone());
            (tabs, folder_only, cleared)
        };
        if let Some(focus) = cleared_focus {
            self.publish_focus(focus);
        }
        (tabs, folder_without_selection)
    }

    /// Resolves the single tab under the explorer cursor (picking the last-used pane on a split group row, `EXP-19`).
    fn explorer_single_cursor_tab(&self) -> Option<TabId> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.model.focus.surface != Some(FocusSurface::Explorer) {
            return None;
        }
        match &state.model.focus.cursor {
            Some(FocusTarget::Essential(id) | FocusTarget::PinnedTab(id) | FocusTarget::Tab(id)) => Some(*id),
            Some(FocusTarget::SplitGroup(gid)) => Self::last_used_tab_in_split_group(&state, *gid),
            _ => None,
        }
    }

    fn handle_explorer_half_page(&self, down: bool, count: Option<u32>) -> CommandResult {
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return Ok(());
            }
            let rows = focus::explorer_targets(&state.model);
            if rows.is_empty() {
                return Ok(());
            }
            let half = state
                .model
                .focus
                .explorer_visible_rows
                .map(|vis| (vis / 2).max(1) as isize)
                .unwrap_or(HALF_PAGE_STEP);
            let mult = count.unwrap_or(1).clamp(1, 1000) as isize;
            let delta = if down { half * mult } else { -(half * mult) };
            let cur = if state.model.focus.on_footer() {
                if down {
                    return Ok(());
                }
                rows.len()
            } else {
                state
                    .model
                    .focus
                    .cursor
                    .as_ref()
                    .and_then(|c| rows.iter().position(|t| t == c))
                    .unwrap_or(state.model.focus.cursor_index)
            };
            let next = (cur as isize + delta).clamp(0, (rows.len() - 1) as isize) as usize;
            state.model.focus.cursor = Some(rows[next].clone());
            state.model.focus.cursor_index = next;
            state.model.focus.hints = None;
            state.model.focus.sync_selection(&rows);
            state.model.focus.clone()
        };
        self.bus.publish(Event::FocusChanged { focus });
        Ok(())
    }

    /// `o` (`explorer.preview`): activates tab, toggles folder, or presses footer button while keeping Explorer open (`EXP-09`).
    fn handle_explorer_preview(&self) -> CommandResult {
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
        self.activate_explorer_target(target, false, self.current_tab_source())
    }

    fn handle_explorer_fold(&self, collapsed: Option<bool>) -> CommandResult {
        let (updated_folder, updated_focus) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return Ok(());
            }
            let folder_id = match &state.model.focus.cursor {
                Some(FocusTarget::Folder(id)) => Some(*id),
                Some(FocusTarget::Tab(tab_id)) | Some(FocusTarget::PinnedTab(tab_id)) => {
                    state.model.tab(*tab_id).and_then(|t| t.folder_id)
                }
                _ => None,
            };
            let Some(folder_id) = folder_id else {
                return Ok(());
            };
            let Some(folder) = state.model.folders.iter_mut().find(|f| f.id == folder_id) else {
                return Ok(());
            };
            let next = collapsed.unwrap_or(!folder.collapsed);
            let folder_changed = if folder.collapsed != next {
                folder.collapsed = next;
                Some(folder.clone())
            } else {
                None
            };
            state.model.focus.cursor = Some(FocusTarget::Folder(folder_id));
            let rows = focus::explorer_targets(&state.model);
            if let Some(pos) = rows.iter().position(|r| *r == FocusTarget::Folder(folder_id)) {
                state.model.focus.cursor_index = pos;
            }
            state.model.focus.sync_selection(&rows);
            (folder_changed, state.model.focus.clone())
        };
        if let Some(folder) = updated_folder {
            self.bus.publish(Event::FolderUpdated { folder });
        }
        self.publish_focus(updated_focus);
        Ok(())
    }

    fn handle_explorer_add_folder(&self) -> CommandResult {
        self.handle_folder_create(None, None, None)?;
        let (new_folder_id, updated_focus) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(folder_id) = state.model.folders.last().map(|f| f.id) else {
                return Ok(());
            };
            if state.model.focus.surface == Some(FocusSurface::Explorer) {
                let _ = state.model.focus.clear_selection();
                state.model.focus.cursor = Some(FocusTarget::Folder(folder_id));
                let rows = focus::explorer_targets(&state.model);
                if let Some(pos) = rows.iter().position(|r| *r == FocusTarget::Folder(folder_id)) {
                    state.model.focus.cursor_index = pos;
                }
            }
            (folder_id, state.model.focus.clone())
        };
        self.publish_focus(updated_focus);
        self.request_view(ViewRequest::FolderRename {
            folder_id: new_folder_id,
        });
        Ok(())
    }

    fn handle_explorer_delete(&self) -> CommandResult {
        let (tabs, folder_only) = self.explorer_target_tabs_and_clear();
        if let Some(folder_id) = folder_only {
            return self.handle_folder_delete(folder_id);
        }
        if tabs.is_empty() {
            return Ok(());
        }
        let tab_source = self.current_tab_source();
        self.handle_tab_close_multiple(tabs, tab_source.as_ref())?;
        self.sync_explorer_cursor_after_mutation();
        Ok(())
    }

    fn handle_explorer_rename(&self) -> CommandResult {
        let target = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return Ok(());
            }
            let _ = state.model.focus.clear_selection();
            state.model.focus.cursor.clone()
        };
        self.publish_focus(self.model().focus);
        match target {
            Some(FocusTarget::Folder(folder_id)) => self.handle_folder_start_rename(folder_id),
            Some(FocusTarget::PinnedTab(tab_id) | FocusTarget::Tab(tab_id)) => self.handle_tab_start_rename(tab_id),
            Some(FocusTarget::SplitGroup(gid)) => {
                let tab_id = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    Self::last_used_tab_in_split_group(&state, gid)
                };
                if let Some(tab_id) = tab_id {
                    self.handle_tab_start_rename(tab_id)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn handle_explorer_duplicate(&self) -> CommandResult {
        let (tabs, _) = self.explorer_target_tabs_and_clear();
        for tab_id in tabs {
            let _ = self.handle_tab_duplicate(tab_id);
        }
        Ok(())
    }

    fn handle_explorer_toggle_pin(&self) -> CommandResult {
        let (tabs, _) = self.explorer_target_tabs_and_clear();
        for tab_id in tabs {
            let kind = self.model().tab(tab_id).map(|t| t.kind);
            match kind {
                Some(TabKind::Essential) => {
                    let _ = self.handle_tab_remove_from_essentials(tab_id);
                }
                Some(TabKind::Pinned) => {
                    let _ = self.handle_tab_unpin(tab_id);
                }
                Some(TabKind::Normal) => {
                    let _ = self.handle_tab_pin(tab_id);
                }
                None => {}
            }
        }
        self.sync_explorer_cursor_after_mutation();
        Ok(())
    }

    fn handle_explorer_toggle_essential(&self) -> CommandResult {
        let (tabs, _) = self.explorer_target_tabs_and_clear();
        if tabs.is_empty() {
            return Ok(());
        }
        let all_essential = {
            let model = self.model();
            tabs.iter()
                .filter_map(|id| model.tab(*id))
                .all(|tab| tab.kind == TabKind::Essential)
        };
        for tab_id in tabs {
            if all_essential {
                let _ = self.handle_tab_remove_from_essentials(tab_id);
            } else if self.model().tab(tab_id).is_some_and(|t| t.kind != TabKind::Essential) {
                let _ = self.handle_tab_add_to_essentials(tab_id);
            }
        }
        self.sync_explorer_cursor_after_mutation();
        Ok(())
    }

    fn handle_explorer_reload(&self) -> CommandResult {
        let (tabs, _) = self.explorer_target_tabs_and_clear();
        for tab_id in tabs {
            let _ = self.invoke(Command::NavigationReload {
                tab_id: Some(tab_id),
                bypass_cache: false,
            });
        }
        Ok(())
    }

    fn handle_explorer_unload(&self) -> CommandResult {
        let (tabs, _) = self.explorer_target_tabs_and_clear();
        for tab_id in tabs {
            let _ = self.handle_tab_unload(tab_id);
        }
        Ok(())
    }

    fn handle_explorer_toggle_mute(&self) -> CommandResult {
        let (tabs, _) = self.explorer_target_tabs_and_clear();
        for tab_id in tabs {
            let _ = self.handle_tab_toggle_mute(tab_id);
        }
        Ok(())
    }

    /// Opens the `"Move to Folder"` picker for explorer `>` (`from_explorer = true`) or `<leader>bf` (`false`) (`EXP-10`, `NAV-03`).
    pub(super) fn handle_move_to_folder_pick(&self, from_explorer: bool) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let tab_ids: Vec<TabId> = if from_explorer {
            let (tabs, _) = self.explorer_target_tabs_and_clear();
            tabs
        } else {
            self.effective_keyed_tab()
                .filter(|id| self.model().tab(*id).is_some())
                .into_iter()
                .collect()
        };
        if tab_ids.is_empty() {
            return Ok(());
        }
        let csv = tab_ids.iter().map(|id| id.0.to_string()).collect::<Vec<_>>().join(",");

        let items = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let mut items = Vec::new();
            let mut new_item = PickerItem::new("folder.new", "New Folder", "explorer.move_to_folder_apply")
                .key("n")
                .icon("folder");
            new_item
                .command_args
                .push(("create_new".to_string(), "true".to_string()));
            new_item.command_args.push(("tab_ids".to_string(), csv.clone()));
            new_item
                .command_args
                .push(("rename_new".to_string(), from_explorer.to_string()));
            items.push(new_item);

            if let Some(space_id) = state.model.active_space {
                for folder in state.model.folders_in_space(space_id) {
                    let all_in_folder = tab_ids
                        .iter()
                        .all(|id| state.model.tab(*id).and_then(|t| t.folder_id) == Some(folder.id));
                    if all_in_folder {
                        continue;
                    }
                    let mut item = PickerItem::new(
                        format!("folder.{}", folder.id.0),
                        folder.name.clone(),
                        "explorer.move_to_folder_apply",
                    )
                    .icon("folder");
                    item.command_args
                        .push(("folder_id".to_string(), folder.id.0.to_string()));
                    item.command_args.push(("tab_ids".to_string(), csv.clone()));
                    items.push(item);
                }
            }
            items
        };

        let request = PickerRequest::new("move-to-folder", "Move to Folder", PickerLayout::List, items);
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.active_picker = Some(request.clone());
        }
        self.request_view(ViewRequest::PickerOpen { request });
        Ok(())
    }

    fn handle_move_to_folder_apply(
        &self,
        folder_id: Option<FolderId>,
        create_new: bool,
        tab_ids: Vec<TabId>,
        rename_new: bool,
    ) -> CommandResult {
        if tab_ids.is_empty() {
            return Ok(());
        }
        let dest_folder = if create_new {
            self.handle_folder_create(None, None, None)?;
            self.model().folders.last().map(|f| f.id)
        } else {
            folder_id
        };
        let Some(dest_id) = dest_folder else {
            return Ok(());
        };
        for tab_id in tab_ids {
            let _ = self.handle_tab_move_to_folder(tab_id, Some(dest_id));
        }
        if create_new && rename_new {
            let focus = {
                let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                if state.model.focus.surface == Some(FocusSurface::Explorer) {
                    state.model.focus.cursor = Some(FocusTarget::Folder(dest_id));
                    let rows = focus::explorer_targets(&state.model);
                    if let Some(pos) = rows.iter().position(|r| *r == FocusTarget::Folder(dest_id)) {
                        state.model.focus.cursor_index = pos;
                    }
                }
                state.model.focus.clone()
            };
            self.publish_focus(focus);
            self.request_view(ViewRequest::FolderRename { folder_id: dest_id });
        } else {
            self.sync_explorer_cursor_after_mutation();
        }
        Ok(())
    }

    /// `y` / `Y`: copies URLs or Markdown links silently (no toast!) and clears selection (`EXP-17`).
    fn handle_explorer_yank(&self, markdown: bool) -> CommandResult {
        let (tabs, _) = self.explorer_target_tabs_and_clear();
        if tabs.is_empty() {
            return Ok(());
        }
        let lines: Vec<String> = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            tabs.iter()
                .filter_map(|id| state.model.tab(*id))
                .map(|tab| {
                    if markdown {
                        format!("[{}]({})", tab.display_title(), tab.url)
                    } else {
                        tab.url.clone()
                    }
                })
                .collect()
        };
        if !lines.is_empty() {
            let joined = lines.join("\n");
            self.record_yank(&joined);
            self.bus.publish(Event::ClipboardCopyRequested { text: joined });
        }
        Ok(())
    }

    fn handle_explorer_move_out_of_folder(&self) -> CommandResult {
        let (tabs, _) = self.explorer_target_tabs_and_clear();
        for tab_id in tabs {
            let _ = self.invoke(Command::TabMoveToFolder {
                tab_id: Some(tab_id),
                folder_id: None,
            });
        }
        self.sync_explorer_cursor_after_mutation();
        Ok(())
    }

    /// `<A-j>` / `<A-k>`: reorders the cursor row within its section without switching `active_tab` (`EXP-18`).
    fn handle_explorer_reorder(&self, delta: isize) -> CommandResult {
        enum ReorderPlan {
            Tab(TabId, TabMoveTarget),
            Folder(FolderId, usize, Option<FolderId>),
            SplitGroup(Vec<TabId>, usize),
        }

        let plan: Option<ReorderPlan> = (|| {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return None;
            }
            match state.model.focus.cursor.clone()? {
                FocusTarget::Essential(tab_id) => {
                    let essentials = state.model.essential_tabs();
                    let len = essentials.len();
                    if len < 2 {
                        return None;
                    }
                    let pos = essentials.iter().position(|t| t.id == tab_id)?;
                    let next = (pos as isize + delta).clamp(0, (len - 1) as isize) as usize;
                    (next != pos).then_some(ReorderPlan::Tab(tab_id, TabMoveTarget::Essential { index: next }))
                }
                FocusTarget::PinnedTab(tab_id) => {
                    let space_id = state.model.tab(tab_id)?.space_id.or(state.model.active_space)?;
                    let pinned = state.model.pinned_tabs_in_space(space_id);
                    let len = pinned.len();
                    if len < 2 {
                        return None;
                    }
                    let pos = pinned.iter().position(|t| t.id == tab_id)?;
                    let next = (pos as isize + delta).clamp(0, (len - 1) as isize) as usize;
                    (next != pos).then_some(ReorderPlan::Tab(
                        tab_id,
                        TabMoveTarget::Pinned {
                            space_id: Some(space_id),
                            index: next,
                        },
                    ))
                }
                FocusTarget::Folder(folder_id) => {
                    let folder = state.model.folder(folder_id)?;
                    let space_id = folder.space_id;
                    let parent = state.model.folder_parents.get(&folder_id).copied();
                    let space_folder_indices: Vec<(usize, FolderId)> = state
                        .model
                        .folders
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| {
                            f.space_id == space_id && state.model.folder_parents.get(&f.id).copied() == parent
                        })
                        .map(|(idx, f)| (idx, f.id))
                        .collect();
                    if space_folder_indices.len() < 2 {
                        return None;
                    }
                    let local_pos = space_folder_indices.iter().position(|(_, id)| *id == folder_id)?;
                    let next_local =
                        (local_pos as isize + delta).clamp(0, (space_folder_indices.len() - 1) as isize) as usize;
                    if next_local == local_pos {
                        return None;
                    }
                    let target_global = space_folder_indices[next_local].0;
                    Some(ReorderPlan::Folder(folder_id, target_global, parent))
                }
                FocusTarget::Tab(tab_id) => {
                    let tab = state.model.tab(tab_id)?;
                    if let Some(folder_id) = tab.folder_id {
                        let folder = state.model.folder(folder_id)?;
                        let len = folder.tab_ids.len();
                        if len < 2 {
                            return None;
                        }
                        let pos = folder.tab_ids.iter().position(|id| *id == tab_id)?;
                        let next = (pos as isize + delta).clamp(0, (len - 1) as isize) as usize;
                        (next != pos).then_some(ReorderPlan::Tab(
                            tab_id,
                            TabMoveTarget::Folder { folder_id, index: next },
                        ))
                    } else {
                        let space_id = tab.space_id.or(state.model.active_space)?;
                        let normal = state.model.normal_tabs_in_space(space_id);
                        let len = normal.len();
                        if len < 2 {
                            return None;
                        }
                        let pos = normal.iter().position(|t| t.id == tab_id)?;
                        let next = (pos as isize + delta).clamp(0, (len - 1) as isize) as usize;
                        (next != pos).then_some(ReorderPlan::Tab(
                            tab_id,
                            TabMoveTarget::Normal {
                                space_id: Some(space_id),
                                index: next,
                            },
                        ))
                    }
                }
                FocusTarget::SplitGroup(gid) => {
                    let group = state.model.split_group(gid)?;
                    let space_id = group.space_id;
                    let normal: Vec<TabId> = state
                        .model
                        .normal_tabs_in_space(space_id)
                        .into_iter()
                        .map(|t| t.id)
                        .collect();
                    let first_pos = normal.iter().position(|id| group.tab_ids.contains(id))?;
                    let remaining_len = normal.iter().filter(|id| !group.tab_ids.contains(id)).count();
                    if remaining_len == 0 {
                        return None;
                    }
                    let next = (first_pos as isize + delta).clamp(0, remaining_len as isize) as usize;
                    (next != first_pos).then_some(ReorderPlan::SplitGroup(group.tab_ids.clone(), next))
                }
                _ => None,
            }
        })();

        let Some(plan) = plan else {
            return Ok(());
        };
        match plan {
            ReorderPlan::Tab(tab_id, target) => {
                self.handle_tab_move(tab_id, target)?;
            }
            ReorderPlan::Folder(folder_id, target_index, parent_id) => {
                self.handle_folder_reorder(folder_id, target_index, parent_id)?;
            }
            ReorderPlan::SplitGroup(group_tabs, insert_at) => {
                let space_id = self.model().active_space;
                for (offset, tab_id) in group_tabs.into_iter().enumerate() {
                    self.handle_tab_move(
                        tab_id,
                        TabMoveTarget::Normal {
                            space_id,
                            index: insert_at + offset,
                        },
                    )?;
                }
            }
        }
        self.sync_explorer_cursor_after_mutation();
        Ok(())
    }

    /// `<C-v>` / `<C-s>` in Explorer: divides ONLY the active pane with the cursor's tab instead of replacing existing
    /// panes in the split (`WIN-12`).
    fn handle_explorer_split(&self, direction: SplitDirection) -> CommandResult {
        let target_tab = self.explorer_single_cursor_tab();
        let _ = self.explorer_target_tabs_and_clear();
        let (active_tab, is_full, is_empty) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let active = state.model.active_tab;
            let is_empty = active.is_some_and(|id| state.vim.empty_split_pane.is_some_and(|e| e.empty_tab == id));
            let is_full = active
                .and_then(|id| state.model.split_group_for_tab(id))
                .is_some_and(|g| g.tab_ids.len() >= crate::split::MAX_SPLIT_PANES);
            (active, is_full, is_empty)
        };
        let (Some(active), Some(target)) = (active_tab, target_tab) else {
            return Ok(());
        };
        if active == target || is_empty {
            return Ok(());
        }
        if is_full {
            self.show_toast(crate::toasts::messages::SPLIT_VIEW_LIMIT.to_string(), None, None);
            return Ok(());
        }
        self.handle_explorer_close()?;
        let zone = match direction {
            SplitDirection::Vertical => crate::split::DropZone::Bottom,
            SplitDirection::Horizontal | SplitDirection::Grid => crate::split::DropZone::Right,
        };
        self.divide_pane_with_tab(active, target, zone)
    }

    fn handle_explorer_menu(&self) -> CommandResult {
        let target = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return Ok(());
            }
            let _ = state.model.focus.clear_selection();
            state.model.focus.cursor.clone()
        };
        self.publish_focus(self.model().focus);
        match target {
            Some(FocusTarget::Folder(folder_id)) => {
                self.request_view(ViewRequest::FolderMenu { folder_id });
            }
            Some(FocusTarget::Essential(tab_id) | FocusTarget::PinnedTab(tab_id) | FocusTarget::Tab(tab_id)) => {
                self.request_view(ViewRequest::TabMenu { tab_id });
            }
            Some(FocusTarget::SplitGroup(gid)) => {
                let tab_id = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    Self::last_used_tab_in_split_group(&state, gid)
                };
                if let Some(tab_id) = tab_id {
                    self.request_view(ViewRequest::TabMenu { tab_id });
                }
            }
            Some(FocusTarget::SidebarFooter(item_id)) => {
                self.request_view(ViewRequest::ToolbarItemMenu { item_id });
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_explorer_create(&self) -> CommandResult {
        self.request_view(ViewRequest::CreateNewMenu);
        Ok(())
    }

    /// `s` in Toolbar mode: labels visible toolbar items and moves cursor on match without closing (`TB-02`).
    fn handle_toolbar_jump(&self) -> CommandResult {
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if !state.model.vim_mode_enabled {
                return Ok(());
            }
            state.model.focus.surface = Some(FocusSurface::Toolbar);
            let targets = focus::toolbar_targets(&state.model);
            state.model.focus.hints = if targets.is_empty() {
                None
            } else {
                Some(HintSession::new(FocusSurface::Toolbar, targets).with_action(HintMatchAction::MoveCursor))
            };
            state.model.focus.clone()
        };
        self.set_input_mode(InputMode::Toolbar);
        self.bus.publish(Event::FocusChanged { focus });
        Ok(())
    }

    /// `m` in Toolbar mode: opens the focused button's menu (history menu on Back/Forward, extension menu, or toolbar context menu, `TB-03`).
    fn handle_toolbar_menu(&self) -> CommandResult {
        let item_id = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            match &state.model.focus.cursor {
                Some(FocusTarget::ToolbarItem(id)) => Some(id.clone()),
                _ => None,
            }
        };
        if let Some(item_id) = item_id {
            self.request_view(ViewRequest::ToolbarItemMenu { item_id });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_half_page_yank_silent_and_toolbar_jump_move_cursor() {
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        core.invoke(Command::ExplorerOpen).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Explorer);

        // Dynamic viewport capacity halves `<C-d>` / `<C-u>` (`EXP-15`).
        core.invoke(Command::Vim(crate::vim_keys::VimCommand::ReportExplorerViewport {
            visible_rows: 6,
            first_visible: 0,
            end_visible: 6,
        }))
        .unwrap();
        core.invoke(Command::ExplorerMove {
            direction: focus::FocusDirection::First,
            count: None,
        })
        .unwrap();
        core.invoke(Command::from_name_and_args("explorer.half_page_down", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().focus.cursor_index, 3);
        core.invoke(Command::from_name_and_args("explorer.half_page_up", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().focus.cursor_index, 0);

        // `y` and `Y` copy silently without showing a toast and clear selection (`EXP-17`).
        let toasts_before = core.model().toasts.len();
        core.invoke(Command::ExplorerSelect { range: false }).unwrap();
        assert_eq!(core.model().focus.selection.len(), 1);
        core.invoke(Command::from_name_and_args("explorer.yank", &[]).unwrap())
            .unwrap();
        assert!(core.model().focus.selection.is_empty());
        assert_eq!(core.model().toasts.len(), toasts_before);

        core.invoke(Command::from_name_and_args("explorer.yank_markdown", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().toasts.len(), toasts_before);

        // `s` in Toolbar mode moves cursor without leaving Toolbar mode (`TB-02`).
        core.invoke(Command::ToolbarFocusOpen).unwrap();
        core.invoke(Command::from_name_and_args("toolbar.jump", &[]).unwrap())
            .unwrap();
        let hints = core.model().focus.hints.expect("toolbar hints");
        let last_entry = hints.entries.last().unwrap().clone();
        core.invoke(Command::FocusHintInput { key: last_entry.label }).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Toolbar);
        assert_eq!(core.model().focus.cursor, Some(last_entry.target));
    }

    #[test]
    fn library_vim_actions_ignore_closed_library_and_forward_counts_when_open() {
        use crate::chrome_actions::LibraryVimAction;
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        let sub = core.subscribe();

        // With library closed (`InputMode::Normal`), `library.*` space actions must NOT touch `active_space` (`LIB-01`).
        let spaces_before = core.model().spaces.clone();
        core.invoke(Command::from_name_and_args("library.rename_space", &[]).unwrap())
            .unwrap();
        core.invoke(Command::from_name_and_args("library.add", &[]).unwrap())
            .unwrap();
        core.invoke(
            Command::from_name_and_args("library.move_space_right", &[("count".to_string(), "2".to_string())]).unwrap(),
        )
        .unwrap();
        assert_eq!(core.model().spaces, spaces_before);
        while let Ok(ev) = sub.try_recv() {
            assert!(
                !matches!(
                    ev,
                    Event::ViewRequested {
                        request: ViewRequest::SpaceRename { .. }
                            | ViewRequest::SpaceCreateFormOpen
                            | ViewRequest::LibraryVimAction { .. },
                        ..
                    }
                ),
                "unexpected view request while library closed: {ev:?}"
            );
        }

        // Enter Library mode: counted motions and `<CR>`/`o` (`library.open` without section) emit `LibraryVimAction` (`LIB-02`, `LIB-06`).
        core.invoke(Command::LibraryFocusSet { focused: true }).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Library);
        while sub.try_recv().is_ok() {}

        let cmds = crate::vim_keys::commands_for_key("library.down", &[], Some(4)).unwrap();
        assert_eq!(cmds.len(), 1);
        core.invoke(cmds.into_iter().next().unwrap()).unwrap();
        core.invoke(Command::from_name_and_args("library.open", &[]).unwrap())
            .unwrap();
        core.invoke(Command::from_name_and_args("library.rename_space", &[]).unwrap())
            .unwrap();

        let mut seen = Vec::new();
        while let Ok(ev) = sub.try_recv() {
            if let Event::ViewRequested {
                request: ViewRequest::LibraryVimAction { action, count },
                ..
            } = ev
            {
                seen.push((action, count));
            }
        }
        assert_eq!(
            seen,
            vec![
                (LibraryVimAction::Down, 4),
                (LibraryVimAction::Activate, 1),
                (LibraryVimAction::RenameSpace, 1),
            ]
        );
    }
}
