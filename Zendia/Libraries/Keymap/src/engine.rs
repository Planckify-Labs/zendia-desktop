/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The vim key engine: modes, counts, multi-key sequences and `timeoutlen`.
//!
//! It behaves as the old Zendia fork's key engine did. It holds no window or page state, so the same engine serves the
//! page's modes (normal, insert, visual) and the browser's own surfaces (explorer, toolbar, library, window mode).
//! [`KeyEngine::feed`] answers at once whether a key was used, which is what a key handler needs to decide whether the
//! page gets the key. Timers are the caller's: the engine says when it wants one ([`KeyEngine::armed_timer`]) and the
//! caller fires it after [`KeyEngine::timeoutlen`].
//!
//! For each key, in this order:
//! 1. `<Esc>` with keys or a count pending cancels them.
//! 2. `<BS>` with keys pending takes back the last key.
//! 3. With nothing pending, in a mode with counts, `1`-`9` start a count and `0`-`9` continue it.
//! 4. Otherwise the pending keys plus this key are looked up. A key that continues nothing runs the pending keys if
//!    they are a mapping themselves, then starts over on its own. A key that starts nothing is the page's, unless the
//!    mode swallows unmapped keys. A complete mapping with nothing longer runs. A prefix waits for the next key: with
//!    no timeout when it is no mapping itself and `wait_for_prefix` is on (which-key), otherwise for `timeoutlen`,
//!    after which a pending mapping runs and anything else is dropped.

use crate::notation::{KeyCode, KeyModifiers, KeyStroke};
use crate::table::{Binding, CHROME_MODES, KeyTable, Profile};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

/// The default `timeoutlen` (Zendia's `zendia.timeoutlen`), which settles a sequence that is a mapping and a prefix of
/// longer ones at once.
pub const DEFAULT_TIMEOUTLEN: Duration = Duration::from_millis(300);

/// Normal mode: counts, and keys that start nothing reach the page.
pub const NORMAL_MODE: &str = "n";
/// Insert mode: typing reaches the page; no counts.
pub const INSERT_MODE: &str = "i";
/// Visual mode on the page's text: counts, and stray keys never reach the page.
pub const VISUAL_MODE: &str = "v";

/// How a mode treats counts and keys it has no mapping for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeOptions {
    /// Whether digits typed before a mapping are a count (`5j`).
    pub counts: bool,
    /// Whether a key that starts no mapping is left to the page. When false the engine swallows it.
    pub pass_unmapped: bool,
}

/// A mapping that matched, with the count typed before it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyMatch {
    pub binding: Binding,
    /// The typed count, or `None` when none was given.
    pub count: Option<u32>,
    /// The mode the mapping matched in.
    pub mode: String,
}

/// What has been typed towards a mapping so far, for which-key and the mode indicator.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingKeys {
    /// The pending keys as notation tokens (`<Space>`, `b`).
    pub tokens: Vec<String>,
    pub count: Option<u32>,
    /// The which-key group name of the pending prefix, if it has one.
    pub group: Option<String>,
}

/// [`KeyEngine::set_mode`] was asked for a mode that was never defined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownMode(pub String);

impl fmt::Display for UnknownMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown mode \"{}\"", self.0)
    }
}

impl std::error::Error for UnknownMode {}

/// One compiled key table per mode of a profile.
#[derive(Clone, Debug, Default)]
pub struct ModeTables {
    tables: BTreeMap<String, KeyTable>,
}

impl ModeTables {
    /// Compiles `profile` once per mode it names, plus the page's modes and [`CHROME_MODES`].
    pub fn from_profile(profile: &Profile) -> Self {
        let mut modes: Vec<String> = [NORMAL_MODE, INSERT_MODE, VISUAL_MODE]
            .into_iter()
            .chain(CHROME_MODES)
            .map(str::to_string)
            .collect();
        for binding in &profile.bindings {
            if let Some(mode) = &binding.mode
                && !modes.contains(mode)
            {
                modes.push(mode.clone());
            }
        }
        let tables = modes
            .into_iter()
            .map(|mode| {
                let table = KeyTable::from_profile_for_mode(profile, &mode);
                (mode, table)
            })
            .collect();
        Self { tables }
    }

    /// The table of `mode`, if the profile has one.
    pub fn table(&self, mode: &str) -> Option<&KeyTable> {
        self.tables.get(mode)
    }
}

/// The key engine. See the module documentation for how each key is treated.
#[derive(Clone, Debug)]
pub struct KeyEngine {
    tables: Arc<ModeTables>,
    modes: BTreeMap<String, ModeOptions>,
    mode: String,
    pending: Vec<KeyStroke>,
    count: String,
    timer: Option<u64>,
    timer_generation: u64,
    timeoutlen: Duration,
    wait_for_prefix: bool,
    state_changed: bool,
}

impl KeyEngine {
    /// An engine in normal mode with the page's three modes defined: `n` (counts, unmapped keys pass), `i` (no counts,
    /// unmapped keys pass) and `v` (counts, unmapped keys are swallowed).
    pub fn new(tables: Arc<ModeTables>) -> Self {
        let mut engine = Self {
            tables,
            modes: BTreeMap::new(),
            mode: NORMAL_MODE.to_string(),
            pending: Vec::new(),
            count: String::new(),
            timer: None,
            timer_generation: 0,
            timeoutlen: DEFAULT_TIMEOUTLEN,
            wait_for_prefix: false,
            state_changed: false,
        };
        engine.define_mode(
            NORMAL_MODE,
            ModeOptions {
                counts: true,
                pass_unmapped: true,
            },
        );
        engine.define_mode(
            INSERT_MODE,
            ModeOptions {
                counts: false,
                pass_unmapped: true,
            },
        );
        engine.define_mode(
            VISUAL_MODE,
            ModeOptions {
                counts: true,
                pass_unmapped: false,
            },
        );
        engine
    }

    /// An engine for the browser's own surfaces: [`CHROME_MODES`] take counts and leave unmapped keys alone.
    pub fn for_chrome(tables: Arc<ModeTables>) -> Self {
        let mut engine = Self::new(tables);
        for mode in CHROME_MODES {
            engine.define_mode(
                mode,
                ModeOptions {
                    counts: true,
                    pass_unmapped: true,
                },
            );
        }
        engine
    }

    /// Sets how long an ambiguous sequence waits for its next key.
    pub fn with_timeoutlen(mut self, timeoutlen: Duration) -> Self {
        self.timeoutlen = timeoutlen;
        self
    }

    /// Waits for the next key without a timeout after a prefix that is not a mapping itself, the way which-key does.
    /// `timeoutlen` then only settles ambiguous sequences.
    pub fn with_wait_for_prefix(mut self, wait_for_prefix: bool) -> Self {
        self.wait_for_prefix = wait_for_prefix;
        self
    }

    /// Adds a mode or changes how an existing one treats counts and unmapped keys.
    pub fn define_mode(&mut self, name: &str, options: ModeOptions) {
        self.modes.insert(name.to_string(), options);
    }

    pub fn mode(&self) -> &str {
        &self.mode
    }

    pub fn timeoutlen(&self) -> Duration {
        self.timeoutlen
    }

    pub fn set_timeoutlen(&mut self, timeoutlen: Duration) {
        self.timeoutlen = timeoutlen;
    }

    /// Replaces the key tables, for a changed profile or leader. The mode is kept and anything pending is dropped.
    pub fn set_tables(&mut self, tables: Arc<ModeTables>) {
        self.tables = tables;
        self.reset();
    }

    /// Switches modes. Anything half typed is dropped.
    pub fn set_mode(&mut self, name: &str) -> Result<(), UnknownMode> {
        if !self.modes.contains_key(name) {
            return Err(UnknownMode(name.to_string()));
        }
        if name == self.mode {
            return Ok(());
        }
        self.mode = name.to_string();
        self.clear();
        self.notify();
        Ok(())
    }

    /// What has been typed towards a mapping so far.
    pub fn pending(&self) -> PendingKeys {
        let group = if self.pending.is_empty() {
            None
        } else {
            self.tables
                .table(&self.mode)
                .and_then(|table| table.group_for(&self.pending))
                .map(|group| group.name.clone())
        };
        PendingKeys {
            tokens: self.pending.iter().map(ToString::to_string).collect(),
            count: self.count_value(),
            group,
        }
    }

    /// Whether the mode, the pending keys or the count changed since the last call.
    pub fn take_state_changed(&mut self) -> bool {
        std::mem::take(&mut self.state_changed)
    }

    /// The generation of the timer the engine is waiting on, if any. The caller fires it with [`Self::fire_timer`]
    /// after [`Self::timeoutlen`]. A later key re-arms or cancels it, which makes an older generation stale.
    pub fn armed_timer(&self) -> Option<u64> {
        self.timer
    }

    /// Feeds one key. A key with no stroke (a bare modifier, a dead key) is never used.
    ///
    /// Returns whether the engine used the key. A key that was not used belongs to the page. Each mapping that runs
    /// is passed to `on_match`, which may switch the engine's mode before any key that follows is handled.
    pub fn feed(&mut self, stroke: Option<KeyStroke>, on_match: &mut dyn FnMut(&mut KeyEngine, KeyMatch)) -> bool {
        let Some(stroke) = stroke else {
            return false;
        };
        let options = self.mode_options();

        if stroke == escape() && (!self.pending.is_empty() || !self.count.is_empty()) {
            // <Esc> abandons a half typed sequence or count, as in vim.
            self.clear();
            self.notify();
            return true;
        }

        if stroke == backspace() && !self.pending.is_empty() {
            // Back up one key, like <BS> in which-key.
            self.pending.pop();
            if self.pending.is_empty() {
                self.cancel_timer();
            } else {
                let is_mapping = self.binding_at(&self.pending.clone()).is_some();
                self.wait_for_next_key(is_mapping);
            }
            self.notify();
            return true;
        }

        if self.pending.is_empty() && options.counts && is_count_digit(stroke, !self.count.is_empty()) {
            if let KeyCode::Char(digit) = stroke.key {
                self.count.push(digit);
            }
            self.notify();
            return true;
        }

        let tables = Arc::clone(&self.tables);
        let table = tables.table(&self.mode);
        let mut sequence = self.pending.clone();
        sequence.push(stroke);
        let node = table.and_then(|table| table.node_at(&sequence));

        let Some(node) = node else {
            if !self.pending.is_empty() {
                // The key does not continue the sequence. A complete shorter mapping runs first, then the key starts
                // over on its own.
                let fallback = self.binding_at(&self.pending.clone());
                let count = self.take_count();
                self.clear();
                match fallback {
                    Some(binding) => self.run_match(binding, count, on_match),
                    None => self.notify(),
                }
                return self.feed(Some(stroke), on_match);
            }
            let had_count = !self.count.is_empty();
            self.clear();
            if had_count {
                self.notify();
            }
            return !options.pass_unmapped;
        };

        if node.children.is_empty() {
            let count = self.take_count();
            self.clear();
            if let Some(binding) = node.binding.clone() {
                self.run_match(binding, count, on_match);
            } else {
                self.notify();
            }
            return true;
        }

        let is_mapping = node.binding.is_some();
        self.pending = sequence;
        self.wait_for_next_key(is_mapping);
        self.notify();
        true
    }

    /// Whether [`Self::feed`] would use `stroke`, without changing anything.
    pub fn will_use(&self, stroke: Option<KeyStroke>) -> bool {
        let mut preview = self.clone();
        preview.feed(stroke, &mut |_, _| {})
    }

    /// Fires the timer of `generation` once `timeoutlen` has passed: a pending mapping runs, anything else is dropped.
    /// A stale generation does nothing.
    pub fn fire_timer(&mut self, generation: u64, on_match: &mut dyn FnMut(&mut KeyEngine, KeyMatch)) {
        if self.timer != Some(generation) {
            return;
        }
        self.timer = None;
        let mapping = self.binding_at(&self.pending.clone());
        let count = self.take_count();
        self.clear();
        match mapping {
            Some(binding) => self.run_match(binding, count, on_match),
            None => self.notify(),
        }
    }

    /// Drops anything half typed, as when focus moves or the page navigates.
    pub fn reset(&mut self) {
        if !self.pending.is_empty() || !self.count.is_empty() {
            self.clear();
            self.notify();
        }
    }

    fn mode_options(&self) -> ModeOptions {
        self.modes.get(&self.mode).copied().unwrap_or(ModeOptions {
            counts: false,
            pass_unmapped: true,
        })
    }

    fn binding_at(&self, strokes: &[KeyStroke]) -> Option<Binding> {
        self.tables
            .table(&self.mode)
            .and_then(|table| table.node_at(strokes))
            .and_then(|node| node.binding.clone())
    }

    /// A prefix of longer mappings, and maybe a mapping itself: wait for the next key, or for `timeoutlen` to pass.
    fn wait_for_next_key(&mut self, is_mapping: bool) {
        if self.wait_for_prefix && !is_mapping {
            self.cancel_timer();
        } else {
            self.arm_timer();
        }
    }

    fn arm_timer(&mut self) {
        self.timer_generation += 1;
        self.timer = Some(self.timer_generation);
    }

    fn cancel_timer(&mut self) {
        self.timer = None;
    }

    fn count_value(&self) -> Option<u32> {
        if self.count.is_empty() {
            return None;
        }
        Some(
            self.count
                .parse::<u64>()
                .map_or(u32::MAX, |value| value.min(u64::from(u32::MAX)) as u32),
        )
    }

    fn take_count(&mut self) -> Option<u32> {
        let count = self.count_value();
        self.count.clear();
        count
    }

    fn clear(&mut self) {
        self.cancel_timer();
        self.pending.clear();
        self.count.clear();
    }

    fn run_match(&mut self, binding: Binding, count: Option<u32>, on_match: &mut dyn FnMut(&mut KeyEngine, KeyMatch)) {
        self.notify();
        let mode = self.mode.clone();
        on_match(self, KeyMatch { binding, count, mode });
    }

    fn notify(&mut self) {
        self.state_changed = true;
    }
}

fn escape() -> KeyStroke {
    KeyStroke::new(KeyModifiers::NONE, KeyCode::Escape)
}

fn backspace() -> KeyStroke {
    KeyStroke::new(KeyModifiers::NONE, KeyCode::Backspace)
}

/// `1`-`9` start a count and `0`-`9` continue one, so a `0` with nothing counted is a key of its own.
pub fn is_count_digit(stroke: KeyStroke, counting: bool) -> bool {
    if !stroke.modifiers.is_empty() {
        return false;
    }
    match stroke.key {
        KeyCode::Char('0') => counting,
        KeyCode::Char(c) => c.is_ascii_digit(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    //! The old fork's key engine tests (`test_key_engine.js`), case for case.

    use super::*;
    use crate::notation::KeySequence;
    use std::str::FromStr;

    struct Setup {
        engine: KeyEngine,
        matches: Vec<(String, Option<u32>, String)>,
    }

    fn profile(mappings: &[(&str, &str, &str)], groups: &[(&str, &str, &str)]) -> Profile {
        let mut toml = String::from("name = \"test\"\nleader = \"<Space>\"\n");
        for (mode, keys, command) in mappings {
            toml.push_str(&format!(
                "[[bindings]]\nmode = \"{mode}\"\nkeys = '{keys}'\ncommand = \"{command}\"\n"
            ));
        }
        for (mode, keys, name) in groups {
            toml.push_str(&format!(
                "[[groups]]\nmode = \"{mode}\"\nkeys = '{keys}'\nname = \"{name}\"\n"
            ));
        }
        Profile::from_toml(&toml).unwrap()
    }

    fn setup_with(mappings: &[(&str, &str, &str)], groups: &[(&str, &str, &str)], wait_for_prefix: bool) -> Setup {
        let tables = Arc::new(ModeTables::from_profile(&profile(mappings, groups)));
        let engine = KeyEngine::new(tables)
            .with_timeoutlen(Duration::from_millis(300))
            .with_wait_for_prefix(wait_for_prefix);
        Setup {
            engine,
            matches: Vec::new(),
        }
    }

    fn setup(mappings: &[(&str, &str, &str)]) -> Setup {
        setup_with(mappings, &[], false)
    }

    fn token(text: &str) -> Option<KeyStroke> {
        Some(KeyStroke::from_str(text).unwrap())
    }

    impl Setup {
        fn feed(&mut self, text: &str) -> bool {
            let matches = &mut self.matches;
            self.engine.feed(token(text), &mut |_, m| {
                matches.push((m.binding.invocation.command, m.count, m.mode));
            })
        }

        fn feed_all(&mut self, texts: &[&str]) -> Vec<bool> {
            texts.iter().map(|text| self.feed(text)).collect()
        }

        fn fire(&mut self) {
            let Some(generation) = self.engine.armed_timer() else {
                return;
            };
            let matches = &mut self.matches;
            self.engine.fire_timer(generation, &mut |_, m| {
                matches.push((m.binding.invocation.command, m.count, m.mode));
            });
        }

        fn timers(&self) -> usize {
            usize::from(self.engine.armed_timer().is_some())
        }

        fn rhs(&self) -> Vec<&str> {
            self.matches.iter().map(|(rhs, _, _)| rhs.as_str()).collect()
        }

        fn rhs_count(&self) -> Vec<(&str, Option<u32>)> {
            self.matches
                .iter()
                .map(|(rhs, count, _)| (rhs.as_str(), *count))
                .collect()
        }
    }

    #[test]
    fn single_key() {
        let mut s = setup(&[("n", "j", "scroll.down")]);
        assert!(s.feed("j"), "a mapped key is used");
        assert_eq!(s.matches, vec![("scroll.down".to_string(), None, "n".to_string())]);
    }

    #[test]
    fn unmapped_keys_reach_the_page_in_normal_mode() {
        let mut s = setup(&[("n", "j", "scroll.down")]);
        assert!(!s.feed("q"), "an unmapped key is left to the page");
        assert!(!s.feed("<C-c>"), "copy still works");
        assert!(!s.engine.feed(None, &mut |_, _| {}));
        assert!(s.matches.is_empty());
    }

    #[test]
    fn sequence() {
        let mut s = setup(&[("n", "gg", "scroll.top")]);
        assert!(s.feed("g"));
        assert!(s.matches.is_empty(), "waits for the rest of the sequence");
        assert_eq!(s.engine.pending().tokens, vec!["g"]);
        assert!(s.feed("g"));
        assert_eq!(s.rhs(), vec!["scroll.top"]);
        assert_eq!(s.timers(), 0, "no timer is left behind");
        assert!(s.engine.pending().tokens.is_empty());
    }

    #[test]
    fn counts() {
        let mut s = setup(&[("n", "j", "scroll.down"), ("n", "gg", "scroll.top")]);
        s.feed_all(&["1", "2", "j"]);
        s.feed_all(&["3", "g", "g"]);
        assert_eq!(s.rhs_count(), vec![("scroll.down", Some(12)), ("scroll.top", Some(3))]);
    }

    #[test]
    fn zero_is_a_key_unless_counting() {
        let mut s = setup(&[("n", "0", "scroll.left"), ("n", "j", "scroll.down")]);
        s.feed_all(&["0"]);
        s.feed_all(&["1", "0", "j"]);
        assert_eq!(s.rhs_count(), vec![("scroll.left", None), ("scroll.down", Some(10))]);
    }

    #[test]
    fn count_before_unmapped_key_is_dropped() {
        let mut s = setup(&[("n", "j", "scroll.down")]);
        assert!(s.feed("5"));
        assert!(!s.feed("q"), "the unmapped key still reaches the page");
        assert_eq!(s.engine.pending().count, None, "the count is gone");
        s.feed("j");
        assert_eq!(s.rhs_count(), vec![("scroll.down", None)]);
    }

    #[test]
    fn timeout_runs_complete_prefix() {
        let mut s = setup(&[("n", "<leader>b", "buffer.menu"), ("n", "<leader>bd", "tabs.close")]);
        s.feed_all(&["<Space>", "b"]);
        assert!(s.matches.is_empty(), "ambiguous: waits for more");
        s.fire();
        assert_eq!(s.rhs_count(), vec![("buffer.menu", None)]);
        assert!(s.engine.pending().tokens.is_empty());
    }

    #[test]
    fn timeout_drops_incomplete_prefix() {
        let mut s = setup(&[("n", "<leader>bd", "tabs.close")]);
        s.feed_all(&["<Space>", "b"]);
        s.fire();
        assert!(s.matches.is_empty());
        assert!(s.engine.pending().tokens.is_empty());
    }

    #[test]
    fn broken_sequence_runs_prefix_then_key() {
        let mut s = setup(&[
            ("n", "g", "go.menu"),
            ("n", "gg", "scroll.top"),
            ("n", "j", "scroll.down"),
        ]);
        s.feed_all(&["g", "j"]);
        assert_eq!(
            s.rhs(),
            vec!["go.menu", "scroll.down"],
            "the shorter mapping runs, then the key on its own"
        );
    }

    #[test]
    fn broken_sequence_without_prefix_mapping() {
        let mut s = setup(&[("n", "gg", "scroll.top"), ("n", "j", "scroll.down")]);
        assert_eq!(s.feed_all(&["g", "j"]), vec![true, true]);
        assert_eq!(s.rhs(), vec!["scroll.down"]);
        assert!(s.feed("g"));
        assert!(!s.feed("q"), "an unmapped key after a broken prefix passes");
    }

    #[test]
    fn escape_abandons_pending() {
        let mut s = setup(&[("n", "<leader>b", "buffer.menu"), ("n", "<leader>bd", "tabs.close")]);
        s.feed_all(&["3", "<Space>", "b"]);
        assert!(s.feed("<Esc>"), "<Esc> is used to cancel");
        assert_eq!(s.timers(), 0);
        assert!(s.matches.is_empty(), "the prefix mapping does not run");
        assert_eq!(s.engine.pending(), PendingKeys::default());
        assert!(!s.feed("<Esc>"), "with nothing pending <Esc> passes");
    }

    #[test]
    fn modes() {
        let mut s = setup(&[("n", "j", "scroll.down"), ("i", "<Esc>", "mode.normal")]);
        s.engine.set_mode("i").unwrap();
        assert!(!s.feed("j"), "typing in insert mode reaches the page");
        assert!(!s.feed("5"), "no counts in insert mode");
        assert!(s.feed("<Esc>"));
        assert_eq!(s.matches, vec![("mode.normal".to_string(), None, "i".to_string())]);
        assert_eq!(s.engine.set_mode("nope"), Err(UnknownMode("nope".to_string())));
    }

    #[test]
    fn mode_change_drops_pending() {
        let mut s = setup(&[("n", "gg", "scroll.top")]);
        s.feed_all(&["4", "g"]);
        s.engine.set_mode("i").unwrap();
        assert!(s.engine.pending().tokens.is_empty());
        assert_eq!(s.engine.pending().count, None);
        assert_eq!(s.timers(), 0);
    }

    #[test]
    fn custom_mode_swallows_unmapped() {
        let mut s = setup(&[("hint", "a", "hint.pick")]);
        s.engine.define_mode(
            "hint",
            ModeOptions {
                counts: false,
                pass_unmapped: false,
            },
        );
        s.engine.set_mode("hint").unwrap();
        assert!(s.feed("z"), "hint mode keeps stray keys from the page");
    }

    #[test]
    fn state_change_and_group() {
        let mut s = setup_with(
            &[("n", "<leader>bd", "tabs.close")],
            &[("n", "<leader>b", "buffer")],
            false,
        );
        s.feed_all(&["2", "<Space>", "b"]);
        assert!(s.engine.take_state_changed());
        assert_eq!(
            s.engine.pending(),
            PendingKeys {
                tokens: vec!["<Space>".to_string(), "b".to_string()],
                count: Some(2),
                group: Some("buffer".to_string()),
            }
        );
    }

    #[test]
    fn wait_for_prefix_like_which_key() {
        let mut s = setup_with(
            &[
                ("n", "<leader>bd", "tabs.close"),
                ("n", "g", "go.menu"),
                ("n", "gg", "scroll.top"),
            ],
            &[],
            true,
        );
        s.feed("<Space>");
        assert_eq!(s.timers(), 0, "a pure prefix waits with no timeout");
        s.feed("b");
        assert_eq!(s.timers(), 0, "so does a deeper one");
        s.feed("d");
        assert_eq!(s.rhs(), vec!["tabs.close"]);

        s.feed("g");
        assert_eq!(s.timers(), 1, "a prefix that is also a mapping still times out");
        s.fire();
        assert_eq!(s.rhs(), vec!["tabs.close", "go.menu"]);
    }

    #[test]
    fn backspace_backs_up_one_key() {
        let mut s = setup_with(
            &[
                ("n", "<leader>bd", "tabs.close"),
                ("n", "<leader>e", "zen.sidebar.toggle"),
            ],
            &[],
            true,
        );
        s.feed("<Space>");
        s.feed("b");
        assert!(s.feed("<BS>"), "<BS> is used while keys are pending");
        assert_eq!(s.engine.pending().tokens, vec!["<Space>"]);
        s.feed("e");
        assert_eq!(s.rhs(), vec!["zen.sidebar.toggle"]);

        s.feed("<Space>");
        s.feed("<BS>");
        assert!(s.engine.pending().tokens.is_empty(), "back to nothing pending");
        assert!(!s.feed("<BS>"), "with nothing pending <BS> is the page's");
    }

    #[test]
    fn stale_timers_do_nothing() {
        let mut s = setup(&[("n", "g", "go.menu"), ("n", "gg", "scroll.top")]);
        s.feed("g");
        let stale = s.engine.armed_timer().unwrap();
        s.feed("<Esc>");
        s.feed("g");
        let matches = &mut s.matches;
        s.engine.fire_timer(stale, &mut |_, m| {
            matches.push((m.binding.invocation.command, m.count, m.mode))
        });
        assert!(s.matches.is_empty());
        assert_eq!(s.engine.pending().tokens, vec!["g"]);
    }

    #[test]
    fn will_use_changes_nothing() {
        let s = setup(&[("n", "gg", "scroll.top")]);
        assert!(s.engine.will_use(token("g")));
        assert!(!s.engine.will_use(token("q")));
        assert!(s.engine.pending().tokens.is_empty());
    }

    #[test]
    fn chrome_modes_inherit_the_leader_keys() {
        let tables = Arc::new(ModeTables::from_profile(&Profile::vim()));
        let mut engine = KeyEngine::for_chrome(tables.clone()).with_wait_for_prefix(true);
        engine.set_mode("e").unwrap();
        let mut ran = Vec::new();
        for key in ["<Space>", "b", "d"] {
            assert!(engine.feed(token(key), &mut |_, m| ran.push(m.binding.invocation.command)));
        }
        assert_eq!(ran, vec!["tab.close"], "<leader>bd works in the explorer too");

        // The explorer's own `<leader>e` wins over normal mode's.
        let explorer = tables.table("e").unwrap();
        let leader_e = KeySequence::parse("<Space>e").unwrap();
        let own = explorer
            .bindings()
            .iter()
            .find(|binding| binding.keys == leader_e)
            .expect("explorer <leader>e");
        assert_eq!(own.invocation.command, "explorer.quit");
        assert!(
            explorer
                .group_for(KeySequence::parse("<Space>b").unwrap().strokes())
                .is_some()
        );
    }
}
