/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Split view and window mode actions of the vim profile: moving between panes, resizing, zooming, swapping,
//! rotating, edge-moving, grabbing, and picking panes.

use crate::commands::{Command, CommandDescriptor, CommandError};
use crate::focus::PaneDirection;

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

/// Split view and pane-focus actions invoked by vim keymaps.
#[derive(Clone, Debug, PartialEq)]
pub enum SplitAction {
    /// Focus the neighboring pane in `direction` (`<C-h/j/k/l>`, `<leader>wh/j/k/l>`, `w` mode `h/j/k/l`).
    Focus { direction: PaneDirection },
    /// Go back to the last focused pane or panel (`<C-\>`).
    PanesLast,
    /// Cycle to the next pane in the active split group (`<leader>ww`, `w` mode `w`).
    Next,
    /// Switch to the most recently used other pane in the active split group (`<leader>wp`, `w` mode `p`).
    Last,
    /// Open a picker for the panes of the active split group (`<leader>wf`, `w` mode `f`).
    Pick,
    /// Move the active pane out of its split group into its own tab (`<leader>wt`, `w` mode `t` without `tab_id`).
    RemoveActiveTab,
    /// Toggle maximizing the active pane within its split group (`<leader>wm`, `w` mode `m`).
    Zoom,
    /// Reset all panes in the active split group to equal sizes (`<leader>w=`, `w` mode `=`).
    Equalize,
    /// Resize the active pane by one step (`<C-Up/Down/Left/Right>`, `<leader>w+/-/>/<`).
    Resize { kind: ResizeKind },
    /// Swap the active pane with the last-used other pane (`<leader>wx`, `w` mode `x`).
    Swap,
    /// Rotate panes forward (`<leader>wr`) or backward (`<leader>wR`).
    Rotate { reverse: bool },
    /// Move the active pane to an edge of the split layout (`<leader>wH/J/K/L`, `w` mode `H/J/K/L`).
    Edge { direction: PaneDirection },
    /// Enter carry/grab mode for the active pane (`<leader>wg`, `w` mode `g`).
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
            Self::PanesLast => "panes.last",
            Self::Next => "split.next",
            Self::Last => "split.last",
            Self::Pick => "split.pick",
            Self::RemoveActiveTab => "split.remove_tab",
            Self::Zoom => "split.zoom",
            Self::Equalize => "split.equalize",
            Self::Resize { kind } => kind.name(),
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

/// Parses the commands of this module, or returns `None` for a name it does not own.
pub fn parse_command(name: &str, _args: &[(String, String)]) -> Result<Option<Command>, CommandError> {
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
        "panes.last" => SplitAction::PanesLast,
        "split.next" => SplitAction::Next,
        "split.last" => SplitAction::Last,
        "split.pick" => SplitAction::Pick,
        "split.zoom" => SplitAction::Zoom,
        "split.equalize" => SplitAction::Equalize,
        "split.grow_width" => SplitAction::Resize {
            kind: ResizeKind::GrowWidth,
        },
        "split.shrink_width" => SplitAction::Resize {
            kind: ResizeKind::ShrinkWidth,
        },
        "split.grow_height" => SplitAction::Resize {
            kind: ResizeKind::GrowHeight,
        },
        "split.shrink_height" => SplitAction::Resize {
            kind: ResizeKind::ShrinkHeight,
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
    let items: &[(&str, &str, &str)] = &[
        (
            "split.focus_left",
            "Go to Left Window",
            "Focus the split pane to the left",
        ),
        ("split.focus_down", "Go to Lower Window", "Focus the split pane below"),
        ("split.focus_up", "Go to Upper Window", "Focus the split pane above"),
        (
            "split.focus_right",
            "Go to Right Window",
            "Focus the split pane to the right",
        ),
        (
            "panes.last",
            "Go to Last Window",
            "Switch focus back to the previous pane",
        ),
        (
            "split.next",
            "Next Window",
            "Focus the next pane in the active split view",
        ),
        (
            "split.last",
            "Last Window",
            "Focus the previously active pane in the split view",
        ),
        ("split.pick", "Pick Window", "Pick a pane from the active split view"),
        (
            "split.zoom",
            "Maximize Window",
            "Toggle maximizing the active split pane",
        ),
        ("split.equalize", "Equalize Windows", "Reset split panes to equal sizes"),
        (
            "split.grow_width",
            "Increase Window Width",
            "Widen the active split pane",
        ),
        (
            "split.shrink_width",
            "Decrease Window Width",
            "Narrow the active split pane",
        ),
        (
            "split.grow_height",
            "Increase Window Height",
            "Increase the height of the active split pane",
        ),
        (
            "split.shrink_height",
            "Decrease Window Height",
            "Decrease the height of the active split pane",
        ),
        (
            "split.swap",
            "Swap with Last Used Window",
            "Swap the active pane with the last-used pane",
        ),
        ("split.rotate", "Rotate Windows", "Rotate split panes forward"),
        (
            "split.rotate_back",
            "Rotate Windows Backward",
            "Rotate split panes backward",
        ),
        (
            "split.edge_left",
            "Move Window to Left Edge",
            "Move the active pane to the left edge",
        ),
        (
            "split.edge_down",
            "Move Window to Bottom Edge",
            "Move the active pane to the bottom edge",
        ),
        (
            "split.edge_up",
            "Move Window to Top Edge",
            "Move the active pane to the top edge",
        ),
        (
            "split.edge_right",
            "Move Window to Right Edge",
            "Move the active pane to the right edge",
        ),
        (
            "split.grab",
            "Move Window...",
            "Pick up the active pane to move it in window mode",
        ),
    ];
    items
        .iter()
        .map(|&(name, title, description)| CommandDescriptor {
            name: name.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            arguments: Vec::new(),
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
