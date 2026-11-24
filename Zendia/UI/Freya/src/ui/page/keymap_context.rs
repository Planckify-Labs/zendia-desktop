/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Window-scoped keymap context resolving Freya keyboard events through [`zendia_keymap`] into core commands.

use crate::input;
use crate::ui::core_view::CoreView;
use freya::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use zendia_browser_core::commands::Command;
use zendia_keymap::{CommandInvocation, KeyResolver, KeyTable, Resolution};

struct KeymapInner {
    table: KeyTable,
    resolver: KeyResolver,
    consumed_key_codes: HashSet<Code>,
}

/// Shared keymap state installed in Freya context so both global key handlers and [`PageSurface`]s share one table
/// and resolver without double-processing key events.
#[derive(Clone)]
pub struct KeymapController {
    inner: Rc<RefCell<KeymapInner>>,
}

impl PartialEq for KeymapController {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl KeymapController {
    pub fn new(table: KeyTable) -> Self {
        Self {
            inner: Rc::new(RefCell::new(KeymapInner {
                table,
                resolver: KeyResolver::new(),
                consumed_key_codes: HashSet::new(),
            })),
        }
    }

    pub fn standard() -> Self {
        Self::new(KeyTable::standard())
    }

    /// Non-mutating preview used by focused [`PageSurface`]s on `KeyDown` (which fires before `GlobalKeyDown`) to
    /// decide whether the keymap will consume the key instead of forwarding it to the web page.
    pub fn will_consume_key_down(&self, e: &KeyboardEventData) -> bool {
        let Some(stroke) = input::key_stroke_from_event(&e.key, e.code, e.modifiers) else {
            return false;
        };
        let mut inner = self.inner.borrow_mut();
        let consumed = inner.resolver.will_consume(&inner.table, stroke);
        if consumed {
            inner.consumed_key_codes.insert(e.code);
        }
        consumed
    }

    /// Returns true if the corresponding `KeyDown` for `code` was consumed by the keymap, preventing a stray `KeyUp`
    /// from reaching the page.
    pub fn take_consumed_key_up(&self, code: Code) -> bool {
        self.inner.borrow_mut().consumed_key_codes.remove(&code)
    }

    /// Resolves a global `KeyDown` event against the active keymap table and invokes the resulting command on `core`.
    pub fn handle_global_key_down(&self, e: &KeyboardEventData, core: &CoreView) -> Resolution {
        let Some(stroke) = input::key_stroke_from_event(&e.key, e.code, e.modifiers) else {
            return Resolution::Unmatched;
        };
        let resolution = {
            let mut inner = self.inner.borrow_mut();
            let table = inner.table.clone();
            inner.resolver.resolve(&table, stroke)
        };
        if let Resolution::Matched(ref invocation) = resolution {
            dispatch_invocation(invocation, core);
        }
        resolution
    }

    /// Replaces the compiled key table (for instance when switching profiles or receiving a table from InputServer).
    #[allow(dead_code)]
    pub fn set_table(&self, table: KeyTable) {
        let mut inner = self.inner.borrow_mut();
        inner.table = table;
        inner.resolver.reset();
        inner.consumed_key_codes.clear();
    }
}

fn dispatch_invocation(invocation: &CommandInvocation, core: &CoreView) {
    let args = invocation.args_as_pairs();
    match Command::from_name_and_args(&invocation.command, &args) {
        Ok(command) => {
            core.invoke_or_log(command);
        }
        Err(error) => {
            eprintln!(
                "zendia: keymap command `{}` failed to parse: {error}",
                invocation.command
            );
        }
    }
}

/// Installs the standard [`KeymapController`] into Freya context.
pub fn use_provide_keymap() -> KeymapController {
    use_provide_context(KeymapController::standard)
}

/// Accesses the window's [`KeymapController`] from Freya context.
pub fn use_keymap() -> KeymapController {
    use_consume::<KeymapController>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use zendia_keymap::{KeyCode as MapKeyCode, KeyModifiers as MapModifiers, KeyStroke};

    #[test]
    fn converts_freya_key_events_to_keymap_strokes() {
        assert_eq!(
            input::key_stroke_from_event(&Key::Character("t".to_string()), Code::KeyT, Modifiers::CONTROL),
            Some(KeyStroke::from_str("<C-t>").unwrap())
        );
        assert_eq!(
            input::key_stroke_from_event(
                &Key::Named(NamedKey::Tab),
                Code::Tab,
                Modifiers::CONTROL | Modifiers::SHIFT
            ),
            Some(KeyStroke::from_str("<C-S-Tab>").unwrap())
        );
        assert_eq!(
            input::key_stroke_from_event(&Key::Named(NamedKey::ArrowLeft), Code::ArrowLeft, Modifiers::ALT),
            Some(KeyStroke::from_str("<A-Left>").unwrap())
        );
        assert_eq!(
            input::key_stroke_from_event(&Key::Named(NamedKey::F5), Code::F5, Modifiers::empty()),
            Some(KeyStroke::from_str("<F5>").unwrap())
        );
        // X11 control char fallback (Ctrl+T -> 0x14, Ctrl+Alt+M -> 0x0d '\r' or NamedKey::Enter with Code::KeyM).
        assert_eq!(
            input::key_stroke_from_event(&Key::Character("\x14".to_string()), Code::KeyT, Modifiers::CONTROL),
            Some(KeyStroke::new(MapModifiers::CTRL, MapKeyCode::Char('t')))
        );
        assert_eq!(
            input::key_stroke_from_event(
                &Key::Character("\r".to_string()),
                Code::KeyM,
                Modifiers::CONTROL | Modifiers::ALT
            ),
            Some(KeyStroke::new(
                MapModifiers::CTRL | MapModifiers::ALT,
                MapKeyCode::Char('m')
            ))
        );
        assert_eq!(
            input::key_stroke_from_event(
                &Key::Named(NamedKey::Enter),
                Code::KeyM,
                Modifiers::CONTROL | Modifiers::ALT
            ),
            Some(KeyStroke::from_str("<C-A-m>").unwrap())
        );
        assert_eq!(
            input::key_stroke_from_event(&Key::Named(NamedKey::Enter), Code::Enter, Modifiers::CONTROL),
            Some(KeyStroke::from_str("<C-CR>").unwrap())
        );
        // Bare modifiers do not produce strokes.
        assert_eq!(
            input::key_stroke_from_event(&Key::Named(NamedKey::Control), Code::ControlLeft, Modifiers::CONTROL),
            None
        );
    }

    #[test]
    fn keymap_controller_tracks_consumed_key_up() {
        let controller = KeymapController::standard();
        let ctrl_t = KeyboardEventData::new(Key::Character("t".to_string()), Code::KeyT, Modifiers::CONTROL);
        assert!(controller.will_consume_key_down(&ctrl_t));
        assert!(controller.take_consumed_key_up(Code::KeyT));
        assert!(!controller.take_consumed_key_up(Code::KeyT));

        let plain_a = KeyboardEventData::new(Key::Character("a".to_string()), Code::KeyA, Modifiers::empty());
        assert!(!controller.will_consume_key_down(&plain_a));
        assert!(!controller.take_consumed_key_up(Code::KeyA));
    }
}
