/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for [`crate::split_actions`], Window mode carry/preview/drop (`ZendiaWindowMode.sys.mjs`), split
//! pickers (`ZendiaWindows.sys.mjs`), and the side-panel focus row (`ZendiaPanes.sys.mjs`, `WIN-01`..`WIN-23`).

use super::StubBackend;
use super::vim::{EmptySplitPane, PanePlace};
use crate::BrowserCore;
use crate::address_bar::AddressBarFilter;
use crate::agent::{AGENT_PANEL_ID, SidePanelPrefs, resolve_stacked_panels};
#[cfg(test)]
use crate::commands::Command;
use crate::commands::CommandResult;
use crate::contributions::{SidePanelId, SidePanelLayout, SidePanelSide};
use crate::events::{Event, ViewRequest};
use crate::focus::{
    self, FocusState, FocusSurface, FocusTarget, HintMatchAction, HintSession, LandingKind, PaneDirection,
    WindowCarryState,
};
use crate::model::{InputMode, SplitGroup, SplitGroupId, Tab, TabId, TabKind};
use crate::picker::{PickerItem, PickerLayout, PickerRequest};
use crate::split::{self, DropZone, MAX_SPLIT_PANES, SplitAxis, SplitNode};
use crate::split_actions::{RESIZE_STEP, ResizeKind, SplitAction};
use crate::tab_source::TabSource;
use crate::toasts;
use std::sync::Arc;

impl StubBackend {
    pub(super) fn handle_split_actions(&self, action: SplitAction) -> CommandResult {
        match action {
            SplitAction::Focus { direction } => self.handle_split_focus(direction).map(|_| ()),
            SplitAction::PanesFocus { direction } => self.handle_panes_focus(direction),
            SplitAction::PanesLast => self.handle_panes_last(),
            SplitAction::SplitDivide { zone } => self.handle_split_divide(zone),
            SplitAction::SplitPickApply {
                anchor_tab_id,
                picked_tab_id,
                zone,
                open_url,
            } => self.handle_split_pick_apply(anchor_tab_id, picked_tab_id, zone, open_url),
            SplitAction::AddPick => self.handle_split_add_pick(),
            SplitAction::AddCarry {
                anchor_tab_id,
                picked_tab_id,
            } => self.handle_split_add_carry(anchor_tab_id, picked_tab_id),
            SplitAction::WindowEscape => self.handle_window_escape(),
            SplitAction::Next => self.handle_split_next(),
            SplitAction::Last => self.handle_split_last(),
            SplitAction::Pick => self.handle_split_pick(),
            SplitAction::RemoveActiveTab => {
                let tab_id = self.resolve_target_tab(None, "split.remove_tab")?;
                let tab_source = self.current_tab_source();
                self.close_split_pane(tab_id, tab_source.as_ref())
            }
            SplitAction::Zoom { tab_id } => self.handle_split_zoom(tab_id),
            SplitAction::Equalize => self.handle_split_equalize(),
            SplitAction::Resize { kind, count } => self.handle_split_resize(kind, count),
            SplitAction::UpdateTree { group_id, tree } => self.handle_split_update_tree(group_id, tree),
            SplitAction::Swap => self.start_window_carry(None),
            SplitAction::Rotate { reverse } => self.handle_split_rotate(reverse),
            SplitAction::Edge { direction } => self.handle_split_edge(direction),
            SplitAction::Grab => self.start_window_carry(None),
        }
    }

    /// Opens `tab_id`'s split chip for `1.2 s` when a split mutation occurs outside Window mode (`WIN-11`).
    pub(super) fn flash_pane_chip(&self, tab_id: TabId) {
        let flash = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.input_mode == InputMode::Window {
                return;
            }
            if state.model.split_group_for_tab(tab_id).is_none() {
                return;
            }
            let seq = state.vim.chip_flash_seq.wrapping_add(1);
            state.vim.chip_flash_seq = seq;
            state.model.chip_flash = Some((tab_id, seq));
            (tab_id, seq)
        };
        self.bus.publish(Event::PaneChipFlashed {
            tab_id: flash.0,
            seq: flash.1,
        });
    }

    /// Cancels any active Window-mode carry without closing Window mode unless `close_after_carry` was set.
    pub(super) fn cancel_window_carry(&self) -> CommandResult {
        let (was_carrying, close_after, active_tab, focus) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let was_carrying = state.model.focus.window_carry.take().is_some();
            let close_after = if was_carrying {
                std::mem::replace(&mut state.model.focus.close_after_carry, false)
            } else {
                false
            };
            (
                was_carrying,
                close_after,
                state.model.active_tab,
                state.model.focus.clone(),
            )
        };
        if !was_carrying {
            return Ok(());
        }
        if close_after {
            self.handle_focus_close(FocusSurface::Window)?;
            if let Some(tab_id) = active_tab {
                self.flash_pane_chip(tab_id);
            }
        } else {
            self.publish_focus(focus);
        }
        Ok(())
    }

    fn handle_window_escape(&self) -> CommandResult {
        let is_carrying = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.focus.window_carry.is_some()
        };
        if is_carrying {
            self.cancel_window_carry()
        } else {
            self.handle_focus_close(FocusSurface::Window)
        }
    }

    /// Removes `tab_id` from its split group into its own tab (or drops an empty URL placeholder pane) and focuses
    /// the last-used remaining pane (`closePane` in `ZendiaWindows.sys.mjs:351-368`, `WIN-19`, `WIN-20`).
    pub(super) fn close_split_pane(&self, tab_id: TabId, tab_source: Option<&Arc<dyn TabSource>>) -> CommandResult {
        self.cancel_window_carry()?;

        let empty_anchor = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.vim.empty_split_pane.is_some_and(|e| e.empty_tab == tab_id) {
                state.vim.empty_split_pane.take().map(|e| e.anchor_tab)
            } else {
                None
            }
        };
        if let Some(anchor) = empty_anchor {
            if let Some(source) = tab_source {
                let _ = source.close_tab(tab_id, true);
            } else {
                let _ = self.handle_stub_tab_close(tab_id);
            }
            if self.model().tab(anchor).is_some() {
                if let Some(source) = tab_source {
                    let _ = source.activate_tab(anchor);
                } else {
                    self.feed_tab_activated(Some(anchor));
                }
            }
            return Ok(());
        }

        let (next_pane, events) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(group) = state.model.split_group_for_tab(tab_id).cloned() else {
                return Ok(());
            };
            let empty_id = state.vim.empty_split_pane.map(|e| e.empty_tab);
            let others: Vec<TabId> = group
                .tab_ids
                .iter()
                .copied()
                .filter(|&p| p != tab_id && Some(p) != empty_id)
                .collect();
            let next = last_used_of(&others, &state.vim.tab_recency);
            let events = super::page_layouts::detach_from_groups(&mut state, &[tab_id], None);
            (next, events)
        };
        for ev in events {
            self.bus.publish(ev);
        }
        if let Some(next_id) = next_pane {
            if let Some(source) = tab_source {
                source.activate_tab(next_id)?;
            } else {
                self.feed_tab_activated(Some(next_id));
            }
            self.refresh_window_mode_focus();
        }
        Ok(())
    }

    /// Cleans up an empty URL split pane or pending split URL bar when the floating address bar closes without
    /// submitting (`WIN-20`).
    pub(super) fn handle_address_bar_cancelled(&self) -> CommandResult {
        let empty = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.vim.pending_split_url_bar = None;
            state.vim.empty_split_pane.take()
        };
        if let Some(empty) = empty {
            let tab_source = self.current_tab_source();
            if let Some(ref source) = tab_source {
                let _ = source.close_tab(empty.empty_tab, true);
            } else {
                let _ = self.handle_stub_tab_close(empty.empty_tab);
            }
            if self.model().tab(empty.anchor_tab).is_some() {
                if let Some(ref source) = tab_source {
                    let _ = source.activate_tab(empty.anchor_tab);
                } else {
                    self.feed_tab_activated(Some(empty.anchor_tab));
                }
            }
        }
        Ok(())
    }

    fn activate_split_pane(&self, tab_id: TabId) -> CommandResult {
        if let Some(source) = self.current_tab_source() {
            source.activate_tab(tab_id)?;
        } else {
            self.feed_tab_activated(Some(tab_id));
        }
        self.refresh_window_mode_focus();
        Ok(())
    }

    fn refresh_window_mode_focus(&self) {
        let updated_focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface == Some(FocusSurface::Window) {
                if let Some(active) = state.model.active_tab {
                    state.model.focus.cursor = Some(FocusTarget::Pane(active));
                }
                if let Some(carry) = state.model.focus.window_carry.clone() {
                    let targets =
                        focus::compute_landing_targets_with_mode(&state.model, carry.carried_tab, carry.adding);
                    if targets.is_empty() {
                        state.model.focus.window_carry = None;
                    } else {
                        let selected = carry.selected.min(targets.len() - 1);
                        state.model.focus.window_carry = Some(WindowCarryState {
                            targets,
                            selected,
                            ..carry
                        });
                    }
                }
                Some(state.model.focus.clone())
            } else {
                None
            }
        };
        if let Some(focus) = updated_focus {
            self.bus.publish(Event::FocusChanged { focus });
        }
    }

    /// Candidate tabs in the active space eligible to join a split beside `current`: visible, non-closing, non-empty,
    /// and not already in any split group (`paneCandidates` in `ZendiaWindows.sys.mjs:52-61`).
    fn pane_candidates(&self, current: TabId) -> Vec<Tab> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let empty_id = state.vim.empty_split_pane.map(|e| e.empty_tab);
        state
            .model
            .tabs_in_active_space_order()
            .into_iter()
            .filter(|&id| id != current && Some(id) != empty_id && !state.model.is_glance_tab(id))
            .filter_map(|id| state.model.tab(id).cloned())
            .filter(|tab| !tab.closing && tab.split_group_id.is_none())
            .collect()
    }

    /// `<leader>|` / `wv` (`Right`) and `<leader>-` / `ws` (`Bottom`) (`split` in `ZendiaWindows.sys.mjs:79-118`, `WIN-01`).
    fn handle_split_divide(&self, zone: DropZone) -> CommandResult {
        let (current, is_empty, is_full) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(current) = state.model.active_tab else {
                return Ok(());
            };
            let is_empty = state.vim.empty_split_pane.is_some_and(|e| e.empty_tab == current);
            let is_full = state
                .model
                .split_group_for_tab(current)
                .is_some_and(|g| g.tab_ids.len() >= MAX_SPLIT_PANES);
            (current, is_empty, is_full)
        };
        if is_empty {
            return Ok(());
        }
        if is_full {
            self.show_toast(toasts::messages::SPLIT_VIEW_LIMIT.to_string(), None, None);
            return Ok(());
        }

        let candidates = self.pane_candidates(current);
        if candidates.is_empty() {
            return self.open_url_pane(current, zone);
        }

        let title = match zone {
            DropZone::Bottom | DropZone::Top => "Split Window Below",
            DropZone::Left | DropZone::Right => "Split Window Right",
        };
        let side_str = zone.as_str().to_string();
        let mut items = Vec::with_capacity(candidates.len() + 1);
        items.push(
            PickerItem::new("open-url", "Open URL…", "split.pick_apply")
                .detail("any website")
                .key("/")
                .arg("anchor_tab_id", current.0.to_string())
                .arg("side", side_str.clone())
                .arg("open_url", "true"),
        );
        for tab in &candidates {
            let mut item = PickerItem::new(format!("tab.{}", tab.id.0), tab.display_title(), "split.pick_apply")
                .detail(host_of_url(&tab.url))
                .arg("anchor_tab_id", current.0.to_string())
                .arg("picked_tab_id", tab.id.0.to_string())
                .arg("side", side_str.clone());
            if let Some(ref icon) = tab.favicon {
                item = item.icon(icon.clone());
            }
            items.push(item);
        }

        let id_str = match zone {
            DropZone::Bottom | DropZone::Top => "split-below",
            DropZone::Left | DropZone::Right => "split-right",
        };
        let request = PickerRequest::new(id_str, title, PickerLayout::List, items);
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.active_picker = Some(request.clone());
        }
        self.request_view(ViewRequest::PickerOpen { request });
        Ok(())
    }

    fn handle_split_pick_apply(
        &self,
        anchor_tab_id: TabId,
        picked_tab_id: Option<TabId>,
        zone: DropZone,
        open_url: bool,
    ) -> CommandResult {
        // Drop the picker's choice if the selected tab changed while the picker was open (`VimParity.md:760`).
        let still_current = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.active_tab == Some(anchor_tab_id) && state.model.tab(anchor_tab_id).is_some()
        };
        if !still_current {
            return Ok(());
        }
        if open_url {
            return self.open_url_pane(anchor_tab_id, zone);
        }
        if let Some(picked) = picked_tab_id {
            return self.divide_pane_with_tab(anchor_tab_id, picked, zone);
        }
        Ok(())
    }

    /// Opens the URL bar for a new split pane beside `anchor` (`openUrlPane` in `ZendiaWindows.sys.mjs:126-172`, `WIN-20`).
    fn open_url_pane(&self, anchor: TabId, side: DropZone) -> CommandResult {
        let already_in_split = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.split_group_for_tab(anchor).is_some()
        };
        if already_in_split {
            {
                let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                state.vim.pending_split_url_bar = Some((anchor, side));
            }
            self.request_view(ViewRequest::AddressBarOpen {
                new_tab: true,
                initial_query: None,
                filter: AddressBarFilter::All,
            });
            return Ok(());
        }

        let tab_source = self.current_tab_source();
        let new_tab = {
            {
                let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                state.last_opened_tab = None;
                state.suppress_bg_tab_toast = true;
            }
            let open_res = match &tab_source {
                Some(source) => source.new_tab(Some("about:blank"), Some(anchor), false),
                None => self.handle_stub_tab_new(Some("about:blank".to_string()), Some(anchor), false),
            };
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.suppress_bg_tab_toast = false;
            open_res?;
            let Some(id) = state.last_opened_tab.take() else {
                return Ok(());
            };
            state.vim.empty_split_pane = Some(EmptySplitPane {
                empty_tab: id,
                anchor_tab: anchor,
                side,
            });
            id
        };

        self.split_with("split.open_url_pane", anchor, new_tab, side)?;
        if let Some(ref source) = tab_source {
            source.activate_tab(new_tab)?;
        } else {
            self.feed_tab_activated(Some(new_tab));
        }
        self.request_view(ViewRequest::AddressBarOpen {
            new_tab: false,
            initial_query: None,
            filter: AddressBarFilter::All,
        });
        Ok(())
    }

    /// Divides `anchor`'s pane with `picked` on `zone`, pinning an unpinned tab (with toast) when mixed with a pinned
    /// tab and focusing `picked` (`splitWith` in `ZendiaWindows.sys.mjs:183-240`, `WIN-01`, `WIN-10`, `WIN-16`).
    pub(super) fn divide_pane_with_tab(&self, anchor: TabId, picked: TabId, zone: DropZone) -> CommandResult {
        if anchor == picked {
            return Ok(());
        }
        let tab_source = self.current_tab_source();
        let (anchor_tab, picked_tab, anchor_group, picked_group, is_empty) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let is_empty = state
                .vim
                .empty_split_pane
                .is_some_and(|e| e.empty_tab == anchor || e.empty_tab == picked);
            (
                state.model.tab(anchor).cloned(),
                state.model.tab(picked).cloned(),
                state.model.split_group_for_tab(anchor).map(|g| (g.id, g.tab_ids.len())),
                state.model.split_group_for_tab(picked).map(|g| g.id),
                is_empty,
            )
        };
        let (Some(anchor_t), Some(picked_t)) = (anchor_tab, picked_tab) else {
            return Ok(());
        };
        if is_empty || picked_t.closing {
            return Ok(());
        }
        if let Some(pg) = picked_group {
            if anchor_group.map(|(ag, _)| ag) == Some(pg) {
                return self.activate_split_pane(picked);
            }
            return Ok(());
        }
        if anchor_group.is_some_and(|(_, len)| len >= MAX_SPLIT_PANES) {
            self.show_toast(toasts::messages::SPLIT_VIEW_LIMIT.to_string(), None, None);
            return Ok(());
        }

        // Mixing pinned (non-essential) and unpinned pins the unpinned tab and shows the fork's toast (`WIN-10`).
        let tab_to_pin = if anchor_t.kind == TabKind::Pinned && picked_t.kind == TabKind::Normal {
            Some(picked)
        } else if picked_t.kind == TabKind::Pinned && anchor_t.kind == TabKind::Normal {
            Some(anchor)
        } else {
            None
        };
        if let Some(pin_id) = tab_to_pin {
            let _ = self.handle_tab_pin(pin_id);
        }

        self.split_with("split.divide", anchor, picked, zone)?;
        if let Some(ref source) = tab_source {
            source.activate_tab(picked)?;
        } else {
            self.feed_tab_activated(Some(picked));
        }
        if tab_to_pin.is_some() {
            self.show_toast(toasts::messages::SPLIT_PINNED.to_string(), None, None);
        }
        self.refresh_window_mode_focus();
        Ok(())
    }

    /// `<leader>wa` / `wa` (`addPane` in `ZendiaWindows.sys.mjs:470-497`, `WIN-02`, `WIN-16`).
    fn handle_split_add_pick(&self) -> CommandResult {
        let (current, is_empty, is_full) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(current) = state.model.active_tab else {
                return Ok(());
            };
            let is_empty = state.vim.empty_split_pane.is_some_and(|e| e.empty_tab == current);
            let is_full = state
                .model
                .split_group_for_tab(current)
                .is_some_and(|g| g.tab_ids.len() >= MAX_SPLIT_PANES);
            (current, is_empty, is_full)
        };
        if is_empty {
            return Ok(());
        }
        if is_full {
            self.show_toast(toasts::messages::SPLIT_VIEW_LIMIT.to_string(), None, None);
            return Ok(());
        }
        let candidates = self.pane_candidates(current);
        if candidates.is_empty() {
            return Ok(());
        }
        let items: Vec<PickerItem> = candidates
            .iter()
            .map(|tab| {
                let mut item = PickerItem::new(format!("tab.{}", tab.id.0), tab.display_title(), "split.add_carry")
                    .detail(host_of_url(&tab.url))
                    .arg("anchor_tab_id", current.0.to_string())
                    .arg("picked_tab_id", tab.id.0.to_string());
                if let Some(ref icon) = tab.favicon {
                    item = item.icon(icon.clone());
                }
                item
            })
            .collect();
        let request = PickerRequest::new("split-add", "Add to Split", PickerLayout::List, items);
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.active_picker = Some(request.clone());
        }
        self.request_view(ViewRequest::PickerOpen { request });
        Ok(())
    }

    fn handle_split_add_carry(&self, anchor_tab_id: TabId, picked_tab_id: TabId) -> CommandResult {
        if !self.vim_enabled() {
            return self.divide_pane_with_tab(anchor_tab_id, picked_tab_id, DropZone::Right);
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.active_tab != Some(anchor_tab_id) || state.model.tab(picked_tab_id).is_none() {
                return Ok(());
            }
            let was_open = state.model.focus.surface == Some(FocusSurface::Window);
            if !was_open {
                let visible_rows = state.model.focus.explorer_visible_rows;
                let visible_range = state.model.focus.explorer_visible_range;
                state.model.focus = FocusState {
                    surface: Some(FocusSurface::Window),
                    cursor: Some(FocusTarget::Pane(anchor_tab_id)),
                    close_after_carry: true,
                    explorer_visible_rows: visible_rows,
                    explorer_visible_range: visible_range,
                    ..FocusState::default()
                };
            }
            let targets = focus::compute_landing_targets_with_mode(&state.model, picked_tab_id, true);
            if targets.is_empty() {
                return Ok(());
            }
            let selected = targets
                .iter()
                .position(|t| {
                    matches!(
                        t.kind,
                        LandingKind::PaneSide {
                            target_tab,
                            zone: DropZone::Right
                        } if target_tab == anchor_tab_id
                    )
                })
                .unwrap_or(0);
            state.model.focus.window_carry = Some(WindowCarryState {
                carried_tab: picked_tab_id,
                adding: true,
                targets,
                selected,
            });
            state.model.focus.clone()
        };
        self.set_input_mode(InputMode::Window);
        self.publish_focus(focus);
        Ok(())
    }

    /// Starts carrying the active pane (`wg`, `wx`, or `wH/J/K/L` when not already carrying) (`grab` in
    /// `ZendiaWindowMode.sys.mjs:205-221`, `WIN-03`, `WIN-07`, `WIN-22`).
    pub(super) fn start_window_carry(&self, edge: Option<DropZone>) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(active) = state.model.active_tab else {
                return Ok(());
            };
            let Some(group) = state.model.active_split_group() else {
                // `WIN-22`: without a split of at least 2 panes, `wg`/`wx`/`wH/J/K/L` do nothing.
                return Ok(());
            };
            let panes = split::panes_in_reading_order(group);
            if panes.len() < 2 || !panes.contains(&active) {
                return Ok(());
            }
            let others: Vec<TabId> = panes.into_iter().filter(|&p| p != active).collect();
            let first_other = last_used_of(&others, &state.vim.tab_recency).unwrap_or(others[0]);

            let was_open = state.model.focus.surface == Some(FocusSurface::Window);
            if !was_open {
                let visible_rows = state.model.focus.explorer_visible_rows;
                let visible_range = state.model.focus.explorer_visible_range;
                state.model.focus = FocusState {
                    surface: Some(FocusSurface::Window),
                    cursor: Some(FocusTarget::Pane(active)),
                    close_after_carry: true,
                    explorer_visible_rows: visible_rows,
                    explorer_visible_range: visible_range,
                    ..FocusState::default()
                };
            }

            let targets = focus::compute_landing_targets_with_mode(&state.model, active, false);
            if targets.is_empty() {
                return Ok(());
            }
            let selected = match edge {
                Some(zone) => focus::target_edge_index(&targets, zone).unwrap_or(0),
                None => targets
                    .iter()
                    .position(|t| matches!(t.kind, LandingKind::PaneSwap { target_tab } if target_tab == first_other))
                    .unwrap_or(0),
            };
            state.model.focus.window_carry = Some(WindowCarryState {
                carried_tab: active,
                adding: false,
                targets,
                selected,
            });
            state.model.focus.clone()
        };
        self.set_input_mode(InputMode::Window);
        self.publish_focus(focus);
        Ok(())
    }

    /// Commits the active carry on `<CR>` (`window.drop`, `ZendiaWindowMode.sys.mjs:466-502`, `WIN-03`, `WIN-07`,
    /// `WIN-13`, `WIN-17`).
    pub(super) fn drop_window_carry(&self) -> CommandResult {
        let (carry, close_after, active_before) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(carry) = state.model.focus.window_carry.take() else {
                return Ok(());
            };
            let close_after = std::mem::replace(&mut state.model.focus.close_after_carry, false);
            (carry, close_after, state.model.active_tab)
        };
        let Some(target) = carry.selected_target().cloned() else {
            return Ok(());
        };
        let carried_tab = carry.carried_tab;

        if carry.adding {
            match target.kind {
                LandingKind::PaneSide { target_tab, zone } => {
                    self.divide_pane_with_tab(target_tab, carried_tab, zone)?;
                }
                LandingKind::SplitEdge { zone } => {
                    if let Some(anchor) = active_before {
                        self.add_tab_at_split_edge(anchor, carried_tab, zone)?;
                    }
                }
                LandingKind::PaneSwap { .. } => {}
            }
        } else {
            let updated_group = {
                let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                let group_id = state.model.split_group_for_tab(carried_tab).map(|g| g.id);
                if let Some(gid) = group_id
                    && let Some(group) = state.model.split_groups.iter_mut().find(|g| g.id == gid)
                    && let Some(mut tree) = split::effective_group_tree(group)
                {
                    let changed = match target.kind {
                        LandingKind::PaneSwap { target_tab } => split::swap_leaves(&mut tree, carried_tab, target_tab),
                        LandingKind::PaneSide { target_tab, zone } => {
                            split::move_leaf(&mut tree, carried_tab, target_tab, zone)
                        }
                        LandingKind::SplitEdge { zone } => split::move_leaf_to_edge(&mut tree, carried_tab, zone),
                    };
                    if changed {
                        split::sync_group_from_tree(group, tree);
                        group.zoomed_tab = None;
                        Some(group.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some(group) = updated_group {
                self.bus.publish(Event::SplitGroupUpdated { group });
            }
            self.activate_split_pane(carried_tab)?;
        }

        if close_after {
            self.handle_focus_close(FocusSurface::Window)?;
            self.flash_pane_chip(carried_tab);
        } else {
            self.refresh_window_mode_focus();
        }
        Ok(())
    }

    fn add_tab_at_split_edge(&self, anchor: TabId, picked: TabId, zone: DropZone) -> CommandResult {
        let had_group = self.model().split_group_for_tab(anchor).is_some();
        if !had_group {
            return self.divide_pane_with_tab(anchor, picked, zone);
        }
        // Join the split (handling pinned mixing & toast), then restore `anchor`'s share and move `picked` to the
        // outer edge so existing panes keep their relative sizes (`placePane` in `ZendiaWindows.sys.mjs:527-536`).
        let saved_tree = self
            .model()
            .split_group_for_tab(anchor)
            .and_then(split::effective_group_tree);
        self.divide_pane_with_tab(anchor, picked, DropZone::Right)?;
        if let Some(mut base_tree) = saved_tree {
            let updated_group = {
                let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                let gid = state.model.split_group_for_tab(picked).map(|g| g.id);
                if let Some(gid) = gid
                    && let Some(group) = state.model.split_groups.iter_mut().find(|g| g.id == gid)
                    && group.tab_ids.contains(&picked)
                {
                    split::move_leaf_to_edge(&mut base_tree, picked, zone);
                    split::sync_group_from_tree(group, base_tree);
                    Some(group.clone())
                } else {
                    None
                }
            };
            if let Some(group) = updated_group {
                self.bus.publish(Event::SplitGroupUpdated { group });
            }
        }
        Ok(())
    }

    /// Returns `true` if focus moved (or carry target moved).
    fn handle_split_focus(&self, direction: PaneDirection) -> Result<bool, crate::commands::CommandError> {
        let moved_carry = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface == Some(FocusSurface::Window)
                && let Some(carry) = state.model.focus.window_carry.as_mut()
            {
                carry.selected = focus::step_landing_target(&carry.targets, carry.selected, direction);
                Some(state.model.focus.clone())
            } else {
                None
            }
        };
        if let Some(focus) = moved_carry {
            self.bus.publish(Event::FocusChanged { focus });
            return Ok(true);
        }

        let neighbor = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(active) = state.model.active_tab else {
                return Ok(false);
            };
            let Some(group) = state.model.active_split_group() else {
                return Ok(false);
            };
            let (dx, dy) = direction.delta();
            split::pane_beside(group, active, dx, dy, &state.vim.tab_recency)
        };
        if let Some(target) = neighbor {
            self.activate_split_pane(target)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// `<C-h/j/k/l>`: steps along `[left panels, split panes / page, right panels]`, swallowing at the ends (`WIN-06`).
    fn handle_panes_focus(&self, direction: PaneDirection) -> CommandResult {
        let in_panel = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.vim.focused_panel.is_some()
        };
        if in_panel {
            match direction {
                PaneDirection::Left => {
                    let _ = self.step_panel_row(-1);
                }
                PaneDirection::Right => {
                    let _ = self.step_panel_row(1);
                }
                PaneDirection::Up | PaneDirection::Down => {}
            }
            return Ok(());
        }

        if self.handle_split_focus(direction)? {
            return Ok(());
        }
        match direction {
            PaneDirection::Left => {
                let _ = self.step_panel_row(-1);
            }
            PaneDirection::Right => {
                let _ = self.step_panel_row(1);
            }
            PaneDirection::Up | PaneDirection::Down => {}
        }
        Ok(())
    }

    /// `<C-\>` (`panes.last`): returns to the previous place in `[left panels, page, right panels]` (`WIN-06`).
    fn handle_panes_last(&self) -> CommandResult {
        let next_place = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let row = Self::compute_panel_row(&state);
            let current = match &state.vim.focused_panel {
                Some(pid) if row.contains(&PanePlace::Panel(pid.clone())) => PanePlace::Panel(pid.clone()),
                _ => PanePlace::Page,
            };
            state
                .vim
                .pane_focus_recency
                .iter()
                .find(|place| **place != current && row.contains(place))
                .cloned()
        };
        if let Some(place) = next_place {
            self.focus_pane_place(place);
        }
        Ok(())
    }

    fn compute_panel_row(state: &super::StubState) -> Vec<PanePlace> {
        let open = &state.vim.open_panels;
        if open.is_empty() {
            return vec![PanePlace::Page];
        }
        let inputs: Vec<(SidePanelPrefs, f32)> = open
            .iter()
            .map(|id| {
                let prefs = state.model.panel_prefs(id);
                let min_w = SidePanelPrefs::min_width_for(id.as_str());
                (prefs, min_w)
            })
            .collect();
        let stacked = resolve_stacked_panels(&inputs, 1120.0, 1.0);
        let mut left_split = Vec::new();
        let mut left_float = Vec::new();
        let mut right_float = Vec::new();
        let mut right_split = Vec::new();
        for ((id, (prefs, _)), placed) in open.iter().zip(inputs.iter()).zip(stacked.iter()) {
            match (prefs.side, placed.layout.effective_layout) {
                (SidePanelSide::Left, SidePanelLayout::Split) => left_split.push(PanePlace::Panel(id.clone())),
                (SidePanelSide::Left, _) => left_float.push(PanePlace::Panel(id.clone())),
                (SidePanelSide::Right, SidePanelLayout::Split) => right_split.push(PanePlace::Panel(id.clone())),
                (SidePanelSide::Right, _) => right_float.push(PanePlace::Panel(id.clone())),
            }
        }
        right_float.reverse();
        right_split.reverse();

        let mut row = Vec::with_capacity(open.len() + 1);
        row.extend(left_split);
        row.extend(left_float);
        row.push(PanePlace::Page);
        row.extend(right_float);
        row.extend(right_split);
        row
    }

    fn step_panel_row(&self, dx: isize) -> bool {
        let next_place = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let row = Self::compute_panel_row(&state);
            let current = match &state.vim.focused_panel {
                Some(pid) if row.contains(&PanePlace::Panel(pid.clone())) => PanePlace::Panel(pid.clone()),
                _ => PanePlace::Page,
            };
            let Some(pos) = row.iter().position(|p| p == &current) else {
                return false;
            };
            let next_idx = pos as isize + dx;
            if next_idx < 0 || next_idx >= row.len() as isize {
                return false;
            }
            row[next_idx as usize].clone()
        };
        self.focus_pane_place(next_place);
        true
    }

    fn focus_pane_place(&self, place: PanePlace) {
        let panel_id = match &place {
            PanePlace::Page => None,
            PanePlace::Panel(id) => Some(id.clone()),
        };
        self.record_panel_focus(panel_id.clone());
        self.request_view(ViewRequest::PanelFocusChanged { panel_id });
    }

    pub(super) fn record_panel_focus(&self, panel_id: Option<SidePanelId>) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let prev = match &state.vim.focused_panel {
            Some(pid) => PanePlace::Panel(pid.clone()),
            None => PanePlace::Page,
        };
        let next = match &panel_id {
            Some(pid) => PanePlace::Panel(pid.clone()),
            None => PanePlace::Page,
        };
        state.vim.focused_panel = panel_id;
        if state.vim.pane_focus_recency.is_empty() {
            state.vim.pane_focus_recency.push(prev.clone());
        }
        if state.vim.pane_focus_recency.first() != Some(&next) {
            state.vim.pane_focus_recency.retain(|p| p != &next);
            state.vim.pane_focus_recency.insert(0, next);
        }
    }

    pub(super) fn track_panel_view_request(&self, request: &ViewRequest) {
        match request {
            ViewRequest::PanelOpen { panel_id, focus } => {
                {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state.vim.open_panels.retain(|id| id != panel_id);
                    state.vim.open_panels.push(panel_id.clone());
                }
                if *focus {
                    self.record_panel_focus(Some(panel_id.clone()));
                }
            }
            ViewRequest::PanelClose { panel_id } => {
                let was_focused = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state.vim.open_panels.retain(|id| id != panel_id);
                    state.vim.focused_panel.as_ref() == Some(panel_id)
                };
                if was_focused {
                    self.record_panel_focus(None);
                }
            }
            ViewRequest::PanelToggle { panel_id } => {
                let already_open = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state.vim.open_panels.contains(panel_id)
                };
                if already_open {
                    self.track_panel_view_request(&ViewRequest::PanelClose {
                        panel_id: panel_id.clone(),
                    });
                } else {
                    self.track_panel_view_request(&ViewRequest::PanelOpen {
                        panel_id: panel_id.clone(),
                        focus: true,
                    });
                }
            }
            ViewRequest::AgentModelCard | ViewRequest::AgentInstructions => {
                let agent_id = SidePanelId::new(AGENT_PANEL_ID);
                let already_open = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state.vim.open_panels.contains(&agent_id)
                };
                if !already_open {
                    self.track_panel_view_request(&ViewRequest::PanelOpen {
                        panel_id: agent_id,
                        focus: true,
                    });
                }
            }
            _ => {}
        }
    }

    /// `ww` (`split.next`): cycles panes in reading order (`panesInOrder`, `WIN-23`) and flashes chip outside Window mode (`WIN-11`).
    fn handle_split_next(&self) -> CommandResult {
        let next = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(active) = state.model.active_tab else {
                return Ok(());
            };
            let Some(group) = state.model.active_split_group() else {
                return Ok(());
            };
            let panes = split::panes_in_reading_order(group);
            let Some(pos) = panes.iter().position(|&id| id == active) else {
                return Ok(());
            };
            panes.get((pos + 1) % panes.len()).copied()
        };
        if let Some(target) = next {
            self.activate_split_pane(target)?;
            self.flash_pane_chip(target);
        }
        Ok(())
    }

    /// `wp` (`split.last`): focuses last-used other pane and flashes chip outside Window mode (`WIN-11`).
    fn handle_split_last(&self) -> CommandResult {
        let target = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(active) = state.model.active_tab else {
                return Ok(());
            };
            let Some(group) = state.model.active_split_group() else {
                return Ok(());
            };
            last_other_pane(group, active, &state.vim.tab_recency)
        };
        if let Some(target) = target {
            self.activate_split_pane(target)?;
            self.flash_pane_chip(target);
        }
        Ok(())
    }

    /// `wf` (`split.pick`): puts `HintSession` labels directly on pane chips (`ZendiaWindowMode.sys.mjs:397-431`, `WIN-05`).
    fn handle_split_pick(&self) -> CommandResult {
        if !self.vim_enabled() {
            return Ok(());
        }
        let focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.hints.is_some() {
                return Ok(());
            }
            let carried = state
                .model
                .focus
                .window_carry
                .as_ref()
                .filter(|c| !c.adding)
                .map(|c| c.carried_tab);
            let Some(group) = state.model.active_split_group() else {
                return Ok(());
            };
            let panes: Vec<TabId> = split::panes_in_reading_order(group)
                .into_iter()
                .filter(|&tab| Some(tab) != carried)
                .collect();
            let min_needed = if carried.is_some() { 1 } else { 2 };
            if panes.len() < min_needed {
                return Ok(());
            }

            let was_open = state.model.focus.surface == Some(FocusSurface::Window);
            if !was_open {
                let visible_rows = state.model.focus.explorer_visible_rows;
                let visible_range = state.model.focus.explorer_visible_range;
                let cursor = state
                    .model
                    .active_tab
                    .map(FocusTarget::Pane)
                    .or_else(|| panes.first().copied().map(FocusTarget::Pane));
                state.model.focus = FocusState {
                    surface: Some(FocusSurface::Window),
                    cursor,
                    transient_for_hints: true,
                    explorer_visible_rows: visible_rows,
                    explorer_visible_range: visible_range,
                    ..FocusState::default()
                };
            }
            let targets = panes.into_iter().map(FocusTarget::Pane).collect();
            state.model.focus.hints =
                Some(HintSession::new(FocusSurface::Window, targets).with_action(HintMatchAction::ActivatePane));
            state.model.focus.clone()
        };
        self.set_input_mode(InputMode::Window);
        self.publish_focus(focus);
        Ok(())
    }

    /// `wm` (`split.zoom`) and pane chip zoom button: toggles `group.zoomed_tab` so the zoomed pane fills 100% of the
    /// view while keeping underlying pane sizes intact (`togglePaneZoom`, `WIN-09`, `WIN-11`, `WIN-14`).
    fn handle_split_zoom(&self, explicit_tab: Option<TabId>) -> CommandResult {
        let target_tab = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(tab_id) = explicit_tab.or(state.model.active_tab) else {
                return Ok(());
            };
            let Some(group) = state.model.split_group_for_tab(tab_id) else {
                return Ok(());
            };
            if group.tab_ids.len() < 2 {
                return Ok(());
            }
            tab_id
        };

        let currently_zoomed = self.model().split_group_for_tab(target_tab).and_then(|g| g.zoomed_tab);

        if currently_zoomed != Some(target_tab) && self.model().active_tab != Some(target_tab) {
            // Select first: picking another pane is what brings the others back (`ZenViewSplitter.mjs:1878`).
            self.activate_split_pane(target_tab)?;
        }

        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let gid = state.model.split_group_for_tab(target_tab).map(|g| g.id);
            if let Some(gid) = gid
                && let Some(group) = state.model.split_groups.iter_mut().find(|g| g.id == gid)
            {
                if group.zoomed_tab == Some(target_tab) {
                    group.zoomed_tab = None;
                } else {
                    group.zoomed_tab = Some(target_tab);
                }
                Some(group.clone())
            } else {
                None
            }
        };
        if let Some(group) = updated {
            self.bus.publish(Event::SplitGroupUpdated { group });
            self.flash_pane_chip(target_tab);
            self.refresh_window_mode_focus();
        }
        Ok(())
    }

    fn handle_split_equalize(&self) -> CommandResult {
        let (updated, active) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let active = state.model.active_tab;
            let gid = state.model.active_split_group().map(|g| g.id);
            let updated = if let Some(gid) = gid
                && let Some(group) = state.model.split_groups.iter_mut().find(|g| g.id == gid)
                && let Some(mut tree) = split::effective_group_tree(group)
            {
                split::equalize_tree(&mut tree);
                split::sync_group_from_tree(group, tree);
                group.zoomed_tab = None;
                Some(group.clone())
            } else {
                None
            };
            (updated, active)
        };
        if let Some(group) = updated {
            self.bus.publish(Event::SplitGroupUpdated { group });
            if let Some(tab_id) = active {
                self.flash_pane_chip(tab_id);
            }
            self.refresh_window_mode_focus();
        }
        Ok(())
    }

    fn handle_split_resize(&self, kind: ResizeKind, count: Option<u32>) -> CommandResult {
        let mult = count.unwrap_or(1).clamp(1, crate::vim_keys::MAX_REPEAT) as f32;
        let sign = match kind {
            ResizeKind::GrowWidth | ResizeKind::GrowHeight => 1.0,
            ResizeKind::ShrinkWidth | ResizeKind::ShrinkHeight => -1.0,
        };
        let axis = match kind {
            ResizeKind::GrowWidth | ResizeKind::ShrinkWidth => SplitAxis::Row,
            ResizeKind::GrowHeight | ResizeKind::ShrinkHeight => SplitAxis::Column,
        };
        let delta_pct = sign * RESIZE_STEP * 100.0 * mult;

        let (updated, active) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(active) = state.model.active_tab else {
                return Ok(());
            };
            let gid = state.model.active_split_group().map(|g| g.id);
            let updated = if let Some(gid) = gid
                && let Some(group) = state.model.split_groups.iter_mut().find(|g| g.id == gid)
                && group.tab_ids.len() >= 2
                && let Some(mut tree) = split::effective_group_tree(group)
            {
                if split::resize_leaf(&mut tree, active, axis, delta_pct) {
                    split::sync_group_from_tree(group, tree);
                    group.zoomed_tab = None;
                    Some(group.clone())
                } else {
                    None
                }
            } else {
                None
            };
            (updated, active)
        };
        if let Some(group) = updated {
            self.bus.publish(Event::SplitGroupUpdated { group });
            self.flash_pane_chip(active);
            self.refresh_window_mode_focus();
        }
        Ok(())
    }

    fn handle_split_update_tree(&self, group_id: SplitGroupId, tree: SplitNode) -> CommandResult {
        let updated = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(group) = state.model.split_groups.iter_mut().find(|g| g.id == group_id) {
                let leaves = tree.leaves();
                if leaves.len() == group.tab_ids.len() && leaves.iter().all(|id| group.tab_ids.contains(id)) {
                    split::sync_group_from_tree(group, tree);
                    group.zoomed_tab = None;
                    Some(group.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(group) = updated {
            self.bus.publish(Event::SplitGroupUpdated { group });
            self.refresh_window_mode_focus();
        }
        Ok(())
    }

    /// `wr` / `wR` (`split.rotate` / `split.rotate_back`): rotates leaves in tree order while keeping slot sizes (`WIN-17`).
    fn handle_split_rotate(&self, reverse: bool) -> CommandResult {
        let (updated, active) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let active = state.model.active_tab;
            let gid = state.model.active_split_group().map(|g| g.id);
            let updated = if let Some(gid) = gid
                && let Some(group) = state.model.split_groups.iter_mut().find(|g| g.id == gid)
                && group.tab_ids.len() >= 2
                && let Some(mut tree) = split::effective_group_tree(group)
                && split::rotate_leaves(&mut tree, reverse)
            {
                split::sync_group_from_tree(group, tree);
                group.zoomed_tab = None;
                Some(group.clone())
            } else {
                None
            };
            (updated, active)
        };
        if let Some(group) = updated {
            self.bus.publish(Event::SplitGroupUpdated { group });
            if let Some(tab_id) = active {
                self.flash_pane_chip(tab_id);
            }
            self.refresh_window_mode_focus();
        }
        Ok(())
    }

    /// `wH/J/K/L` (`split.edge_*`): moves carry target to outer edge when already carrying, else starts carry preview
    /// at that outer edge (`toEdge` in `ZendiaWindows.sys.mjs:431-445`, `WIN-03`, `WIN-13`).
    fn handle_split_edge(&self, direction: PaneDirection) -> CommandResult {
        let zone = direction.drop_zone();
        let retargeted = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface == Some(FocusSurface::Window)
                && let Some(carry) = state.model.focus.window_carry.as_mut()
            {
                if let Some(idx) = focus::target_edge_index(&carry.targets, zone) {
                    carry.selected = idx;
                }
                Some(state.model.focus.clone())
            } else {
                None
            }
        };
        if let Some(focus) = retargeted {
            self.publish_focus(focus);
            return Ok(());
        }
        self.start_window_carry(Some(zone))
    }
}

fn last_used_of(candidates: &[TabId], recency: &[TabId]) -> Option<TabId> {
    recency
        .iter()
        .rev()
        .copied()
        .find(|id| candidates.contains(id))
        .or_else(|| candidates.first().copied())
}

fn last_other_pane(group: &SplitGroup, active: TabId, recency: &[TabId]) -> Option<TabId> {
    let panes = split::panes_in_reading_order(group);
    let others: Vec<TabId> = panes.into_iter().filter(|&id| id != active).collect();
    last_used_of(&others, recency)
}

fn host_of_url(url: &str) -> String {
    let trimmed = url.trim();
    if let Some(rest) = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
    {
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        if !host.is_empty() {
            return host.to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SplitDirection;

    fn two_pane_core() -> (StubBackend, TabId, TabId) {
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        let t1 = core.model().active_tab.unwrap();
        core.invoke(Command::TabNew {
            url: Some("https://second.example".to_string()),
            after: None,
            activate: true,
        })
        .unwrap();
        let t2 = core.model().active_tab.unwrap();
        core.invoke(Command::SplitTabs {
            tab_ids: vec![t1, t2],
            direction: SplitDirection::Horizontal,
        })
        .unwrap();
        core.invoke(Command::TabActivate { tab_id: t1 }).unwrap();
        (core, t1, t2)
    }

    #[test]
    fn split_focus_next_last_zoom_unzoom_on_switch_rotate_and_resize() {
        let (core, t1, t2) = two_pane_core();
        assert_eq!(core.model().active_tab, Some(t1));

        core.invoke(Command::from_name_and_args("split.focus_right", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_tab, Some(t2));

        core.invoke(Command::from_name_and_args("split.last", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_tab, Some(t1));

        core.invoke(Command::from_name_and_args("split.next", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_tab, Some(t2));

        // Resize (`WIN-18`: 5% * count, min 7%) and equalize.
        core.invoke(
            Command::from_name_and_args("split.grow_width", &[("count".to_string(), "2".to_string())]).unwrap(),
        )
        .unwrap();
        let resized = core.model().active_split_group().unwrap().ratios.clone();
        assert!((resized[1] - 0.60).abs() < 1e-3);

        core.invoke(Command::from_name_and_args("split.equalize", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_split_group().unwrap().ratios, vec![0.5, 0.5]);

        // Full-view zoom (`WIN-09`) keeps ratios untouched and unzooms when switching panes (`WIN-14`).
        core.invoke(Command::from_name_and_args("split.zoom", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_split_group().unwrap().zoomed_tab, Some(t2));
        assert_eq!(core.model().active_split_group().unwrap().ratios, vec![0.5, 0.5]);
        core.invoke(Command::from_name_and_args("split.next", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_tab, Some(t1));
        assert_eq!(
            core.model().active_split_group().unwrap().zoomed_tab,
            None,
            "picking another pane ends zoom (WIN-14)"
        );

        // Rotate keeps slot sizes (`WIN-17`).
        core.invoke(Command::from_name_and_args("split.grow_width", &[]).unwrap())
            .unwrap();
        let widths_before: Vec<f32> = split::group_pane_rects(core.model().active_split_group().unwrap())
            .into_iter()
            .map(|(_, r)| r.width)
            .collect();
        core.invoke(Command::from_name_and_args("split.rotate", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_split_group().unwrap().tab_ids, vec![t2, t1]);
        let widths_after: Vec<f32> = split::group_pane_rects(core.model().active_split_group().unwrap())
            .into_iter()
            .map(|(_, r)| r.width)
            .collect();
        assert_eq!(widths_before, widths_after);
    }

    #[test]
    fn split_divide_picker_pinned_mixing_carry_escape_edge_drop_limit_and_close_stay() {
        let core = StubBackend::new_for_external_tabs();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        for url in [
            "https://one.example",
            "https://two.example",
            "https://three.example",
            "https://four.example",
            "https://five.example",
        ] {
            core.invoke(Command::TabNew {
                url: Some(url.to_string()),
                after: None,
                activate: true,
            })
            .unwrap();
        }
        let ids: Vec<TabId> = core.model().tabs.iter().map(|t| t.id).collect();
        core.invoke(Command::TabActivate { tab_id: ids[0] }).unwrap();

        // `WIN-22`: `wg` without a split does nothing.
        core.invoke(Command::from_name_and_args("split.grab", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);

        // Pin `ids[0]` and run `split.vertical` (`<leader>|`, `WIN-01`): opens `"Split Window Right"` picker.
        core.invoke(Command::TabPin { tab_id: Some(ids[0]) }).unwrap();
        core.invoke(Command::from_name_and_args("split.vertical", &[]).unwrap())
            .unwrap();
        let picker = core.state.lock().unwrap().active_picker.clone().expect("split picker");
        assert_eq!(picker.title, "Split Window Right");
        assert_eq!(picker.items[0].title, "Open URL…");
        assert_eq!(picker.items[0].key.as_deref(), Some("/"));

        // Pick `ids[1]` (unpinned) -> pins `ids[1]` with `"Tab pinned to join this split."` toast (`WIN-10`)!
        let item_id = format!("tab.{}", ids[1].0);
        core.invoke(Command::PickerChoose {
            request_id: Some(picker.id),
            item_id,
        })
        .unwrap();
        assert_eq!(core.model().active_tab, Some(ids[1]));
        assert_eq!(core.model().tab(ids[1]).unwrap().kind, TabKind::Pinned);
        assert!(
            core.model()
                .toasts
                .iter()
                .any(|t| t.message == toasts::messages::SPLIT_PINNED)
        );

        // Divide ONLY `ids[1]` below (`split.horizontal`) with `ids[2]` (`WIN-01`, `WIN-10`).
        core.invoke(Command::from_name_and_args("split.horizontal", &[]).unwrap())
            .unwrap();
        let below_picker = core.state.lock().unwrap().active_picker.clone().unwrap();
        assert_eq!(below_picker.title, "Split Window Below");
        core.invoke(Command::PickerChoose {
            request_id: Some(below_picker.id),
            item_id: format!("tab.{}", ids[2].0),
        })
        .unwrap();
        let rects = split::group_pane_rects(core.model().active_split_group().unwrap());
        let r0 = rects.iter().find(|(id, _)| *id == ids[0]).unwrap().1;
        let r2 = rects.iter().find(|(id, _)| *id == ids[2]).unwrap().1;
        assert!((r0.width - 0.5).abs() < 1e-3 && (r0.height - 1.0).abs() < 1e-3);
        assert!((r2.x - 0.5).abs() < 1e-3 && (r2.y - 0.5).abs() < 1e-3);

        // Open Window mode explicitly (`<leader>w<Space>`), press `H` (`split.edge_left`), then `<Esc>` (`WIN-03`, `WIN-04`):
        // `<Esc>` cancels the carry while keeping Window mode open!
        core.invoke(Command::WindowModeOpen).unwrap();
        core.invoke(Command::from_name_and_args("split.edge_left", &[]).unwrap())
            .unwrap();
        assert!(core.model().focus.window_carry.is_some());
        core.invoke(Command::from_name_and_args("window.escape", &[]).unwrap())
            .unwrap();
        assert!(core.model().focus.window_carry.is_none());
        assert_eq!(core.model().input_mode, InputMode::Window);
        core.invoke(Command::from_name_and_args("window.quit", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);

        // Outside Window mode, `wJ` (`split.edge_down`) previews carry and `<CR>` (`window.drop`) moves `ids[2]` to bottom
        // outer edge and closes Window mode (`WIN-03`, `WIN-07`, `WIN-13`).
        core.invoke(Command::from_name_and_args("split.edge_down", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().input_mode, InputMode::Window);
        core.invoke(Command::WindowModeDrop).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
        let rects_after_edge = split::group_pane_rects(core.model().active_split_group().unwrap());
        let bottom = rects_after_edge.iter().find(|(id, _)| *id == ids[2]).unwrap().1;
        assert!((bottom.width - 1.0).abs() < 1e-3);

        // Add 4th pane via `wa` (`split.add` -> carry -> `<CR>`, `WIN-02`), then verify 5th refuses with limit toast
        // and NO extra blank tabs (`WIN-16`)!
        core.invoke(Command::from_name_and_args("split.add", &[]).unwrap())
            .unwrap();
        let add_picker = core.state.lock().unwrap().active_picker.clone().unwrap();
        assert_eq!(add_picker.title, "Add to Split");
        core.invoke(Command::PickerChoose {
            request_id: Some(add_picker.id),
            item_id: format!("tab.{}", ids[3].0),
        })
        .unwrap();
        assert!(core.model().focus.window_carry.as_ref().is_some_and(|c| c.adding));
        core.invoke(Command::WindowModeDrop).unwrap();
        assert_eq!(core.model().active_split_group().unwrap().tab_ids.len(), 4);

        let tabs_before_limit = core.model().tabs.len();
        core.invoke(Command::from_name_and_args("split.add", &[]).unwrap())
            .unwrap();
        assert_eq!(
            core.model().tabs.len(),
            tabs_before_limit,
            "WIN-16: no stray blank tab opened at 4-pane limit"
        );
        assert!(
            core.model()
                .toasts
                .iter()
                .any(|t| t.message == toasts::messages::SPLIT_VIEW_LIMIT)
        );

        // `wt` (`split.remove_tab`) detaches `ids[3]` and focuses the last-used remaining pane (`ids[2]`, `WIN-19`)!
        core.invoke(Command::Splits(SplitAction::RemoveActiveTab)).unwrap();
        assert_eq!(core.model().active_tab, Some(ids[2]));
        assert_eq!(core.model().active_split_group().unwrap().tab_ids.len(), 3);

        // Closing a pane's tab (`<leader>bd`) stays in the split on the first remaining pane (`WIN-15`)!
        core.invoke(Command::TabClose {
            tab_id: Some(ids[2]),
            force: true,
        })
        .unwrap();
        assert_eq!(core.model().active_tab, Some(ids[0]));
        assert!(core.model().active_split_group().is_some());
    }

    #[test]
    fn panel_row_steps_across_left_and_right_panels_and_returns_with_panes_last() {
        let (core, t1, t2) = two_pane_core();
        let left_panel = SidePanelId::new("zendia.agents");
        let right_panel = SidePanelId::new("zendia.agent");
        core.invoke(Command::PanelSetSide {
            panel_id: left_panel.clone(),
            side: SidePanelSide::Left,
        })
        .unwrap();
        core.invoke(Command::PanelSetSide {
            panel_id: right_panel.clone(),
            side: SidePanelSide::Right,
        })
        .unwrap();
        core.invoke(Command::PanelOpen {
            panel_id: left_panel.clone(),
            focus: false,
        })
        .unwrap();
        core.invoke(Command::PanelOpen {
            panel_id: right_panel.clone(),
            focus: false,
        })
        .unwrap();

        // Start on left split pane `t1`: `<C-l>` moves to `t2` first (`split panes come first`, `WIN-06`), then next
        // `<C-l>` steps into `right_panel`, and another `<C-l>` at the right end is swallowed!
        assert_eq!(core.model().active_tab, Some(t1));
        core.invoke(Command::from_name_and_args("panes.focus_right", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_tab, Some(t2));
        assert_eq!(core.state.lock().unwrap().vim.focused_panel, None);

        core.invoke(Command::from_name_and_args("panes.focus_right", &[]).unwrap())
            .unwrap();
        assert_eq!(core.state.lock().unwrap().vim.focused_panel, Some(right_panel.clone()));

        core.invoke(Command::from_name_and_args("panes.focus_right", &[]).unwrap())
            .unwrap();
        assert_eq!(core.state.lock().unwrap().vim.focused_panel, Some(right_panel.clone()));

        // `<C-\>` (`panes.last`) jumps back to Page, and `<C-\>` again jumps back to `right_panel`!
        core.invoke(Command::from_name_and_args("panes.last", &[]).unwrap())
            .unwrap();
        assert_eq!(core.state.lock().unwrap().vim.focused_panel, None);
        core.invoke(Command::from_name_and_args("panes.last", &[]).unwrap())
            .unwrap();
        assert_eq!(core.state.lock().unwrap().vim.focused_panel, Some(right_panel));

        // Step left back to Page (`t2`), left to `t1`, then left into `left_panel`!
        core.invoke(Command::from_name_and_args("panes.focus_left", &[]).unwrap())
            .unwrap();
        assert_eq!(core.state.lock().unwrap().vim.focused_panel, None);
        core.invoke(Command::from_name_and_args("panes.focus_left", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_tab, Some(t1));
        core.invoke(Command::from_name_and_args("panes.focus_left", &[]).unwrap())
            .unwrap();
        assert_eq!(core.state.lock().unwrap().vim.focused_panel, Some(left_panel));
    }
}
