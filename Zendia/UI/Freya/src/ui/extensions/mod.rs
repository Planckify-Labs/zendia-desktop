/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! WebExtension UI surfaces: real icon renderer, HTML popup popover, modal consent prompt, install card, and shortcut
//! matcher.

pub mod icon;
pub mod install;
pub mod popup;
pub mod prompt;

pub use icon::ExtensionIconView;
pub use install::ExtensionInstallCard;
pub use popup::{
    ExtensionPopupOverlay, ExtensionPopupState, OpenExtensionPopup, assign_popup_hint_labels, popup_hints_will_consume,
    try_use_extension_popup_state, use_extension_popup_state, use_provide_extension_popup_state,
};
pub use prompt::ExtensionPromptOverlay;

use freya::prelude::{Code, KeyboardEventData, Modifiers};
use zendia_browser_core::modules::ExtensionCommand;

/// Matches a keyboard event against registered extension manifest shortcuts (e.g. `"Ctrl+Shift+Y"`, `"Alt+M"`).
pub fn match_extension_shortcut<'a>(
    event: &KeyboardEventData,
    commands: &'a [ExtensionCommand],
) -> Option<&'a ExtensionCommand> {
    commands.iter().find(|cmd| shortcut_matches_event(&cmd.shortcut, event))
}

fn shortcut_matches_event(shortcut: &str, event: &KeyboardEventData) -> bool {
    let mut need_ctrl = false;
    let mut need_alt = false;
    let mut need_shift = false;
    let mut need_meta = false;
    let mut key_token: Option<&str> = None;

    for raw in shortcut.split('+') {
        let part = raw.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "command" | "macctrl" => need_ctrl = true,
            "alt" | "option" => need_alt = true,
            "shift" => need_shift = true,
            "meta" | "super" => need_meta = true,
            _ => key_token = Some(part),
        }
    }

    let Some(token) = key_token else {
        return false;
    };

    if event.modifiers.contains(Modifiers::CONTROL) != need_ctrl
        || event.modifiers.contains(Modifiers::ALT) != need_alt
        || event.modifiers.contains(Modifiers::SHIFT) != need_shift
        || event.modifiers.contains(Modifiers::META) != need_meta
    {
        return false;
    }

    code_matches_token(event.code, token)
}

fn code_matches_token(code: Code, token: &str) -> bool {
    let upper = token.to_ascii_uppercase();
    match upper.as_str() {
        "A" => code == Code::KeyA,
        "B" => code == Code::KeyB,
        "C" => code == Code::KeyC,
        "D" => code == Code::KeyD,
        "E" => code == Code::KeyE,
        "F" => code == Code::KeyF,
        "G" => code == Code::KeyG,
        "H" => code == Code::KeyH,
        "I" => code == Code::KeyI,
        "J" => code == Code::KeyJ,
        "K" => code == Code::KeyK,
        "L" => code == Code::KeyL,
        "M" => code == Code::KeyM,
        "N" => code == Code::KeyN,
        "O" => code == Code::KeyO,
        "P" => code == Code::KeyP,
        "Q" => code == Code::KeyQ,
        "R" => code == Code::KeyR,
        "S" => code == Code::KeyS,
        "T" => code == Code::KeyT,
        "U" => code == Code::KeyU,
        "V" => code == Code::KeyV,
        "W" => code == Code::KeyW,
        "X" => code == Code::KeyX,
        "Y" => code == Code::KeyY,
        "Z" => code == Code::KeyZ,
        "0" => code == Code::Digit0,
        "1" => code == Code::Digit1,
        "2" => code == Code::Digit2,
        "3" => code == Code::Digit3,
        "4" => code == Code::Digit4,
        "5" => code == Code::Digit5,
        "6" => code == Code::Digit6,
        "7" => code == Code::Digit7,
        "8" => code == Code::Digit8,
        "9" => code == Code::Digit9,
        "SPACE" => code == Code::Space,
        "UP" => code == Code::ArrowUp,
        "DOWN" => code == Code::ArrowDown,
        "LEFT" => code == Code::ArrowLeft,
        "RIGHT" => code == Code::ArrowRight,
        "HOME" => code == Code::Home,
        "END" => code == Code::End,
        "PAGEUP" => code == Code::PageUp,
        "PAGEDOWN" => code == Code::PageDown,
        "INSERT" => code == Code::Insert,
        "DELETE" => code == Code::Delete,
        "COMMA" | "," => code == Code::Comma,
        "PERIOD" | "." => code == Code::Period,
        "F1" => code == Code::F1,
        "F2" => code == Code::F2,
        "F3" => code == Code::F3,
        "F4" => code == Code::F4,
        "F5" => code == Code::F5,
        "F6" => code == Code::F6,
        "F7" => code == Code::F7,
        "F8" => code == Code::F8,
        "F9" => code == Code::F9,
        "F10" => code == Code::F10,
        "F11" => code == Code::F11,
        "F12" => code == Code::F12,
        _ => false,
    }
}
