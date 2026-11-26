/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! View-local sidebar state: inline rename drafts, active drag-reorder preview, and gallery hover preview.
//!
//! Only transient UI state lives here. Tabs, spaces, essentials, pinned tabs, folders, and notifications live in
//! [`zendia_browser_core`].

use crate::ui::theme::{ResolvedScheme, ResolvedTheme, Rgba, motion};
use freya::prelude::*;
use zendia_browser_core::model::{FolderId, SpaceId, TabId, TabMoveTarget};
use zendia_browser_core::split::DropZone;

/// Where a dragged tab lands when it is released over the page: beside `target_tab`'s pane, on side `zone`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageDrop {
    pub target_tab: TabId,
    pub zone: DropZone,
}

/// Active tab drag-and-drop reorder state kept strictly in the Freya view.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarDragState {
    pub tab_id: TabId,
    pub tab_title: String,
    pub tab_favicon: Option<String>,
    pub pointer_x: f32,
    pub pointer_y: f32,
    pub target: Option<TabMoveTarget>,
    /// Set by the page area while the pointer is over the page; releasing then splits instead of reordering.
    pub page_drop: Option<PageDrop>,
    /// When true, renders a deterministic static drag preview (used by the UI gallery).
    pub is_preview: bool,
}

/// Active folder drag-to-reorder payload `(folder_id, name, pointer_x, pointer_y, target_index)`.
pub type FolderDragTuple = (FolderId, String, f32, f32, Option<usize>);

/// View-local state shared across sidebar components for inline renaming, selection, hover, and drag reordering.
#[derive(Clone, Copy, PartialEq)]
pub struct SidebarViewState {
    pub renaming_tab: State<Option<TabId>>,
    pub renaming_folder: State<Option<FolderId>>,
    pub renaming_space: State<Option<SpaceId>>,
    pub rename_draft: State<String>,
    pub drag_state: State<Option<SidebarDragState>>,
    pub preview_hovered_tab: State<Option<TabId>>,
    pub preview_hovered_separator: State<bool>,
    /// Pointer is currently inside the sidebar frame (`#navigator-toolbox[zen-has-implicit-hover="true"]`).
    pub sidebar_hovered: State<bool>,
    /// Multi-selected vertical tab IDs (`Ctrl+Click` / `Shift+Click`).
    pub selected_tabs: State<Vec<TabId>>,
    /// Anchor tab for `Shift+Click` range selection.
    pub selection_anchor: State<Option<TabId>>,
    /// Tab currently playing its rename commit confirmation pulse (`250ms`).
    pub recently_renamed_tab: State<Option<TabId>>,
    pub rename_pulse_seq: State<u64>,
    /// Live keyboard modifier tracking for pointer clicks (`Ctrl+Click`, `Shift+Click`).
    pub ctrl_held: State<bool>,
    pub shift_held: State<bool>,
    /// Active folder drag-to-reorder state `(folder_id, name, pointer_x, pointer_y, target_index)` (`FLD-05`/`DND-01`).
    pub dragging_folder: State<Option<FolderDragTuple>>,
    /// Target tab highlighted for 300ms center-50% split-view merge during tab drag (`DND-03`).
    pub split_merge_target: State<Option<TabId>>,
    /// Sequence counter cancelling stale 300ms split-merge dwell timers (`DND-03`).
    pub split_dwell_seq: State<u64>,
    /// True when the active tab drag pointer has moved outside window bounds (`DND-04` tear-off preview).
    pub detached_out_of_bounds: State<bool>,
    /// Active space switcher drag-to-reorder state `(space_id, target_index)` (`SPC-01`).
    pub dragging_space: State<Option<(SpaceId, usize)>>,
    /// Spaces whose pinned tabs section is currently collapsed via the SpaceHeader chevron (`SPC-04`).
    pub collapsed_pinned_spaces: State<Vec<SpaceId>>,
    /// Sequence counter cancelling stale 1000ms edge-hover space switch timers during drag (`DND-02`).
    pub edge_switch_seq: State<u64>,
    /// Gallery preview: draw the media controls as if hovered (title, artist and progress shown).
    pub preview_media_expanded: State<bool>,
    /// Compact mode: the hidden sidebar is slid out over the page.
    pub compact_revealed: State<bool>,
    /// Compact mode: the user pinned the sidebar out with `layout.compact_reveal_sidebar`.
    pub compact_pinned: State<bool>,
    /// Compact mode: brief tab-switch flash keeping the sidebar visible (`800ms`).
    pub compact_flash: State<bool>,
    /// Compact mode: no hover-reveal while the sidebar is collapsing after compact mode was turned on.
    pub compact_ignore_next_hover: State<bool>,
    /// Sequence counter cancelling stale sidebar hide timers.
    pub compact_hide_seq: State<u64>,
    /// Sequence counter cancelling stale tab-switch flash timers.
    pub compact_flash_seq: State<u64>,
}

impl SidebarViewState {
    /// Begins inline renaming for `tab_id` seeded with its current display title.
    pub fn start_tab_rename(&self, tab_id: TabId, initial_title: String) {
        let mut folder = self.renaming_folder;
        folder.set_if_modified(None);
        let mut space = self.renaming_space;
        space.set_if_modified(None);
        let mut draft = self.rename_draft;
        draft.set(initial_title);
        let mut tab = self.renaming_tab;
        tab.set(Some(tab_id));
    }

    /// Begins inline renaming for `folder_id` seeded with its current name.
    pub fn start_folder_rename(&self, folder_id: FolderId, initial_name: String) {
        let mut tab = self.renaming_tab;
        tab.set_if_modified(None);
        let mut space = self.renaming_space;
        space.set_if_modified(None);
        let mut draft = self.rename_draft;
        draft.set(initial_name);
        let mut folder = self.renaming_folder;
        folder.set(Some(folder_id));
    }

    /// Begins inline renaming for `space_id` seeded with its current name.
    pub fn start_space_rename(&self, space_id: SpaceId, initial_name: String) {
        let mut tab = self.renaming_tab;
        tab.set_if_modified(None);
        let mut folder = self.renaming_folder;
        folder.set_if_modified(None);
        let mut draft = self.rename_draft;
        draft.set(initial_name);
        let mut space = self.renaming_space;
        space.set(Some(space_id));
    }

    /// Whether an inline tab, folder, or space rename field is currently active.
    pub fn is_renaming(&self) -> bool {
        self.renaming_tab.peek().is_some()
            || self.renaming_folder.peek().is_some()
            || self.renaming_space.peek().is_some()
    }

    /// Cancels any active inline tab, folder, or space rename without invoking a core command.
    pub fn cancel_rename(&self) {
        let mut tab = self.renaming_tab;
        tab.set_if_modified(None);
        let mut folder = self.renaming_folder;
        folder.set_if_modified(None);
        let mut space = self.renaming_space;
        space.set_if_modified(None);
    }

    /// Plays the 250ms confirmation scale pulse on `tab_id` after rename commit (`REN-03`).
    pub fn trigger_rename_pulse(&self, tab_id: TabId) {
        let mut seq_state = self.rename_pulse_seq;
        let next_seq = seq_state.peek().wrapping_add(1);
        seq_state.set(next_seq);
        let mut renamed = self.recently_renamed_tab;
        renamed.set(Some(tab_id));
        spawn(async move {
            timer(motion::TAB_RENAME_PULSE).await;
            if *seq_state.peek() == next_seq {
                renamed.set_if_modified(None);
            }
        });
    }

    /// Whether `tab_id` is currently part of a multi-tab selection.
    pub fn is_tab_selected(&self, tab_id: TabId) -> bool {
        self.selected_tabs.read().contains(&tab_id)
    }

    /// Toggles `tab_id` in the multi-selection set (`Ctrl+Click`), seeding with `active_tab` if starting fresh.
    pub fn toggle_tab_selection(&self, tab_id: TabId, active_tab: Option<TabId>) {
        let mut selected = self.selected_tabs;
        let mut anchor = self.selection_anchor;
        let mut list = selected.peek().clone();
        if list.is_empty()
            && let Some(active) = active_tab
            && active != tab_id
        {
            list.push(active);
        }
        if let Some(pos) = list.iter().position(|id| *id == tab_id) {
            list.remove(pos);
        } else {
            list.push(tab_id);
        }
        selected.set(list);
        anchor.set(Some(tab_id));
    }

    /// Selects a contiguous range from the selection anchor (or `active_tab`) to `tab_id` (`Shift+Click`).
    pub fn select_tab_range(&self, tab_id: TabId, active_tab: Option<TabId>, ordered: &[TabId]) {
        let mut selected = self.selected_tabs;
        let mut anchor_state = self.selection_anchor;
        let anchor = (*anchor_state.peek()).or(active_tab).unwrap_or(tab_id);
        let range = compute_tab_selection_range(ordered, anchor, tab_id);
        selected.set(range);
        if anchor_state.peek().is_none() {
            anchor_state.set(Some(anchor));
        }
    }

    /// Clears the multi-tab selection set.
    pub fn clear_tab_selection(&self) {
        let mut selected = self.selected_tabs;
        if !selected.peek().is_empty() {
            selected.set(Vec::new());
        }
        let mut anchor = self.selection_anchor;
        anchor.set_if_modified(None);
    }

    /// Toggles collapsed pinned-tabs visibility for `space_id` (`SPC-04`).
    pub fn toggle_pinned_collapsed(&self, space_id: SpaceId) {
        let mut list = self.collapsed_pinned_spaces;
        let mut next = list.peek().clone();
        if let Some(pos) = next.iter().position(|id| *id == space_id) {
            next.remove(pos);
        } else {
            next.push(space_id);
        }
        list.set(next);
    }

    /// Whether pinned tabs are collapsed for `space_id` (`SPC-04`).
    pub fn is_pinned_collapsed(&self, space_id: SpaceId) -> bool {
        self.collapsed_pinned_spaces.read().contains(&space_id)
    }

    /// Clears transient drag flags (`split_merge_target`, `detached_out_of_bounds`, dwell timers).
    pub fn clear_drag_extras(&self) {
        let mut merge = self.split_merge_target;
        merge.set_if_modified(None);
        let mut dwell = self.split_dwell_seq;
        let next_dwell = dwell.peek().wrapping_add(1);
        dwell.set(next_dwell);
        let mut detached = self.detached_out_of_bounds;
        detached.set_if_modified(false);
        let mut edge = self.edge_switch_seq;
        let next_edge = edge.peek().wrapping_add(1);
        edge.set(next_edge);
    }

    /// Clears all transient sidebar view state (rename, drag, selection, and gallery preview flags).
    pub fn reset(&self) {
        self.cancel_rename();
        self.clear_tab_selection();
        self.clear_drag_extras();
        let mut drag = self.drag_state;
        drag.set_if_modified(None);
        let mut df = self.dragging_folder;
        df.set_if_modified(None);
        let mut ds = self.dragging_space;
        ds.set_if_modified(None);
        let mut cp = self.collapsed_pinned_spaces;
        if !cp.peek().is_empty() {
            cp.set(Vec::new());
        }
        let mut hovered = self.preview_hovered_tab;
        hovered.set_if_modified(None);
        let mut sep = self.preview_hovered_separator;
        sep.set_if_modified(false);
        let mut sb_hov = self.sidebar_hovered;
        sb_hov.set_if_modified(false);
        let mut renamed = self.recently_renamed_tab;
        renamed.set_if_modified(None);
        let mut ctrl = self.ctrl_held;
        ctrl.set_if_modified(false);
        let mut shift = self.shift_held;
        shift.set_if_modified(false);
        let mut media = self.preview_media_expanded;
        media.set_if_modified(false);
        let mut revealed = self.compact_revealed;
        revealed.set_if_modified(false);
        let mut pinned = self.compact_pinned;
        pinned.set_if_modified(false);
        let mut flash = self.compact_flash;
        flash.set_if_modified(false);
        let mut ignore = self.compact_ignore_next_hover;
        ignore.set_if_modified(false);
    }

    /// Updates the drag drop target while a live drag is in progress.
    pub fn set_drag_target(&self, target: TabMoveTarget) {
        let mut drag = self.drag_state;
        let should_update = drag
            .peek()
            .as_ref()
            .is_some_and(|d| !d.is_preview && d.target != Some(target));
        if should_update && let Some(current) = drag.write().as_mut() {
            current.target = Some(target);
        }
    }
}

/// Computes the inclusive slice of tab IDs between `anchor` and `target` in sidebar display order.
pub fn compute_tab_selection_range(ordered: &[TabId], anchor: TabId, target: TabId) -> Vec<TabId> {
    let Some(a) = ordered.iter().position(|id| *id == anchor) else {
        return vec![target];
    };
    let Some(b) = ordered.iter().position(|id| *id == target) else {
        return vec![target];
    };
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    ordered[lo..=hi].to_vec()
}

/// Installs [`SidebarViewState`] into Freya context for the current window.
pub fn use_provide_sidebar_state() -> SidebarViewState {
    let renaming_tab = use_state(|| None::<TabId>);
    let renaming_folder = use_state(|| None::<FolderId>);
    let renaming_space = use_state(|| None::<SpaceId>);
    let rename_draft = use_state(String::new);
    let drag_state = use_state(|| None::<SidebarDragState>);
    let preview_hovered_tab = use_state(|| None::<TabId>);
    let preview_hovered_separator = use_state(|| false);
    let sidebar_hovered = use_state(|| false);
    let selected_tabs = use_state(Vec::<TabId>::new);
    let selection_anchor = use_state(|| None::<TabId>);
    let recently_renamed_tab = use_state(|| None::<TabId>);
    let rename_pulse_seq = use_state(|| 0u64);
    let ctrl_held = use_state(|| false);
    let shift_held = use_state(|| false);
    let dragging_folder = use_state(|| None::<FolderDragTuple>);
    let split_merge_target = use_state(|| None::<TabId>);
    let split_dwell_seq = use_state(|| 0u64);
    let detached_out_of_bounds = use_state(|| false);
    let dragging_space = use_state(|| None::<(SpaceId, usize)>);
    let collapsed_pinned_spaces = use_state(Vec::<SpaceId>::new);
    let edge_switch_seq = use_state(|| 0u64);
    let preview_media_expanded = use_state(|| false);
    let compact_revealed = use_state(|| false);
    let compact_pinned = use_state(|| false);
    let compact_flash = use_state(|| false);
    let compact_ignore_next_hover = use_state(|| false);
    let compact_hide_seq = use_state(|| 0u64);
    let compact_flash_seq = use_state(|| 0u64);

    use_provide_context(|| SidebarViewState {
        renaming_tab,
        renaming_folder,
        renaming_space,
        rename_draft,
        drag_state,
        preview_hovered_tab,
        preview_hovered_separator,
        sidebar_hovered,
        selected_tabs,
        selection_anchor,
        recently_renamed_tab,
        rename_pulse_seq,
        ctrl_held,
        shift_held,
        dragging_folder,
        split_merge_target,
        split_dwell_seq,
        detached_out_of_bounds,
        dragging_space,
        collapsed_pinned_spaces,
        edge_switch_seq,
        preview_media_expanded,
        compact_revealed,
        compact_pinned,
        compact_flash,
        compact_ignore_next_hover,
        compact_hide_seq,
        compact_flash_seq,
    })
}

/// Consumes [`SidebarViewState`] if installed in this window.
pub fn try_use_sidebar_state() -> Option<SidebarViewState> {
    try_consume_context::<SidebarViewState>()
}

/// Subtle colored badge tint for known sample favicon icon names, matching the crisp colored site favicons in the
/// reference screenshots while falling back to a neutral surface when a tab has no favicon.
pub fn favicon_badge_colors(favicon: Option<&str>, theme: &ResolvedTheme) -> (Color, Color) {
    let (badge_rgba, fg) = match favicon {
        Some("mail") => (Rgba::rgb(42, 166, 214), Color::WHITE),
        Some("code") => (Rgba::rgb(32, 174, 128), Color::WHITE),
        Some("sparkles") | Some("palette") => (Rgba::rgb(228, 72, 134), Color::WHITE),
        Some("flame") => (Rgba::rgb(236, 122, 36), Color::WHITE),
        Some("book") => (Rgba::rgb(52, 122, 232), Color::WHITE),
        Some("terminal") => (Rgba::rgb(112, 108, 212), Color::WHITE),
        Some("compass") => (Rgba::rgb(56, 152, 206), Color::WHITE),
        _ => {
            let bg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.10),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.08),
            };
            let icon_fg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.72).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(30, 34, 48, 0.72).to_color(),
            };
            return (bg.to_color(), icon_fg);
        }
    };
    (badge_rgba.to_color(), fg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_tab_selection_range_handles_forward_and_backward() {
        let ordered = [TabId(1), TabId(2), TabId(3), TabId(4), TabId(5)];
        assert_eq!(
            compute_tab_selection_range(&ordered, TabId(2), TabId(4)),
            vec![TabId(2), TabId(3), TabId(4)]
        );
        assert_eq!(
            compute_tab_selection_range(&ordered, TabId(4), TabId(2)),
            vec![TabId(2), TabId(3), TabId(4)]
        );
        assert_eq!(
            compute_tab_selection_range(&ordered, TabId(99), TabId(3)),
            vec![TabId(3)]
        );
    }
}
