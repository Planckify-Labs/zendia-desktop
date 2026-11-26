/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for the vim key engine's commands: the page's modes, the pending keys, scrolling and field edits.

use super::StubBackend;
use crate::BrowserCore;
use crate::commands::CommandResult;
use crate::input_state::which_key_for_prefix;
use crate::model::{InputMode, TabId};
use crate::vim_keys::VimCommand;
use zendia_automation::AutomationRequest;

impl StubBackend {
    pub(super) fn handle_vim_command(&self, command: VimCommand) -> CommandResult {
        let name = command.name();
        match command {
            VimCommand::ModeNormal => {
                // <Esc> in a field leaves it, so the keys that follow are normal mode's and not typed into it.
                self.with_vim_state(|state| state.explicit_insert = false);
                self.set_page_hints(None);
                self.set_page_mode(InputMode::Normal);
                self.ask_page(AutomationRequest::BlurField)
            }
            VimCommand::ModeInsert => {
                self.with_vim_state(|state| state.explicit_insert = true);
                self.set_page_mode(InputMode::Insert);
                Ok(())
            }
            VimCommand::ReportPending { keys, count } => {
                self.report_pending_keys(keys, count);
                Ok(())
            }
            VimCommand::EditableFocusChanged { tab_id, editing } => {
                let tab_id = self.resolve_target_tab(tab_id, name)?;
                let is_active = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state.vim.page_editing.insert(tab_id, editing);
                    state.model.active_tab == Some(tab_id)
                };
                if is_active {
                    self.follow_field_focus(editing);
                }
                Ok(())
            }
            VimCommand::SyncPageMode => {
                let editing = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state.vim.explicit_insert = false;
                    let active = state.model.active_tab;
                    active
                        .and_then(|tab| state.vim.page_editing.get(&tab).copied())
                        .unwrap_or(false)
                };
                let mode = self.current_input_mode();
                if matches!(mode, InputMode::Normal | InputMode::Insert | InputMode::Visual) {
                    self.set_page_mode(if editing { InputMode::Insert } else { InputMode::Normal });
                }
                Ok(())
            }
            VimCommand::LeaveField => {
                self.with_vim_state(|state| state.explicit_insert = false);
                let Some(tab_id) = self.model().active_tab else {
                    return Ok(());
                };
                if self.page_is_editing(tab_id) {
                    self.ask_page_for_tab(tab_id, AutomationRequest::BlurField)?;
                }
                if matches!(self.current_input_mode(), InputMode::Insert | InputMode::Visual) {
                    self.set_page_mode(InputMode::Normal);
                }
                Ok(())
            }
            VimCommand::ScrollPage { tab_id, motion, count } => {
                let tab_id = self.resolve_target_tab(tab_id, name)?;
                let count = if motion.is_counted() {
                    count.unwrap_or(1).clamp(1, 100_000)
                } else {
                    1
                };
                self.ask_page_for_tab(
                    tab_id,
                    AutomationRequest::Scroll {
                        motion: motion.automation(),
                        count,
                    },
                )
            }
            VimCommand::EditField { tab_id, edit } => {
                let tab_id = self.resolve_target_tab(tab_id, name)?;
                match self.current_tab_source() {
                    Some(source) => source.edit_field(tab_id, edit),
                    None => Ok(()),
                }
            }
        }
    }

    /// Whether a field of `tab_id`'s page has the focus, as last reported.
    pub fn page_is_editing(&self, tab_id: TabId) -> bool {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.vim.page_editing.get(&tab_id).copied().unwrap_or(false)
    }

    /// Leaves the page's modes when vim mode is turned off, so turning it back on starts in normal mode.
    pub(super) fn reset_vim_page_mode(&self) {
        self.with_vim_state(|state| state.explicit_insert = false);
        if matches!(self.current_input_mode(), InputMode::Insert | InputMode::Visual) {
            self.set_input_mode(InputMode::Normal);
        }
        self.report_pending_keys(Vec::new(), None);
    }

    /// A field took the focus, the user's or the page's: normal and visual mode become insert mode ("clicking into a field
    /// ends visual mode").
    /// It lost the focus: insert mode becomes normal mode, unless insert mode was asked for with `I`.
    fn follow_field_focus(&self, editing: bool) {
        let (enabled, mode, explicit_insert) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            (
                state.model.vim_mode_enabled,
                state.model.input_mode,
                state.vim.explicit_insert,
            )
        };
        if !enabled {
            return;
        }
        match (editing, mode) {
            // A field the page focused by itself (a search box on load) is typed into at once, as in the standard
            // keys.
            (true, InputMode::Normal) => self.set_page_mode(InputMode::Insert),
            // Clicking into a field ends visual mode.
            (true, InputMode::Visual) => {
                self.set_visual_caret(None);
                self.set_page_mode(InputMode::Insert);
            }
            (false, InputMode::Insert) if !explicit_insert => self.set_page_mode(InputMode::Normal),
            _ => {}
        }
    }

    fn set_page_mode(&self, mode: InputMode) {
        if self.current_input_mode() != mode {
            self.set_input_mode(mode);
        }
    }

    fn report_pending_keys(&self, keys: Vec<String>, count: Option<u32>) {
        let (mode, same_keys, same_count) = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            (
                state.model.input_mode,
                state.model.pending_keys == keys,
                state.model.pending_count == count,
            )
        };
        if !same_count {
            self.set_pending_count(count);
        }
        if !same_keys {
            let which_key = which_key_for_prefix(mode, &keys).map(|view| view.to_which_key_state());
            self.set_pending_keys(keys);
            self.set_which_key(which_key);
        }
    }

    fn current_input_mode(&self) -> InputMode {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .model
            .input_mode
    }

    fn with_vim_state(&self, change: impl FnOnce(&mut VimState)) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        change(&mut state.vim);
    }
}

/// What the stub remembers for the vim key engine.
#[derive(Debug, Default)]
pub(super) struct VimState {
    /// Insert mode was asked for with `I`, so it lasts until `<Esc>` whatever has the focus.
    pub(super) explicit_insert: bool,
    /// Whether a field of each tab's page has the focus.
    pub(super) page_editing: std::collections::BTreeMap<TabId, bool>,
    /// Most-recently-activated tabs (oldest first, newest last) for `tab.alternate` and `split.last`.
    pub(super) tab_recency: Vec<TabId>,
    /// Saved split ratios before `split.zoom` maximized a pane, so a second `split.zoom` restores them.
    pub(super) zoomed_splits: std::collections::BTreeMap<crate::model::SplitGroupId, Vec<f32>>,
    /// Deduplicated ring of recently yanked texts.
    pub(super) yank_ring: crate::yank_webchat_actions::YankRing,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrowserCore;
    use crate::commands::Command;

    fn vim_core() -> StubBackend {
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        core
    }

    fn focus(core: &StubBackend, editing: bool) {
        core.invoke(Command::Vim(VimCommand::EditableFocusChanged { tab_id: None, editing }))
            .unwrap();
    }

    #[test]
    fn field_focus_switches_between_normal_and_insert() {
        let core = vim_core();
        focus(&core, true);
        assert_eq!(core.model().input_mode, InputMode::Insert);
        focus(&core, false);
        assert_eq!(core.model().input_mode, InputMode::Normal);
    }

    #[test]
    fn explicit_insert_outlasts_the_field() {
        let core = vim_core();
        core.invoke(Command::Vim(VimCommand::ModeInsert)).unwrap();
        focus(&core, true);
        focus(&core, false);
        assert_eq!(core.model().input_mode, InputMode::Insert, "I lasts until <Esc>");
        core.invoke(Command::Vim(VimCommand::ModeNormal)).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
    }

    #[test]
    fn field_focus_does_nothing_with_vim_off() {
        let core = StubBackend::new();
        focus(&core, true);
        assert_eq!(core.model().input_mode, InputMode::Normal);
    }

    #[test]
    fn pending_keys_reach_the_model_with_which_key() {
        let core = vim_core();
        core.invoke(Command::Vim(VimCommand::ReportPending {
            keys: vec!["<Space>".to_string()],
            count: Some(3),
        }))
        .unwrap();
        let model = core.model();
        assert_eq!(model.pending_keys, vec!["<Space>"]);
        assert_eq!(model.pending_count, Some(3));
        assert_eq!(model.which_key.as_ref().map(|w| w.title.as_str()), Some("SPC"));

        core.invoke(Command::Vim(VimCommand::ReportPending {
            keys: Vec::new(),
            count: None,
        }))
        .unwrap();
        let model = core.model();
        assert!(model.pending_keys.is_empty());
        assert!(model.which_key.is_none());
    }

    #[test]
    fn turning_vim_off_leaves_insert_mode() {
        let core = vim_core();
        focus(&core, true);
        core.invoke(Command::VimSetEnabled { enabled: false }).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
    }
}
