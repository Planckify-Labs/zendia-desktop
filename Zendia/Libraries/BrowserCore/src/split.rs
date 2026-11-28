/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Split view layout tree, geometry, and Glance state.
//!
//! A split group shows 2 to 4 tabs in the page card at once, laid out from a recursive [`SplitNode`] tree matching the
//! old fork's `ZenViewSplitter.mjs` (`nsSplitNode` / `nsSplitLeafNode`). Both the tree and the legacy
//! `(direction, ratios)` fields live on [`crate::model::SplitGroup`] so every view (Freya, Headless, Gallery) places,
//! resizes, swaps, rotates, and focuses panes identically. Rectangles here are fractions (`0.0..=1.0`) of the page
//! card; each view adds its own gaps.

use crate::model::{SplitDirection, SplitGroup, TabId};

/// Most panes one split group may hold, as in Zen.
pub const MAX_SPLIT_PANES: usize = 4;

/// Smallest fraction a pane may shrink to when resized (`7%`, matching `zen.splitView.min-resize-width` = `7`, `WIN-18`).
pub const MIN_PANE_FRACTION: f32 = 0.07;

/// Minimum `size_in_parent` percentage (`7.0`) corresponding to [`MIN_PANE_FRACTION`].
const MIN_NODE_PERCENT: f32 = MIN_PANE_FRACTION * 100.0;

/// Tolerance in percent of the split view when comparing shared edges and overlaps (`ZendiaWindows.sys.mjs:256`).
const EDGE_PERCENT: f32 = 0.5;

/// Axis along which a [`SplitNode::Container`] arranges its children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitAxis {
    /// Children sit side by side horizontally (`ZenViewSplitter` `"row"`).
    Row,
    /// Children stack top to bottom (`ZenViewSplitter` `"column"`).
    Column,
}

impl SplitAxis {
    pub fn from_zone(zone: DropZone) -> Self {
        match zone {
            DropZone::Left | DropZone::Right => Self::Row,
            DropZone::Top | DropZone::Bottom => Self::Column,
        }
    }
}

/// Recursive split layout tree node (`ZenViewSplitter.mjs` `nsSplitNode` / `nsSplitLeafNode`, `WIN-10`, `WIN-17`).
#[derive(Clone, Debug, PartialEq)]
pub enum SplitNode {
    Leaf {
        tab_id: TabId,
        /// Percentage (`0.0..=100.0`) of the parent container along the parent's [`SplitAxis`].
        size_in_parent: f32,
    },
    Container {
        axis: SplitAxis,
        /// Percentage (`0.0..=100.0`) of the parent container along the parent's [`SplitAxis`] (`100.0` at root).
        size_in_parent: f32,
        children: Vec<SplitNode>,
    },
}

impl SplitNode {
    pub fn size_in_parent(&self) -> f32 {
        match self {
            Self::Leaf { size_in_parent, .. } | Self::Container { size_in_parent, .. } => *size_in_parent,
        }
    }

    pub fn set_size_in_parent(&mut self, next: f32) {
        match self {
            Self::Leaf { size_in_parent, .. } | Self::Container { size_in_parent, .. } => *size_in_parent = next,
        }
    }

    /// Returns all leaf [`TabId`]s in DFS tree order (`#leavesOf` in `ZenViewSplitter.mjs`).
    pub fn leaves(&self) -> Vec<TabId> {
        let mut out = Vec::new();
        collect_leaves(self, &mut out);
        out
    }
}

fn collect_leaves(node: &SplitNode, out: &mut Vec<TabId>) {
    match node {
        SplitNode::Leaf { tab_id, .. } => out.push(*tab_id),
        SplitNode::Container { children, .. } => {
            for child in children {
                collect_leaves(child, out);
            }
        }
    }
}

/// A pane's place in the page card, as fractions of its width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Which way a resize handle moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleAxis {
    /// The handle is a vertical bar between panes side by side; it moves along x.
    Column,
    /// The handle is a horizontal bar between stacked panes; it moves along y.
    Row,
}

/// A draggable boundary between panes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitHandle {
    /// Index of the boundary; pass back to [`resize_ratios`] or [`resize_tree_handle`].
    pub index: usize,
    pub axis: HandleAxis,
    /// Position along the axis, as a fraction of the page card.
    pub position: f32,
    /// Where the handle starts and ends across the other axis, as fractions.
    pub span_start: f32,
    pub span_end: f32,
}

/// Builds a [`SplitNode`] tree from legacy `(direction, tab_ids, ratios)` (`calculateLayoutTree` in `ZenViewSplitter.mjs`).
pub fn build_tree(direction: SplitDirection, tab_ids: &[TabId], ratios: &[f32]) -> Option<SplitNode> {
    if tab_ids.is_empty() {
        return None;
    }
    if tab_ids.len() == 1 {
        return Some(SplitNode::Leaf {
            tab_id: tab_ids[0],
            size_in_parent: 100.0,
        });
    }
    let norm = normalized_ratios(direction, tab_ids.len(), ratios);
    match direction {
        SplitDirection::Grid if tab_ids.len() >= 3 => {
            let col = norm[0] * 100.0;
            let row = norm[1] * 100.0;
            let left_col = SplitNode::Container {
                axis: SplitAxis::Column,
                size_in_parent: col,
                children: vec![
                    SplitNode::Leaf {
                        tab_id: tab_ids[0],
                        size_in_parent: row,
                    },
                    SplitNode::Leaf {
                        tab_id: tab_ids[1],
                        size_in_parent: 100.0 - row,
                    },
                ],
            };
            let right = if tab_ids.len() == 3 {
                SplitNode::Leaf {
                    tab_id: tab_ids[2],
                    size_in_parent: 100.0 - col,
                }
            } else {
                SplitNode::Container {
                    axis: SplitAxis::Column,
                    size_in_parent: 100.0 - col,
                    children: vec![
                        SplitNode::Leaf {
                            tab_id: tab_ids[2],
                            size_in_parent: row,
                        },
                        SplitNode::Leaf {
                            tab_id: tab_ids[3],
                            size_in_parent: 100.0 - row,
                        },
                    ],
                }
            };
            Some(SplitNode::Container {
                axis: SplitAxis::Row,
                size_in_parent: 100.0,
                children: vec![left_col, right],
            })
        }
        SplitDirection::Vertical => Some(SplitNode::Container {
            axis: SplitAxis::Column,
            size_in_parent: 100.0,
            children: tab_ids
                .iter()
                .zip(norm)
                .map(|(&tab_id, share)| SplitNode::Leaf {
                    tab_id,
                    size_in_parent: share * 100.0,
                })
                .collect(),
        }),
        SplitDirection::Horizontal | SplitDirection::Grid => Some(SplitNode::Container {
            axis: SplitAxis::Row,
            size_in_parent: 100.0,
            children: tab_ids
                .iter()
                .zip(norm)
                .map(|(&tab_id, share)| SplitNode::Leaf {
                    tab_id,
                    size_in_parent: share * 100.0,
                })
                .collect(),
        }),
    }
}

/// Returns a [`SplitGroup`]'s active [`SplitNode`] tree, synthesizing one from `(direction, tab_ids, ratios)` when
/// `group.tree` is `None` or out of date with `group.tab_ids`.
pub fn effective_group_tree(group: &SplitGroup) -> Option<SplitNode> {
    if let Some(ref tree) = group.tree {
        let leaves = tree.leaves();
        if leaves.len() == group.tab_ids.len() && leaves.iter().all(|id| group.tab_ids.contains(id)) {
            return Some(tree.clone());
        }
    }
    build_tree(group.direction, &group.tab_ids, &group.ratios)
}

/// Synchronizes `group.tab_ids`, `group.direction`, and `group.ratios` from `tree` after a tree mutation.
pub fn sync_group_from_tree(group: &mut SplitGroup, tree: SplitNode) {
    let leaves = tree.leaves();
    group.tab_ids = leaves;
    match &tree {
        SplitNode::Leaf { .. } => {
            group.ratios = vec![1.0];
        }
        SplitNode::Container { axis, children, .. } => {
            let all_leaves = children.iter().all(|c| matches!(c, SplitNode::Leaf { .. }));
            if all_leaves {
                group.direction = match axis {
                    SplitAxis::Row => SplitDirection::Horizontal,
                    SplitAxis::Column => SplitDirection::Vertical,
                };
                let sum: f32 = children.iter().map(|c| c.size_in_parent().max(0.01)).sum();
                group.ratios = children.iter().map(|c| c.size_in_parent().max(0.01) / sum).collect();
            } else {
                group.direction = SplitDirection::Grid;
                let col_share = children
                    .first()
                    .map(|c| (c.size_in_parent() / 100.0).clamp(MIN_PANE_FRACTION, 1.0 - MIN_PANE_FRACTION))
                    .unwrap_or(0.5);
                let row_share = children
                    .iter()
                    .find_map(|c| match c {
                        SplitNode::Container { children: inner, .. } => inner.first().map(|first| {
                            (first.size_in_parent() / 100.0).clamp(MIN_PANE_FRACTION, 1.0 - MIN_PANE_FRACTION)
                        }),
                        _ => None,
                    })
                    .unwrap_or(0.5);
                group.ratios = vec![col_share, row_share];
            }
        }
    }
    group.tree = Some(tree);
}

/// Computes `(tab_id, rect)` for every leaf in `tree` in `0.0..=1.0` page-card fractions.
pub fn tree_pane_rects(tree: &SplitNode) -> Vec<(TabId, PaneRect)> {
    let mut out = Vec::new();
    layout_tree_rects(
        tree,
        PaneRect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        &mut out,
    );
    out
}

fn layout_tree_rects(node: &SplitNode, bounds: PaneRect, out: &mut Vec<(TabId, PaneRect)>) {
    match node {
        SplitNode::Leaf { tab_id, .. } => out.push((*tab_id, bounds)),
        SplitNode::Container { axis, children, .. } => {
            if children.is_empty() {
                return;
            }
            let total: f32 = children.iter().map(|c| c.size_in_parent().max(0.001)).sum();
            let mut offset = 0.0_f32;
            for (idx, child) in children.iter().enumerate() {
                let share = if idx + 1 == children.len() {
                    (1.0 - offset).max(0.0)
                } else {
                    (child.size_in_parent().max(0.001) / total).clamp(0.0, (1.0 - offset).max(0.0))
                };
                let child_bounds = match axis {
                    SplitAxis::Row => PaneRect {
                        x: bounds.x + offset * bounds.width,
                        y: bounds.y,
                        width: share * bounds.width,
                        height: bounds.height,
                    },
                    SplitAxis::Column => PaneRect {
                        x: bounds.x,
                        y: bounds.y + offset * bounds.height,
                        width: bounds.width,
                        height: share * bounds.height,
                    },
                };
                offset += share;
                layout_tree_rects(child, child_bounds, out);
            }
        }
    }
}

/// Computes `(tab_id, rect)` for every pane in `group` using its layout tree.
pub fn group_pane_rects(group: &SplitGroup) -> Vec<(TabId, PaneRect)> {
    effective_group_tree(group)
        .map(|tree| tree_pane_rects(&tree))
        .unwrap_or_default()
}

/// Returns the panes of `group` in reading order: rows from the top (rounded to integer percent), each from the left
/// (`ZenViewSplitter.mjs:1935-1948`, `WIN-23`).
pub fn panes_in_reading_order(group: &SplitGroup) -> Vec<TabId> {
    let mut panes = group_pane_rects(group);
    panes.sort_by(|(_, a), (_, b)| {
        let ay = (a.y * 100.0).round() as i32;
        let by = (b.y * 100.0).round() as i32;
        ay.cmp(&by)
            .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });
    panes.into_iter().map(|(tab_id, _)| tab_id).collect()
}

/// Finds the neighboring pane of `from` in direction `(dx, dy)` sharing that edge with the largest overlap, breaking
/// ties by `recency` (`paneBeside` in `ZendiaWindows.sys.mjs:293-325`, `WIN-08`).
pub fn pane_beside(group: &SplitGroup, from: TabId, dx: f32, dy: f32, recency: &[TabId]) -> Option<TabId> {
    let rects: Vec<(TabId, f32, f32, f32, f32)> = group_pane_rects(group)
        .into_iter()
        .map(|(tab, r)| {
            (
                tab,
                r.x * 100.0,
                (r.x + r.width) * 100.0,
                r.y * 100.0,
                (r.y + r.height) * 100.0,
            )
        })
        .collect();
    let (_, here_l, here_r, here_t, here_b) = *rects.iter().find(|(tab, ..)| *tab == from)?;

    #[derive(Clone, Copy)]
    struct Candidate {
        tab: TabId,
        gap: f32,
        overlap: f32,
    }

    let mut beside = Vec::new();
    for &(tab, l, r, t, b) in &rects {
        if tab == from {
            continue;
        }
        let gap = if dx != 0.0 {
            if dx > 0.0 { l - here_r } else { here_l - r }
        } else if dy > 0.0 {
            t - here_b
        } else {
            here_t - b
        };
        let overlap = if dx != 0.0 {
            b.min(here_b) - t.max(here_t)
        } else {
            r.min(here_r) - l.max(here_l)
        };
        if gap >= -EDGE_PERCENT && overlap > EDGE_PERCENT {
            beside.push(Candidate { tab, gap, overlap });
        }
    }
    if beside.is_empty() {
        return None;
    }
    let nearest = beside.iter().map(|c| c.gap).fold(f32::INFINITY, f32::min);
    let touching: Vec<Candidate> = beside.into_iter().filter(|c| c.gap <= nearest + EDGE_PERCENT).collect();
    let most = touching.iter().map(|c| c.overlap).fold(f32::NEG_INFINITY, f32::max);
    let best: Vec<TabId> = touching
        .into_iter()
        .filter(|c| c.overlap >= most - EDGE_PERCENT)
        .map(|c| c.tab)
        .collect();

    recency
        .iter()
        .rev()
        .copied()
        .find(|id| best.contains(id))
        .or_else(|| best.first().copied())
}

/// Divides ONLY `target_tab`'s leaf inside `tree`, placing `new_tab` on `zone` taking `share` (`0.0..1.0`, `0.5` by
/// default) of `target_tab` (`splitIntoNode` in `ZenViewSplitter.mjs:1132-1163`, `WIN-01`, `WIN-10`).
pub fn split_into_leaf(tree: &mut SplitNode, target_tab: TabId, new_tab: TabId, zone: DropZone, share: f32) -> bool {
    let split_axis = SplitAxis::from_zone(zone);
    let insert_before = zone.places_first();
    let share = share.clamp(MIN_PANE_FRACTION, 1.0 - MIN_PANE_FRACTION);
    split_into_node_rec(tree, target_tab, new_tab, split_axis, insert_before, share)
}

fn split_into_node_rec(
    node: &mut SplitNode,
    target_tab: TabId,
    new_tab: TabId,
    split_axis: SplitAxis,
    insert_before: bool,
    share: f32,
) -> bool {
    match node {
        SplitNode::Leaf { tab_id, size_in_parent } if *tab_id == target_tab => {
            let old_size = *size_in_parent;
            let existing = SplitNode::Leaf {
                tab_id: target_tab,
                size_in_parent: (1.0 - share) * 100.0,
            };
            let inserted = SplitNode::Leaf {
                tab_id: new_tab,
                size_in_parent: share * 100.0,
            };
            let children = if insert_before {
                vec![inserted, existing]
            } else {
                vec![existing, inserted]
            };
            *node = SplitNode::Container {
                axis: split_axis,
                size_in_parent: old_size,
                children,
            };
            true
        }
        SplitNode::Leaf { .. } => false,
        SplitNode::Container { axis, children, .. } => {
            if *axis == split_axis
                && let Some(idx) = children
                    .iter()
                    .position(|c| matches!(c, SplitNode::Leaf { tab_id, .. } if *tab_id == target_tab))
            {
                let old_size = children[idx].size_in_parent();
                children[idx].set_size_in_parent((1.0 - share) * old_size);
                let inserted = SplitNode::Leaf {
                    tab_id: new_tab,
                    size_in_parent: share * old_size,
                };
                let at = if insert_before { idx } else { idx + 1 };
                children.insert(at, inserted);
                return true;
            }
            for child in children.iter_mut() {
                if split_into_node_rec(child, target_tab, new_tab, split_axis, insert_before, share) {
                    return true;
                }
            }
            false
        }
    }
}

/// Removes `target_tab` from `tree`, preserving remaining siblings' relative shares and collapsing single-child or
/// same-axis nested containers (`removeNode` in `ZenViewSplitter.mjs:690-729`, `WIN-17`).
pub fn remove_leaf(tree: &mut SplitNode, target_tab: TabId) -> bool {
    let removed = remove_leaf_rec(tree, target_tab);
    if removed {
        tree.set_size_in_parent(100.0);
    }
    removed
}

fn remove_leaf_rec(node: &mut SplitNode, target_tab: TabId) -> bool {
    let SplitNode::Container {
        axis,
        size_in_parent,
        children,
    } = node
    else {
        return false;
    };
    let parent_axis = *axis;
    let parent_size = *size_in_parent;

    if let Some(idx) = children
        .iter()
        .position(|c| matches!(c, SplitNode::Leaf { tab_id, .. } if *tab_id == target_tab))
    {
        let removed_node = children.remove(idx);
        let removed_size = removed_node.size_in_parent();
        if children.len() != 1 {
            let denom = (100.0 - removed_size).max(0.001);
            let scale = 100.0 / denom;
            for child in children.iter_mut() {
                let next = child.size_in_parent() * scale;
                child.set_size_in_parent(next);
            }
        } else {
            let mut leftover = children.remove(0);
            leftover.set_size_in_parent(parent_size);
            *node = leftover;
        }
        return true;
    }

    for i in 0..children.len() {
        if remove_leaf_rec(&mut children[i], target_tab) {
            // Flatten if `children[i]` collapsed into a Container with the same axis as `parent_axis`.
            if let SplitNode::Container {
                axis: child_axis,
                size_in_parent: child_size,
                children: grand,
            } = &children[i]
                && *child_axis == parent_axis
            {
                let scale = *child_size / 100.0;
                let spliced: Vec<SplitNode> = grand
                    .iter()
                    .cloned()
                    .map(|mut g| {
                        let s = g.size_in_parent() * scale;
                        g.set_size_in_parent(s);
                        g
                    })
                    .collect();
                children.splice(i..=i, spliced);
            }
            return true;
        }
    }
    false
}

/// Swaps two panes in `tree`, each taking the other's place and size (`swapNodes` in `ZenViewSplitter.mjs:1114-1123`, `WIN-17`).
pub fn swap_leaves(tree: &mut SplitNode, a: TabId, b: TabId) -> bool {
    if a == b {
        return false;
    }
    let leaves = tree.leaves();
    if !leaves.contains(&a) || !leaves.contains(&b) {
        return false;
    }
    swap_leaves_rec(tree, a, b);
    true
}

fn swap_leaves_rec(node: &mut SplitNode, a: TabId, b: TabId) {
    match node {
        SplitNode::Leaf { tab_id, .. } => {
            if *tab_id == a {
                *tab_id = b;
            } else if *tab_id == b {
                *tab_id = a;
            }
        }
        SplitNode::Container { children, .. } => {
            for child in children {
                swap_leaves_rec(child, a, b);
            }
        }
    }
}

/// Rotates panes one slot forward (`backward = false`, `<C-w>r`) or backward (`backward = true`, `<C-w>R`) in tree
/// order while keeping every slot's size (`rotatePanes` in `ZenViewSplitter.mjs:1983-1996`, `WIN-17`).
pub fn rotate_leaves(tree: &mut SplitNode, backward: bool) -> bool {
    let tabs = tree.leaves();
    if tabs.len() < 2 {
        return false;
    }
    let rotated: Vec<TabId> = (0..tabs.len())
        .map(|i| {
            let from = if backward { i + 1 } else { i + tabs.len() - 1 };
            tabs[from % tabs.len()]
        })
        .collect();
    let mut iter = rotated.into_iter();
    assign_leaves_rec(tree, &mut iter);
    true
}

fn assign_leaves_rec(node: &mut SplitNode, iter: &mut impl Iterator<Item = TabId>) {
    match node {
        SplitNode::Leaf { tab_id, .. } => {
            if let Some(next) = iter.next() {
                *tab_id = next;
            }
        }
        SplitNode::Container { children, .. } => {
            for child in children {
                assign_leaves_rec(child, iter);
            }
        }
    }
}

/// Moves `tab` inside `tree` beside `target_tab` on `zone`, preserving remaining panes' shares (`movePane` in
/// `ZenViewSplitter.mjs:2007-2027`, `WIN-17`).
pub fn move_leaf(tree: &mut SplitNode, tab: TabId, target_tab: TabId, zone: DropZone) -> bool {
    if tab == target_tab {
        return false;
    }
    let leaves = tree.leaves();
    if !leaves.contains(&tab) || !leaves.contains(&target_tab) {
        return false;
    }
    if !remove_leaf(tree, tab) {
        return false;
    }
    split_into_leaf(tree, target_tab, tab, zone, 0.5)
}

/// Moves (or inserts) `tab` at an outer edge of `tree`, spanning the full height or width of the split (`movePaneToEdge`
/// in `ZenViewSplitter.mjs:2037-2064`, `WIN-13`, `WIN-17`).
pub fn move_leaf_to_edge(tree: &mut SplitNode, tab: TabId, zone: DropZone) -> bool {
    let already_in_tree = tree.leaves().contains(&tab);
    if already_in_tree && tree.leaves().len() < 2 {
        return false;
    }
    if already_in_tree {
        remove_leaf(tree, tab);
    }
    insert_at_root_edge(tree, tab, zone);
    true
}

fn insert_at_root_edge(root: &mut SplitNode, tab: TabId, zone: DropZone) {
    let edge_axis = SplitAxis::from_zone(zone);
    let prepend = zone.places_first();
    match root {
        SplitNode::Container { axis, children, .. } if *axis == edge_axis => {
            let count = (children.len() + 1) as f32;
            let scale = (count - 1.0) / count;
            for child in children.iter_mut() {
                let s = child.size_in_parent() * scale;
                child.set_size_in_parent(s);
            }
            let leaf = SplitNode::Leaf {
                tab_id: tab,
                size_in_parent: 100.0 / count,
            };
            if prepend {
                children.insert(0, leaf);
            } else {
                children.push(leaf);
            }
        }
        _ => {
            let mut old_root = root.clone();
            old_root.set_size_in_parent(50.0);
            let leaf = SplitNode::Leaf {
                tab_id: tab,
                size_in_parent: 50.0,
            };
            let children = if prepend {
                vec![leaf, old_root]
            } else {
                vec![old_root, leaf]
            };
            *root = SplitNode::Container {
                axis: edge_axis,
                size_in_parent: 100.0,
                children,
            };
        }
    }
}

/// Fraction (`0.0..=1.0`) of the split view a pane or added tab would take at `zone` (`edgeShare` in
/// `ZenViewSplitter.mjs:2074-2084`).
pub fn edge_share(tree: &SplitNode, carried_tab: Option<TabId>, zone: DropZone) -> f32 {
    let edge_axis = SplitAxis::from_zone(zone);
    match tree {
        SplitNode::Container { axis, children, .. } if *axis == edge_axis => {
            let direct_child = carried_tab.is_some_and(|id| {
                children
                    .iter()
                    .any(|c| matches!(c, SplitNode::Leaf { tab_id, .. } if *tab_id == id))
            });
            let count = children.len() + usize::from(!direct_child);
            1.0 / (count.max(2) as f32)
        }
        _ => 0.5,
    }
}

/// Gives every child at every level of `tree` an equal share (`equalizePanes` in `ZenViewSplitter.mjs:1905-1919`).
pub fn equalize_tree(tree: &mut SplitNode) {
    if let SplitNode::Container { children, .. } = tree {
        let share = 100.0 / (children.len().max(1) as f32);
        for child in children.iter_mut() {
            child.set_size_in_parent(share);
            equalize_tree(child);
        }
    }
}

/// Resizes `tab` along `target_axis` by `delta_pct_of_root` (in percent of the whole split, e.g. `+5.0` or `-5.0`),
/// trading space with the next sibling in the nearest matching ancestor container (or the previous one when last),
/// clamped to `7%` (`resizePane` in `ZenViewSplitter.mjs:2096-2129`, `WIN-18`).
pub fn resize_leaf(tree: &mut SplitNode, tab: TabId, target_axis: SplitAxis, delta_pct_of_root: f32) -> bool {
    resize_leaf_rec(
        tree,
        tab,
        target_axis,
        delta_pct_of_root,
        PaneRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        },
    )
    .unwrap_or(false)
}

/// Returns:
/// - `None` if `tab` is not inside `node`.
/// - `Some(true)` if an inner container already handled the resize (or this container handled it and changed sizes).
/// - `Some(false)` if `tab` was found inside `node`, an inner container handled it as a no-op (clamped), or no matching
///   axis container has been reached yet (handled by outer caller via `contains_leaf`).
fn resize_leaf_rec(
    node: &mut SplitNode,
    tab: TabId,
    target_axis: SplitAxis,
    delta_pct: f32,
    bounds_pct: PaneRect,
) -> Option<bool> {
    match node {
        SplitNode::Leaf { tab_id, .. } => (*tab_id == tab).then_some(false),
        SplitNode::Container { axis, children, .. } => {
            if children.is_empty() {
                return None;
            }
            let total: f32 = children.iter().map(|c| c.size_in_parent().max(0.001)).sum();
            let mut offset = 0.0_f32;
            let mut found_idx = None;
            let mut child_bounds_list = Vec::with_capacity(children.len());
            for child in children.iter() {
                let share = child.size_in_parent().max(0.001) / total;
                let cb = match axis {
                    SplitAxis::Row => PaneRect {
                        x: bounds_pct.x + offset * bounds_pct.width,
                        y: bounds_pct.y,
                        width: share * bounds_pct.width,
                        height: bounds_pct.height,
                    },
                    SplitAxis::Column => PaneRect {
                        x: bounds_pct.x,
                        y: bounds_pct.y + offset * bounds_pct.height,
                        width: bounds_pct.width,
                        height: share * bounds_pct.height,
                    },
                };
                offset += share;
                child_bounds_list.push(cb);
            }

            for (i, child) in children.iter().enumerate() {
                if child.leaves().contains(&tab) {
                    found_idx = Some(i);
                    break;
                }
            }
            let idx = found_idx?;

            // First see if a deeper container has `target_axis`! (`while (child.parent.direction != direction)`).
            let deeper_has_axis = has_ancestor_with_axis(&children[idx], tab, target_axis);
            if deeper_has_axis {
                return resize_leaf_rec(&mut children[idx], tab, target_axis, delta_pct, child_bounds_list[idx]);
            }

            if *axis != target_axis || children.len() < 2 {
                return Some(false);
            }

            // Normalize children so `size_in_parent` sums to `100.0` before applying `minResizeWidth = 7`.
            for child in children.iter_mut() {
                let norm = child.size_in_parent().max(0.001) * (100.0 / total);
                child.set_size_in_parent(norm);
            }

            let neighbor_idx = if idx + 1 < children.len() { idx + 1 } else { idx - 1 };
            let extent = match axis {
                SplitAxis::Row => bounds_pct.width.max(1.0),
                SplitAxis::Column => bounds_pct.height.max(1.0),
            };
            let child_size = children[idx].size_in_parent();
            let neighbor_size = children[neighbor_idx].size_in_parent();
            let raw_change = (delta_pct * 100.0) / extent;
            let change = raw_change.clamp(MIN_NODE_PERCENT - child_size, neighbor_size - MIN_NODE_PERCENT);
            if change.abs() < 0.01 {
                return Some(false);
            }
            children[idx].set_size_in_parent(child_size + change);
            children[neighbor_idx].set_size_in_parent(neighbor_size - change);
            Some(true)
        }
    }
}

fn has_ancestor_with_axis(node: &SplitNode, tab: TabId, target_axis: SplitAxis) -> bool {
    let SplitNode::Container { axis, children, .. } = node else {
        return false;
    };
    let Some(child) = children.iter().find(|c| c.leaves().contains(&tab)) else {
        return false;
    };
    if *axis == target_axis {
        return true;
    }
    has_ancestor_with_axis(child, tab, target_axis)
}

/// Computes all draggable [`SplitHandle`]s across a recursive [`SplitNode`] tree.
pub fn tree_split_handles(tree: &SplitNode) -> Vec<SplitHandle> {
    let mut out = Vec::new();
    collect_tree_handles(
        tree,
        PaneRect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        &mut out,
    );
    out
}

fn collect_tree_handles(node: &SplitNode, bounds: PaneRect, out: &mut Vec<SplitHandle>) {
    let SplitNode::Container { axis, children, .. } = node else {
        return;
    };
    if children.len() < 2 {
        return;
    }
    let total: f32 = children.iter().map(|c| c.size_in_parent().max(0.001)).sum();
    let mut offset = 0.0_f32;
    for (i, child) in children.iter().enumerate() {
        let share = child.size_in_parent().max(0.001) / total;
        let child_bounds = match axis {
            SplitAxis::Row => PaneRect {
                x: bounds.x + offset * bounds.width,
                y: bounds.y,
                width: share * bounds.width,
                height: bounds.height,
            },
            SplitAxis::Column => PaneRect {
                x: bounds.x,
                y: bounds.y + offset * bounds.height,
                width: bounds.width,
                height: share * bounds.height,
            },
        };
        offset += share;
        if i + 1 < children.len() {
            let handle_index = out.len();
            let handle = match axis {
                SplitAxis::Row => SplitHandle {
                    index: handle_index,
                    axis: HandleAxis::Column,
                    position: bounds.x + offset * bounds.width,
                    span_start: bounds.y,
                    span_end: bounds.y + bounds.height,
                },
                SplitAxis::Column => SplitHandle {
                    index: handle_index,
                    axis: HandleAxis::Row,
                    position: bounds.y + offset * bounds.height,
                    span_start: bounds.x,
                    span_end: bounds.x + bounds.width,
                },
            };
            out.push(handle);
        }
        collect_tree_handles(child, child_bounds, out);
    }
}

/// Drags handle `handle_index` (from [`tree_split_handles`]) to `root_position` (`0.0..=1.0` of the page card) and
/// returns the updated [`SplitNode`] tree, clamping adjacent children to [`MIN_PANE_FRACTION`].
pub fn resize_tree_handle(tree: &SplitNode, handle_index: usize, root_position: f32) -> SplitNode {
    if !root_position.is_finite() {
        return tree.clone();
    }
    let mut cloned = tree.clone();
    let mut cursor = 0usize;
    resize_tree_handle_rec(
        &mut cloned,
        handle_index,
        root_position,
        PaneRect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        &mut cursor,
    );
    cloned
}

fn resize_tree_handle_rec(
    node: &mut SplitNode,
    target_handle: usize,
    root_pos: f32,
    bounds: PaneRect,
    cursor: &mut usize,
) -> bool {
    let SplitNode::Container { axis, children, .. } = node else {
        return false;
    };
    if children.len() < 2 {
        return false;
    }
    let total: f32 = children.iter().map(|c| c.size_in_parent().max(0.001)).sum();
    for child in children.iter_mut() {
        let norm = child.size_in_parent().max(0.001) * (100.0 / total);
        child.set_size_in_parent(norm);
    }

    let mut offset = 0.0_f32;
    for i in 0..children.len() {
        let share = children[i].size_in_parent() / 100.0;
        let child_bounds = match axis {
            SplitAxis::Row => PaneRect {
                x: bounds.x + offset * bounds.width,
                y: bounds.y,
                width: share * bounds.width,
                height: bounds.height,
            },
            SplitAxis::Column => PaneRect {
                x: bounds.x,
                y: bounds.y + offset * bounds.height,
                width: bounds.width,
                height: share * bounds.height,
            },
        };
        offset += share;

        if i + 1 < children.len() {
            let my_index = *cursor;
            *cursor += 1;
            if my_index == target_handle {
                let local_frac = match axis {
                    SplitAxis::Row => (root_pos - bounds.x) / bounds.width.max(0.001),
                    SplitAxis::Column => (root_pos - bounds.y) / bounds.height.max(0.001),
                };
                let before_pct: f32 = children[..i].iter().map(|c| c.size_in_parent()).sum();
                let pair_pct = children[i].size_in_parent() + children[i + 1].size_in_parent();
                let min_pct = MIN_NODE_PERCENT.min(pair_pct * 0.5);
                let first_pct = (local_frac * 100.0 - before_pct).clamp(min_pct, pair_pct - min_pct);
                children[i].set_size_in_parent(first_pct);
                children[i + 1].set_size_in_parent(pair_pct - first_pct);
                return true;
            }
        }

        if resize_tree_handle_rec(&mut children[i], target_handle, root_pos, child_bounds, cursor) {
            return true;
        }
    }
    false
}

/// Equalizes the container owning `handle_index` (and its subtree), matching double-click on a splitter in
/// `ZenViewSplitter.mjs:2323-2325`.
pub fn equalize_tree_at_handle(tree: &SplitNode, handle_index: usize) -> SplitNode {
    let mut cloned = tree.clone();
    let mut cursor = 0usize;
    equalize_at_handle_rec(&mut cloned, handle_index, &mut cursor);
    cloned
}

fn equalize_at_handle_rec(node: &mut SplitNode, target_handle: usize, cursor: &mut usize) -> bool {
    let SplitNode::Container { children, .. } = node else {
        return false;
    };
    if children.len() < 2 {
        return false;
    }
    for i in 0..children.len() {
        if i + 1 < children.len() {
            let my_index = *cursor;
            *cursor += 1;
            if my_index == target_handle {
                equalize_tree(node);
                return true;
            }
        }
        if equalize_at_handle_rec(&mut children[i], target_handle, cursor) {
            return true;
        }
    }
    false
}

/// The ratios a new group of `count` panes starts with.
///
/// Side-by-side and stacked groups keep one share per pane. A grid keeps two: the left column's width and the row
/// boundary inside the columns.
pub fn default_ratios(direction: SplitDirection, count: usize) -> Vec<f32> {
    match direction {
        SplitDirection::Grid if count >= 3 => vec![0.5, 0.5],
        _ => vec![1.0 / count.max(1) as f32; count.max(1)],
    }
}

/// Brings stored ratios back into shape for `count` panes, falling back to the defaults when they do not fit.
pub fn normalized_ratios(direction: SplitDirection, count: usize, ratios: &[f32]) -> Vec<f32> {
    let expected = default_ratios(direction, count);
    if ratios.len() != expected.len() || ratios.iter().any(|ratio| !ratio.is_finite() || *ratio <= 0.0) {
        return expected;
    }
    if matches!(direction, SplitDirection::Grid) && count >= 3 {
        return ratios
            .iter()
            .map(|ratio| ratio.clamp(MIN_PANE_FRACTION, 1.0 - MIN_PANE_FRACTION))
            .collect();
    }
    let total: f32 = ratios.iter().sum();
    ratios.iter().map(|ratio| ratio / total).collect()
}

/// Lays out `count` panes of a group from legacy `(direction, ratios)`.
pub fn pane_rects(direction: SplitDirection, count: usize, ratios: &[f32]) -> Vec<PaneRect> {
    if count == 0 {
        return Vec::new();
    }
    let fake_ids: Vec<TabId> = (0..count as u64).map(TabId).collect();
    build_tree(direction, &fake_ids, ratios)
        .map(|tree| tree_pane_rects(&tree).into_iter().map(|(_, rect)| rect).collect())
        .unwrap_or_default()
}

/// The resize handles of a legacy `(direction, count, ratios)` group.
pub fn split_handles(direction: SplitDirection, count: usize, ratios: &[f32]) -> Vec<SplitHandle> {
    if count < 2 {
        return Vec::new();
    }
    let ratios = normalized_ratios(direction, count, ratios);
    match direction {
        SplitDirection::Grid if count >= 3 => {
            let column = ratios[0];
            let row = ratios[1];
            let mut handles = vec![
                SplitHandle {
                    index: 0,
                    axis: HandleAxis::Column,
                    position: column,
                    span_start: 0.0,
                    span_end: 1.0,
                },
                SplitHandle {
                    index: 1,
                    axis: HandleAxis::Row,
                    position: row,
                    span_start: 0.0,
                    span_end: column,
                },
            ];
            if count == 4 {
                handles.push(SplitHandle {
                    index: 1,
                    axis: HandleAxis::Row,
                    position: row,
                    span_start: column,
                    span_end: 1.0,
                });
            }
            handles
        }
        _ => {
            let axis = if matches!(direction, SplitDirection::Vertical) {
                HandleAxis::Row
            } else {
                HandleAxis::Column
            };
            let mut offset = 0.0;
            ratios[..ratios.len() - 1]
                .iter()
                .enumerate()
                .map(|(index, share)| {
                    offset += share;
                    SplitHandle {
                        index,
                        axis,
                        position: offset,
                        span_start: 0.0,
                        span_end: 1.0,
                    }
                })
                .collect()
        }
    }
}

/// Moves one handle to `position` (a fraction of the page card) and returns the new ratios.
///
/// For side-by-side and stacked groups only the two panes beside the handle change size. No pane shrinks below
/// [`MIN_PANE_FRACTION`].
pub fn resize_ratios(direction: SplitDirection, count: usize, ratios: &[f32], index: usize, position: f32) -> Vec<f32> {
    let mut ratios = normalized_ratios(direction, count, ratios);
    if !position.is_finite() {
        return ratios;
    }
    if matches!(direction, SplitDirection::Grid) && count >= 3 {
        if let Some(ratio) = ratios.get_mut(index) {
            *ratio = position.clamp(MIN_PANE_FRACTION, 1.0 - MIN_PANE_FRACTION);
        }
        return ratios;
    }
    if index + 1 >= ratios.len() {
        return ratios;
    }
    let before: f32 = ratios[..index].iter().sum();
    let pair = ratios[index] + ratios[index + 1];
    let first = (position - before).clamp(
        MIN_PANE_FRACTION.min(pair / 2.0),
        pair - MIN_PANE_FRACTION.min(pair / 2.0),
    );
    ratios[index] = first;
    ratios[index + 1] = pair - first;
    ratios
}

/// Which side of a pane a dragged tab is dropped on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DropZone {
    Left,
    Right,
    Top,
    Bottom,
}

impl DropZone {
    /// The split direction a drop on this side creates.
    pub fn direction(self) -> SplitDirection {
        match self {
            Self::Left | Self::Right => SplitDirection::Horizontal,
            Self::Top | Self::Bottom => SplitDirection::Vertical,
        }
    }

    /// Whether the dropped tab goes before the existing pane.
    pub fn places_first(self) -> bool {
        matches!(self, Self::Left | Self::Top)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "top" => Some(Self::Top),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }
}

/// Picks the drop zone for a point inside the page card, given as fractions.
///
/// The card is cut along its diagonals, so the nearest edge wins.
pub fn drop_zone_at(x: f32, y: f32) -> DropZone {
    let x = x.clamp(0.0, 1.0);
    let y = y.clamp(0.0, 1.0);
    let to_left = x;
    let to_right = 1.0 - x;
    let to_top = y;
    let to_bottom = 1.0 - y;
    let nearest = to_left.min(to_right).min(to_top).min(to_bottom);
    if nearest == to_left {
        DropZone::Left
    } else if nearest == to_right {
        DropZone::Right
    } else if nearest == to_top {
        DropZone::Top
    } else {
        DropZone::Bottom
    }
}

pub fn direction_as_str(direction: SplitDirection) -> &'static str {
    match direction {
        SplitDirection::Horizontal => "horizontal",
        SplitDirection::Vertical => "vertical",
        SplitDirection::Grid => "grid",
    }
}

pub fn parse_direction(value: &str) -> Option<SplitDirection> {
    match value {
        "horizontal" | "vsep" | "columns" => Some(SplitDirection::Horizontal),
        "vertical" | "hsep" | "rows" => Some(SplitDirection::Vertical),
        "grid" => Some(SplitDirection::Grid),
        _ => None,
    }
}

/// A page previewed in a card over the current page.
///
/// The Glance page is a real tab with its own Hub view. It is left out of the sidebar's lists and drawn next to its
/// parent's row instead, until it is closed, expanded into a normal tab, or split with its parent.
#[derive(Clone, Debug, PartialEq)]
pub struct Glance {
    pub tab_id: TabId,
    pub parent_tab_id: Option<TabId>,
    /// Where the open animation starts, as fractions of the page card (the link that was clicked).
    pub origin: Option<(f32, f32)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SpaceId, SplitGroupId};

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn side_by_side_panes_share_the_width() {
        let rects = pane_rects(SplitDirection::Horizontal, 2, &[0.5, 0.5]);
        assert_eq!(rects.len(), 2);
        assert!(approx(rects[0].width, 0.5) && approx(rects[1].x, 0.5) && approx(rects[1].height, 1.0));
    }

    #[test]
    fn stacked_panes_share_the_height() {
        let rects = pane_rects(SplitDirection::Vertical, 3, &[]);
        assert_eq!(rects.len(), 3);
        assert!(approx(rects[2].y, 2.0 / 3.0) && approx(rects[2].width, 1.0));
    }

    #[test]
    fn three_pane_grid_has_a_full_height_right_column_and_reading_order() {
        let rects = pane_rects(SplitDirection::Grid, 3, &[0.5, 0.5]);
        assert_eq!(rects.len(), 3);
        assert!(approx(rects[0].height, 0.5) && approx(rects[1].y, 0.5));
        assert!(approx(rects[2].x, 0.5) && approx(rects[2].height, 1.0));

        // Reading order (`WIN-23`): top-left (1), top-right (3), then bottom-left (2).
        let ids = vec![TabId(1), TabId(2), TabId(3)];
        let tree = build_tree(SplitDirection::Grid, &ids, &[0.5, 0.5]).unwrap();
        let group = SplitGroup {
            id: SplitGroupId(1),
            space_id: SpaceId(1),
            direction: SplitDirection::Grid,
            ratios: vec![0.5, 0.5],
            tab_ids: ids,
            tree: Some(tree),
            zoomed_tab: None,
        };
        assert_eq!(panes_in_reading_order(&group), vec![TabId(1), TabId(3), TabId(2)]);
    }

    #[test]
    fn four_pane_grid_is_two_by_two() {
        let rects = pane_rects(SplitDirection::Grid, 4, &[0.4, 0.6]);
        assert!(approx(rects[3].x, 0.4) && approx(rects[3].y, 0.6) && approx(rects[3].width, 0.6));
        assert_eq!(split_handles(SplitDirection::Grid, 4, &[0.4, 0.6]).len(), 3);
    }

    #[test]
    fn recursive_tree_divides_single_pane_keeps_sizes_on_swap_rotate_remove_and_edge() {
        // Start with A | B at 70% / 30%.
        let mut tree = build_tree(SplitDirection::Horizontal, &[TabId(1), TabId(2)], &[0.7, 0.3]).unwrap();
        // Divide ONLY pane B (`TabId(2)`) vertically with C (`TabId(3)`): A stays 70% full-height!
        assert!(split_into_leaf(&mut tree, TabId(2), TabId(3), DropZone::Bottom, 0.5));
        let rects = tree_pane_rects(&tree);
        assert!(approx(rects[0].1.width, 0.7) && approx(rects[0].1.height, 1.0));
        assert!(approx(rects[1].1.x, 0.7) && approx(rects[1].1.width, 0.3) && approx(rects[1].1.height, 0.5));
        assert!(approx(rects[2].1.x, 0.7) && approx(rects[2].1.y, 0.5) && approx(rects[2].1.height, 0.5));

        // Swap A and B (`WIN-17`): B takes the 70% left slot, A takes the 30%x50% top-right slot!
        assert!(swap_leaves(&mut tree, TabId(1), TabId(2)));
        let swapped = tree_pane_rects(&tree);
        assert_eq!(swapped[0].0, TabId(2));
        assert!(approx(swapped[0].1.width, 0.7));
        assert_eq!(swapped[1].0, TabId(1));
        assert!(approx(swapped[1].1.width, 0.3));

        // Rotate forward (`WIN-17`): slots keep 70% and 30%x50% sizes.
        assert!(rotate_leaves(&mut tree, false));
        let rotated = tree_pane_rects(&tree);
        assert_eq!(rotated[0].0, TabId(3));
        assert!(approx(rotated[0].1.width, 0.7));

        // Move `TabId(1)` to bottom outer edge (`WIN-13`): spans full width (`1.0`)!
        assert!(move_leaf_to_edge(&mut tree, TabId(1), DropZone::Bottom));
        let edged = tree_pane_rects(&tree);
        let bottom_pane = edged.iter().find(|(id, _)| *id == TabId(1)).unwrap().1;
        assert!(approx(bottom_pane.width, 1.0) && approx(bottom_pane.y, 0.5));

        // Remove `TabId(1)` (`WIN-17`): remaining two panes return to full height keeping their 70/30 width share!
        assert!(remove_leaf(&mut tree, TabId(1)));
        let after_rm = tree_pane_rects(&tree);
        assert_eq!(after_rm.len(), 2);
        assert!(approx(after_rm[0].1.width, 0.7) && approx(after_rm[0].1.height, 1.0));
        assert!(approx(after_rm[1].1.width, 0.3) && approx(after_rm[1].1.height, 1.0));
    }

    #[test]
    fn shared_edge_neighbor_uses_overlap_and_recency() {
        // Right column has two stacked panes (2 = 60% height, 3 = 40% height); from left pane (1), moving right picks
        // pane 2 (most overlap), and when equal (50/50) picks last used (`WIN-08`).
        let mut tree = build_tree(SplitDirection::Horizontal, &[TabId(1), TabId(2)], &[0.5, 0.5]).unwrap();
        split_into_leaf(&mut tree, TabId(2), TabId(3), DropZone::Bottom, 0.5);
        let mut group = SplitGroup {
            id: SplitGroupId(1),
            space_id: SpaceId(1),
            direction: SplitDirection::Grid,
            ratios: vec![0.5, 0.5],
            tab_ids: vec![TabId(1), TabId(2), TabId(3)],
            tree: Some(tree),
            zoomed_tab: None,
        };
        assert_eq!(
            pane_beside(&group, TabId(1), 1.0, 0.0, &[TabId(2), TabId(3)]),
            Some(TabId(3)),
            "equal overlap picks most recently used"
        );
        // Resize pane 2 taller so it overlaps more than pane 3 -> pane 2 wins even though pane 3 is newer!
        let mut taller = group.tree.clone().unwrap();
        assert!(resize_leaf(&mut taller, TabId(2), SplitAxis::Column, 10.0));
        group.tree = Some(taller);
        assert_eq!(
            pane_beside(&group, TabId(1), 1.0, 0.0, &[TabId(2), TabId(3)]),
            Some(TabId(2))
        );
    }

    #[test]
    fn bad_ratios_fall_back_to_equal_shares() {
        assert_eq!(normalized_ratios(SplitDirection::Horizontal, 2, &[1.0]), vec![0.5, 0.5]);
        assert_eq!(
            normalized_ratios(SplitDirection::Horizontal, 2, &[f32::NAN, 1.0]),
            vec![0.5, 0.5]
        );
        let scaled = normalized_ratios(SplitDirection::Horizontal, 2, &[2.0, 6.0]);
        assert!(approx(scaled[0], 0.25) && approx(scaled[1], 0.75));
    }

    #[test]
    fn resizing_moves_only_the_neighbouring_panes() {
        let ratios = resize_ratios(SplitDirection::Horizontal, 3, &[], 1, 0.8);
        assert!(approx(ratios[0], 1.0 / 3.0));
        assert!(approx(ratios[1], 0.8 - 1.0 / 3.0));
        assert!(approx(ratios.iter().sum::<f32>(), 1.0));
    }

    #[test]
    fn resizing_keeps_seven_percent_minimum_pane_size() {
        assert!(approx(MIN_PANE_FRACTION, 0.07));
        let ratios = resize_ratios(SplitDirection::Horizontal, 2, &[0.5, 0.5], 0, 0.99);
        assert!(approx(ratios[1], 0.07));
        let grid = resize_ratios(SplitDirection::Grid, 3, &[0.5, 0.5], 0, 0.01);
        assert!(approx(grid[0], 0.07));
    }

    #[test]
    fn handles_sit_between_panes() {
        let handles = split_handles(SplitDirection::Horizontal, 2, &[0.3, 0.7]);
        assert_eq!(handles.len(), 1);
        assert!(approx(handles[0].position, 0.3));
        assert_eq!(handles[0].axis, HandleAxis::Column);
        let grid = split_handles(SplitDirection::Grid, 3, &[0.5, 0.5]);
        assert_eq!(grid[1].axis, HandleAxis::Row);
        assert!(approx(grid[1].span_end, 0.5));
    }

    #[test]
    fn drop_zone_is_the_nearest_edge() {
        assert_eq!(drop_zone_at(0.05, 0.5), DropZone::Left);
        assert_eq!(drop_zone_at(0.95, 0.4), DropZone::Right);
        assert_eq!(drop_zone_at(0.5, 0.1), DropZone::Top);
        assert_eq!(drop_zone_at(0.5, 0.9), DropZone::Bottom);
        assert_eq!(DropZone::Top.direction(), SplitDirection::Vertical);
    }

    #[test]
    fn directions_parse_zen_names() {
        assert_eq!(parse_direction("vsep"), Some(SplitDirection::Horizontal));
        assert_eq!(parse_direction("hsep"), Some(SplitDirection::Vertical));
        assert_eq!(parse_direction("grid"), Some(SplitDirection::Grid));
        assert_eq!(parse_direction("diagonal"), None);
    }
}
