/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Split view, panel-row, and window mode actions of the vim profile: dividing panes (`split.vertical`,
//! `split.horizontal`), adding tabs into a split (`split.add`), moving along the `[left panels, split panes / page,
//! right panels]` row (`panes.*`), resizing, zooming, swapping, rotating, edge-moving, grabbing, and picking panes.

use crate::commands::{ArgumentSchema, ArgumentType, Command, CommandDescriptor, CommandError};
use crate::focus::PaneDirection;
use crate::model::{SplitGroupId, TabId};
use crate::split::{DropZone, SplitNode};

/// How much one resize step (`+`, `-`, `>`, `<`) changes a pane's share, matching the fork's 5%.
pub const RESIZE_STEP: f32 = 0.05;

/// Axis and direction of a split pane resize.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeKind {
    GrowWidth,
    ShrinkWidth,
    GrowHeight,
    ShrinkHeight,
}

impl ResizeKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::GrowWidth => "split.grow_width",
            Self::ShrinkWidth => "split.shrink_width",
            Self::GrowHeight => "split.grow_height",
            Self::ShrinkHeight => "split.shrink_height",
        }
    }
}

/// Split view, panel-row, and pane-focus actions invoked by vim keymaps and split controls.
#[derive(Clone, Debug, PartialEq)]
pub enum SplitAction {
    /// Focus the neighboring split pane in `direction` (`<leader>wh/j/k/l>`, `w` mode `h/j/k/l`).
    Focus { direction: PaneDirection },
    /// Step along the `[left panels, split panes / page, right panels]` row (`<C-h/j/k/l>`, `WIN-06`).
    PanesFocus { direction: PaneDirection },
    /// Go back to the last focused place (page or side panel) (`<C-\>`, `WIN-06`).
    PanesLast,
    /// Open the `"Split Window Right"` or `"Split Window Below"` picker (or URL bar) to divide the current pane (`WIN-01`).
    SplitDivide { zone: DropZone },
    /// Internal callback executed when an item is chosen in `"Split Window Right"` / `"Split Window Below"` (`WIN-01`).
    SplitPickApply {
        anchor_tab_id: TabId,
        picked_tab_id: Option<TabId>,
        zone: DropZone,
        open_url: bool,
    },
    /// Open the `"Add to Split"` picker and carry the chosen tab in Window mode (`<leader>wa`, `w` mode `a`, `WIN-02`).
    AddPick,
    /// Internal callback executed when a tab is chosen in `"Add to Split"` (`WIN-02`).
    AddCarry { anchor_tab_id: TabId, picked_tab_id: TabId },
    /// `<Esc>` in Window mode: cancels any active carry (keeping Window mode open unless opened only to carry), or
    /// leaves Window mode when nothing is carried (`WIN-04`).
    WindowEscape,
    /// Cycle to the next pane in reading order (`<leader>ww`, `w` mode `w`, `WIN-23`).
    Next,
    /// Switch to the most recently used other pane in the active split group (`<leader>wp`, `w` mode `p`).
    Last,
    /// Label every pane chip for quick focus or carry-target jump (`<leader>wf`, `w` mode `f`, `WIN-05`).
    Pick,
    /// Move the active pane out of its split group into its own tab (`<leader>wt`, `w` mode `t` without `tab_id`, `WIN-19`).
    RemoveActiveTab,
    /// Toggle filling the split view with `tab_id` (active pane by default) (`<leader>wm`, `w` mode `m`, chip zoom, `WIN-09`).
    Zoom { tab_id: Option<TabId> },
    /// Reset all panes in the active split group to equal sizes (`<leader>w=`, `w` mode `=`).
    Equalize,
    /// Resize the active pane by `RESIZE_STEP * count` (`<C-Up/Down/Left/Right>`, `<leader>w+/-/>/<`, `WIN-18`).
    Resize { kind: ResizeKind, count: Option<u32> },
    /// Commit an updated [`SplitNode`] layout tree (e.g. after splitter handle drag or double-click equalize).
    UpdateTree { group_id: SplitGroupId, tree: SplitNode },
    /// Preview a swap of the active pane with the last-used other pane (`<leader>wx`, `w` mode `x`, `WIN-03`).
    Swap,
    /// Rotate panes forward (`<leader>wr`) or backward (`<leader>wR`) while keeping slot sizes (`WIN-17`).
    Rotate { reverse: bool },
    /// Preview moving the active pane (or jump the active carry) to an outer edge (`<leader>wH/J/K/L`, `w` mode `H/J/K/L`, `WIN-03`, `WIN-13`).
    Edge { direction: PaneDirection },
    /// Enter carry/grab mode for the active pane (`<leader>wg`, `w` mode `g`, `WIN-03`, `WIN-07`, `WIN-22`).
    Grab,
}

impl SplitAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Focus { direction } => match direction {
                PaneDirection::Left => "split.focus_left",
                PaneDirection::Down => "split.focus_down",
                PaneDirection::Up => "split.focus_up",
                PaneDirection::Right => "split.focus_right",
            },
            Self::PanesFocus { direction } => match direction {
                PaneDirection::Left => "panes.focus_left",
                PaneDirection::Down => "panes.focus_down",
                PaneDirection::Up => "panes.focus_up",
                PaneDirection::Right => "panes.focus_right",
            },
            Self::PanesLast => "panes.last",
            Self::SplitDivide {
                zone: DropZone::Bottom | DropZone::Top,
            } => "split.horizontal",
            Self::SplitDivide { .. } => "split.vertical",
            Self::SplitPickApply { .. } => "split.pick_apply",
            Self::AddPick => "split.add",
            Self::AddCarry { .. } => "split.add_carry",
            Self::WindowEscape => "window.escape",
            Self::Next => "split.next",
            Self::Last => "split.last",
            Self::Pick => "split.pick",
            Self::RemoveActiveTab => "split.remove_tab",
            Self::Zoom { .. } => "split.zoom",
            Self::Equalize => "split.equalize",
            Self::Resize { kind, .. } => kind.name(),
            Self::UpdateTree { .. } => "split.update_tree",
            Self::Swap => "split.swap",
            Self::Rotate { reverse: false } => "split.rotate",
            Self::Rotate { reverse: true } => "split.rotate_back",
            Self::Edge { direction } => match direction {
                PaneDirection::Left => "split.edge_left",
                PaneDirection::Down => "split.edge_down",
                PaneDirection::Up => "split.edge_up",
                PaneDirection::Right => "split.edge_right",
            },
            Self::Grab => "split.grab",
        }
    }
}

fn find_arg<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

fn parse_opt_u64(command: &str, args: &[(String, String)], key: &str) -> Result<Option<u64>, CommandError> {
    find_arg(args, key)
        .map(|raw| {
            raw.parse::<u64>().map_err(|_| CommandError::InvalidArgument {
                command: command.to_string(),
                message: format!("expected integer `{key}`, got `{raw}`"),
            })
        })
        .transpose()
}

fn parse_req_u64(command: &str, args: &[(String, String)], key: &str) -> Result<u64, CommandError> {
    parse_opt_u64(command, args, key)?.ok_or_else(|| CommandError::InvalidArgument {
        command: command.to_string(),
        message: format!("missing required `{key}`"),
    })
}

/// Parses the commands of this module, or returns `None` for a name it does not own.
pub fn parse_command(name: &str, args: &[(String, String)]) -> Result<Option<Command>, CommandError> {
    let count = parse_opt_u64(name, args, "count")?.map(|c| c.min(u64::from(u32::MAX)) as u32);
    let action = match name {
        "split.focus_left" => SplitAction::Focus {
            direction: PaneDirection::Left,
        },
        "split.focus_down" => SplitAction::Focus {
            direction: PaneDirection::Down,
        },
        "split.focus_up" => SplitAction::Focus {
            direction: PaneDirection::Up,
        },
        "split.focus_right" => SplitAction::Focus {
            direction: PaneDirection::Right,
        },
        "panes.focus_left" => SplitAction::PanesFocus {
            direction: PaneDirection::Left,
        },
        "panes.focus_down" => SplitAction::PanesFocus {
            direction: PaneDirection::Down,
        },
        "panes.focus_up" => SplitAction::PanesFocus {
            direction: PaneDirection::Up,
        },
        "panes.focus_right" => SplitAction::PanesFocus {
            direction: PaneDirection::Right,
        },
        "panes.last" => SplitAction::PanesLast,
        "split.vertical" => SplitAction::SplitDivide { zone: DropZone::Right },
        "split.horizontal" => SplitAction::SplitDivide { zone: DropZone::Bottom },
        "split.pick_apply" => {
            let anchor_tab_id = TabId(parse_req_u64(name, args, "anchor_tab_id")?);
            let picked_tab_id = parse_opt_u64(name, args, "picked_tab_id")?.map(TabId);
            let zone = find_arg(args, "side")
                .and_then(DropZone::parse)
                .unwrap_or(DropZone::Right);
            let open_url = find_arg(args, "open_url") == Some("true");
            SplitAction::SplitPickApply {
                anchor_tab_id,
                picked_tab_id,
                zone,
                open_url,
            }
        }
        "split.add_carry" => {
            let anchor_tab_id = TabId(parse_req_u64(name, args, "anchor_tab_id")?);
            let picked_tab_id = TabId(parse_req_u64(name, args, "picked_tab_id")?);
            SplitAction::AddCarry {
                anchor_tab_id,
                picked_tab_id,
            }
        }
        "window.escape" => SplitAction::WindowEscape,
        "split.next" => SplitAction::Next,
        "split.last" => SplitAction::Last,
        "split.pick" => SplitAction::Pick,
        "split.zoom" => SplitAction::Zoom {
            tab_id: parse_opt_u64(name, args, "tab_id")?.map(TabId),
        },
        "split.equalize" => SplitAction::Equalize,
        "split.grow_width" => SplitAction::Resize {
            kind: ResizeKind::GrowWidth,
            count,
        },
        "split.shrink_width" => SplitAction::Resize {
            kind: ResizeKind::ShrinkWidth,
            count,
        },
        "split.grow_height" => SplitAction::Resize {
            kind: ResizeKind::GrowHeight,
            count,
        },
        "split.shrink_height" => SplitAction::Resize {
            kind: ResizeKind::ShrinkHeight,
            count,
        },
        "split.swap" => SplitAction::Swap,
        "split.rotate" => SplitAction::Rotate { reverse: false },
        "split.rotate_back" => SplitAction::Rotate { reverse: true },
        "split.edge_left" => SplitAction::Edge {
            direction: PaneDirection::Left,
        },
        "split.edge_down" => SplitAction::Edge {
            direction: PaneDirection::Down,
        },
        "split.edge_up" => SplitAction::Edge {
            direction: PaneDirection::Up,
        },
        "split.edge_right" => SplitAction::Edge {
            direction: PaneDirection::Right,
        },
        "split.grab" => SplitAction::Grab,
        _ => return Ok(None),
    };
    Ok(Some(Command::Splits(action)))
}

/// The descriptors of this module's commands.
pub fn descriptors() -> Vec<CommandDescriptor> {
    let count_arg = || ArgumentSchema::new("count", ArgumentType::Integer, false, "How many steps (defaults to 1)");
    let items: &[(&str, &str, &str, bool)] = &[
        (
            "split.focus_left",
            "Go to Left Window",
            "Focus the split pane to the left",
            false,
        ),
        (
            "split.focus_down",
            "Go to Lower Window",
            "Focus the split pane below",
            false,
        ),
        (
            "split.focus_up",
            "Go to Upper Window",
            "Focus the split pane above",
            false,
        ),
        (
            "split.focus_right",
            "Go to Right Window",
            "Focus the split pane to the right",
            false,
        ),
        (
            "panes.focus_left",
            "Go to Left Window or Panel",
            "Focus the split pane or side panel to the left",
            false,
        ),
        (
            "panes.focus_down",
            "Go to Lower Window or Panel",
            "Focus the split pane below",
            false,
        ),
        (
            "panes.focus_up",
            "Go to Upper Window or Panel",
            "Focus the split pane above",
            false,
        ),
        (
            "panes.focus_right",
            "Go to Right Window or Panel",
            "Focus the split pane or side panel to the right",
            false,
        ),
        (
            "panes.last",
            "Go to Last Window",
            "Switch focus back to the previous pane or side panel",
            false,
        ),
        (
            "split.vertical",
            "Split Window Right",
            "Split the current pane with another tab or URL on the right",
            false,
        ),
        (
            "split.horizontal",
            "Split Window Below",
            "Split the current pane with another tab or URL below",
            false,
        ),
        (
            "split.next",
            "Go to Next Window",
            "Focus the next pane in reading order",
            false,
        ),
        (
            "split.last",
            "Go to Last Used Window",
            "Focus the previously active pane in the split view",
            false,
        ),
        (
            "split.pick",
            "Pick Window",
            "Label each split pane chip to pick one",
            false,
        ),
        (
            "split.zoom",
            "Fill View with Window",
            "Toggle filling the split view with the active pane",
            false,
        ),
        (
            "split.equalize",
            "Make Windows Equal",
            "Reset split panes to equal sizes",
            false,
        ),
        (
            "split.grow_width",
            "Increase Window Width",
            "Widen the active split pane",
            true,
        ),
        (
            "split.shrink_width",
            "Decrease Window Width",
            "Narrow the active split pane",
            true,
        ),
        (
            "split.grow_height",
            "Increase Window Height",
            "Increase the height of the active split pane",
            true,
        ),
        (
            "split.shrink_height",
            "Decrease Window Height",
            "Decrease the height of the active split pane",
            true,
        ),
        (
            "split.swap",
            "Swap with Last Used Window",
            "Preview swapping the active pane with the last-used pane",
            false,
        ),
        ("split.rotate", "Rotate Windows", "Rotate split panes forward", false),
        (
            "split.rotate_back",
            "Rotate Windows Back",
            "Rotate split panes backward",
            false,
        ),
        (
            "split.edge_left",
            "Move Window to Far Left",
            "Preview moving the active pane to the far left edge",
            false,
        ),
        (
            "split.edge_down",
            "Move Window to Bottom",
            "Preview moving the active pane to the bottom edge",
            false,
        ),
        (
            "split.edge_up",
            "Move Window to Top",
            "Preview moving the active pane to the top edge",
            false,
        ),
        (
            "split.edge_right",
            "Move Window to Far Right",
            "Preview moving the active pane to the far right edge",
            false,
        ),
        (
            "split.grab",
            "Move Window…",
            "Pick up the active pane to move it in window mode",
            false,
        ),
    ];
    items
        .iter()
        .map(|&(name, title, description, counted)| CommandDescriptor {
            name: name.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            arguments: if counted { vec![count_arg()] } else { Vec::new() },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_split_action_descriptors_round_trip() {
        for desc in descriptors() {
            let parsed = Command::from_name_and_args(&desc.name, &[]).unwrap();
            assert_eq!(parsed.name(), desc.name);
        }
    }
}
