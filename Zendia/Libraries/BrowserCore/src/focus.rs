/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Core focus model for vim focus surfaces: sidebar explorer, toolbar focus, hint labels, and window mode.
//!
//! Targets are always core identifiers (tabs, folders, split groups, toolbar contribution ids, pane tab ids) rather
//! than view-local widget ids, so Freya, headless runners, and tests share the same traversal, selection, hint-label,
//! and split-pane landing rules.

use crate::contributions::{ContributionId, ToolbarArea, ToolbarItem};
use crate::model::{BrowserModel, FolderId, SidebarSide, SplitDirection, SplitGroup, SplitGroupId, TabId};
use crate::split::{self, DropZone, PaneRect};

/// Zendia's hint-label alphabet (matches `ZendiaPicker.sys.mjs`: `j` and `k` are omitted because they move cursors).
pub const HINT_ALPHABET: &str = "asdfghlqweruiopzxcvbnmty";

/// Which vim focus surface is currently active.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FocusSurface {
    Explorer,
    Toolbar,
    Window,
}

/// A core-owned target addressed by explorer focus, toolbar focus, hint labels, or window mode.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FocusTarget {
    Essential(TabId),
    PinnedTab(TabId),
    Folder(FolderId),
    Tab(TabId),
    SplitGroup(SplitGroupId),
    SidebarFooter(ContributionId),
    ToolbarItem(ContributionId),
    Pane(TabId),
}

/// Cursor motion within explorer or toolbar focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FocusDirection {
    Next,
    Previous,
    First,
    Last,
    Into,
    Out,
}

impl FocusDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Next => "next",
            Self::Previous => "previous",
            Self::First => "first",
            Self::Last => "last",
            Self::Into => "into",
            Self::Out => "out",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "next" | "down" | "right" => Some(Self::Next),
            "previous" | "prev" | "up" | "left" => Some(Self::Previous),
            "first" | "start" | "top" => Some(Self::First),
            "last" | "end" | "bottom" => Some(Self::Last),
            "into" | "in" | "expand" => Some(Self::Into),
            "out" | "parent" | "collapse" => Some(Self::Out),
            _ => None,
        }
    }
}

/// Spatial direction for moving or focusing panes in window mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaneDirection {
    Left,
    Right,
    Up,
    Down,
}

impl PaneDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "left" | "h" => Some(Self::Left),
            "right" | "l" => Some(Self::Right),
            "up" | "top" | "k" => Some(Self::Up),
            "down" | "bottom" | "j" => Some(Self::Down),
            _ => None,
        }
    }

    pub fn delta(self) -> (f32, f32) {
        match self {
            Self::Left => (-1.0, 0.0),
            Self::Right => (1.0, 0.0),
            Self::Up => (0.0, -1.0),
            Self::Down => (0.0, 1.0),
        }
    }

    pub fn drop_zone(self) -> DropZone {
        match self {
            Self::Left => DropZone::Left,
            Self::Right => DropZone::Right,
            Self::Up => DropZone::Top,
            Self::Down => DropZone::Bottom,
        }
    }
}

/// A single target-to-label binding in an active hint session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HintEntry {
    pub target: FocusTarget,
    pub label: String,
}

/// What picking a hint label does once matched (`EXP-06`, `TB-02`, `NAV-04`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HintMatchAction {
    /// Move the active surface's cursor to the matched target without activating or closing (`s` in explorer/toolbar).
    #[default]
    MoveCursor,
    /// Activate the matched sidebar row's last-used tab and close any transient sidebar (`<leader>bj` `tab.pick`).
    ActivateTabAndClose,
    /// Focus the matched split pane (`wf` / pane hints).
    ActivatePane,
}

/// Active jump-hint labels over a focus surface, narrowed as keys are typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HintSession {
    pub surface: FocusSurface,
    pub entries: Vec<HintEntry>,
    pub typed: String,
    pub action: HintMatchAction,
}

/// Result of feeding one key into a [`HintSession`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HintOutcome {
    Matched(FocusTarget),
    Narrowed,
    NoMatch,
}

impl HintSession {
    /// Creates a hint session assigning equal-length labels from [`HINT_ALPHABET`] to `targets`.
    pub fn new(surface: FocusSurface, targets: Vec<FocusTarget>) -> Self {
        let labels = hint_labels(targets.len());
        let entries = targets
            .into_iter()
            .zip(labels)
            .map(|(target, label)| HintEntry { target, label })
            .collect();
        Self {
            surface,
            entries,
            typed: String::new(),
            action: HintMatchAction::MoveCursor,
        }
    }

    /// Attaches a custom [`HintMatchAction`] to run when a label matches.
    pub fn with_action(mut self, action: HintMatchAction) -> Self {
        self.action = action;
        self
    }

    /// Creates a hint session with labels of at least `min_len` characters (useful for prefix-narrowing previews and
    /// tests).
    pub fn with_min_len(surface: FocusSurface, targets: Vec<FocusTarget>, min_len: usize) -> Self {
        let labels = hint_labels_min_len(targets.len(), min_len);
        let entries = targets
            .into_iter()
            .zip(labels)
            .map(|(target, label)| HintEntry { target, label })
            .collect();
        Self {
            surface,
            entries,
            typed: String::new(),
            action: HintMatchAction::MoveCursor,
        }
    }

    /// Entries still visible under the currently typed prefix.
    pub fn visible_entries(&self) -> Vec<&HintEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.label.starts_with(self.typed.as_str()))
            .collect()
    }

    /// Returns `(matched_prefix, remaining_suffix)` for `target` when its label matches the typed prefix.
    pub fn label_parts(&self, target: &FocusTarget) -> Option<(&str, &str)> {
        let entry = self.entries.iter().find(|entry| &entry.target == target)?;
        let suffix = entry.label.strip_prefix(self.typed.as_str())?;
        Some((&entry.label[..self.typed.len()], suffix))
    }

    /// Feeds one typed character into the session, returning whether a target matched, candidates narrowed, or nothing
    /// matched.
    pub fn type_char(&mut self, ch: char) -> HintOutcome {
        let lower = ch.to_ascii_lowercase();
        if !lower.is_ascii_alphanumeric() {
            return HintOutcome::NoMatch;
        }
        self.typed.push(lower);
        if let Some(exact) = self.entries.iter().find(|entry| entry.label == self.typed) {
            return HintOutcome::Matched(exact.target.clone());
        }
        if self.entries.iter().any(|entry| entry.label.starts_with(&self.typed)) {
            HintOutcome::Narrowed
        } else {
            HintOutcome::NoMatch
        }
    }
}

/// Where a carried pane can land in window mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LandingKind {
    PaneSide { target_tab: TabId, zone: DropZone },
    PaneSwap { target_tab: TabId },
    SplitEdge { zone: DropZone },
}

/// One landing candidate in window mode, with its preview area (in 0..1 page-card fractions) and caption.
#[derive(Clone, Debug, PartialEq)]
pub struct LandingTarget {
    pub kind: LandingKind,
    pub rect: PaneRect,
    pub label: String,
}

/// Active carry state in window mode: a pane (or tab being added) picked up and hovering over a landing target.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowCarryState {
    pub carried_tab: TabId,
    /// True when carrying an unsplit tab into the split (`wa` / `"Add to Split"`), false when moving an existing pane.
    pub adding: bool,
    pub targets: Vec<LandingTarget>,
    pub selected: usize,
}

impl WindowCarryState {
    pub fn selected_target(&self) -> Option<&LandingTarget> {
        self.targets.get(self.selected)
    }
}

/// Core-owned state of the active vim focus surface (explorer, toolbar, or window mode).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FocusState {
    pub surface: Option<FocusSurface>,
    pub cursor: Option<FocusTarget>,
    /// Remembered row index in `explorer_targets` so closing the cursor's tab keeps the row position (`EXP-05`).
    pub cursor_index: usize,
    /// Remembered button index on the sidebar footer (`EXP-04`).
    pub footer_index: usize,
    /// Rows toggled individually with `<Tab>`, unioned with `visual_anchor` to form `selection` (`EXP-14`).
    pub picked: Vec<FocusTarget>,
    pub selection: Vec<FocusTarget>,
    pub visual_anchor: Option<FocusTarget>,
    pub hints: Option<HintSession>,
    pub window_carry: Option<WindowCarryState>,
    /// True when Window mode was opened solely to carry a pane/tab and closes once the drop or cancel finishes (`WIN-07`).
    pub close_after_carry: bool,
    /// True when toolbar focus was entered directly from explorer so compact mode keeps the sidebar out (`VimParity.md:658`).
    pub toolbar_from_sidebar: bool,
    /// True when explorer or window mode was opened transiently for hints (`<leader>bj` or `wf`) and closes once done (`NAV-04`, `WIN-05`).
    pub transient_for_hints: bool,
    /// Visible row capacity reported by `SidebarFrame` for dynamic `<C-d>` / `<C-u>` half-page jumps (`EXP-15`).
    pub explorer_visible_rows: Option<usize>,
    /// Visible `[start, end)` slice within `explorer_targets` reported by `SidebarFrame` for `s` / `<leader>bj` (`EXP-06`).
    pub explorer_visible_range: Option<(usize, usize)>,
}

impl FocusState {
    pub fn is_cursor(&self, target: &FocusTarget) -> bool {
        self.cursor.as_ref() == Some(target)
    }

    pub fn is_selected(&self, target: &FocusTarget) -> bool {
        self.selection.iter().any(|item| item == target)
    }

    pub fn on_footer(&self) -> bool {
        matches!(self.cursor, Some(FocusTarget::SidebarFooter(_)))
    }

    /// Recomputes `selection` as the ordered union of `picked` and any active `visual_anchor` range (`EXP-14`).
    pub fn sync_selection(&mut self, rows: &[FocusTarget]) {
        self.picked.retain(|item| rows.contains(item));
        if let Some(ref anchor) = self.visual_anchor
            && !rows.contains(anchor)
        {
            self.visual_anchor = None;
        }
        let range = match (&self.visual_anchor, &self.cursor) {
            (Some(anchor), Some(cursor)) if !matches!(cursor, FocusTarget::SidebarFooter(_)) => {
                visual_range(rows, anchor, cursor)
            }
            _ => Vec::new(),
        };
        let mut union = Vec::new();
        for row in rows {
            if self.picked.contains(row) || range.contains(row) {
                union.push(row.clone());
            }
        }
        self.selection = union;
    }

    /// Clears both `<Tab>` picks and `V` visual range, returning `true` if anything was selected (`EXP-12`, `EXP-17`).
    pub fn clear_selection(&mut self) -> bool {
        let had = self.visual_anchor.is_some() || !self.picked.is_empty() || !self.selection.is_empty();
        self.visual_anchor = None;
        self.picked.clear();
        self.selection.clear();
        had
    }

    pub fn clear(&mut self) {
        let explorer_visible_rows = self.explorer_visible_rows;
        let explorer_visible_range = self.explorer_visible_range;
        *self = Self {
            explorer_visible_rows,
            explorer_visible_range,
            ..Self::default()
        };
    }
}

/// Generates `count` equal-length, prefix-free hint labels using [`HINT_ALPHABET`].
pub fn hint_labels(count: usize) -> Vec<String> {
    hint_labels_min_len(count, 1)
}

/// Generates `count` equal-length, prefix-free hint labels of at least `min_len` characters using [`HINT_ALPHABET`].
pub fn hint_labels_min_len(count: usize, min_len: usize) -> Vec<String> {
    if count == 0 {
        return Vec::new();
    }
    let alphabet: Vec<char> = HINT_ALPHABET.chars().collect();
    let mut len = min_len.max(1);
    let mut labels: Vec<String> = alphabet.iter().map(|ch| ch.to_string()).collect();
    let mut current_len = 1;
    while labels.len() < count || current_len < len {
        let mut next = Vec::with_capacity(labels.len() * alphabet.len());
        for prefix in &labels {
            for &ch in &alphabet {
                let mut combined = String::with_capacity(prefix.len() + 1);
                combined.push_str(prefix);
                combined.push(ch);
                next.push(combined);
            }
        }
        labels = next;
        current_len += 1;
        len = len.max(current_len);
    }
    labels.truncate(count);
    labels
}

fn folder_has_collapsed_ancestor(model: &BrowserModel, folder_id: FolderId) -> bool {
    let mut cur = folder_id;
    for _ in 0..5 {
        let Some(&parent_id) = model.folder_parents.get(&cur) else {
            return false;
        };
        if parent_id == cur {
            return false;
        }
        if model.folder(parent_id).is_some_and(|parent| parent.collapsed) {
            return true;
        }
        cur = parent_id;
    }
    false
}

/// Ordered row targets the sidebar explorer walks (`ZendiaExplorer.explorerRows`), matching the exact drawn order in
/// `SidebarFrame`: essentials, visible pinned tabs and folders (with visible child tabs), then normal tabs with each
/// split group collapsed into a single row where its first tab sits. Sidebar footer buttons are separate (`EXP-04`).
pub fn explorer_targets(model: &BrowserModel) -> Vec<FocusTarget> {
    let mut out = Vec::new();
    for tab in model.essential_tabs() {
        out.push(FocusTarget::Essential(tab.id));
    }

    if let Some(space_id) = model.active_space {
        let pinned_collapsed = model.collapsed_pinned_spaces.contains(&space_id);
        if !pinned_collapsed {
            for tab in model.pinned_tabs_in_space(space_id) {
                out.push(FocusTarget::PinnedTab(tab.id));
            }
            for folder in model.folders_in_space(space_id) {
                if folder_has_collapsed_ancestor(model, folder.id) {
                    continue;
                }
                out.push(FocusTarget::Folder(folder.id));
                for tab in model.visible_folder_tabs(folder.id) {
                    out.push(FocusTarget::Tab(tab.id));
                }
            }
        }
        let mut seen_groups: Vec<SplitGroupId> = Vec::new();
        for tab in model.normal_tabs_in_space(space_id) {
            if let Some(group) = model.split_group_for_tab(tab.id) {
                if seen_groups.contains(&group.id) {
                    continue;
                }
                seen_groups.push(group.id);
                out.push(FocusTarget::SplitGroup(group.id));
            } else {
                out.push(FocusTarget::Tab(tab.id));
            }
        }
    } else {
        for tab in &model.tabs {
            if model.is_glance_tab(tab.id) {
                continue;
            }
            let target = FocusTarget::Tab(tab.id);
            if !out.contains(&FocusTarget::Essential(tab.id)) && !out.contains(&target) {
                out.push(target);
            }
        }
    }

    out
}

/// Ordered sidebar footer buttons (`#zen-sidebar-foot-buttons`) in visual left-to-right order (`EXP-04`).
pub fn explorer_footer_targets(model: &BrowserModel) -> Vec<FocusTarget> {
    let mut foot_items: Vec<&ToolbarItem> = model
        .contributions
        .toolbar_items
        .iter()
        .filter(|item| item.visible && model.layout.toolbar_area_of(item) == Some(ToolbarArea::SidebarBottom))
        .collect();
    foot_items.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
    if model.layout.sidebar_side == SidebarSide::Right && foot_items.len() > 1 {
        // Right sidebar reverses `#zen-sidebar-foot-buttons`: `[+] [Chat] [Agents] ... [Library]`.
        let first = foot_items.remove(0);
        foot_items.reverse();
        foot_items.push(first);
    }
    foot_items
        .into_iter()
        .map(|item| FocusTarget::SidebarFooter(item.id.clone()))
        .collect()
}

/// Resolves the exact explorer row representing `tab_id` if visible (`None` if hidden or not in this space).
pub fn explorer_row_for_tab(model: &BrowserModel, tab_id: TabId) -> Option<FocusTarget> {
    let targets = explorer_targets(model);
    if let Some(group) = model.split_group_for_tab(tab_id) {
        let group_target = FocusTarget::SplitGroup(group.id);
        if targets.contains(&group_target) {
            return Some(group_target);
        }
    }
    for candidate in [
        FocusTarget::Essential(tab_id),
        FocusTarget::PinnedTab(tab_id),
        FocusTarget::Tab(tab_id),
    ] {
        if targets.contains(&candidate) {
            return Some(candidate);
        }
    }
    if let Some(folder_id) = model.tab(tab_id).and_then(|tab| tab.folder_id) {
        let folder_target = FocusTarget::Folder(folder_id);
        if targets.contains(&folder_target) {
            return Some(folder_target);
        }
    }
    None
}

/// Resolves the explorer target that represents `tab_id` (falling back to the first row when `tab_id` is absent).
pub fn explorer_target_for_tab(model: &BrowserModel, tab_id: TabId) -> Option<FocusTarget> {
    explorer_row_for_tab(model, tab_id).or_else(|| explorer_targets(model).into_iter().next())
}

/// Ordered targets toolbar focus walks, matching the reading order (top-to-bottom, left-to-right) of the active
/// [`crate::model::ToolbarLayout`].
pub fn toolbar_targets(model: &BrowserModel) -> Vec<FocusTarget> {
    let layout = &model.layout;
    let items = &model.contributions.toolbar_items;

    let collect_sorted = |area: ToolbarArea| -> Vec<&ToolbarItem> {
        let mut bucket: Vec<&ToolbarItem> = items
            .iter()
            .filter(|item| item.visible && layout.toolbar_area_of(item) == Some(area))
            .collect();
        bucket.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
        bucket
    };

    let mut ordered: Vec<&ToolbarItem> = Vec::new();
    if layout.has_top_toolbar() {
        // The top bar reads left to right. Buttons kept at the top of the sidebar come before it when the sidebar is
        // on the left, and after it when the sidebar is on the right.
        let in_sidebar = collect_sorted(ToolbarArea::SidebarTopStart);
        if layout.sidebar_side == SidebarSide::Left {
            ordered.extend(in_sidebar.iter().copied());
        }
        ordered.extend(collect_sorted(ToolbarArea::TopBarStart));
        ordered.extend(collect_sorted(ToolbarArea::TopBarCenter));
        ordered.extend(collect_sorted(ToolbarArea::TopBarEnd));
        if layout.sidebar_side == SidebarSide::Right {
            ordered.extend(in_sidebar);
        }
    } else if layout.has_compact_top_bar()
        && model.focus.surface == Some(FocusSurface::Toolbar)
        && !model.focus.toolbar_from_sidebar
    {
        // Opened from the page in single-toolbar compact mode: only the compact top bar is shown (`ZendiaToolbar.sys.mjs:130`).
        ordered.extend(collect_sorted(ToolbarArea::TopBarEnd));
    } else {
        ordered.extend(collect_sorted(ToolbarArea::SidebarTopStart));
        ordered.extend(collect_sorted(ToolbarArea::SidebarTopEnd));
        ordered.extend(collect_sorted(ToolbarArea::SidebarUrlBar));
        if layout.has_compact_top_bar() {
            ordered.extend(collect_sorted(ToolbarArea::TopBarEnd));
        }
    }

    let mut result: Vec<FocusTarget> = ordered
        .into_iter()
        .map(|item| FocusTarget::ToolbarItem(item.id.clone()))
        .collect();

    // Include visible window control buttons in toolbar focus order (`VIM-TB-03`, `ZendiaToolbar.sys.mjs:74`).
    if layout.has_top_toolbar() || layout.has_compact_top_bar() || layout.sidebar_side == SidebarSide::Right {
        for id in [
            "builtin.window_minimize",
            "builtin.window_maximize",
            "builtin.window_close",
        ] {
            result.push(FocusTarget::ToolbarItem(ContributionId::new(id)));
        }
    }

    result
}

/// Ordered pane targets window mode walks: the active split group's panes in reading order, or the lone active tab (`WIN-23`).
pub fn window_targets(model: &BrowserModel) -> Vec<FocusTarget> {
    if let Some(group) = model.active_split_group() {
        split::panes_in_reading_order(group)
            .into_iter()
            .map(FocusTarget::Pane)
            .collect()
    } else if let Some(tab_id) = model.active_tab {
        vec![FocusTarget::Pane(tab_id)]
    } else {
        Vec::new()
    }
}

/// Targets for any [`FocusSurface`].
pub fn targets_for_surface(model: &BrowserModel, surface: FocusSurface) -> Vec<FocusTarget> {
    match surface {
        FocusSurface::Explorer => explorer_targets(model),
        FocusSurface::Toolbar => toolbar_targets(model),
        FocusSurface::Window => window_targets(model),
    }
}

/// Side effect requested when stepping into or out of a folder in the explorer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderStepEffect {
    Expand(FolderId),
    Collapse(FolderId),
}

/// Computes the next cursor target when moving within `targets`.
pub fn step_linear(
    targets: &[FocusTarget],
    current: Option<&FocusTarget>,
    direction: FocusDirection,
) -> Option<FocusTarget> {
    if targets.is_empty() {
        return None;
    }
    let idx = current
        .and_then(|cursor| targets.iter().position(|t| t == cursor))
        .unwrap_or(0);
    let next_idx = match direction {
        FocusDirection::Next | FocusDirection::Into => (idx + 1).min(targets.len() - 1),
        FocusDirection::Previous | FocusDirection::Out => idx.saturating_sub(1),
        FocusDirection::First => 0,
        FocusDirection::Last => targets.len() - 1,
    };
    Some(targets[next_idx].clone())
}

/// Computes the next explorer cursor and any folder expand/collapse effect for `direction`.
pub fn step_explorer(
    model: &BrowserModel,
    current: Option<&FocusTarget>,
    direction: FocusDirection,
) -> (Option<FocusTarget>, Option<FolderStepEffect>) {
    let targets = explorer_targets(model);
    if targets.is_empty() {
        return (None, None);
    }

    match direction {
        FocusDirection::Into => {
            if let Some(FocusTarget::Folder(folder_id)) = current
                && let Some(folder) = model.folder(*folder_id)
            {
                if folder.collapsed {
                    let first_child = model
                        .folder_tabs(*folder_id)
                        .first()
                        .map(|tab| FocusTarget::Tab(tab.id))
                        .unwrap_or(FocusTarget::Folder(*folder_id));
                    return (Some(first_child), Some(FolderStepEffect::Expand(*folder_id)));
                }
                if let Some(first_visible) = model.visible_folder_tabs(*folder_id).first() {
                    return (Some(FocusTarget::Tab(first_visible.id)), None);
                }
            }
            (current.cloned().or_else(|| targets.first().cloned()), None)
        }
        FocusDirection::Out => {
            match current {
                Some(FocusTarget::Tab(tab_id)) => {
                    if let Some(folder_id) = model.tab(*tab_id).and_then(|tab| tab.folder_id) {
                        return (Some(FocusTarget::Folder(folder_id)), None);
                    }
                }
                Some(FocusTarget::Folder(folder_id)) => {
                    if let Some(folder) = model.folder(*folder_id)
                        && !folder.collapsed
                    {
                        return (
                            Some(FocusTarget::Folder(*folder_id)),
                            Some(FolderStepEffect::Collapse(*folder_id)),
                        );
                    }
                }
                _ => {}
            }
            (current.cloned().or_else(|| targets.first().cloned()), None)
        }
        other => (step_linear(&targets, current, other), None),
    }
}

/// Computes the inclusive visual selection range between `anchor` and `cursor` within `targets`.
pub fn visual_range(targets: &[FocusTarget], anchor: &FocusTarget, cursor: &FocusTarget) -> Vec<FocusTarget> {
    let Some(from) = targets.iter().position(|t| t == anchor) else {
        return Vec::new();
    };
    let Some(to) = targets.iter().position(|t| t == cursor) else {
        return Vec::new();
    };
    let (start, end) = if from <= to { (from, to) } else { (to, from) };
    targets[start..=end].to_vec()
}

/// Computes all window-mode landing targets for carrying `carried_tab` across the active split group (`adding = false`).
pub fn compute_landing_targets(model: &BrowserModel, carried_tab: TabId) -> Vec<LandingTarget> {
    compute_landing_targets_with_mode(model, carried_tab, false)
}

/// Computes all window-mode landing targets for carrying an existing pane (`adding = false`) or bringing a new tab
/// into the split (`adding = true`, `wa` / `"Add to Split"`, `ZendiaWindowMode.sys.mjs:280-333`).
pub fn compute_landing_targets_with_mode(model: &BrowserModel, carried_tab: TabId, adding: bool) -> Vec<LandingTarget> {
    let group = model.active_split_group();
    let panes: Vec<(TabId, PaneRect)> = if let Some(group) = group {
        let by_id = split::group_pane_rects(group);
        split::panes_in_reading_order(group)
            .into_iter()
            .filter_map(|id| by_id.iter().copied().find(|(tid, _)| *tid == id))
            .collect()
    } else if adding && let Some(active) = model.active_tab {
        vec![(
            active,
            PaneRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
        )]
    } else {
        return Vec::new();
    };

    let mut targets = Vec::new();
    for (tab_id, rect) in panes {
        if !adding && tab_id == carried_tab {
            continue;
        }
        let sides = [
            (
                DropZone::Left,
                PaneRect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width * 0.5,
                    height: rect.height,
                },
                if adding { "Split left" } else { "Move here" },
            ),
            (
                DropZone::Right,
                PaneRect {
                    x: rect.x + rect.width * 0.5,
                    y: rect.y,
                    width: rect.width * 0.5,
                    height: rect.height,
                },
                if adding { "Split right" } else { "Move here" },
            ),
            (
                DropZone::Top,
                PaneRect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height * 0.5,
                },
                if adding { "Split above" } else { "Move here" },
            ),
            (
                DropZone::Bottom,
                PaneRect {
                    x: rect.x,
                    y: rect.y + rect.height * 0.5,
                    width: rect.width,
                    height: rect.height * 0.5,
                },
                if adding { "Split below" } else { "Move here" },
            ),
        ];
        for (zone, half_rect, label) in sides {
            targets.push(LandingTarget {
                kind: LandingKind::PaneSide {
                    target_tab: tab_id,
                    zone,
                },
                rect: half_rect,
                label: label.to_string(),
            });
        }
        if !adding {
            targets.push(LandingTarget {
                kind: LandingKind::PaneSwap { target_tab: tab_id },
                rect,
                label: "Swap".to_string(),
            });
        }
    }

    let tree = group.and_then(split::effective_group_tree);
    let edge_defs = [
        (
            DropZone::Left,
            if adding {
                "Split at left edge"
            } else {
                "Move to left edge"
            },
        ),
        (
            DropZone::Right,
            if adding {
                "Split at right edge"
            } else {
                "Move to right edge"
            },
        ),
        (DropZone::Top, if adding { "Split at top" } else { "Move to top" }),
        (
            DropZone::Bottom,
            if adding { "Split at bottom" } else { "Move to bottom" },
        ),
    ];
    for (zone, label) in edge_defs {
        let share = tree
            .as_ref()
            .map(|t| split::edge_share(t, (!adding).then_some(carried_tab), zone))
            .unwrap_or(0.5);
        let rect = match zone {
            DropZone::Left => PaneRect {
                x: 0.0,
                y: 0.0,
                width: share,
                height: 1.0,
            },
            DropZone::Right => PaneRect {
                x: 1.0 - share,
                y: 0.0,
                width: share,
                height: 1.0,
            },
            DropZone::Top => PaneRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: share,
            },
            DropZone::Bottom => PaneRect {
                x: 0.0,
                y: 1.0 - share,
                width: 1.0,
                height: share,
            },
        };
        targets.push(LandingTarget {
            kind: LandingKind::SplitEdge { zone },
            rect,
            label: label.to_string(),
        });
    }

    targets
}

fn rect_center(rect: PaneRect) -> (f32, f32) {
    (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
}

/// Finds the nearest inner landing target in `direction` using Zendia's directional score (`along + 2 * across`,
/// `ZendiaWindowMode.sys.mjs:361-390`). Outer edges are targeted explicitly via `H`, `J`, `K`, `L`
/// ([`target_edge_index`]).
pub fn step_landing_target(targets: &[LandingTarget], current: usize, direction: PaneDirection) -> usize {
    let Some(origin) = targets.get(current) else {
        return 0;
    };
    let (ox, oy) = rect_center(origin.rect);
    let (dx, dy) = direction.delta();
    let mut best = None;
    let mut best_score = f32::INFINITY;

    for (i, target) in targets.iter().enumerate() {
        if matches!(target.kind, LandingKind::SplitEdge { .. }) || i == current {
            continue;
        }
        let (tx, ty) = rect_center(target.rect);
        let along = if dx != 0.0 { (tx - ox) * dx } else { (ty - oy) * dy };
        let across = if dx != 0.0 { (ty - oy).abs() } else { (tx - ox).abs() };
        if along <= 0.005 {
            continue;
        }
        let score = along + 2.0 * across;
        if score < best_score {
            best_score = score;
            best = Some(i);
        }
    }

    best.unwrap_or(current)
}

/// Finds the landing target index for outer `zone` (`H`, `J`, `K`, `L` while carrying).
pub fn target_edge_index(targets: &[LandingTarget], zone: DropZone) -> Option<usize> {
    targets
        .iter()
        .position(|target| matches!(target.kind, LandingKind::SplitEdge { zone: z } if z == zone))
}

/// Finds the neighboring pane of `active_tab` in `direction`, if one exists in `group` (`WIN-08`).
pub fn neighbor_pane(group: &SplitGroup, active_tab: TabId, direction: PaneDirection) -> Option<TabId> {
    let (dx, dy) = direction.delta();
    split::pane_beside(group, active_tab, dx, dy, &[])
}

/// Computes the updated pane order and split direction when moving `active_tab` in `direction` inside `group`.
///
/// If there is a pane in `direction`, `active_tab` swaps positions with it. For a 2-pane split, moving perpendicular to
/// the split axis pivots between horizontal and vertical layouts (matching Vim's `<C-w>H/J/K/L`).
pub fn move_pane_layout(
    group: &SplitGroup,
    active_tab: TabId,
    direction: PaneDirection,
) -> Option<(Vec<TabId>, SplitDirection)> {
    let current_idx = group.tab_ids.iter().position(|&id| id == active_tab)?;
    if let Some(neighbor) = neighbor_pane(group, active_tab, direction) {
        let neighbor_idx = group.tab_ids.iter().position(|&id| id == neighbor)?;
        let mut next = group.tab_ids.clone();
        next.swap(current_idx, neighbor_idx);
        return Some((next, group.direction));
    }

    if group.tab_ids.len() == 2 {
        let other = group.tab_ids[1 - current_idx];
        let next_dir = direction.drop_zone().direction();
        let next_order = if direction.drop_zone().places_first() {
            vec![active_tab, other]
        } else {
            vec![other, active_tab]
        };
        if next_dir != group.direction || next_order != group.tab_ids {
            return Some((next_order, next_dir));
        }
    }
    None
}

/// Computes the updated pane order and split direction when dropping `carried_tab` onto `target`.
pub fn apply_landing_target(
    group: &SplitGroup,
    carried_tab: TabId,
    target: &LandingTarget,
) -> Option<(Vec<TabId>, SplitDirection)> {
    let mut tab_ids = group.tab_ids.clone();
    match target.kind {
        LandingKind::PaneSwap { target_tab } => {
            let a = tab_ids.iter().position(|&id| id == carried_tab)?;
            let b = tab_ids.iter().position(|&id| id == target_tab)?;
            tab_ids.swap(a, b);
            Some((tab_ids, group.direction))
        }
        LandingKind::PaneSide { target_tab, zone } => {
            tab_ids.retain(|&id| id != carried_tab);
            let at = tab_ids.iter().position(|&id| id == target_tab)?;
            let insert_at = if zone.places_first() { at } else { at + 1 };
            tab_ids.insert(insert_at, carried_tab);
            let direction = if tab_ids.len() >= 3 {
                SplitDirection::Grid
            } else {
                zone.direction()
            };
            Some((tab_ids, direction))
        }
        LandingKind::SplitEdge { zone } => {
            tab_ids.retain(|&id| id != carried_tab);
            if zone.places_first() {
                tab_ids.insert(0, carried_tab);
            } else {
                tab_ids.push(carried_tab);
            }
            let direction = if tab_ids.len() >= 3 {
                match zone {
                    DropZone::Left | DropZone::Right => SplitDirection::Horizontal,
                    DropZone::Top | DropZone::Bottom => SplitDirection::Vertical,
                }
            } else {
                zone.direction()
            };
            Some((tab_ids, direction))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SpaceId, SplitGroup, SplitGroupId, ToolbarLayout};
    use crate::stub::sample_model;

    #[test]
    fn hint_labels_follow_fork_alphabet_and_equal_length_rule() {
        let single = hint_labels(9);
        assert_eq!(single, vec!["a", "s", "d", "f", "g", "h", "l", "q", "w"]);
        assert!(!HINT_ALPHABET.contains('j'));
        assert!(!HINT_ALPHABET.contains('k'));

        let exact_alphabet = hint_labels(HINT_ALPHABET.len());
        assert_eq!(exact_alphabet.len(), 24);
        assert!(exact_alphabet.iter().all(|label| label.len() == 1));

        let double = hint_labels(26);
        assert_eq!(double.len(), 26);
        assert!(double.iter().all(|label| label.len() == 2));
        assert_eq!(double[0], "aa");
        assert_eq!(double[1], "as");
        assert_eq!(double[24], "sa");
    }

    #[test]
    fn hint_session_narrows_and_matches_on_prefix() {
        let targets = vec![
            FocusTarget::Tab(TabId(1)),
            FocusTarget::Tab(TabId(2)),
            FocusTarget::Tab(TabId(3)),
            FocusTarget::Tab(TabId(4)),
        ];
        let mut session = HintSession::with_min_len(FocusSurface::Explorer, targets, 2);
        assert_eq!(session.visible_entries().len(), 4);
        assert_eq!(session.entries[0].label, "aa");
        assert_eq!(session.entries[1].label, "as");

        assert_eq!(session.type_char('a'), HintOutcome::Narrowed);
        assert_eq!(session.label_parts(&FocusTarget::Tab(TabId(2))), Some(("a", "s")));
        assert_eq!(session.type_char('s'), HintOutcome::Matched(FocusTarget::Tab(TabId(2))));
    }

    #[test]
    fn explorer_targets_walk_drawn_order_with_folders_and_split_groups() {
        let mut model = sample_model(true);
        // Split tabs 10 and 11 into one group.
        let group_id = SplitGroupId(1);
        for tab in &mut model.tabs {
            if tab.id == TabId(10) || tab.id == TabId(11) {
                tab.split_group_id = Some(group_id);
            }
        }
        model.split_groups.push(SplitGroup {
            id: group_id,
            space_id: SpaceId(1),
            direction: SplitDirection::Horizontal,
            tab_ids: vec![TabId(10), TabId(11)],
            ratios: vec![0.5, 0.5],
            tree: None,
            zoomed_tab: None,
        });

        let targets = explorer_targets(&model);
        // Starts with essentials, then pinned, then folders (with visible children), then normal tabs + single split group.
        assert!(matches!(targets.first(), Some(FocusTarget::Essential(_))));
        assert!(targets.iter().any(|t| matches!(t, FocusTarget::PinnedTab(_))));
        assert!(targets.iter().any(|t| matches!(t, FocusTarget::Folder(_))));
        assert_eq!(
            targets
                .iter()
                .filter(|t| **t == FocusTarget::SplitGroup(group_id))
                .count(),
            1
        );
        assert!(!targets.contains(&FocusTarget::Tab(TabId(10))));
        assert!(!targets.contains(&FocusTarget::Tab(TabId(11))));
    }

    #[test]
    fn explorer_step_moves_into_and_out_of_folders() {
        let model = sample_model(true);
        let collapsed_folder = model.folders.iter().find(|f| f.collapsed).expect("collapsed folder");
        let expanded_folder = model.folders.iter().find(|f| !f.collapsed).expect("expanded folder");

        // Stepping into a collapsed folder requests expanding it and lands on its first child.
        let (target, effect) = step_explorer(
            &model,
            Some(&FocusTarget::Folder(collapsed_folder.id)),
            FocusDirection::Into,
        );
        assert_eq!(effect, Some(FolderStepEffect::Expand(collapsed_folder.id)));
        assert_eq!(target, Some(FocusTarget::Tab(collapsed_folder.tab_ids[0])));

        // Stepping into an expanded folder moves onto its first visible child with no toggle.
        let (child_target, child_effect) = step_explorer(
            &model,
            Some(&FocusTarget::Folder(expanded_folder.id)),
            FocusDirection::Into,
        );
        assert_eq!(child_effect, None);
        assert_eq!(child_target, Some(FocusTarget::Tab(expanded_folder.tab_ids[0])));

        // Stepping out from a child tab returns to its parent folder.
        let (parent_target, parent_effect) = step_explorer(&model, child_target.as_ref(), FocusDirection::Out);
        assert_eq!(parent_effect, None);
        assert_eq!(parent_target, Some(FocusTarget::Folder(expanded_folder.id)));

        // Stepping out from an expanded folder requests collapsing it.
        let (_, collapse_effect) = step_explorer(&model, parent_target.as_ref(), FocusDirection::Out);
        assert_eq!(collapse_effect, Some(FolderStepEffect::Collapse(expanded_folder.id)));
    }

    #[test]
    fn toolbar_targets_follow_layout_order() {
        let mut model = sample_model(true);
        // A left sidebar: on the right, the single toolbar also holds the window buttons.
        model.layout.sidebar_side = SidebarSide::Left;
        model.layout.toolbar_layout = ToolbarLayout::Single;
        let single_ids: Vec<String> = toolbar_targets(&model)
            .into_iter()
            .map(|t| match t {
                FocusTarget::ToolbarItem(id) => id.as_str().to_string(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(
            single_ids,
            vec![
                "builtin.app_menu",
                "builtin.sidebar_toggle",
                "builtin.back",
                "builtin.forward",
                "builtin.reload",
                "builtin.url_bar",
            ]
        );

        model.layout.toolbar_layout = ToolbarLayout::Multiple;
        let multi_ids: Vec<String> = toolbar_targets(&model)
            .into_iter()
            .map(|t| match t {
                FocusTarget::ToolbarItem(id) => id.as_str().to_string(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(
            multi_ids,
            vec![
                "builtin.sidebar_toggle",
                "builtin.back",
                "builtin.forward",
                "builtin.reload",
                "builtin.url_bar",
                "zendia.agents",
                "zendia.chat",
                "zendia.webchat",
                "builtin.app_menu",
                "builtin.window_minimize",
                "builtin.window_maximize",
                "builtin.window_close",
            ]
        );
    }

    #[test]
    fn window_mode_landing_targets_and_pane_moves() {
        let mut model = sample_model(true);
        let group = SplitGroup {
            id: SplitGroupId(1),
            space_id: SpaceId(1),
            direction: SplitDirection::Grid,
            tab_ids: vec![TabId(12), TabId(11), TabId(10)],
            ratios: vec![0.5, 0.5],
            tree: None,
            zoomed_tab: None,
        };
        model.split_groups = vec![group.clone()];
        model.active_tab = Some(TabId(12));

        // 2 other panes * 5 (4 sides + swap) + 4 outer edges = 14 landing targets.
        let targets = compute_landing_targets(&model, TabId(12));
        assert_eq!(targets.len(), 14);
        assert_eq!(neighbor_pane(&group, TabId(12), PaneDirection::Right), Some(TabId(10)));
        assert_eq!(neighbor_pane(&group, TabId(12), PaneDirection::Down), Some(TabId(11)));

        // `hjkl` steps among inner pane targets while `H/J/K/L` (`target_edge_index`) jumps onto outer edges (`WIN-03`).
        let mut cur = targets
            .iter()
            .position(|t| matches!(t.kind, LandingKind::PaneSwap { .. }))
            .unwrap();
        for _ in 0..7 {
            cur = step_landing_target(&targets, cur, PaneDirection::Right);
        }
        assert!(!matches!(targets[cur].kind, LandingKind::SplitEdge { .. }));
        let right_edge = target_edge_index(&targets, DropZone::Right).expect("right edge");
        assert!(matches!(
            targets[right_edge].kind,
            LandingKind::SplitEdge { zone: DropZone::Right }
        ));
        let back = step_landing_target(&targets, right_edge, PaneDirection::Left);
        assert!(!matches!(targets[back].kind, LandingKind::SplitEdge { .. }));

        // Adding mode omits Swap and uses Split labels (`WIN-02`).
        let add_targets = compute_landing_targets_with_mode(&model, TabId(9), true);
        assert_eq!(add_targets.len(), 3 * 4 + 4);
        assert!(
            add_targets
                .iter()
                .all(|t| !matches!(t.kind, LandingKind::PaneSwap { .. }))
        );
        assert!(add_targets.iter().any(|t| t.label == "Split right"));
        assert!(add_targets.iter().any(|t| t.label == "Split at left edge"));

        let (moved, dir) = move_pane_layout(&group, TabId(12), PaneDirection::Right).expect("move right");
        assert_eq!(moved, vec![TabId(10), TabId(11), TabId(12)]);
        assert_eq!(dir, SplitDirection::Grid);
    }
}
