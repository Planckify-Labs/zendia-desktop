/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for [`crate::tab_actions`].

use super::StubBackend;
use crate::BrowserCore;
use crate::address_bar::{AddressBarFilter, resolve_clipboard_paste_url};
use crate::commands::{Command, CommandError, CommandResult};
use crate::events::{Event, ViewRequest};
use crate::model::{LayoutSettings, SpaceId, TabId, TabKind, TabMoveTarget, ThemeScheme};
use crate::picker::{PickerItem, PickerLayout, PickerRequest};
use crate::tab_actions::{TabAction, parent_url};
use crate::tab_source::TabUpdate;

impl StubBackend {
    pub(super) fn handle_tab_actions(&self, action: TabAction) -> CommandResult {
        let tab_source = self.current_tab_source();
        match action {
            TabAction::TabAlternate => self.handle_tab_alternate(tab_source.as_ref()),
            TabAction::TabCloseOthers => self.handle_tab_close_relative(RelativeClose::Others, tab_source.as_ref()),
            TabAction::TabCloseRight => self.handle_tab_close_relative(RelativeClose::Right, tab_source.as_ref()),
            TabAction::TabCloseLeft => self.handle_tab_close_relative(RelativeClose::Left, tab_source.as_ref()),
            TabAction::TabCloseUnpinned => self.handle_space_clear_tabs(None),
            TabAction::TabMoveNext => self.handle_tab_move_relative(1),
            TabAction::TabMovePrev => self.handle_tab_move_relative(-1),
            TabAction::TabReopenClosed => self.handle_tab_reopen_closed(tab_source.as_ref()),
            TabAction::TabPick => self.handle_tab_pick_hints(),
            TabAction::TabMoveToFolderPick => self.handle_move_to_folder_pick(false),
            TabAction::TabMenu => {
                let tab_id = self.resolve_target_tab(None, "tab.menu")?;
                self.request_view(ViewRequest::TabMenu { tab_id });
                Ok(())
            }
            TabAction::SpaceFirst => self.handle_space_edge(false),
            TabAction::SpaceLast => self.handle_space_edge(true),
            TabAction::SpaceCloseActive => self.handle_space_close_active(),
            TabAction::SpaceRenameActive => {
                let space_id = self.active_space_or_err("space.rename")?;
                self.handle_space_or_routing_command(Command::SpaceStartRename { space_id })
            }
            TabAction::SpaceIconActive => {
                let space_id = self.active_space_or_err("space.icon")?;
                self.handle_space_or_routing_command(Command::SpaceStartIconPicker { space_id })
            }
            TabAction::SpaceThemeActive => {
                let space_id = self.active_space_or_err("space.theme")?;
                self.handle_space_or_routing_command(Command::SpaceOpenThemePicker { space_id })
            }
            TabAction::SpaceUnloadActive => {
                let space_id = self.active_space_or_err("space.unload")?;
                self.handle_space_or_routing_command(Command::SpaceUnload { space_id })
            }
            TabAction::UrlUp => self.handle_url_up(tab_source.as_ref()),
            TabAction::UrlPaste { new_tab, text } => match text {
                Some(raw) => self.handle_url_paste_text(&raw, new_tab, tab_source.as_ref()),
                None => {
                    self.request_view(ViewRequest::UrlPaste { new_tab });
                    Ok(())
                }
            },
            TabAction::FindTabs => {
                self.request_view(ViewRequest::AddressBarOpen {
                    new_tab: false,
                    initial_query: None,
                    filter: AddressBarFilter::Tabs,
                });
                Ok(())
            }
            TabAction::FindRecent => {
                self.request_view(ViewRequest::AddressBarOpen {
                    new_tab: false,
                    initial_query: None,
                    filter: AddressBarFilter::History,
                });
                Ok(())
            }
            TabAction::FindBookmarks => {
                self.request_view(ViewRequest::AddressBarOpen {
                    new_tab: false,
                    initial_query: None,
                    filter: AddressBarFilter::Bookmarks,
                });
                Ok(())
            }
            TabAction::SearchWeb => {
                self.request_view(ViewRequest::AddressBarOpen {
                    new_tab: false,
                    initial_query: None,
                    filter: AddressBarFilter::Search,
                });
                Ok(())
            }
            TabAction::PageFind => {
                self.request_view(ViewRequest::PageFind);
                Ok(())
            }
            TabAction::PageFindQuery { query } => self.handle_page_find_query(query, tab_source.as_ref()),
            TabAction::PageFindSetOptions {
                highlight_all,
                match_case,
                whole_words,
            } => self.handle_page_find_set_options(highlight_all, match_case, whole_words, tab_source.as_ref()),
            TabAction::PageFindClose => {
                let find = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let tab = state.model.page_find.tab_id.take();
                    state.model.page_find.current = None;
                    state.model.page_find.total = None;
                    (tab, state.model.page_find.clone())
                };
                if let (Some(source), Some(tab_id)) = (tab_source.as_ref(), find.0) {
                    let _ = source.find_in_page_end(tab_id);
                }
                self.bus.publish(Event::PageFindChanged { find: find.1 });
                Ok(())
            }
            TabAction::SearchNext | TabAction::SearchPrev => {
                let backwards = matches!(action, TabAction::SearchPrev);
                let (query, shown_on) = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    (state.model.page_find.query.clone(), state.model.page_find.tab_id)
                };
                let Some(tab_id) = self.effective_keyed_tab() else {
                    return Ok(());
                };
                if query.is_empty() {
                    self.request_view(ViewRequest::PageFind);
                    return Ok(());
                }
                if shown_on != Some(tab_id) {
                    return self.handle_page_find_query(query, tab_source.as_ref());
                }
                if let Some(source) = tab_source.as_ref() {
                    source.find_in_page_step(tab_id, backwards)?;
                }
                Ok(())
            }
            TabAction::LayoutReset => {
                let updated = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state.user_customized_compact = false;
                    state.model.layout = LayoutSettings::default();
                    if state.model.vim_mode_enabled {
                        state.model.layout.compact_mode = true;
                        state.model.layout.compact_hide = crate::model::CompactHide::Both;
                        state.model.layout.normalize_compact_state();
                    }
                    state.model.layout.clone()
                };
                self.bus.publish(Event::LayoutSettingsChanged { layout: updated });
                Ok(())
            }
            TabAction::ThemeToggleDark => {
                // Consults OS dark-mode preference when `ThemeScheme::System` is active (`NAV-11`).
                let next = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let is_dark = match state.model.theme.scheme {
                        ThemeScheme::Dark => true,
                        ThemeScheme::Light => false,
                        ThemeScheme::System => state.model.system_prefers_dark.unwrap_or(true),
                    };
                    if is_dark { ThemeScheme::Light } else { ThemeScheme::Dark }
                };
                self.handle_theme_set(Some(next), None)
            }
            TabAction::NotificationDismissAll => self.handle_notification_dismiss_all(),
            TabAction::ExtensionsPick => self.handle_extensions_pick(),
            TabAction::SessionRestore => self.handle_session_restore(tab_source.as_ref()),
            TabAction::WindowMinimize => {
                self.request_view(ViewRequest::WindowMinimize);
                Ok(())
            }
            TabAction::WindowToggleMaximize => {
                self.request_view(ViewRequest::WindowToggleMaximize);
                Ok(())
            }
            TabAction::WindowClose => {
                self.request_view(ViewRequest::WindowClose);
                Ok(())
            }
            TabAction::WindowNew | TabAction::WindowNewBlank => self.invoke(Command::TabNew {
                url: None,
                after: None,
                activate: true,
            }),
            TabAction::WindowNewPrivate | TabAction::PagePrint | TabAction::PageSave => Ok(()),
        }
    }

    fn handle_space_close_active(&self) -> CommandResult {
        let (space_id, space_name, space_count, tab_count) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let space_id = state.model.active_space.ok_or_else(|| CommandError::TargetNotFound {
                command: "space.close".to_string(),
                target: "active space".to_string(),
            })?;
            let space_name = state
                .model
                .space(space_id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "Space".to_string());
            let tab_count = state
                .model
                .tabs
                .iter()
                .filter(|t| t.space_id == Some(space_id) && !state.model.is_glance_tab(t.id))
                .count();
            (space_id, space_name, state.model.spaces.len(), tab_count)
        };
        if space_count <= 1 {
            self.show_toast("Cannot close the last space".to_string(), None, None);
            return Ok(());
        }
        if tab_count > 0 {
            self.request_view(ViewRequest::SpaceDeleteConfirm {
                space_id,
                space_name,
                tab_count,
            });
            return Ok(());
        }
        self.handle_space_or_routing_command(Command::SpaceDelete { space_id })
    }

    fn handle_page_find_query(
        &self,
        query: String,
        tab_source: Option<&std::sync::Arc<dyn crate::tab_source::TabSource>>,
    ) -> CommandResult {
        let Some(tab_id) = self.effective_keyed_tab() else {
            return Ok(());
        };
        let (previous, find) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let previous = state.model.page_find.tab_id;
            let find = &mut state.model.page_find;
            find.current = None;
            find.total = None;
            if query.is_empty() {
                find.tab_id = None;
            } else {
                find.query = query.clone();
                find.tab_id = Some(tab_id);
            }
            (previous, find.clone())
        };
        if let Some(source) = tab_source {
            if let Some(previous) = previous
                && (previous != tab_id || query.is_empty())
            {
                let _ = source.find_in_page_end(previous);
            }
            if !query.is_empty() {
                source.find_in_page_with_options(
                    tab_id,
                    &query,
                    find.match_case,
                    find.highlight_all,
                    find.whole_words,
                )?;
            }
        }
        self.bus.publish(Event::PageFindChanged { find });
        Ok(())
    }

    fn handle_page_find_set_options(
        &self,
        highlight_all: Option<bool>,
        match_case: Option<bool>,
        whole_words: Option<bool>,
        tab_source: Option<&std::sync::Arc<dyn crate::tab_source::TabSource>>,
    ) -> CommandResult {
        let find = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(v) = highlight_all {
                state.model.page_find.highlight_all = v;
            }
            if let Some(v) = match_case {
                state.model.page_find.match_case = v;
            }
            if let Some(v) = whole_words {
                state.model.page_find.whole_words = v;
            }
            state.model.page_find.clone()
        };
        if let (Some(source), Some(tab_id)) = (tab_source, find.tab_id)
            && !find.query.is_empty()
        {
            source.find_in_page_with_options(
                tab_id,
                &find.query,
                find.match_case,
                find.highlight_all,
                find.whole_words,
            )?;
        }
        self.bus.publish(Event::PageFindChanged { find });
        Ok(())
    }

    /// The page counted the matches of find in page, or moved to one.
    pub fn feed_find_result(&self, tab_id: TabId, current: usize, total: Option<usize>) {
        let find = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.page_find.tab_id != Some(tab_id) {
                return;
            }
            state.model.page_find.current = Some(current);
            state.model.page_find.total = total;
            state.model.page_find.clone()
        };
        self.bus.publish(Event::PageFindChanged { find });
    }

    fn active_space_or_err(&self, command: &str) -> Result<SpaceId, CommandError> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.model.active_space.ok_or_else(|| CommandError::TargetNotFound {
            command: command.to_string(),
            target: "active space".to_string(),
        })
    }

    fn handle_space_edge(&self, last: bool) -> CommandResult {
        let target = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if last {
                state.model.spaces.last().map(|s| s.id)
            } else {
                state.model.spaces.first().map(|s| s.id)
            }
        };
        if let Some(space_id) = target {
            self.handle_space_switch(space_id)?;
        }
        Ok(())
    }

    fn handle_tab_alternate(
        &self,
        tab_source: Option<&std::sync::Arc<dyn crate::tab_source::TabSource>>,
    ) -> CommandResult {
        let target = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let ordered = state.model.tabs_in_active_space_order();
            let active = state.model.active_tab;
            let active_split: Vec<TabId> = active
                .and_then(|id| state.model.split_group_for_tab(id))
                .map(|g| g.tab_ids.clone())
                .unwrap_or_else(|| active.into_iter().collect());

            state
                .vim
                .tab_recency
                .iter()
                .rev()
                .copied()
                .find(|id| ordered.contains(id) && !active_split.contains(id))
                .or_else(|| ordered.into_iter().find(|id| !active_split.contains(id)))
        };
        let Some(target) = target else {
            return Ok(());
        };
        if let Some(source) = tab_source {
            source.activate_tab(target)
        } else {
            self.feed_tab_activated(Some(target));
            Ok(())
        }
    }

    fn handle_tab_close_relative(
        &self,
        which: RelativeClose,
        tab_source: Option<&std::sync::Arc<dyn crate::tab_source::TabSource>>,
    ) -> CommandResult {
        let to_close: Vec<TabId> = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(active_id) = state.model.active_tab else {
                return Ok(());
            };
            let Some(space_id) = state.model.active_space else {
                return Ok(());
            };
            let normal: Vec<TabId> = state
                .model
                .normal_tabs_in_space(space_id)
                .into_iter()
                .map(|tab| tab.id)
                .collect();
            let active_pos = normal.iter().position(|id| *id == active_id);
            match which {
                RelativeClose::Others => normal.into_iter().filter(|id| *id != active_id).collect(),
                RelativeClose::Right => match active_pos {
                    Some(pos) => normal.into_iter().skip(pos + 1).collect(),
                    None => Vec::new(),
                },
                RelativeClose::Left => match active_pos {
                    Some(pos) => normal.into_iter().take(pos).collect(),
                    None => Vec::new(),
                },
            }
        };
        self.handle_tab_close_multiple(to_close, tab_source)
    }

    fn handle_tab_move_relative(&self, delta: isize) -> CommandResult {
        let move_req: Option<(TabId, TabMoveTarget)> = (|| {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let active_id = state.model.active_tab?;
            let tab = state.model.tab(active_id)?;
            match tab.kind {
                TabKind::Essential => {
                    let len = state.model.essentials.len();
                    if len < 2 {
                        return None;
                    }
                    let pos = state.model.essentials.iter().position(|e| e.tab_id == active_id)?;
                    let next = (pos as isize + delta).clamp(0, (len - 1) as isize) as usize;
                    if next == pos {
                        return None;
                    }
                    Some((active_id, TabMoveTarget::Essential { index: next }))
                }
                TabKind::Pinned => {
                    let space_id = tab.space_id.or(state.model.active_space)?;
                    let pinned: Vec<TabId> = state
                        .model
                        .pinned_tabs
                        .iter()
                        .filter(|p| p.space_id == space_id)
                        .map(|p| p.tab_id)
                        .collect();
                    let len = pinned.len();
                    if len < 2 {
                        return None;
                    }
                    let pos = pinned.iter().position(|id| *id == active_id)?;
                    let next = (pos as isize + delta).clamp(0, (len - 1) as isize) as usize;
                    if next == pos {
                        return None;
                    }
                    Some((
                        active_id,
                        TabMoveTarget::Pinned {
                            space_id: Some(space_id),
                            index: next,
                        },
                    ))
                }
                TabKind::Normal => {
                    if let Some(folder_id) = tab.folder_id {
                        let folder = state.model.folder(folder_id)?;
                        let len = folder.tab_ids.len();
                        if len < 2 {
                            return None;
                        }
                        let pos = folder.tab_ids.iter().position(|id| *id == active_id)?;
                        let next = (pos as isize + delta).clamp(0, (len - 1) as isize) as usize;
                        if next == pos {
                            return None;
                        }
                        Some((active_id, TabMoveTarget::Folder { folder_id, index: next }))
                    } else {
                        let space_id = tab.space_id.or(state.model.active_space)?;
                        let normal: Vec<TabId> = state
                            .model
                            .tabs
                            .iter()
                            .filter(|t| {
                                t.kind == TabKind::Normal && t.space_id == Some(space_id) && t.folder_id.is_none()
                            })
                            .map(|t| t.id)
                            .collect();
                        let len = normal.len();
                        if len < 2 {
                            return None;
                        }
                        let pos = normal.iter().position(|id| *id == active_id)?;
                        let next = (pos as isize + delta).clamp(0, (len - 1) as isize) as usize;
                        if next == pos {
                            return None;
                        }
                        Some((
                            active_id,
                            TabMoveTarget::Normal {
                                space_id: Some(space_id),
                                index: next,
                            },
                        ))
                    }
                }
            }
        })();
        if let Some((tab_id, target)) = move_req {
            self.handle_tab_move(tab_id, target)?;
        }
        Ok(())
    }

    fn handle_tab_reopen_closed(
        &self,
        tab_source: Option<&std::sync::Arc<dyn crate::tab_source::TabSource>>,
    ) -> CommandResult {
        let popped = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.closed_tabs.is_empty() {
                return Ok(());
            }
            let entry = state.model.closed_tabs.remove(0);
            let snapshot = state.model.closed_tabs.clone();
            (entry, snapshot)
        };
        let (entry, closed_tabs) = popped;
        self.bus.publish(Event::ClosedTabsChanged { closed_tabs });
        if let Some(source) = tab_source {
            source.new_tab(Some(&entry.url), None, true)
        } else {
            self.invoke(Command::TabNew {
                url: Some(entry.url),
                after: None,
                activate: true,
            })
        }
    }

    fn handle_url_up(&self, tab_source: Option<&std::sync::Arc<dyn crate::tab_source::TabSource>>) -> CommandResult {
        let (tab_id, url) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(tab_id) = state.model.effective_keyed_tab() else {
                return Ok(());
            };
            let Some(tab) = state.model.tab(tab_id) else {
                return Ok(());
            };
            (tab.id, tab.url.clone())
        };
        let Some(next_url) = parent_url(&url) else {
            return Ok(());
        };
        if let Some(source) = tab_source {
            source.load_url(tab_id, &next_url)
        } else {
            self.feed_tab_updated(
                tab_id,
                TabUpdate {
                    url: Some(next_url),
                    ..TabUpdate::default()
                },
            );
            Ok(())
        }
    }

    fn handle_url_paste_text(
        &self,
        raw: &str,
        new_tab: bool,
        tab_source: Option<&std::sync::Arc<dyn crate::tab_source::TabSource>>,
    ) -> CommandResult {
        let (resolved, active_tab) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(url) = resolve_clipboard_paste_url(raw, &state.model.search_engine) else {
                return Ok(());
            };
            (url, state.model.effective_keyed_tab())
        };
        if new_tab || active_tab.is_none() {
            if let Some(source) = tab_source {
                source.new_tab(Some(&resolved), active_tab, true)
            } else {
                self.invoke(Command::TabNew {
                    url: Some(resolved),
                    after: active_tab,
                    activate: true,
                })
            }
        } else if let Some(tab_id) = active_tab {
            if let Some(source) = tab_source {
                source.load_url(tab_id, &resolved)
            } else {
                self.invoke(Command::NavigationLoadUrl {
                    tab_id: Some(tab_id),
                    input: resolved,
                })
            }
        } else {
            Ok(())
        }
    }

    fn handle_notification_dismiss_all(&self) -> CommandResult {
        let (notif_ids, toast_ids) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let notif_ids: Vec<_> = state.model.sidebar_notifications.drain(..).map(|n| n.id).collect();
            let toast_ids: Vec<_> = state.model.toasts.drain(..).map(|t| t.id).collect();
            (notif_ids, toast_ids)
        };
        for notification_id in notif_ids {
            self.bus
                .publish(Event::SidebarNotificationDismissed { notification_id });
        }
        for toast_id in toast_ids {
            self.bus.publish(Event::ToastDismissed { toast_id });
        }
        Ok(())
    }

    fn handle_extensions_pick(&self) -> CommandResult {
        let has_live_source = self.current_extension_source().is_some();
        let (vim_enabled, items): (bool, Vec<PickerItem>) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let service_ok = !has_live_source
                || matches!(
                    crate::modules::service_status_in(&state.model.services, crate::model::ServiceId::ExtensionServer),
                    crate::model::ServiceStatus::Running
                );
            let mut exts: Vec<_> = if service_ok {
                state
                    .model
                    .extensions
                    .iter()
                    .filter(|e| e.enabled && !e.is_system && e.has_action)
                    .collect()
            } else {
                Vec::new()
            };
            exts.sort_by_key(|ext| ext.name.to_lowercase());
            let items = exts
                .into_iter()
                .map(|ext| {
                    let mut item = PickerItem::new(ext.id.0.clone(), ext.name.clone(), "extension.invoke_action")
                        .icon(ext.icon.clone());
                    item.icon_data = ext.icon_data.clone();
                    item.command_args.push(("extension_id".to_string(), ext.id.0.clone()));
                    item.command_args
                        .push(("from_keyboard".to_string(), "true".to_string()));
                    item
                })
                .collect();
            (state.model.vim_mode_enabled, items)
        };
        if items.is_empty() {
            self.show_toast("No extension to run.".to_string(), None, None);
            return Ok(());
        }
        if !vim_enabled {
            self.request_view(ViewRequest::ExtensionsMenuToggle);
            return Ok(());
        }
        let request = PickerRequest::new("extensions", "Extensions", PickerLayout::List, items);
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.active_picker = Some(request.clone());
        }
        self.request_view(ViewRequest::PickerOpen { request });
        Ok(())
    }

    /// `<leader>qs` (`session.restore`): restores the previous browser session snapshot (spaces and tabs), falling back
    /// to closed tabs when no snapshot was recorded (`NAV-02`).
    fn handle_session_restore(
        &self,
        tab_source: Option<&std::sync::Arc<dyn crate::tab_source::TabSource>>,
    ) -> CommandResult {
        enum RestoreSource {
            AlreadyRestored,
            Snapshot(crate::session::SessionSnapshot),
            ClosedTabs(Vec<crate::model::HistoryEntry>),
            Empty,
        }

        let source_kind = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.session_restored {
                RestoreSource::AlreadyRestored
            } else if let Some(snapshot) = state.last_session.take().filter(|s| !s.tabs.is_empty()) {
                state.session_restored = true;
                RestoreSource::Snapshot(snapshot)
            } else if !state.model.closed_tabs.is_empty() {
                state.session_restored = true;
                RestoreSource::ClosedTabs(std::mem::take(&mut state.model.closed_tabs))
            } else {
                RestoreSource::Empty
            }
        };

        match source_kind {
            RestoreSource::AlreadyRestored => {
                self.show_toast("Previous session was already restored".to_string(), None, None);
                Ok(())
            }
            RestoreSource::Empty => {
                self.show_toast("No previous session to restore".to_string(), None, None);
                Ok(())
            }
            RestoreSource::ClosedTabs(entries) => {
                self.bus.publish(Event::ClosedTabsChanged {
                    closed_tabs: Vec::new(),
                });
                for entry in entries.into_iter().rev() {
                    if let Some(source) = tab_source {
                        let _ = source.new_tab(Some(&entry.url), None, true);
                    } else {
                        let _ = self.invoke(Command::TabNew {
                            url: Some(entry.url),
                            after: None,
                            activate: true,
                        });
                    }
                }
                Ok(())
            }
            RestoreSource::Snapshot(snapshot) => {
                // Map or create spaces by name.
                let mut space_map: Vec<SpaceId> = Vec::new();
                {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let mut spaces_changed = false;
                    for saved_space in &snapshot.spaces {
                        if let Some(existing) = state
                            .model
                            .spaces
                            .iter()
                            .find(|s| s.name.eq_ignore_ascii_case(&saved_space.name))
                        {
                            space_map.push(existing.id);
                        } else {
                            let created = crate::space_routing::create_space(
                                &mut state.model,
                                Some(saved_space.name.clone()),
                                Some(saved_space.icon.clone()),
                            );
                            space_map.push(created.id);
                            spaces_changed = true;
                        }
                    }
                    if spaces_changed {
                        let spaces = state.model.spaces.clone();
                        drop(state);
                        self.bus.publish(Event::SpacesUpdated { spaces });
                    }
                }

                let mut opened_ids: Vec<TabId> = Vec::new();
                for saved_tab in &snapshot.tabs {
                    let target_space = saved_tab
                        .space_index
                        .and_then(|idx| space_map.get(idx).copied())
                        .or_else(|| self.model().active_space);
                    if let Some(sid) = target_space
                        && self.model().active_space != Some(sid)
                    {
                        let _ = self.handle_space_switch(sid);
                    }
                    {
                        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                        state.last_opened_tab = None;
                    }
                    if let Some(source) = tab_source {
                        let _ = source.new_tab(Some(&saved_tab.url), None, false);
                    } else {
                        let _ = self.handle_stub_tab_new(Some(saved_tab.url.clone()), None, false);
                    }
                    let new_id = self
                        .state
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .last_opened_tab;
                    if let Some(tab_id) = new_id {
                        opened_ids.push(tab_id);
                        if let Some(custom) = &saved_tab.custom_title {
                            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                            if let Some(t) = state.model.tabs.iter_mut().find(|t| t.id == tab_id) {
                                t.custom_title = Some(custom.clone());
                                let updated = t.clone();
                                drop(state);
                                self.bus.publish(Event::TabUpdated { tab: updated });
                            }
                        }
                        match saved_tab.kind {
                            TabKind::Pinned => {
                                let _ = self.handle_tab_pin(tab_id);
                            }
                            TabKind::Essential => {
                                let _ = self.handle_tab_add_to_essentials(tab_id);
                            }
                            TabKind::Normal => {}
                        }
                    }
                }

                if let Some(&dest_space) = space_map.get(snapshot.active_space_index) {
                    let _ = self.handle_space_switch(dest_space);
                }
                if let Some(active_idx) = snapshot.active_tab_index
                    && let Some(&tab_id) = opened_ids.get(active_idx)
                {
                    if let Some(source) = tab_source {
                        let _ = source.activate_tab(tab_id);
                    } else {
                        self.feed_tab_activated(Some(tab_id));
                    }
                }
                self.show_toast("Restored previous session".to_string(), None, None);
                Ok(())
            }
        }
    }
}

#[derive(Clone, Copy)]
enum RelativeClose {
    Others,
    Right,
    Left,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ThemeScheme;
    use crate::session::{SessionSnapshot, SessionSpaceRecord, SessionTabRecord};

    #[test]
    fn tab_alternate_and_reopen_closed_work() {
        let core = StubBackend::new();
        core.invoke(Command::TabNew {
            url: Some("https://first.example/a/b".to_string()),
            after: None,
            activate: true,
        })
        .unwrap();
        let first = core.model().active_tab.unwrap();
        core.invoke(Command::TabNew {
            url: Some("https://second.example".to_string()),
            after: None,
            activate: true,
        })
        .unwrap();
        let second = core.model().active_tab.unwrap();
        assert_ne!(first, second);

        core.invoke(Command::from_name_and_args("tab.alternate", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_tab, Some(first));

        // url.up navigates to parent path.
        core.invoke(Command::from_name_and_args("url.up", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_tab().unwrap().url, "https://first.example/a/");

        // Close and reopen.
        core.invoke(Command::TabClose {
            tab_id: Some(first),
            force: false,
        })
        .unwrap();
        core.invoke(Command::from_name_and_args("tab.reopen_closed", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_tab().unwrap().url, "https://first.example/a/");
    }

    #[test]
    fn theme_toggle_dark_consults_system_preference_and_layout_reset() {
        let core = StubBackend::new();
        assert_eq!(core.model().theme.scheme, ThemeScheme::System);
        // Default system_prefers_dark is true (Dark), so first `<leader>ub` switches to Light (`NAV-11`).
        core.invoke(Command::from_name_and_args("theme.toggle_dark", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().theme.scheme, ThemeScheme::Light);

        // When System is active on a light OS (`prefers_dark = false`), `<leader>ub` switches to Dark (`NAV-11`).
        core.invoke(Command::ThemeSet {
            scheme: Some(ThemeScheme::System),
            overrides: None,
        })
        .unwrap();
        core.invoke(Command::ThemeSetSystemDark { prefers_dark: false })
            .unwrap();
        core.invoke(Command::from_name_and_args("theme.toggle_dark", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().theme.scheme, ThemeScheme::Dark);

        core.invoke(Command::LayoutSetCompactMode { enabled: true }).unwrap();
        assert!(core.model().layout.compact_mode);
        core.invoke(Command::from_name_and_args("layout.reset", &[]).unwrap())
            .unwrap();
        assert!(!core.model().layout.compact_mode);
    }

    #[test]
    fn space_close_requests_confirmation_when_non_empty_and_deletes_when_empty() {
        let core = StubBackend::new();
        let sub = core.subscribe();
        // Sample active space has tabs -> `space.close` emits `ViewRequest::SpaceDeleteConfirm` instead of deleting (`NAV-13`).
        let count_before = core.model().spaces.len();
        core.invoke(Command::from_name_and_args("space.close", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().spaces.len(), count_before);
        let mut saw_confirm = false;
        while let Ok(ev) = sub.try_recv() {
            if matches!(
                ev,
                Event::ViewRequested {
                    request: ViewRequest::SpaceDeleteConfirm { .. },
                    ..
                }
            ) {
                saw_confirm = true;
            }
        }
        assert!(saw_confirm);

        // Creating an empty space and closing it deletes without confirmation.
        core.invoke(Command::SpaceCreate {
            name: Some("Empty".to_string()),
            icon: None,
        })
        .unwrap();
        let empty_id = core.model().spaces.last().unwrap().id;
        core.invoke(Command::SpaceSwitch { space_id: empty_id }).unwrap();
        core.invoke(Command::from_name_and_args("space.close", &[]).unwrap())
            .unwrap();
        assert!(core.model().space(empty_id).is_none());
    }

    #[test]
    fn session_restore_restores_spaces_and_tabs_once() {
        let core = StubBackend::new();
        core.set_last_session(Some(SessionSnapshot {
            spaces: vec![SessionSpaceRecord {
                name: "Archive".to_string(),
                icon: "book".to_string(),
            }],
            active_space_index: 0,
            tabs: vec![SessionTabRecord {
                url: "https://restored.example/doc".to_string(),
                title: "Restored Doc".to_string(),
                custom_title: Some("Saved Title".to_string()),
                space_index: Some(0),
                kind: TabKind::Pinned,
            }],
            active_tab_index: Some(0),
        }));
        core.invoke(Command::from_name_and_args("session.restore", &[]).unwrap())
            .unwrap();
        assert!(core.model().spaces.iter().any(|s| s.name == "Archive"));
        let restored = core
            .model()
            .tabs
            .iter()
            .find(|t| t.url == "https://restored.example/doc")
            .cloned()
            .expect("restored tab");
        assert_eq!(restored.kind, TabKind::Pinned);
        assert_eq!(restored.custom_title.as_deref(), Some("Saved Title"));
    }

    #[test]
    fn space_first_and_last_switch_spaces() {
        let core = StubBackend::new();
        let spaces = core.model().spaces;
        assert!(spaces.len() >= 2);
        core.invoke(Command::from_name_and_args("space.last", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_space, Some(spaces.last().unwrap().id));
        core.invoke(Command::from_name_and_args("space.first", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_space, Some(spaces.first().unwrap().id));
    }

    #[test]
    fn url_paste_allows_only_single_line_http_https_and_searches() {
        let core = StubBackend::new();
        core.invoke(Command::NavigationLoadUrl {
            tab_id: None,
            input: "https://start.example/".to_string(),
        })
        .unwrap();

        for unsafe_text in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "/etc/passwd",
            "data:text/html,<h1>hi</h1>",
            "about:config",
            "https://line1.example\nhttps://line2.example",
            "   ",
        ] {
            core.invoke(
                Command::from_name_and_args("url.paste", &[("text".to_string(), unsafe_text.to_string())]).unwrap(),
            )
            .unwrap();
            assert_eq!(
                core.model().active_tab().unwrap().url,
                "https://start.example/",
                "rejected `{unsafe_text}`"
            );
        }

        core.invoke(
            Command::from_name_and_args("url.paste", &[("text".to_string(), "  example.org/docs  ".to_string())])
                .unwrap(),
        )
        .unwrap();
        assert_eq!(core.model().active_tab().unwrap().url, "https://example.org/docs");

        let count_before = core.model().tabs.len();
        core.invoke(
            Command::from_name_and_args(
                "url.paste_new_tab",
                &[("text".to_string(), "rust borrow checker".to_string())],
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(core.model().tabs.len(), count_before + 1);
        assert!(core.model().active_tab().unwrap().url.starts_with("https://"));
    }
}
