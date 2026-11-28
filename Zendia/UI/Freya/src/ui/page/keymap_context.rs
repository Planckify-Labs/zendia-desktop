/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Window-scoped keymap context resolving Freya keyboard events through [`zendia_keymap`] into core commands.
//!
//! With vim mode off, keys resolve through the standard table. With it on, they go through per-page vim key engines
//! (one per tab or auxiliary view for normal, insert, and visual modes) plus one chrome engine for the browser's own
//! surfaces (explorer, toolbar, library, window mode), falling back to `standard.toml` whenever an idle vim engine
//! leaves a key unused (`RT-01`, `RT-02`, `MD-01`).

use crate::input;
use crate::ui::core_view::CoreView;
use crate::ui::page::surface_store::PageSurfaceContext;
use freya::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use zendia_browser_core::commands::Command;
use zendia_browser_core::input_state::{active_vim_profile, mode_code};
use zendia_browser_core::model::{BrowserModel, InputMode, TabId};
use zendia_browser_core::vim_keys::{VimCommand, commands_for_key};
use zendia_keymap::{CommandInvocation, KeyEngine, KeyMatch, KeyResolver, KeyStroke, KeyTable, ModeTables, Resolution};

const MAX_HELD_KEYS: usize = 64;

/// Which engine slot a key targets: either the window's chrome surface engine or a specific page's engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum EngineSlot {
    Page(Option<TabId>),
    Chrome,
}

impl EngineSlot {
    fn for_model(mode: InputMode, page_id: Option<TabId>) -> Self {
        match mode {
            InputMode::Explorer | InputMode::Toolbar | InputMode::Library | InputMode::Window => Self::Chrome,
            InputMode::Normal | InputMode::Insert | InputMode::Visual => Self::Page(page_id),
        }
    }
}

/// Insert-mode bindings that apply only while an editable field actually holds focus (`test_ctrl_w_outside_a_field_still_closes_the_tab`).
fn insert_binding_requires_field(command: &str) -> bool {
    command.starts_with("edit.") || command.starts_with("yank.put_")
}

struct VimEngines {
    tables: Arc<ModeTables>,
    pages: HashMap<TabId, KeyEngine>,
    fallback_page: KeyEngine,
    chrome: KeyEngine,
    /// Timer generation each engine slot currently has a task waiting on.
    scheduled: HashMap<EngineSlot, u64>,
}

impl VimEngines {
    fn new() -> Self {
        let profile = active_vim_profile();
        let tables = Arc::new(ModeTables::from_profile(&profile));
        // NB: which-key is always on, so a prefix that is no mapping itself waits for the next key with no timeout.
        Self {
            fallback_page: KeyEngine::new(Arc::clone(&tables)).with_wait_for_prefix(true),
            chrome: KeyEngine::for_chrome(Arc::clone(&tables)).with_wait_for_prefix(true),
            tables,
            pages: HashMap::new(),
            scheduled: HashMap::new(),
        }
    }

    fn engine(&mut self, slot: EngineSlot) -> &mut KeyEngine {
        match slot {
            EngineSlot::Chrome => &mut self.chrome,
            EngineSlot::Page(Some(tab_id)) => {
                let tables = Arc::clone(&self.tables);
                let timeoutlen = self.fallback_page.timeoutlen();
                self.pages.entry(tab_id).or_insert_with(|| {
                    KeyEngine::new(tables)
                        .with_wait_for_prefix(true)
                        .with_timeoutlen(timeoutlen)
                })
            }
            EngineSlot::Page(None) => &mut self.fallback_page,
        }
    }

    fn engine_in_mode(&mut self, slot: EngineSlot, mode: InputMode) -> &mut KeyEngine {
        let engine = self.engine(slot);
        if engine.set_mode(mode_code(mode)).is_err() {
            eprintln!("zendia: the vim engine has no mode `{}`", mode_code(mode));
        }
        engine
    }
}

/// What an engine did with a key or a timer, to act on once the engine is no longer borrowed.
struct EngineOutcome {
    slot: EngineSlot,
    matches: Vec<KeyMatch>,
    pending: Option<(Vec<String>, Option<u32>)>,
    timer: Option<(u64, std::time::Duration)>,
}

impl EngineOutcome {
    fn collect(slot: EngineSlot, engine: &mut KeyEngine, matches: Vec<KeyMatch>) -> Self {
        let pending = engine.take_state_changed().then(|| {
            let pending = engine.pending();
            (pending.tokens, pending.count)
        });
        Self {
            slot,
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
    held_keys: VecDeque<KeyboardEventData>,
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
                held_keys: VecDeque::new(),
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
            let slot = EngineSlot::for_model(mode, model.effective_keyed_tab());
            let mut preview = inner.vim.engine(slot).clone();
            let _ = preview.set_mode(mode_code(mode));
            let pending_before = preview.pending();
            let was_idle = pending_before.tokens.is_empty() && pending_before.count.is_none();
            let mut matched = Vec::new();
            let mut vim_used = preview.feed(stroke, &mut |_, m| matched.push(m));
            if mode == InputMode::Insert
                && !model.page_editing
                && matched.len() == 1
                && insert_binding_requires_field(&matched[0].binding.invocation.command)
            {
                vim_used = false;
            }
            if vim_used {
                true
            } else if was_idle && mode == InputMode::Window && stroke.is_some() {
                // `WIN-21`: an unused key in Window mode closes Window mode and re-runs in Normal mode.
                let page_slot = EngineSlot::Page(model.effective_keyed_tab());
                let mut normal_preview = inner.vim.engine(page_slot).clone();
                let _ = normal_preview.set_mode(mode_code(InputMode::Normal));
                if normal_preview.feed(stroke, &mut |_, _| {}) {
                    true
                } else {
                    match stroke {
                        Some(stroke) => inner.resolver.will_consume(&inner.table, stroke),
                        None => false,
                    }
                }
            } else if was_idle {
                match stroke {
                    Some(stroke) => inner.resolver.will_consume(&inner.table, stroke),
                    None => false,
                }
            } else {
                false
            }
        } else {
            match stroke {
                Some(stroke) => inner.resolver.will_consume(&inner.table, stroke),
                None => false,
            }
        };
        if consumed {
            inner.consumed_key_codes.insert(e.code);
        } else {
            inner.consumed_key_codes.remove(&e.code);
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

    /// Queues a `KeyDown` event while an async mode-changing page action is in flight (`RT-04`).
    pub fn hold_key_down(&self, data: KeyboardEventData) {
        let mut inner = self.inner.borrow_mut();
        inner.consumed_key_codes.insert(data.code);
        if inner.held_keys.len() < MAX_HELD_KEYS {
            inner.held_keys.push_back(data);
        }
    }

    /// Replays any queued keys once `mode_transition_pending` clears (`RT-04`).
    pub fn replay_held_keys(&self, core: &CoreView, page_ctx: &PageSurfaceContext) {
        loop {
            if core.model().peek().mode_transition_pending {
                break;
            }
            let next_ev = self.inner.borrow_mut().held_keys.pop_front();
            let Some(ev) = next_ev else {
                break;
            };
            let _ = self.take_consumed_key_up(ev.code);
            if crate::ui::vim::handle_focus_key_down(&ev, core) {
                continue;
            }
            let consumed_by_keymap = {
                let model = core.model().peek();
                self.will_consume_key_down(&ev, &model)
            };
            if consumed_by_keymap {
                let _ = self.take_consumed_key_up(ev.code);
                if !self.handle_global_key_down(&ev, core) {
                    crate::ui::vim::leave_window_mode_on_unused_key(&ev, core, self);
                }
                continue;
            }
            let (keeps_keys, target_view) = {
                let model = core.model().peek();
                let keeps = crate::ui::vim::surface_keeps_keys(&model);
                let vid = model
                    .effective_keyed_tab()
                    .and_then(|tid| model.tab(tid).and_then(|t| t.view_id).or(Some(tid.0)));
                (keeps, vid)
            };
            if !keeps_keys && let Some(view_id) = target_view {
                let down = input::key_event(true, &ev.key, ev.code, ev.modifiers);
                let up = input::key_event(false, &ev.key, ev.code, ev.modifiers);
                let tracker = page_ctx.tracker().borrow();
                tracker.send_key(view_id, down);
                tracker.send_key(view_id, up);
            }
        }
    }

    /// Resolves a global `KeyDown` event and invokes the resulting commands on `core`. Returns whether the key was
    /// used.
    pub fn handle_global_key_down(&self, e: &KeyboardEventData, core: &CoreView) -> bool {
        let stroke = input::key_stroke_from_event(&e.key, e.code, e.modifiers);
        let (vim_enabled, mode, page_id, page_editing) = {
            let model = core.model().peek();
            (
                model.vim_mode_enabled,
                model.input_mode,
                model.effective_keyed_tab(),
                model.page_editing,
            )
        };
        if vim_enabled {
            return self.feed_vim(stroke, mode, page_id, page_editing, core);
        }
        self.resolve_standard(stroke, core)
    }

    fn resolve_standard(&self, stroke: Option<KeyStroke>, core: &CoreView) -> bool {
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

    /// Drops whatever the vim engines have half typed, as when vim mode is turned off.
    pub fn reset_vim(&self) {
        let mut inner = self.inner.borrow_mut();
        for engine in inner.vim.pages.values_mut() {
            engine.reset();
            engine.take_state_changed();
        }
        inner.vim.fallback_page.reset();
        inner.vim.chrome.reset();
        inner.vim.fallback_page.take_state_changed();
        inner.vim.chrome.take_state_changed();
        inner.vim.scheduled.clear();
        inner.held_keys.clear();
    }

    /// Rebuilds the compiled mode tables from [`active_vim_profile`] and applies `timeoutlen_ms` across all engines
    /// (`CFG-02`, `CFG-04`, `CFG-05`).
    pub fn reload_vim_profile(&self, timeoutlen_ms: u64) {
        let profile = active_vim_profile();
        let tables = Arc::new(ModeTables::from_profile(&profile));
        let timeoutlen = std::time::Duration::from_millis(timeoutlen_ms.clamp(10, 10_000));
        let mut inner = self.inner.borrow_mut();
        inner.vim.tables = Arc::clone(&tables);
        inner.vim.fallback_page.set_tables(Arc::clone(&tables));
        inner.vim.fallback_page.set_timeoutlen(timeoutlen);
        inner.vim.fallback_page.take_state_changed();
        inner.vim.chrome.set_tables(Arc::clone(&tables));
        inner.vim.chrome.set_timeoutlen(timeoutlen);
        inner.vim.chrome.take_state_changed();
        for engine in inner.vim.pages.values_mut() {
            engine.set_tables(Arc::clone(&tables));
            engine.set_timeoutlen(timeoutlen);
            engine.take_state_changed();
        }
        inner.vim.scheduled.clear();
    }

    /// Removes the per-page engine for a closed tab or auxiliary view.
    pub fn remove_page(&self, tab_id: TabId) {
        let mut inner = self.inner.borrow_mut();
        inner.vim.pages.remove(&tab_id);
        inner.vim.scheduled.remove(&EngineSlot::Page(Some(tab_id)));
    }

    /// Resets pending keys on `tab_id`'s engine (e.g. when the page navigates to a new document, `MD-02`).
    pub fn reset_page(&self, tab_id: TabId) {
        let mut inner = self.inner.borrow_mut();
        if let Some(engine) = inner.vim.pages.get_mut(&tab_id) {
            engine.reset();
            engine.take_state_changed();
        }
        inner.vim.scheduled.remove(&EngineSlot::Page(Some(tab_id)));
    }

    fn feed_vim(
        &self,
        stroke: Option<KeyStroke>,
        mode: InputMode,
        page_id: Option<TabId>,
        page_editing: bool,
        core: &CoreView,
    ) -> bool {
        let slot = EngineSlot::for_model(mode, page_id);
        let (vim_consumed, was_idle, outcome) = {
            let mut inner = self.inner.borrow_mut();
            let engine = inner.vim.engine_in_mode(slot, mode);
            let pending_before = engine.pending();
            let was_idle = pending_before.tokens.is_empty() && pending_before.count.is_none();

            if mode == InputMode::Insert && !page_editing {
                let mut preview = engine.clone();
                let mut preview_matches = Vec::new();
                let preview_used = preview.feed(stroke, &mut |_, m| preview_matches.push(m));
                if preview_used
                    && preview_matches.len() == 1
                    && insert_binding_requires_field(&preview_matches[0].binding.invocation.command)
                {
                    (false, was_idle, None)
                } else {
                    let mut matches = Vec::new();
                    let consumed = engine.feed(stroke, &mut |_, matched| matches.push(matched));
                    (consumed, was_idle, Some(EngineOutcome::collect(slot, engine, matches)))
                }
            } else {
                let mut matches = Vec::new();
                let consumed = engine.feed(stroke, &mut |_, matched| matches.push(matched));
                (consumed, was_idle, Some(EngineOutcome::collect(slot, engine, matches)))
            }
        };

        if let Some(outcome) = outcome {
            self.act_on(outcome, core);
        }
        if vim_consumed {
            return true;
        }
        if was_idle {
            return self.resolve_standard(stroke, core);
        }
        false
    }

    fn fire_timer(&self, slot: EngineSlot, generation: u64, core: &CoreView) {
        let outcome = {
            let mut inner = self.inner.borrow_mut();
            if inner.vim.scheduled.get(&slot) == Some(&generation) {
                inner.vim.scheduled.remove(&slot);
            }
            let engine = inner.vim.engine(slot);
            let mut matches = Vec::new();
            engine.fire_timer(generation, &mut |_, matched| matches.push(matched));
            EngineOutcome::collect(slot, engine, matches)
        };
        self.act_on(outcome, core);
    }

    fn act_on(&self, outcome: EngineOutcome, core: &CoreView) {
        if let Some((keys, count)) = outcome.pending {
            let is_current_slot = {
                let model = core.model().peek();
                EngineSlot::for_model(model.input_mode, model.effective_keyed_tab()) == outcome.slot
            };
            if is_current_slot {
                report_pending(core, keys, count);
            }
        }
        let tab_before = core.model().peek().effective_keyed_tab();
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
                crate::ui::vim::follow_page_scroll(core);
            }
        }
        // A key that moved to another tab leaves any focused field there (`gi` returns to it), preserving Visual/`I`.
        if !outcome.matches.is_empty() && core.model().peek().effective_keyed_tab() != tab_before {
            core.invoke_or_log(Command::Vim(VimCommand::LeaveField));
        }
        if let Some((generation, timeoutlen)) = outcome.timer {
            let needs_task = {
                let mut inner = self.inner.borrow_mut();
                let prev = inner.vim.scheduled.insert(outcome.slot, generation);
                prev != Some(generation)
            };
            if needs_task {
                let controller = self.clone();
                let core = core.clone();
                let slot = outcome.slot;
                spawn(async move {
                    timer(timeoutlen).await;
                    controller.fire_timer(slot, generation, &core);
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
        assert_eq!(
            input::key_stroke_from_event(&Key::Named(NamedKey::Control), Code::ControlLeft, Modifiers::CONTROL),
            None
        );
    }

    #[test]
    fn normalizes_keys_like_the_fork() {
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
        assert_eq!(
            input::key_stroke_from_event(
                &Key::Character("@".to_string()),
                Code::KeyQ,
                Modifiers::CONTROL | Modifiers::ALT | Modifiers::ALT_GRAPH
            ),
            Some(KeyStroke::char('@'))
        );
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
    fn vim_mode_previews_keys_by_mode_and_falls_back_to_standard_shortcuts() {
        let controller = KeymapController::standard();
        let mut model = BrowserModel {
            vim_mode_enabled: true,
            active_tab: Some(TabId(1)),
            ..BrowserModel::default()
        };
        let j = KeyboardEventData::new(Key::Character("j".to_string()), Code::KeyJ, Modifiers::empty());
        let q = KeyboardEventData::new(Key::Character("q".to_string()), Code::KeyQ, Modifiers::empty());
        let ctrl_t = KeyboardEventData::new(Key::Character("t".to_string()), Code::KeyT, Modifiers::CONTROL);
        let ctrl_shift_t = KeyboardEventData::new(
            Key::Character("T".to_string()),
            Code::KeyT,
            Modifiers::CONTROL | Modifiers::SHIFT,
        );
        let ctrl_w = KeyboardEventData::new(Key::Character("w".to_string()), Code::KeyW, Modifiers::CONTROL);

        assert!(controller.will_consume_key_down(&j, &model), "j scrolls in normal mode");
        assert!(!controller.will_consume_key_down(&q, &model), "q is the page's");
        assert!(
            controller.will_consume_key_down(&ctrl_t, &model),
            "Ctrl+T falls back to standard.toml in normal mode (RT-01)"
        );
        assert!(
            controller.will_consume_key_down(&ctrl_shift_t, &model),
            "Ctrl+Shift+T reopens closed tab in standard.toml (RT-01)"
        );
        assert!(
            controller.will_consume_key_down(&ctrl_w, &model),
            "Ctrl+W closes tab in normal mode (RT-01)"
        );

        // Explicit Insert (`I`) outside a field still lets Ctrl+W close the tab via standard.toml, while plain letters pass to the page.
        model.input_mode = InputMode::Insert;
        model.explicit_insert = true;
        model.page_editing = false;
        assert!(!controller.will_consume_key_down(&j, &model), "j types in insert mode");
        assert!(
            controller.will_consume_key_down(&ctrl_w, &model),
            "Ctrl+W outside a field still closes the tab via standard.toml"
        );

        // Inside a focused field, Ctrl+W is consumed as edit.delete_word_backward.
        model.page_editing = true;
        assert!(controller.will_consume_key_down(&ctrl_w, &model));

        model.input_mode = InputMode::Visual;
        assert!(
            controller.will_consume_key_down(&q, &model),
            "visual mode keeps stray keys"
        );
    }
}
