/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for [`crate::split_actions`].

use super::StubBackend;
use crate::BrowserCore;
use crate::commands::{Command, CommandResult};
use crate::events::ViewRequest;
use crate::focus::{self, FocusSurface, FocusTarget, PaneDirection};
use crate::model::{SplitDirection, SplitGroup, TabId};
use crate::picker::{PickerItem, PickerLayout, PickerRequest};
use crate::split::{self, MIN_PANE_FRACTION};
use crate::split_actions::{RESIZE_STEP, ResizeKind, SplitAction};

impl StubBackend {
    pub(super) fn handle_split_actions(&self, action: SplitAction) -> CommandResult {
        match action {
            SplitAction::Focus { direction } => self.handle_split_focus(direction),
            SplitAction::PanesLast | SplitAction::Last => self.handle_split_last(),
            SplitAction::Next => self.handle_split_next(),
            SplitAction::Pick => self.handle_split_pick(),
            SplitAction::RemoveActiveTab => {
                let tab_id = self.resolve_target_tab(None, "split.remove_tab")?;
                self.invoke(Command::SplitRemoveTab { tab_id })
            }
            SplitAction::Zoom => self.handle_split_zoom(),
            SplitAction::Equalize => self.handle_split_equalize(),
            SplitAction::Resize { kind } => self.handle_split_resize(kind),
            SplitAction::Swap => self.handle_split_swap(),
            SplitAction::Rotate { reverse } => self.handle_split_rotate(reverse),
            SplitAction::Edge { direction } => self.handle_split_edge(direction),
            SplitAction::Grab => self.handle_split_grab(),
        }
    }

    fn activate_split_pane(&self, tab_id: TabId) -> CommandResult {
        if let Some(source) = self.current_tab_source() {
            source.activate_tab(tab_id)?;
        } else {
            self.feed_tab_activated(Some(tab_id));
        }
        let updated_focus = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.focus.surface == Some(FocusSurface::Window) {
                state.model.focus.cursor = Some(FocusTarget::Pane(tab_id));
                Some(state.model.focus.clone())
            } else {
                None
            }
        };
        if let Some(focus) = updated_focus {
            self.bus.publish(crate::events::Event::FocusChanged { focus });
        }
        Ok(())
    }

    fn handle_split_focus(&self, direction: PaneDirection) -> CommandResult {
        // If carrying a pane in window mode, directional keys move the landing preview instead.
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
            self.bus.publish(crate::events::Event::FocusChanged { focus });
            return Ok(());
        }

        let neighbor = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(active) = state.model.active_tab else {
                return Ok(());
            };
            let Some(group) = state.model.active_split_group() else {
                return Ok(());
            };
            find_neighbor_pane(group, active, direction)
        };
        if let Some(target) = neighbor {
            self.activate_split_pane(target)?;
        }
        Ok(())
    }

    fn handle_split_next(&self) -> CommandResult {
        let next = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(active) = state.model.active_tab else {
                return Ok(());
            };
            let Some(group) = state.model.active_split_group() else {
                return Ok(());
            };
            let Some(pos) = group.tab_ids.iter().position(|id| *id == active) else {
                return Ok(());
            };
            group.tab_ids.get((pos + 1) % group.tab_ids.len()).copied()
        };
        if let Some(target) = next {
            self.activate_split_pane(target)?;
        }
        Ok(())
    }

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
        }
        Ok(())
    }

    fn handle_split_pick(&self) -> CommandResult {
        let items: Vec<PickerItem> = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(group) = state.model.active_split_group() else {
                return Ok(());
            };
            group
                .tab_ids
                .iter()
                .filter_map(|tab_id| state.model.tab(*tab_id))
                .map(|tab| {
                    PickerItem::new(tab.id.0.to_string(), tab.display_title(), "tab.activate")
                        .detail(tab.url.clone())
                        .arg("tab_id", tab.id.0.to_string())
                })
                .collect()
        };
        if items.is_empty() {
            return Ok(());
        }
        let request = PickerRequest::new("split-panes", "Pick Window", PickerLayout::List, items);
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.active_picker = Some(request.clone());
        }
        self.request_view(ViewRequest::PickerOpen { request });
        Ok(())
    }

    fn handle_split_zoom(&self) -> CommandResult {
        let plan: Option<(crate::model::SplitGroupId, Vec<f32>)> = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let active = state.model.active_tab;
            let group = state.model.active_split_group().cloned();
            match (active, group) {
                (Some(tab_id), Some(group)) if group.tab_ids.len() >= 2 => {
                    if let Some(saved) = state.vim.zoomed_splits.remove(&group.id) {
                        Some((group.id, saved))
                    } else {
                        let pos = group.tab_ids.iter().position(|id| *id == tab_id).unwrap_or(0);
                        let count = group.tab_ids.len();
                        let saved = split::normalized_ratios(group.direction, count, &group.ratios);
                        state.vim.zoomed_splits.insert(group.id, saved);
                        let zoomed = if matches!(group.direction, SplitDirection::Grid) && count >= 3 {
                            let col = if pos < 2 {
                                1.0 - MIN_PANE_FRACTION
                            } else {
                                MIN_PANE_FRACTION
                            };
                            let row = if pos == 0 || pos == 2 {
                                1.0 - MIN_PANE_FRACTION
                            } else {
                                MIN_PANE_FRACTION
                            };
                            vec![col, row]
                        } else {
                            let other = MIN_PANE_FRACTION.min(0.4 / (count - 1) as f32);
                            let main = (1.0 - other * (count - 1) as f32).max(MIN_PANE_FRACTION);
                            (0..count).map(|i| if i == pos { main } else { other }).collect()
                        };
                        Some((group.id, zoomed))
                    }
                }
                _ => None,
            }
        };
        if let Some((group_id, ratios)) = plan {
            self.invoke(Command::SplitResize { group_id, ratios })?;
        }
        Ok(())
    }

    fn handle_split_equalize(&self) -> CommandResult {
        let plan = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let group = state.model.active_split_group().cloned();
            group.map(|g| {
                state.vim.zoomed_splits.remove(&g.id);
                let ratios = split::default_ratios(g.direction, g.tab_ids.len());
                (g.id, ratios)
            })
        };
        if let Some((group_id, ratios)) = plan {
            self.invoke(Command::SplitResize { group_id, ratios })?;
        }
        Ok(())
    }

    fn handle_split_resize(&self, kind: ResizeKind) -> CommandResult {
        let plan = (|| {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let active = state.model.active_tab?;
            let group = state.model.active_split_group()?.clone();
            if group.tab_ids.len() < 2 {
                return None;
            }
            let count = group.tab_ids.len();
            let pos = group.tab_ids.iter().position(|id| *id == active)?;
            let ratios = split::normalized_ratios(group.direction, count, &group.ratios);
            let next = compute_resized_ratios(group.direction, count, pos, &ratios, kind)?;
            state.vim.zoomed_splits.remove(&group.id);
            Some((group.id, next))
        })();
        if let Some((group_id, ratios)) = plan {
            self.invoke(Command::SplitResize { group_id, ratios })?;
        }
        Ok(())
    }

    fn handle_split_swap(&self) -> CommandResult {
        let plan = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(active) = state.model.active_tab else {
                return Ok(());
            };
            let Some(group) = state.model.active_split_group() else {
                return Ok(());
            };
            let Some(other) = last_other_pane(group, active, &state.vim.tab_recency) else {
                return Ok(());
            };
            let mut tab_ids = group.tab_ids.clone();
            let a = tab_ids.iter().position(|id| *id == active);
            let b = tab_ids.iter().position(|id| *id == other);
            match (a, b) {
                (Some(i), Some(j)) => {
                    tab_ids.swap(i, j);
                    Some((tab_ids, group.direction))
                }
                _ => None,
            }
        };
        if let Some((tab_ids, direction)) = plan {
            self.invoke(Command::SplitTabs { tab_ids, direction })?;
        }
        Ok(())
    }

    fn handle_split_rotate(&self, reverse: bool) -> CommandResult {
        let plan = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(group) = state.model.active_split_group() else {
                return Ok(());
            };
            if group.tab_ids.len() < 2 {
                return Ok(());
            }
            let mut tab_ids = group.tab_ids.clone();
            if reverse {
                tab_ids.rotate_left(1);
            } else {
                tab_ids.rotate_right(1);
            }
            Some((tab_ids, group.direction))
        };
        if let Some((tab_ids, direction)) = plan {
            self.invoke(Command::SplitTabs { tab_ids, direction })?;
        }
        Ok(())
    }

    fn handle_split_edge(&self, direction: PaneDirection) -> CommandResult {
        let plan = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let active = state.model.active_tab;
            let group = state.model.active_split_group();
            match (active, group) {
                (Some(tab_id), Some(group)) => focus::move_pane_layout(group, tab_id, direction),
                _ => None,
            }
        };
        if let Some((tab_ids, direction)) = plan {
            self.invoke(Command::SplitTabs { tab_ids, direction })?;
        }
        Ok(())
    }

    fn handle_split_grab(&self) -> CommandResult {
        let in_window_mode = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.focus.surface == Some(FocusSurface::Window)
        };
        if !in_window_mode {
            self.invoke(Command::WindowModeOpen)?;
        }
        self.invoke(Command::WindowModeCarryToggle)
    }
}

fn last_other_pane(group: &SplitGroup, active: TabId, recency: &[TabId]) -> Option<TabId> {
    recency
        .iter()
        .rev()
        .copied()
        .find(|id| *id != active && group.tab_ids.contains(id))
        .or_else(|| group.tab_ids.iter().copied().find(|id| *id != active))
}

/// Finds the nearest neighboring pane in `direction` from `active`'s rectangle.
fn find_neighbor_pane(group: &SplitGroup, active: TabId, direction: PaneDirection) -> Option<TabId> {
    let rects = split::pane_rects(group.direction, group.tab_ids.len(), &group.ratios);
    let idx = group.tab_ids.iter().position(|id| *id == active)?;
    let origin = *rects.get(idx)?;
    let (dx, dy) = direction.delta();
    let ox = origin.x + origin.width * 0.5;
    let oy = origin.y + origin.height * 0.5;

    let mut best: Option<(TabId, f32)> = None;
    for (i, (&tab_id, rect)) in group.tab_ids.iter().zip(rects.iter()).enumerate() {
        if i == idx {
            continue;
        }
        let cx = rect.x + rect.width * 0.5;
        let cy = rect.y + rect.height * 0.5;
        let vx = cx - ox;
        let vy = cy - oy;
        let forward = vx * dx + vy * dy;
        if forward <= 0.01 {
            continue;
        }
        let cross = (vx * dy - vy * dx).abs();
        let score = forward + cross * 2.0;
        if best.is_none_or(|(_, best_score)| score < best_score) {
            best = Some((tab_id, score));
        }
    }
    best.map(|(id, _)| id)
}

fn compute_resized_ratios(
    direction: SplitDirection,
    count: usize,
    pos: usize,
    ratios: &[f32],
    kind: ResizeKind,
) -> Option<Vec<f32>> {
    let delta = match kind {
        ResizeKind::GrowWidth | ResizeKind::GrowHeight => RESIZE_STEP,
        ResizeKind::ShrinkWidth | ResizeKind::ShrinkHeight => -RESIZE_STEP,
    };
    let is_width = matches!(kind, ResizeKind::GrowWidth | ResizeKind::ShrinkWidth);
    match direction {
        SplitDirection::Grid if count >= 3 => {
            let mut next = ratios.to_vec();
            if is_width {
                let sign = if pos < 2 { 1.0 } else { -1.0 };
                next[0] = (next[0] + sign * delta).clamp(MIN_PANE_FRACTION, 1.0 - MIN_PANE_FRACTION);
            } else {
                let sign = if pos == 0 || pos == 2 { 1.0 } else { -1.0 };
                next[1] = (next[1] + sign * delta).clamp(MIN_PANE_FRACTION, 1.0 - MIN_PANE_FRACTION);
            }
            Some(next)
        }
        SplitDirection::Vertical => {
            if is_width {
                return None;
            }
            let handle_idx = if pos + 1 < count { pos } else { pos - 1 };
            let sign = if pos + 1 < count { 1.0 } else { -1.0 };
            let cur_pos: f32 = ratios[..=handle_idx].iter().sum();
            Some(split::resize_ratios(
                direction,
                count,
                ratios,
                handle_idx,
                cur_pos + sign * delta,
            ))
        }
        SplitDirection::Horizontal | SplitDirection::Grid => {
            if !is_width {
                return None;
            }
            let handle_idx = if pos + 1 < count { pos } else { pos - 1 };
            let sign = if pos + 1 < count { 1.0 } else { -1.0 };
            let cur_pos: f32 = ratios[..=handle_idx].iter().sum();
            Some(split::resize_ratios(
                direction,
                count,
                ratios,
                handle_idx,
                cur_pos + sign * delta,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn split_focus_next_last_swap_rotate_and_resize() {
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

        // Resize and equalize.
        core.invoke(Command::from_name_and_args("split.grow_width", &[]).unwrap())
            .unwrap();
        let resized = core.model().active_split_group().unwrap().ratios.clone();
        assert!(resized[1] > 0.5);

        core.invoke(Command::from_name_and_args("split.equalize", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_split_group().unwrap().ratios, vec![0.5, 0.5]);

        // Zoom and unzoom.
        core.invoke(Command::from_name_and_args("split.zoom", &[]).unwrap())
            .unwrap();
        assert!(core.model().active_split_group().unwrap().ratios[1] > 0.7);
        core.invoke(Command::from_name_and_args("split.zoom", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_split_group().unwrap().ratios, vec![0.5, 0.5]);

        // Rotate and swap.
        core.invoke(Command::from_name_and_args("split.rotate", &[]).unwrap())
            .unwrap();
        assert_eq!(core.model().active_split_group().unwrap().tab_ids, vec![t2, t1]);
    }
}
