/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Window-scoped keymap context resolving Freya keyboard events through [`zendia_keymap`] into core commands.
//!
//! With vim mode off, keys resolve through the standard table. With it on, they go through two vim key engines, as in
//! the old fork: one for the page's modes (normal, insert, visual) and one for the browser's own surfaces (explorer,
//! toolbar, library, window mode), so keys half typed in one never leak into the other. The core's input mode says
//! which engine a key goes to, and the engine reports what it has pending back to the core for the mode indicator and
//! which-key.

use crate::input;
use crate::ui::core_view::CoreView;
use freya::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use zendia_browser_core::commands::Command;
use zendia_browser_core::input_state::{mode_code, vim_profile};
use zendia_browser_core::model::{BrowserModel, InputMode};
use zendia_browser_core::vim_keys::{VimCommand, commands_for_key};
use zendia_keymap::{CommandInvocation, KeyEngine, KeyMatch, KeyResolver, KeyStroke, KeyTable, ModeTables, Resolution};

/// Which of the two vim engines a key goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EngineKind {
    Page,
    Chrome,
}

impl EngineKind {
    fn for_mode(mode: InputMode) -> Self {
        match mode {
            InputMode::Explorer | InputMode::Toolbar | InputMode::Library | InputMode::Window => Self::Chrome,
            InputMode::Normal | InputMode::Insert | InputMode::Visual => Self::Page,
        }
    }
}

struct VimEngines {
    page: KeyEngine,
    chrome: KeyEngine,
    /// The timer generation each engine has a task waiting on.
    scheduled: [Option<u64>; 2],
}

impl VimEngines {
    fn new() -> Self {
        let tables = Arc::new(ModeTables::from_profile(vim_profile()));
        // NB: which-key is always on, so a prefix that is no mapping itself waits for the next key with no timeout.
        Self {
            page: KeyEngine::new(Arc::clone(&tables)).with_wait_for_prefix(true),
            chrome: KeyEngine::for_chrome(tables).with_wait_for_prefix(true),
            scheduled: [None, None],
        }
    }

    fn engine(&mut self, kind: EngineKind) -> &mut KeyEngine {
        match kind {
            EngineKind::Page => &mut self.page,
            EngineKind::Chrome => &mut self.chrome,
        }
    }

    fn engine_in_mode(&mut self, mode: InputMode) -> &mut KeyEngine {
        let engine = self.engine(EngineKind::for_mode(mode));
        if engine.set_mode(mode_code(mode)).is_err() {
            eprintln!("zendia: the vim engine has no mode `{}`", mode_code(mode));
        }
        engine
    }
}

/// What an engine did with a key or a timer, to act on once the engine is no longer borrowed.
struct EngineOutcome {
    kind: EngineKind,
    matches: Vec<KeyMatch>,
    pending: Option<(Vec<String>, Option<u32>)>,
    timer: Option<(u64, std::time::Duration)>,
}

impl EngineOutcome {
    fn collect(kind: EngineKind, engine: &mut KeyEngine, matches: Vec<KeyMatch>) -> Self {
        let pending = engine.take_state_changed().then(|| {
            let pending = engine.pending();
            (pending.tokens, pending.count)
        });
        Self {
            kind,
            matches,
            pending,
            timer: engine.armed_timer().map(|generation| (generation, engine.timeoutlen())),
        }
    }
}

struct KeymapInner {
    table: KeyTable,
    resolver: KeyResolver,
    consumed_key_codes: HashSet<Code>,
    vim: VimEngines,
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
                vim: VimEngines::new(),
            })),
        }
    }

    pub fn standard() -> Self {
        Self::new(KeyTable::standard())
    }

    /// Non-mutating preview used by focused [`PageSurface`]s on `KeyDown` (which fires before `GlobalKeyDown`) to
    /// decide whether the keymap will consume the key instead of forwarding it to the web page.
    pub fn will_consume_key_down(&self, e: &KeyboardEventData, model: &BrowserModel) -> bool {
        let stroke = input::key_stroke_from_event(&e.key, e.code, e.modifiers);
        let mut inner = self.inner.borrow_mut();
        let consumed = if model.vim_mode_enabled {
            let mode = model.input_mode;
            let mut preview = inner.vim.engine(EngineKind::for_mode(mode)).clone();
            let _ = preview.set_mode(mode_code(mode));
            preview.will_use(stroke)
        } else {
            match stroke {
                Some(stroke) => inner.resolver.will_consume(&inner.table, stroke),
                None => false,
            }
        };
        if consumed {
            inner.consumed_key_codes.insert(e.code);
        }
        consumed
    }

    /// Marks a `KeyDown` for `code` as used by the browser rather than the page, so its `KeyUp` stays away from the
    /// page too.
    pub fn consume_key_up(&self, code: Code) {
        self.inner.borrow_mut().consumed_key_codes.insert(code);
    }

    /// Returns true if the corresponding `KeyDown` for `code` was consumed by the keymap, preventing a stray `KeyUp`
    /// from reaching the page.
    pub fn take_consumed_key_up(&self, code: Code) -> bool {
        self.inner.borrow_mut().consumed_key_codes.remove(&code)
    }

    /// Resolves a global `KeyDown` event and invokes the resulting commands on `core`. Returns whether the key was
    /// used.
    pub fn handle_global_key_down(&self, e: &KeyboardEventData, core: &CoreView) -> bool {
        let stroke = input::key_stroke_from_event(&e.key, e.code, e.modifiers);
        let (vim_enabled, mode) = {
            let model = core.model().peek();
            (model.vim_mode_enabled, model.input_mode)
        };
        if vim_enabled {
            return self.feed_vim(stroke, mode, core);
        }
        let Some(stroke) = stroke else {
            return false;
        };
        let resolution = {
            let mut inner = self.inner.borrow_mut();
            let table = inner.table.clone();
            inner.resolver.resolve(&table, stroke)
        };
        if let Resolution::Matched(ref invocation) = resolution {
            dispatch_invocation(invocation, None, core);
        }
        resolution.is_consumed()
    }

    /// Drops whatever the vim engines have half typed, as when vim mode is turned off. The core clears what it shows
    /// of them itself.
    pub fn reset_vim(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.vim.page.reset();
        inner.vim.chrome.reset();
        inner.vim.page.take_state_changed();
        inner.vim.chrome.take_state_changed();
        inner.vim.scheduled = [None, None];
    }

    fn feed_vim(&self, stroke: Option<KeyStroke>, mode: InputMode, core: &CoreView) -> bool {
        let (consumed, outcome) = {
            let mut inner = self.inner.borrow_mut();
            let kind = EngineKind::for_mode(mode);
            let engine = inner.vim.engine_in_mode(mode);
            let mut matches = Vec::new();
            let consumed = engine.feed(stroke, &mut |_, matched| matches.push(matched));
            (consumed, EngineOutcome::collect(kind, engine, matches))
        };
        self.act_on(outcome, core);
        consumed
    }

    fn fire_timer(&self, kind: EngineKind, generation: u64, core: &CoreView) {
        let outcome = {
            let mut inner = self.inner.borrow_mut();
            if inner.vim.scheduled[kind as usize] == Some(generation) {
                inner.vim.scheduled[kind as usize] = None;
            }
            let engine = inner.vim.engine(kind);
            let mut matches = Vec::new();
            engine.fire_timer(generation, &mut |_, matched| matches.push(matched));
            EngineOutcome::collect(kind, engine, matches)
        };
        self.act_on(outcome, core);
    }

    fn act_on(&self, outcome: EngineOutcome, core: &CoreView) {
        if let Some((keys, count)) = outcome.pending {
            report_pending(core, keys, count);
        }
        let tab_before = core.model().peek().active_tab;
        for matched in &outcome.matches {
            let mut invocation = matched.binding.invocation.clone();
            if invocation.command == "visual.object" && !invocation.args.contains_key("keys") {
                invocation.args.insert(
                    "keys".to_string(),
                    zendia_keymap::ArgValue::String(matched.binding.keys.to_string()),
                );
            }
            dispatch_invocation(&invocation, matched.count, core);
            if invocation.command.starts_with("scroll.") {
                // The visual mode cursor follows the page once it has scrolled.
                crate::ui::vim::follow_page_scroll(core);
            }
        }
        // A key that moved to another tab leaves that page in normal mode: a field it kept focused would turn the
        // next <C-h> into a deleted character and the next J into a typed one. `gi` goes back to it.
        if !outcome.matches.is_empty() && core.model().peek().active_tab != tab_before {
            core.invoke_or_log(Command::Vim(VimCommand::LeaveField));
        }
        if let Some((generation, timeoutlen)) = outcome.timer {
            let needs_task = {
                let mut inner = self.inner.borrow_mut();
                let slot = &mut inner.vim.scheduled[outcome.kind as usize];
                let needs_task = *slot != Some(generation);
                *slot = Some(generation);
                needs_task
            };
            if needs_task {
                let controller = self.clone();
                let core = core.clone();
                let kind = outcome.kind;
                spawn(async move {
                    timer(timeoutlen).await;
                    controller.fire_timer(kind, generation, &core);
                });
            }
        }
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

fn report_pending(core: &CoreView, keys: Vec<String>, count: Option<u32>) {
    core.invoke_or_log(Command::Vim(VimCommand::ReportPending { keys, count }));
}

fn dispatch_invocation(invocation: &CommandInvocation, count: Option<u32>, core: &CoreView) {
    let args = invocation.args_as_pairs();
    match commands_for_key(&invocation.command, &args, count) {
        Ok(commands) => {
            for command in commands {
                core.invoke_or_log(command);
            }
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
    fn normalizes_keys_like_the_fork() {
        // A chorded key is the physical key, whatever character the layout gives it.
        assert_eq!(
            input::key_stroke_from_event(&Key::Character("∆".to_string()), Code::KeyJ, Modifiers::ALT),
            Some(KeyStroke::from_str("<A-j>").unwrap())
        );
        assert_eq!(
            input::key_stroke_from_event(
                &Key::Character("D".to_string()),
                Code::KeyD,
                Modifiers::CONTROL | Modifiers::SHIFT
            ),
            Some(KeyStroke::from_str("<C-S-d>").unwrap())
        );
        // AltGr types the character it produces.
        assert_eq!(
            input::key_stroke_from_event(
                &Key::Character("@".to_string()),
                Code::KeyQ,
                Modifiers::CONTROL | Modifiers::ALT | Modifiers::ALT_GRAPH
            ),
            Some(KeyStroke::char('@'))
        );
        // Shift is part of a printable character.
        assert_eq!(
            input::key_stroke_from_event(&Key::Character("H".to_string()), Code::KeyH, Modifiers::SHIFT),
            Some(KeyStroke::from_str("H").unwrap())
        );
        assert_eq!(
            input::key_stroke_from_event(&Key::Character(" ".to_string()), Code::Space, Modifiers::SHIFT)
                .map(|stroke| stroke.to_string()),
            Some("<S-Space>".to_string())
        );
    }

    #[test]
    fn keymap_controller_tracks_consumed_key_up() {
        let controller = KeymapController::standard();
        let model = BrowserModel::default();
        let ctrl_t = KeyboardEventData::new(Key::Character("t".to_string()), Code::KeyT, Modifiers::CONTROL);
        assert!(controller.will_consume_key_down(&ctrl_t, &model));
        assert!(controller.take_consumed_key_up(Code::KeyT));
        assert!(!controller.take_consumed_key_up(Code::KeyT));

        let plain_a = KeyboardEventData::new(Key::Character("a".to_string()), Code::KeyA, Modifiers::empty());
        assert!(!controller.will_consume_key_down(&plain_a, &model));
        assert!(!controller.take_consumed_key_up(Code::KeyA));
    }

    #[test]
    fn vim_mode_previews_keys_by_mode() {
        let controller = KeymapController::standard();
        let mut model = BrowserModel {
            vim_mode_enabled: true,
            ..BrowserModel::default()
        };
        let j = KeyboardEventData::new(Key::Character("j".to_string()), Code::KeyJ, Modifiers::empty());
        let q = KeyboardEventData::new(Key::Character("q".to_string()), Code::KeyQ, Modifiers::empty());
        assert!(controller.will_consume_key_down(&j, &model), "j scrolls in normal mode");
        assert!(!controller.will_consume_key_down(&q, &model), "q is the page's");

        model.input_mode = InputMode::Insert;
        assert!(!controller.will_consume_key_down(&j, &model), "j types in insert mode");

        model.input_mode = InputMode::Visual;
        assert!(
            controller.will_consume_key_down(&q, &model),
            "visual mode keeps stray keys"
        );
    }
}
