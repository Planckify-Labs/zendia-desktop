/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Vim mode overlays: statusline and mascot, which-key popup, hint labels, picker, and focus rings.

pub mod explorer;
pub mod hints;
pub mod toolbar_focus;
pub mod window_mode;

use crate::ui::core_view::CoreView;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::focus::{FocusDirection, PaneDirection};
use zendia_browser_core::model::{BrowserModel, InputMode};

/// Whether an active vim focus surface (explorer, toolbar, or window mode) should capture plain keystrokes instead of
/// forwarding them to the focused web page.
pub fn focus_mode_captures_keys(model: &BrowserModel) -> bool {
    model.vim_mode_enabled
        && matches!(
            model.input_mode,
            InputMode::Explorer | InputMode::Toolbar | InputMode::Window
        )
}

/// Dispatches keyboard navigation for active vim focus surfaces (explorer, hint labels, toolbar focus, window mode).
///
/// Returns `true` when the key was handled and should not fall through to the standard keymap or page.
pub fn handle_focus_key_down(e: &KeyboardEventData, core: &CoreView) -> bool {
    // Let modified shortcuts (Ctrl/Alt/Super) pass through to the standard keymap.
    if e.modifiers
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
    {
        return false;
    }

    let model = core.model().peek().clone();
    if !focus_mode_captures_keys(&model) {
        return false;
    }

    // Active jump hints take priority over mode navigation keys.
    if model.focus.hints.is_some() {
        match &e.key {
            Key::Named(NamedKey::Escape) => {
                core.invoke_or_log(Command::FocusHintCancel);
            }
            Key::Character(text) if text.chars().count() == 1 => {
                core.invoke_or_log(Command::FocusHintInput {
                    key: text.to_ascii_lowercase(),
                });
            }
            _ => {
                core.invoke_or_log(Command::FocusHintCancel);
            }
        }
        return true;
    }

    let shift = e.modifiers.contains(Modifiers::SHIFT);
    match model.input_mode {
        InputMode::Explorer => handle_explorer_key(&e.key, shift, core),
        InputMode::Toolbar => handle_toolbar_key(&e.key, core),
        InputMode::Window => handle_window_mode_key(&e.key, shift, &model, core),
        _ => false,
    }
}

fn handle_explorer_key(key: &Key, shift: bool, core: &CoreView) -> bool {
    let command = match key {
        Key::Named(NamedKey::Escape) => Command::ExplorerClose,
        Key::Named(NamedKey::ArrowDown) => Command::ExplorerMove {
            direction: FocusDirection::Next,
        },
        Key::Named(NamedKey::ArrowUp) => Command::ExplorerMove {
            direction: FocusDirection::Previous,
        },
        Key::Named(NamedKey::ArrowRight) => Command::ExplorerMove {
            direction: FocusDirection::Into,
        },
        Key::Named(NamedKey::ArrowLeft) => Command::ExplorerMove {
            direction: FocusDirection::Out,
        },
        Key::Named(NamedKey::Home) => Command::ExplorerMove {
            direction: FocusDirection::First,
        },
        Key::Named(NamedKey::End) => Command::ExplorerMove {
            direction: FocusDirection::Last,
        },
        Key::Named(NamedKey::Tab) => Command::ExplorerSelect { range: false },
        Key::Named(NamedKey::Enter) => Command::ExplorerActivate,
        Key::Character(ch) => match ch.as_str() {
            "q" => Command::ExplorerClose,
            "j" => Command::ExplorerMove {
                direction: FocusDirection::Next,
            },
            "k" => Command::ExplorerMove {
                direction: FocusDirection::Previous,
            },
            "l" => Command::ExplorerMove {
                direction: FocusDirection::Into,
            },
            "h" => Command::ExplorerMove {
                direction: FocusDirection::Out,
            },
            "g" if !shift => Command::ExplorerMove {
                direction: FocusDirection::First,
            },
            "G" | "g" => Command::ExplorerMove {
                direction: FocusDirection::Last,
            },
            "v" | "V" => Command::ExplorerSelect { range: true },
            "f" => Command::ExplorerPick,
            " " => Command::ExplorerActivate,
            _ => return false,
        },
        _ => return false,
    };
    core.invoke_or_log(command);
    true
}

fn handle_toolbar_key(key: &Key, core: &CoreView) -> bool {
    let command = match key {
        Key::Named(NamedKey::Escape) => Command::ToolbarFocusClose,
        Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::Tab) => {
            Command::ToolbarFocusMove {
                direction: FocusDirection::Next,
            }
        }
        Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowUp) => Command::ToolbarFocusMove {
            direction: FocusDirection::Previous,
        },
        Key::Named(NamedKey::Home) => Command::ToolbarFocusMove {
            direction: FocusDirection::First,
        },
        Key::Named(NamedKey::End) => Command::ToolbarFocusMove {
            direction: FocusDirection::Last,
        },
        Key::Named(NamedKey::Enter) => Command::ToolbarFocusActivate,
        Key::Character(ch) => match ch.as_str() {
            "q" => Command::ToolbarFocusClose,
            "l" | "j" => Command::ToolbarFocusMove {
                direction: FocusDirection::Next,
            },
            "h" | "k" => Command::ToolbarFocusMove {
                direction: FocusDirection::Previous,
            },
            "0" => Command::ToolbarFocusMove {
                direction: FocusDirection::First,
            },
            "$" => Command::ToolbarFocusMove {
                direction: FocusDirection::Last,
            },
            " " => Command::ToolbarFocusActivate,
            _ => return false,
        },
        _ => return false,
    };
    core.invoke_or_log(command);
    true
}

fn handle_window_mode_key(key: &Key, shift: bool, model: &BrowserModel, core: &CoreView) -> bool {
    let carrying = model.focus.window_carry.is_some();
    let command = match key {
        Key::Named(NamedKey::Escape) => {
            if carrying {
                Command::WindowModeCarryToggle
            } else {
                Command::WindowModeClose
            }
        }
        Key::Named(NamedKey::Enter) if carrying => Command::WindowModeDrop,
        Key::Named(NamedKey::ArrowLeft) => Command::WindowModeMovePane {
            direction: PaneDirection::Left,
        },
        Key::Named(NamedKey::ArrowRight) => Command::WindowModeMovePane {
            direction: PaneDirection::Right,
        },
        Key::Named(NamedKey::ArrowUp) => Command::WindowModeMovePane {
            direction: PaneDirection::Up,
        },
        Key::Named(NamedKey::ArrowDown) => Command::WindowModeMovePane {
            direction: PaneDirection::Down,
        },
        Key::Character(ch) => match ch.as_str() {
            "q" => Command::WindowModeClose,
            "1" => Command::WindowModeFocusPane { pane: 1 },
            "2" => Command::WindowModeFocusPane { pane: 2 },
            "3" => Command::WindowModeFocusPane { pane: 3 },
            "4" => Command::WindowModeFocusPane { pane: 4 },
            "m" => Command::WindowModeCarryToggle,
            "H" => Command::WindowModeMovePane {
                direction: PaneDirection::Left,
            },
            "L" => Command::WindowModeMovePane {
                direction: PaneDirection::Right,
            },
            "K" => Command::WindowModeMovePane {
                direction: PaneDirection::Up,
            },
            "J" => Command::WindowModeMovePane {
                direction: PaneDirection::Down,
            },
            "h" | "j" | "k" | "l" => {
                let dir = match ch.as_str() {
                    "h" => PaneDirection::Left,
                    "j" => PaneDirection::Down,
                    "k" => PaneDirection::Up,
                    _ => PaneDirection::Right,
                };
                if carrying || shift {
                    Command::WindowModeMovePane { direction: dir }
                } else if let (Some(group), Some(active)) = (model.active_split_group(), model.active_tab)
                    && let Some(neighbor) = zendia_browser_core::focus::neighbor_pane(group, active, dir)
                    && let Some(idx) = group.tab_ids.iter().position(|&id| id == neighbor)
                {
                    Command::WindowModeFocusPane { pane: idx + 1 }
                } else {
                    return true;
                }
            }
            _ => return false,
        },
        _ => return false,
    };
    core.invoke_or_log(command);
    true
}
