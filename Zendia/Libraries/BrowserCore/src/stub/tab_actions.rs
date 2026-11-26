/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for [`crate::tab_actions`].

use super::StubBackend;
use crate::BrowserCore;
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
            TabAction::TabPick => self.invoke(Command::PickerOpen {
                name: Some("tabs".to_string()),
                request_id: None,
            }),
            TabAction::TabMenu => {
                let tab_id = self.resolve_target_tab(None, "tab.menu")?;
                self.request_view(ViewRequest::TabMenu { tab_id });
                Ok(())
            }
            TabAction::SpaceFirst => self.handle_space_edge(false),
            TabAction::SpaceLast => self.handle_space_edge(true),
            TabAction::SpaceCloseActive => {
                let space_id = self.active_space_or_err("space.close")?;
                self.handle_space_or_routing_command(Command::SpaceDelete { space_id })
            }
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
            TabAction::UrlPaste { new_tab } => {
                self.request_view(ViewRequest::UrlPaste { new_tab });
                Ok(())
            }
            TabAction::FindTabs => {
                self.request_view(ViewRequest::AddressBarOpen {
                    new_tab: false,
                    initial_query: Some("% ".to_string()),
                });
                Ok(())
            }
            TabAction::FindRecent => {
                self.request_view(ViewRequest::AddressBarOpen {
                    new_tab: false,
                    initial_query: Some("^ ".to_string()),
                });
                Ok(())
            }
            TabAction::SearchWeb => {
                self.request_view(ViewRequest::AddressBarOpen {
                    new_tab: false,
                    initial_query: Some("? ".to_string()),
                });
                Ok(())
            }
            TabAction::PageFind => {
                self.request_view(ViewRequest::PageFind);
                Ok(())
            }
            TabAction::PageFindQuery { query } => self.handle_page_find_query(query, tab_source.as_ref()),
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
                // Again with the last text, as the find bar's arrows do; with none yet, the find bar opens.
                let backwards = matches!(action, TabAction::SearchPrev);
                let (query, shown_on) = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    (state.model.page_find.query.clone(), state.model.page_find.tab_id)
                };
                let Some(tab_id) = self.model().active_tab else {
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
                    state.model.layout = LayoutSettings::default();
                    state.model.layout.clone()
                };
                self.bus.publish(Event::LayoutSettingsChanged { layout: updated });
                Ok(())
            }
            TabAction::ThemeToggleDark => {
                let next = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    match state.model.theme.scheme {
                        ThemeScheme::Dark => ThemeScheme::Light,
                        ThemeScheme::Light | ThemeScheme::System => ThemeScheme::Dark,
                    }
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
        }
    }

    fn handle_page_find_query(
        &self,
        query: String,
        tab_source: Option<&std::sync::Arc<dyn crate::tab_source::TabSource>>,
    ) -> CommandResult {
        let Some(tab_id) = self.model().active_tab else {
            return Ok(());
        };
        let (previous, find) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let previous = state.model.page_find.tab_id;
            let find = &mut state.model.page_find;
            find.current = None;
            find.total = None;
            if query.is_empty() {
                // An empty find bar keeps the last text for `n`, and the page shows no matches.
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
                source.find_in_page(tab_id, &query)?;
            }
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

            // Pick the most recently used tab in the active space outside the active split group.
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
            let Some(tab) = state.model.active_tab() else {
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
        let (vim_enabled, items): (bool, Vec<PickerItem>) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let mut exts: Vec<_> = state.model.extensions.iter().filter(|e| e.enabled).collect();
            exts.sort_by_key(|ext| ext.name.to_lowercase());
            let items = exts
                .into_iter()
                .map(|ext| {
                    PickerItem::new(ext.id.0.clone(), ext.name.clone(), "extensions.menu.toggle")
                        .detail(ext.version.clone())
                        .icon("puzzle")
                })
                .collect();
            (state.model.vim_mode_enabled, items)
        };
        if items.is_empty() {
            self.show_toast("No extensions installed".to_string(), None, None);
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

    fn handle_session_restore(
        &self,
        tab_source: Option<&std::sync::Arc<dyn crate::tab_source::TabSource>>,
    ) -> CommandResult {
        let entries = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.closed_tabs.is_empty() {
                return Ok(());
            }
            std::mem::take(&mut state.model.closed_tabs)
        };
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
    fn tab_close_others_and_theme_toggle_and_layout_reset() {
        let core = StubBackend::new();
        let initial_scheme = core.model().theme.scheme;
        core.invoke(Command::from_name_and_args("theme.toggle_dark", &[]).unwrap())
            .unwrap();
        let toggled = core.model().theme.scheme;
        assert_ne!(initial_scheme, toggled);
        assert!(matches!(toggled, ThemeScheme::Dark | ThemeScheme::Light));

        core.invoke(Command::LayoutSetCompactMode { enabled: true }).unwrap();
        assert!(core.model().layout.compact_mode);
        core.invoke(Command::from_name_and_args("layout.reset", &[]).unwrap())
            .unwrap();
        assert!(!core.model().layout.compact_mode);
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
}
