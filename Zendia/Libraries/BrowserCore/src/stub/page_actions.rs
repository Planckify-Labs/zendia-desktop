/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for [`crate::page_actions`]: asks the page's Automation World and acts on its answers, and owns the
//! jump that the UI draws over the page.

use super::StubBackend;
use crate::BrowserCore;
use crate::commands::{Command, CommandResult};
use crate::events::Event;
use crate::model::{InputMode, PageFlash, TabId};
use crate::page_actions::{HintKey, HintOutcome, PageAction, PageHintSession};
use crate::vim_keys::VimCommand;
use zendia_automation::{AutomationRequest, AutomationResponse, JumpKind, ViewportRect};

impl StubBackend {
    pub(super) fn handle_page_actions(&self, action: PageAction) -> CommandResult {
        match action {
            PageAction::PageHintInput { key } => self.feed_page_hint(key),
            PageAction::PageHintRefresh => self.refresh_page_hints(),
            PageAction::PageHintCancel => {
                self.set_page_hints(None);
                Ok(())
            }
            PageAction::VisualEnter => {
                self.set_page_hints(None);
                self.ask_page(AutomationRequest::VisualEnter)
            }
            PageAction::VisualExit => {
                self.set_page_hints(None);
                self.set_visual_caret(None);
                self.set_input_mode(InputMode::Normal);
                self.ask_page(AutomationRequest::VisualExit)
            }
            PageAction::VisualYank => {
                self.set_page_hints(None);
                self.set_visual_caret(None);
                self.set_input_mode(InputMode::Normal);
                self.ask_page(AutomationRequest::VisualYank)
            }
            PageAction::VisualSwapEnds => self.ask_page(AutomationRequest::VisualSwapEnds),
            PageAction::VisualMove { motion, count } => self.ask_page(AutomationRequest::VisualMove {
                motion,
                count: count.unwrap_or(1),
            }),
            PageAction::VisualObject { keys, count } => self.ask_page(AutomationRequest::VisualTextObject {
                keys,
                count: count.unwrap_or(1),
            }),
            PageAction::Jump { kind } => self.start_jump(kind),
            PageAction::InputChoose { count } => {
                self.set_page_hints(None);
                self.ask_page(AutomationRequest::InputChoose { count })
            }
            PageAction::InputLast { count } => self.ask_page(AutomationRequest::InputLast { count }),
            PageAction::InputNext { count } => self.ask_page(AutomationRequest::InputStep {
                delta: count.unwrap_or(1).min(1000) as i32,
            }),
            PageAction::InputPrev { count } => self.ask_page(AutomationRequest::InputStep {
                delta: -(count.unwrap_or(1).min(1000) as i32),
            }),
            PageAction::Captcha { op } => self.ask_page(AutomationRequest::CaptchaAction { action: op }),
        }
    }

    /// Acts on an answer from `tab_id`'s Automation World.
    pub fn feed_automation_response(&self, tab_id: TabId, response: AutomationResponse) {
        match response {
            AutomationResponse::Targets { jump, pattern, targets } => self.on_targets(tab_id, jump, pattern, targets),
            AutomationResponse::Activated { mode, url, message } => {
                if mode.as_deref() == Some("i") && self.model().vim_mode_enabled {
                    self.set_input_mode(InputMode::Insert);
                }
                if let Some(url) = url {
                    // A capital label opens a link the way a Ctrl+click does: in a tab behind this one.
                    let _ = self.invoke(Command::TabNew {
                        url: Some(url),
                        after: Some(tab_id),
                        activate: false,
                    });
                }
                if let Some(message) = message {
                    eprintln!("zendia: automation: {message}");
                }
            }
            AutomationResponse::Press { x, y } => {
                if let Some(source) = self.current_tab_source() {
                    let _ = source.click_at(tab_id, f64::from(x), f64::from(y));
                }
            }
            AutomationResponse::VisualState {
                active,
                yanked,
                caret,
                land,
                flash,
            } => self.on_visual_state(tab_id, active, yanked, caret, land, flash),
            AutomationResponse::ExplicitInsert => {
                let _ = self.invoke(Command::Vim(VimCommand::ModeInsert));
            }
            AutomationResponse::FocusRefused { .. } => {}
            AutomationResponse::Put { ok, text, flash } => {
                if ok {
                    self.flash_page(tab_id, flash);
                } else {
                    // Out of a field, a yank picked from the history goes to the clipboard.
                    self.bus.publish(Event::ClipboardCopyRequested { text });
                    self.show_toast("Copied to the clipboard.".to_string(), None, None);
                }
            }
            AutomationResponse::Cycle { index } => {
                let text = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state.vim.yank_ring.at(index).map(|entry| entry.text.clone())
                };
                if let Some(text) = text {
                    let _ = self.ask_page_for_tab(
                        tab_id,
                        AutomationRequest::YankPut {
                            text,
                            index,
                            replace: true,
                        },
                    );
                }
            }
            AutomationResponse::NeedRuntime | AutomationResponse::Ok => {}
            AutomationResponse::Message { text } => eprintln!("zendia: automation: {text}"),
        }
    }

    pub(super) fn clear_page_hints_on_vim_disabled(&self) {
        self.set_page_hints(None);
        self.set_visual_caret(None);
    }

    /// Asks the active tab's Automation World.
    pub(super) fn ask_page(&self, request: AutomationRequest) -> CommandResult {
        let Some(tab_id) = self.model().active_tab else {
            return Ok(());
        };
        self.ask_page_for_tab(tab_id, request)
    }

    pub(super) fn ask_page_for_tab(&self, tab_id: TabId, request: AutomationRequest) -> CommandResult {
        match self.current_tab_source() {
            Some(source) => source.run_automation(tab_id, request),
            None => Ok(()),
        }
    }

    fn start_jump(&self, kind: JumpKind) -> CommandResult {
        let Some(tab_id) = self.model().active_tab else {
            return Ok(());
        };
        // In visual mode a text pick grows the selection; anywhere else it starts visual mode there.
        let extend = kind != JumpKind::Land && self.model().input_mode == InputMode::Visual;
        let mut session = PageHintSession::new(tab_id, kind, extend);
        session.awaiting = Some(String::new());
        self.set_page_hints(Some(session));
        self.ask_page_for_tab(
            tab_id,
            AutomationRequest::JumpScan {
                jump: kind,
                pattern: String::new(),
            },
        )
    }

    /// The page moved: the visual mode cursor and a jump's targets are found again where they are now.
    fn refresh_page_hints(&self) -> CommandResult {
        if self.model().input_mode == InputMode::Visual {
            self.ask_page(AutomationRequest::Caret)?;
        }
        let request = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(session) = state.model.page_hints.as_mut() else {
                return Ok(());
            };
            if session.awaiting.is_some() {
                return Ok(());
            }
            session.awaiting = Some(session.pattern.clone());
            (
                session.tab_id,
                AutomationRequest::JumpScan {
                    jump: session.kind,
                    pattern: session.pattern.clone(),
                },
            )
        };
        self.ask_page_for_tab(request.0, request.1)
    }

    fn on_targets(&self, tab_id: TabId, jump: JumpKind, pattern: String, targets: Vec<zendia_automation::PageTarget>) {
        let mut session = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.page_hints.clone()
        };
        if jump == JumpKind::Inputs {
            // `i` found several fields: they are labeled to pick one.
            let mut fields = PageHintSession::new(tab_id, JumpKind::Inputs, false);
            fields.set_targets(targets);
            self.set_page_hints(Some(fields));
            return;
        }
        let Some(current) = session.as_mut() else {
            return;
        };
        if current.tab_id != tab_id || current.kind != jump {
            return;
        }
        let first_scan = current.items.is_empty() && current.pattern.is_empty() && pattern.is_empty();
        if first_scan || current.awaiting.as_deref() == Some(pattern.as_str()) && pattern == current.pattern {
            // The first look at the page, or a look again after it moved.
            current.awaiting = None;
            current.set_targets(targets);
        } else {
            current.answer(&pattern, targets);
        }
        // A jump that labels everything at once and finds nothing is over before it starts.
        if current.kind.labels_at_once() && current.items.is_empty() {
            self.set_page_hints(None);
            return;
        }
        let queued: Vec<HintKey> = current.queued.drain(..).collect();
        self.set_page_hints(session);
        for key in queued {
            let _ = self.feed_page_hint(key);
        }
    }

    fn feed_page_hint(&self, key: HintKey) -> CommandResult {
        let (outcome, session) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(session) = state.model.page_hints.as_mut() else {
                return Ok(());
            };
            let outcome = session.feed(key);
            (outcome, state.model.page_hints.clone())
        };
        let Some(session) = session else {
            return Ok(());
        };
        match outcome {
            HintOutcome::Pick { id, new_tab } => {
                self.set_page_hints(None);
                self.ask_page_for_tab(
                    session.tab_id,
                    AutomationRequest::JumpPick {
                        id,
                        new_tab,
                        extend: session.extend,
                    },
                )
            }
            HintOutcome::Ask { pattern } => {
                self.publish_page_hints(Some(session.clone()));
                self.ask_page_for_tab(
                    session.tab_id,
                    AutomationRequest::JumpScan {
                        jump: session.kind,
                        pattern,
                    },
                )
            }
            HintOutcome::Changed => {
                self.publish_page_hints(Some(session));
                Ok(())
            }
            HintOutcome::Finished | HintOutcome::FinishedPassingKey => {
                self.set_page_hints(None);
                Ok(())
            }
            HintOutcome::Ignored => Ok(()),
        }
    }

    fn on_visual_state(
        &self,
        tab_id: TabId,
        active: bool,
        yanked: Option<String>,
        caret: Option<ViewportRect>,
        land: bool,
        flash: Vec<ViewportRect>,
    ) {
        if !self.model().vim_mode_enabled {
            return;
        }
        self.set_visual_caret(if active { caret } else { None });
        let mode = self.model().input_mode;
        if active && mode != InputMode::Visual {
            self.set_input_mode(InputMode::Visual);
        } else if !active && mode == InputMode::Visual {
            self.set_input_mode(InputMode::Normal);
        }
        if let Some(text) = yanked
            && !text.is_empty()
        {
            self.record_yank(&text);
            self.bus.publish(Event::ClipboardCopyRequested { text });
        }
        if !flash.is_empty() {
            self.flash_page(tab_id, flash);
        }
        // A collapsed cursor after `v`: every word and paragraph gets a label, to say where it lands. <Esc> leaves
        // the cursor where it is.
        if land {
            let _ = self.start_jump(JumpKind::Land);
        }
    }

    fn flash_page(&self, tab_id: TabId, rects: Vec<ViewportRect>) {
        let flash = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let serial = state.model.page_flash.as_ref().map_or(1, |flash| flash.serial + 1);
            let flash = PageFlash { serial, tab_id, rects };
            state.model.page_flash = Some(flash.clone());
            flash
        };
        self.bus.publish(Event::PageFlashed { flash });
    }

    pub(super) fn set_page_hints(&self, hints: Option<PageHintSession>) {
        let changed = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.page_hints == hints {
                false
            } else {
                state.model.page_hints = hints.clone();
                true
            }
        };
        if changed {
            self.bus.publish(Event::PageHintsChanged { hints });
        }
    }

    fn publish_page_hints(&self, hints: Option<PageHintSession>) {
        self.bus.publish(Event::PageHintsChanged { hints });
    }

    pub(super) fn set_visual_caret(&self, caret: Option<ViewportRect>) {
        let changed = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.model.visual_caret == caret {
                false
            } else {
                state.model.visual_caret = caret;
                true
            }
        };
        if changed {
            self.bus.publish(Event::VisualCaretChanged { caret });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zendia_automation::PageTarget;

    fn target(id: &str, x: f32) -> PageTarget {
        PageTarget {
            id: id.to_string(),
            x,
            y: 10.0,
            width: 40.0,
            height: 20.0,
            kind: "control".to_string(),
            nexts: Vec::new(),
            distance: 0.0,
            href: None,
        }
    }

    fn vim_core() -> (StubBackend, TabId) {
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        let tab_id = core.model().active_tab.unwrap();
        (core, tab_id)
    }

    #[test]
    fn hints_open_from_the_page_and_close_on_a_pick() {
        let (core, tab_id) = vim_core();
        core.invoke(Command::from_name_and_args("jump.hints", &[]).unwrap())
            .unwrap();
        core.feed_automation_response(
            tab_id,
            AutomationResponse::Targets {
                jump: JumpKind::Hints,
                pattern: String::new(),
                targets: vec![target("one", 10.0), target("two", 60.0)],
            },
        );
        let hints = core.model().page_hints.expect("labels are up");
        assert_eq!(hints.items.len(), 2);
        let key = hints.items[1].label.clone().unwrap();
        core.invoke(Command::from_name_and_args("page_hint.input", &[("key".to_string(), key)]).unwrap())
            .unwrap();
        assert!(core.model().page_hints.is_none());
    }

    #[test]
    fn hints_with_nothing_to_label_end_at_once() {
        let (core, tab_id) = vim_core();
        core.invoke(Command::from_name_and_args("jump.hints", &[]).unwrap())
            .unwrap();
        core.feed_automation_response(
            tab_id,
            AutomationResponse::Targets {
                jump: JumpKind::Hints,
                pattern: String::new(),
                targets: Vec::new(),
            },
        );
        assert!(core.model().page_hints.is_none());
    }

    #[test]
    fn v_lands_by_label_and_y_yanks() {
        let (core, tab_id) = vim_core();
        core.feed_automation_response(
            tab_id,
            AutomationResponse::VisualState {
                active: true,
                yanked: None,
                caret: None,
                land: true,
                flash: Vec::new(),
            },
        );
        assert_eq!(core.model().input_mode, InputMode::Visual);
        assert_eq!(core.model().page_hints.map(|hints| hints.kind), Some(JumpKind::Land));

        core.feed_automation_response(
            tab_id,
            AutomationResponse::VisualState {
                active: false,
                yanked: Some("hello world".to_string()),
                caret: None,
                land: false,
                flash: Vec::new(),
            },
        );
        assert_eq!(core.model().input_mode, InputMode::Normal);
        let state = core.state.lock().unwrap();
        assert_eq!(
            state.vim.yank_ring.at(0).map(|entry| entry.text.as_str()),
            Some("hello world")
        );
    }

    #[test]
    fn several_fields_are_labeled() {
        let (core, tab_id) = vim_core();
        core.feed_automation_response(
            tab_id,
            AutomationResponse::Targets {
                jump: JumpKind::Inputs,
                pattern: String::new(),
                targets: vec![target("f1", 0.0), target("f2", 50.0)],
            },
        );
        assert_eq!(core.model().page_hints.map(|hints| hints.kind), Some(JumpKind::Inputs));
    }
}
