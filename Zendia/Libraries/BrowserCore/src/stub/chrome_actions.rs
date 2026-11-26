/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for [`crate::chrome_actions`].

use super::StubBackend;
use crate::BrowserCore;
use crate::chrome_actions::{ChromeAction, HALF_PAGE_STEP};
use crate::commands::{Command, CommandResult};
use crate::events::{Event, ViewRequest};
use crate::focus::{self, FocusSurface, FocusTarget, HintSession};
use crate::model::{FolderId, InputMode, SplitDirection, TabId};
use crate::picker::{PickerItem, PickerLayout, PickerRequest};

impl StubBackend {
    pub(super) fn handle_chrome_actions(&self, action: ChromeAction) -> CommandResult {
        match action {
            ChromeAction::ExplorerHalfPageDown => self.handle_explorer_step(HALF_PAGE_STEP),
            ChromeAction::ExplorerHalfPageUp => self.handle_explorer_step(-HALF_PAGE_STEP),
            ChromeAction::ExplorerPreview => self.handle_explorer_preview(),
            ChromeAction::ExplorerFoldToggle => self.handle_explorer_fold(None),
            ChromeAction::ExplorerFoldOpen => self.handle_explorer_fold(Some(false)),
            ChromeAction::ExplorerFoldClose => self.handle_explorer_fold(Some(true)),
            ChromeAction::ExplorerFind => self.invoke(Command::PickerOpen {
                name: Some("tabs".to_string()),
                request_id: None,
            }),
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
            ChromeAction::LibraryQuit => {
                self.invoke(Command::LibraryClose)?;
                self.invoke(Command::LibraryFocusSet { focused: false })
            }
            ChromeAction::LibraryGoto(section) => self.invoke(Command::LibraryOpen { section: Some(section) }),
            ChromeAction::Library(action) => self.handle_library_vim_action(action),
        }
    }

    /// The library's keys that change spaces act on the active space here; the rest move the library's own cursor,
    /// which is the window's view state.
    fn handle_library_vim_action(&self, action: crate::chrome_actions::LibraryVimAction) -> CommandResult {
        use crate::chrome_actions::LibraryVimAction;
        let active_space = self.model().active_space;
        match action {
            LibraryVimAction::Add => self.invoke(Command::SpaceCreateFormOpen),
            LibraryVimAction::RenameSpace => match active_space {
                Some(space_id) => self.invoke(Command::SpaceStartRename { space_id }),
                None => Ok(()),
            },
            LibraryVimAction::SpaceTheme => match active_space {
                Some(space_id) => self.invoke(Command::SpaceOpenThemePicker { space_id }),
                None => Ok(()),
            },
            LibraryVimAction::SpaceIcon => match active_space {
                Some(space_id) => self.invoke(Command::SpaceStartIconPicker { space_id }),
                None => Ok(()),
            },
            LibraryVimAction::MoveSpaceLeft | LibraryVimAction::MoveSpaceRight => {
                let model = self.model();
                let Some(active) = model.active_space else {
                    return Ok(());
                };
                let Some(position) = model.spaces.iter().position(|space| space.id == active) else {
                    return Ok(());
                };
                let next = if action == LibraryVimAction::MoveSpaceRight {
                    (position + 1).min(model.spaces.len().saturating_sub(1))
                } else {
                    position.saturating_sub(1)
                };
                if next == position {
                    return Ok(());
                }
                self.invoke(Command::SpaceReorder {
                    space_id: active,
                    index: next,
                })
            }
            _ => {
                self.request_view(ViewRequest::LibraryVimAction { action });
                Ok(())
            }
        }
    }

    fn explorer_target_tabs(&self) -> Vec<TabId> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.model.focus.surface != Some(FocusSurface::Explorer) {
            return Vec::new();
        }
        let targets: Vec<FocusTarget> = if !state.model.focus.selection.is_empty() {
            state.model.focus.selection.clone()
        } else {
            state.model.focus.cursor.clone().into_iter().collect()
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
        tabs
    }

    fn handle_explorer_step(&self, delta: isize) -> CommandResult {
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return Ok(());
            }
            let targets = focus::explorer_targets(&state.model);
            if targets.is_empty() {
                return Ok(());
            }
            let cur = state
                .model
                .focus
                .cursor
                .as_ref()
                .and_then(|c| targets.iter().position(|t| t == c))
                .unwrap_or(0);
            let next = (cur as isize + delta).clamp(0, (targets.len() - 1) as isize) as usize;
            let next_target = targets[next].clone();
            state.model.focus.cursor = Some(next_target.clone());
            state.model.focus.hints = None;
            if let Some(anchor) = state.model.focus.visual_anchor.clone() {
                state.model.focus.selection = focus::visual_range(&targets, &anchor, &next_target);
            }
            state.model.focus.clone()
        };
        self.bus.publish(Event::FocusChanged { focus });
        Ok(())
    }

    fn handle_explorer_preview(&self) -> CommandResult {
        let tab_id = self.explorer_target_tabs().into_iter().next();
        let Some(tab_id) = tab_id else {
            return Ok(());
        };
        if let Some(source) = self.current_tab_source() {
            source.activate_tab(tab_id)
        } else {
            self.feed_tab_activated(Some(tab_id));
            Ok(())
        }
    }

    fn handle_explorer_fold(&self, collapsed: Option<bool>) -> CommandResult {
        let folder_id: Option<FolderId> = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface != Some(FocusSurface::Explorer) {
                return Ok(());
            }
            match &state.model.focus.cursor {
                Some(FocusTarget::Folder(id)) => Some(*id),
                Some(FocusTarget::Tab(tab_id)) | Some(FocusTarget::PinnedTab(tab_id)) => {
                    state.model.tab(*tab_id).and_then(|t| t.folder_id)
                }
                _ => None,
            }
        };
        let Some(folder_id) = folder_id else {
            return Ok(());
        };
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(folder) = state.model.folders.iter_mut().find(|f| f.id == folder_id) else {
                return Ok(());
            };
            let next = collapsed.unwrap_or(!folder.collapsed);
            if folder.collapsed == next {
                return Ok(());
            }
            folder.collapsed = next;
            folder.clone()
        };
        self.bus.publish(Event::FolderUpdated { folder: updated });
        Ok(())
    }

    fn handle_explorer_yank(&self, markdown: bool) -> CommandResult {
        let tabs = self.explorer_target_tabs();
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
            self.bus
                .publish(Event::ClipboardCopyRequested { text: lines.join("\n") });
            let msg = if markdown { "Copied Markdown link" } else { "Copied URL" };
            self.show_toast(msg.to_string(), None, None);
        }
        Ok(())
    }

    fn handle_explorer_move_out_of_folder(&self) -> CommandResult {
        let tabs = self.explorer_target_tabs();
        for tab_id in tabs {
            let _ = self.invoke(Command::TabMoveToFolder {
                tab_id: Some(tab_id),
                folder_id: None,
            });
        }
        Ok(())
    }

    fn handle_explorer_reorder(&self, delta: isize) -> CommandResult {
        let cursor_tab = self.explorer_target_tabs().into_iter().next();
        let Some(tab_id) = cursor_tab else {
            return Ok(());
        };
        let prev_active = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.active_tab
        };
        // Temporarily align active_tab if needed, or move directly via TabMove.
        if prev_active == Some(tab_id) {
            return self.invoke(if delta > 0 {
                Command::Tabs(crate::tab_actions::TabAction::TabMoveNext)
            } else {
                Command::Tabs(crate::tab_actions::TabAction::TabMovePrev)
            });
        }
        self.feed_tab_activated(Some(tab_id));
        let res = self.invoke(if delta > 0 {
            Command::Tabs(crate::tab_actions::TabAction::TabMoveNext)
        } else {
            Command::Tabs(crate::tab_actions::TabAction::TabMovePrev)
        });
        self.feed_tab_activated(prev_active);
        res
    }

    fn handle_explorer_split(&self, direction: SplitDirection) -> CommandResult {
        let target_tab = self.explorer_target_tabs().into_iter().next();
        let active_tab = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.active_tab
        };
        let (Some(active), Some(target)) = (active_tab, target_tab) else {
            return Ok(());
        };
        if active == target {
            return Ok(());
        }
        self.invoke(Command::ExplorerClose)?;
        self.invoke(Command::SplitTabs {
            tab_ids: vec![active, target],
            direction,
        })
    }

    fn handle_explorer_menu(&self) -> CommandResult {
        let tab_id = self.explorer_target_tabs().into_iter().next();
        if let Some(tab_id) = tab_id {
            self.request_view(ViewRequest::TabMenu { tab_id });
        }
        Ok(())
    }

    fn handle_explorer_create(&self) -> CommandResult {
        let items = vec![
            PickerItem::new("new-tab", "New Tab", "tab.new").key("t"),
            PickerItem::new("new-folder", "New Folder", "folder.create").key("f"),
        ];
        let request = PickerRequest::new("explorer-create", "New", PickerLayout::List, items);
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.active_picker = Some(request.clone());
        }
        self.request_view(ViewRequest::PickerOpen { request });
        Ok(())
    }

    fn handle_toolbar_jump(&self) -> CommandResult {
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if !state.model.vim_mode_enabled {
                return Ok(());
            }
            let targets = focus::toolbar_targets(&state.model);
            state.model.focus.surface = Some(FocusSurface::Toolbar);
            state.model.focus.hints = if targets.is_empty() {
                None
            } else {
                Some(HintSession::new(FocusSurface::Toolbar, targets))
            };
            state.model.focus.clone()
        };
        self.set_input_mode(InputMode::Toolbar);
        self.bus.publish(Event::FocusChanged { focus });
        Ok(())
    }

    fn handle_toolbar_menu(&self) -> CommandResult {
        self.request_view(ViewRequest::AppMenuToggle);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_half_page_preview_and_toolbar_jump_work() {
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        core.invoke(Command::ExplorerOpen).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Explorer);

        core.invoke(Command::from_name_and_args("explorer.half_page_down", &[]).unwrap())
            .unwrap();
        core.invoke(Command::from_name_and_args("explorer.half_page_up", &[]).unwrap())
            .unwrap();
        core.invoke(Command::from_name_and_args("explorer.yank", &[]).unwrap())
            .unwrap();
        core.invoke(Command::from_name_and_args("explorer.yank_markdown", &[]).unwrap())
            .unwrap();

        core.invoke(Command::ToolbarFocusOpen).unwrap();
        core.invoke(Command::from_name_and_args("toolbar.jump", &[]).unwrap())
            .unwrap();
        assert!(core.model().focus.hints.is_some());
    }
}
