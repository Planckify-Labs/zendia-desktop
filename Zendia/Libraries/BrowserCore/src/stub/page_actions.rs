/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for [`crate::page_actions`]: asks the page's Automation World and acts on its answers, and owns the
//! jump that the UI draws over each page (including staying CAPTCHA jumps and the keyboard cursor, `CAP-01`..`CAP-09`).

use super::StubBackend;
use crate::BrowserCore;
use crate::captcha::{
    CHALLENGE_POLL_MS, CHALLENGE_SETTLE_MS, NO_CAPTCHA_CURSOR_BANNER, PadStepOutcome, PointerPadOptions, VACANT_POLLS,
    WATCH_AFTER_CLICK_MS, middle, rects_meet,
};
use crate::commands::{Command, CommandResult};
use crate::events::Event;
use crate::model::{InputMode, PageFlash, PageFlashKind, TabId};
use crate::page_actions::{HintKey, HintOutcome, PageAction, PageHintItem, PageHintSession};
use crate::tab_source::PointerStep;
use zendia_automation::{AutomationRequest, AutomationResponse, CaptchaOp, JumpKind, PageTarget, ViewportRect};

struct VisualStateUpdate {
    active: bool,
    yanked: Option<String>,
    copied_by_page: bool,
    caret: Option<ViewportRect>,
    land: bool,
    flash: Vec<ViewportRect>,
}

struct TargetsUpdate {
    jump: JumpKind,
    pattern: String,
    targets: Vec<PageTarget>,
    captcha_tokens: u32,
    captcha_areas: Vec<ViewportRect>,
    viewport: Option<(f32, f32)>,
}

impl StubBackend {
    pub(super) fn handle_page_actions(&self, action: PageAction) -> CommandResult {
        match action {
            PageAction::PageHintInput { key } => self.feed_page_hint(key),
            PageAction::PageHintRefresh => self.refresh_page_hints(),
            PageAction::PageHintPoll { seq } => self.poll_page_hints(seq),
            PageAction::PageHintClearNotice { seq } => self.clear_page_hint_notice(seq),
            PageAction::PageHintCancel => {
                if let Some(tab_id) = self.effective_keyed_tab() {
                    self.set_page_hints_for_tab(tab_id, None);
                }
                Ok(())
            }
            PageAction::VisualEnter => {
                let Some(tab_id) = self.effective_keyed_tab() else {
                    return Ok(());
                };
                self.set_page_hints_for_tab(tab_id, None);
                self.ask_page_mode_changing(tab_id, AutomationRequest::VisualEnter)
            }
            PageAction::VisualExit => {
                if let Some(tab_id) = self.effective_keyed_tab() {
                    self.set_page_hints_for_tab(tab_id, None);
                    self.set_visual_caret_for_tab(tab_id, None);
                    self.set_page_mode_for_tab(tab_id, InputMode::Normal);
                    self.ask_page_for_tab(tab_id, AutomationRequest::VisualExit)
                } else {
                    Ok(())
                }
            }
            PageAction::VisualYank => {
                if let Some(tab_id) = self.effective_keyed_tab() {
                    self.set_page_hints_for_tab(tab_id, None);
                    self.set_visual_caret_for_tab(tab_id, None);
                    self.set_page_mode_for_tab(tab_id, InputMode::Normal);
                    self.ask_page_for_tab(tab_id, AutomationRequest::VisualYank)
                } else {
                    Ok(())
                }
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
            PageAction::Jump { kind, or_page } => self.start_jump(kind, or_page),
            PageAction::InputChoose { count } => {
                let Some(tab_id) = self.effective_keyed_tab() else {
                    return Ok(());
                };
                self.set_page_hints_for_tab(tab_id, None);
                self.ask_page_mode_changing(tab_id, AutomationRequest::InputChoose { count })
            }
            PageAction::InputFirst { count } => {
                let Some(tab_id) = self.effective_keyed_tab() else {
                    return Ok(());
                };
                self.ask_page_mode_changing(tab_id, AutomationRequest::InputFirst { count })
            }
            PageAction::InputLast { count } => {
                let Some(tab_id) = self.effective_keyed_tab() else {
                    return Ok(());
                };
                self.ask_page_mode_changing(tab_id, AutomationRequest::InputLast { count })
            }
            PageAction::InputNext { count } => {
                let Some(tab_id) = self.effective_keyed_tab() else {
                    return Ok(());
                };
                self.ask_page_mode_changing(
                    tab_id,
                    AutomationRequest::InputStep {
                        delta: count.unwrap_or(1).min(1000) as i32,
                    },
                )
            }
            PageAction::InputPrev { count } => {
                let Some(tab_id) = self.effective_keyed_tab() else {
                    return Ok(());
                };
                self.ask_page_mode_changing(
                    tab_id,
                    AutomationRequest::InputStep {
                        delta: -(count.unwrap_or(1).min(1000) as i32),
                    },
                )
            }
            PageAction::Captcha { op } => {
                let Some(tab_id) = self.effective_keyed_tab() else {
                    return Ok(());
                };
                if op == CaptchaOp::Answer {
                    self.ask_page_mode_changing(tab_id, AutomationRequest::CaptchaAction { action: op })
                } else {
                    self.ask_page_for_tab(tab_id, AutomationRequest::CaptchaAction { action: op })
                }
            }
            PageAction::CaptchaFill { text } => self.ask_page(AutomationRequest::CaptchaFill { text }),
        }
    }

    /// Acts on an answer from `tab_id`'s Automation World.
    pub fn feed_automation_response(&self, tab_id: TabId, response: AutomationResponse) {
        if !matches!(
            response,
            AutomationResponse::PageMoved { .. }
                | AutomationResponse::PageYank { .. }
                | AutomationResponse::CaptchaWatchReport { .. }
                | AutomationResponse::CaptchaWatchGone { .. }
        ) {
            self.set_mode_transition_pending(tab_id, false);
        }

        match response {
            AutomationResponse::Targets {
                jump,
                pattern,
                targets,
                captcha_tokens,
                captcha_areas,
                viewport,
            } => self.on_targets(
                tab_id,
                TargetsUpdate {
                    jump,
                    pattern,
                    targets,
                    captcha_tokens,
                    captcha_areas,
                    viewport,
                },
            ),
            AutomationResponse::CaptchaAreas {
                areas,
                captcha_tokens,
                viewport,
            } => self.on_captcha_areas(tab_id, areas, captcha_tokens, viewport),
            AutomationResponse::Activated { mode, url, message } => {
                if mode.as_deref() == Some("i") && self.model().vim_mode_enabled {
                    let mut release_steps = Vec::new();
                    {
                        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                        let page = state.vim.pages.entry(tab_id).or_default();
                        page.editing = true;
                        page.mode = InputMode::Insert;
                        // Keep staying CAPTCHA labels alive while typing into the challenge's answer field (`CAP-08`).
                        if let Some(hints) = page.page_hints.as_mut() {
                            if hints.staying {
                                hints.switching = false;
                                hints.schedule_poll(CHALLENGE_POLL_MS);
                            } else {
                                if let Some(mut pad) = hints.cursor_pad.take() {
                                    release_steps = pad.release_if_held();
                                }
                                page.page_hints = None;
                            }
                        }
                        page.visual_caret = None;
                    }
                    self.dispatch_pointer_for_tab(tab_id, &release_steps);
                    self.sync_published_vim_state();
                }
                if let Some(url) = url {
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
            AutomationResponse::Press { x, y, ctrl } => {
                if let Some(source) = self.current_tab_source() {
                    let _ = source.click_at_with_modifiers(tab_id, f64::from(x), f64::from(y), ctrl);
                }
                // After a challenge press, ensure the top window keeps keyboard focus (`ZendiaJumpContent:1264`, `CAP-07`).
                let is_staying = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state
                        .vim
                        .pages
                        .get(&tab_id)
                        .and_then(|p| p.page_hints.as_ref())
                        .is_some_and(|h| h.staying)
                };
                if is_staying {
                    let _ = self.ask_page_for_tab(tab_id, AutomationRequest::RefocusTop);
                }
            }
            AutomationResponse::VisualState {
                active,
                yanked,
                copied_by_page,
                caret,
                land,
                flash,
            } => self.on_visual_state(
                tab_id,
                VisualStateUpdate {
                    active,
                    yanked,
                    copied_by_page,
                    caret,
                    land,
                    flash,
                },
            ),
            AutomationResponse::ExplicitInsert => {
                {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let page = state.vim.pages.entry(tab_id).or_default();
                    page.explicit_insert = true;
                    page.mode = InputMode::Insert;
                    page.page_hints = None;
                    page.visual_caret = None;
                }
                self.sync_published_vim_state();
            }
            AutomationResponse::EditableFocus { editing } => {
                self.apply_editable_focus(tab_id, editing);
            }
            AutomationResponse::PageMoved { scrolled } => {
                self.bus.publish(Event::PageMoved { tab_id, scrolled });
                if self.effective_keyed_tab() == Some(tab_id) {
                    let _ = self.refresh_page_hints_for_tab(tab_id);
                }
            }
            AutomationResponse::PageYank { text, host } => {
                if !text.trim().is_empty() {
                    self.record_yank_from_page(&text, tab_id, Some(host));
                }
            }
            AutomationResponse::CaptchaWatchReport {
                frame_id,
                top,
                url,
                detections,
                state,
            } => {
                let now_ms = crate::yank_webchat_actions::now_unix_ms();
                let published = {
                    let mut st = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let (record, _changed) =
                        st.captcha_watch
                            .report(tab_id, &url, &detections, &state, frame_id, top, now_ms);
                    let next_finding = record.as_ref().map(|r| r.text.clone());
                    let prev_finding = st.model.tab_captcha_findings.get(&tab_id).cloned();
                    if next_finding != prev_finding {
                        if let Some(ref text) = next_finding {
                            st.model.tab_captcha_findings.insert(tab_id, text.clone());
                        } else {
                            st.model.tab_captcha_findings.remove(&tab_id);
                        }
                        Some(next_finding)
                    } else {
                        None
                    }
                };
                if let Some(finding) = published {
                    self.bus.publish(Event::TabCaptchaFindingChanged { tab_id, finding });
                }
            }
            AutomationResponse::CaptchaWatchGone { frame_id } => {
                let now_ms = crate::yank_webchat_actions::now_unix_ms();
                let published = {
                    let mut st = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let (record, _changed) = st.captcha_watch.drop_frame(tab_id, frame_id, now_ms);
                    let next_finding = record.as_ref().map(|r| r.text.clone());
                    let prev_finding = st.model.tab_captcha_findings.get(&tab_id).cloned();
                    if next_finding != prev_finding {
                        if let Some(ref text) = next_finding {
                            st.model.tab_captcha_findings.insert(tab_id, text.clone());
                        } else {
                            st.model.tab_captcha_findings.remove(&tab_id);
                        }
                        Some(next_finding)
                    } else {
                        None
                    }
                };
                if let Some(finding) = published {
                    self.bus.publish(Event::TabCaptchaFindingChanged { tab_id, finding });
                }
            }
            AutomationResponse::ReaderMode { active: _, message } => {
                if let Some(msg) = message {
                    self.show_toast(msg, None, None);
                }
            }
            AutomationResponse::Put { ok, text, flash } => {
                if ok {
                    self.flash_page(tab_id, flash, PageFlashKind::Put);
                } else {
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
        let mut releases: Vec<(TabId, Vec<PointerStep>)> = Vec::new();
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            for (&tab_id, page) in state.vim.pages.iter_mut() {
                if let Some(mut hints) = page.page_hints.take()
                    && let Some(mut pad) = hints.cursor_pad.take()
                {
                    let steps = pad.release_if_held();
                    if !steps.is_empty() {
                        releases.push((tab_id, steps));
                    }
                }
                page.visual_caret = None;
                page.mode_transition_pending = false;
            }
        }
        for (tab_id, steps) in releases {
            self.dispatch_pointer_for_tab(tab_id, &steps);
        }
        self.sync_published_vim_state();
    }

    pub(super) fn dispatch_pointer_for_tab(&self, tab_id: TabId, steps: &[PointerStep]) {
        if steps.is_empty() {
            return;
        }
        if let Some(source) = self.current_tab_source() {
            let _ = source.dispatch_pointer_steps(tab_id, steps);
        }
    }

    /// Asks the effective keyed page's Automation World.
    pub(super) fn ask_page(&self, request: AutomationRequest) -> CommandResult {
        let Some(tab_id) = self.effective_keyed_tab() else {
            return Ok(());
        };
        self.ask_page_for_tab(tab_id, request)
    }

    pub(super) fn ask_page_mode_changing(&self, tab_id: TabId, request: AutomationRequest) -> CommandResult {
        match self.current_tab_source() {
            Some(source) => {
                self.set_mode_transition_pending(tab_id, true);
                let res = source.run_automation(tab_id, request);
                if res.is_err() {
                    self.set_mode_transition_pending(tab_id, false);
                }
                res
            }
            None => Ok(()),
        }
    }

    pub(super) fn ask_page_for_tab(&self, tab_id: TabId, request: AutomationRequest) -> CommandResult {
        match self.current_tab_source() {
            Some(source) => source.run_automation(tab_id, request),
            None => Ok(()),
        }
    }

    fn start_jump(&self, kind: JumpKind, or_page: bool) -> CommandResult {
        let Some(tab_id) = self.effective_keyed_tab() else {
            return Ok(());
        };
        self.start_jump_for_tab(tab_id, kind, or_page)
    }

    fn start_jump_for_tab(&self, tab_id: TabId, kind: JumpKind, or_page: bool) -> CommandResult {
        let page_mode = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state
                .vim
                .pages
                .get(&tab_id)
                .map(|p| p.mode)
                .unwrap_or(InputMode::Normal)
        };
        let extend = kind != JumpKind::Land && page_mode == InputMode::Visual;
        let mut session = PageHintSession::new(tab_id, kind, extend);
        session.or_page = or_page;
        session.awaiting = Some(String::new());
        self.set_page_hints_for_tab(tab_id, Some(session));
        if kind == JumpKind::Captcha {
            let _ = self.ask_page_for_tab(tab_id, AutomationRequest::RefocusTop);
        }
        self.ask_page_for_tab(
            tab_id,
            AutomationRequest::JumpScan {
                jump: kind,
                pattern: String::new(),
                top_only: false,
                include_fields: false,
                watch_mutations: false,
            },
        )
    }

    /// The page moved: the visual mode cursor and a jump's targets are found again where they are now.
    fn refresh_page_hints(&self) -> CommandResult {
        let Some(tab_id) = self.effective_keyed_tab() else {
            return Ok(());
        };
        self.refresh_page_hints_for_tab(tab_id)
    }

    fn refresh_page_hints_for_tab(&self, tab_id: TabId) -> CommandResult {
        let in_visual = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state
                .vim
                .pages
                .get(&tab_id)
                .is_some_and(|p| p.mode == InputMode::Visual)
        };
        if in_visual {
            self.ask_page_for_tab(tab_id, AutomationRequest::Caret)?;
        }
        let request = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(page) = state.vim.pages.get_mut(&tab_id) else {
                return Ok(());
            };
            let Some(session) = page.page_hints.as_mut() else {
                return Ok(());
            };
            if session.cursor_pad.is_some() {
                return Ok(());
            }
            if session.awaiting.is_some() {
                session.refresh_queued = true;
                drop(state);
                self.sync_published_vim_state();
                return Ok(());
            }
            session.refresh_queued = false;
            session.awaiting = Some(session.pattern.clone());
            AutomationRequest::JumpScan {
                jump: session.kind,
                pattern: session.pattern.clone(),
                top_only: false,
                include_fields: false,
                watch_mutations: false,
            }
        };
        self.sync_published_vim_state();
        self.ask_page_for_tab(tab_id, request)
    }

    fn poll_page_hints(&self, seq: Option<u64>) -> CommandResult {
        let Some(tab_id) = self.effective_keyed_tab() else {
            return Ok(());
        };
        let req = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(page) = state.vim.pages.get_mut(&tab_id) else {
                return Ok(());
            };
            let Some(session) = page.page_hints.as_mut() else {
                return Ok(());
            };
            if let Some(expected) = seq
                && session.poll_seq != expected
            {
                return Ok(());
            }
            if let Some(pad) = session.cursor_pad.as_ref() {
                if !pad.watches_captcha {
                    return Ok(());
                }
                session.schedule_poll(CHALLENGE_POLL_MS);
                Some(AutomationRequest::CaptchaAreas)
            } else if session.staying {
                session.schedule_poll(CHALLENGE_POLL_MS);
                if session.awaiting.is_some() {
                    None
                } else {
                    session.awaiting = Some(String::new());
                    Some(AutomationRequest::JumpScan {
                        jump: session.kind,
                        pattern: String::new(),
                        top_only: false,
                        include_fields: false,
                        watch_mutations: false,
                    })
                }
            } else {
                None
            }
        };
        self.sync_published_vim_state();
        if let Some(request) = req {
            self.ask_page_for_tab(tab_id, request)?;
        }
        Ok(())
    }

    fn clear_page_hint_notice(&self, seq: u64) -> CommandResult {
        let Some(tab_id) = self.effective_keyed_tab() else {
            return Ok(());
        };
        let changed = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(session) = state.vim.pages.get_mut(&tab_id).and_then(|p| p.page_hints.as_mut())
                && session.notice_seq == seq
            {
                session.notice = None;
                if let Some(pad) = session.cursor_pad.as_mut() {
                    pad.notice = None;
                }
                true
            } else {
                false
            }
        };
        if changed {
            self.sync_published_vim_state();
        }
        Ok(())
    }

    fn on_captcha_areas(
        &self,
        tab_id: TabId,
        areas: Vec<ViewportRect>,
        captcha_tokens: u32,
        viewport: Option<(f32, f32)>,
    ) {
        let mut session = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.vim.pages.get(&tab_id).and_then(|p| p.page_hints.clone())
        };
        let Some(current) = session.as_mut() else {
            return;
        };
        if let Some(vp) = viewport {
            current.viewport = vp;
        }
        current.captcha_areas = areas.clone();
        if let Some(drag) = current.pending_cursor_drag.take() {
            let found = !areas.is_empty();
            let staying = current.staying;
            let init_steps = current.open_cursor_pad(PointerPadOptions {
                areas,
                viewport: current.viewport,
                drag,
                grab: None,
                grab_rect: None,
                slider: false,
                track: None,
                piece: None,
                lattice: None,
                banner: if found {
                    String::new()
                } else {
                    NO_CAPTCHA_CURSOR_BANNER.to_string()
                },
                can_verify: staying || found,
                watches_captcha: staying || found,
                standalone: false,
                click_order: false,
            });
            self.set_page_hints_for_tab(tab_id, session);
            let _ = self.ask_page_for_tab(tab_id, AutomationRequest::RefocusTop);
            self.dispatch_pointer_for_tab(tab_id, &init_steps);
            return;
        }
        if current.staying && current.tokens_recorded && captcha_tokens > current.initial_tokens {
            self.set_page_hints_for_tab(tab_id, None);
            return;
        }
        let mut pad_closed = false;
        let mut release_steps = Vec::new();
        if let Some(pad) = current.cursor_pad.as_mut()
            && pad.watches_captcha
            && pad.holding.is_none()
        {
            let still_there = areas
                .iter()
                .any(|now| pad.areas.iter().any(|prev| rects_meet(*now, *prev)));
            pad.gone_polls = if still_there { 0 } else { pad.gone_polls + 1 };
            if pad.gone_polls >= VACANT_POLLS {
                release_steps = pad.release_if_held();
                pad_closed = true;
            }
        }
        self.dispatch_pointer_for_tab(tab_id, &release_steps);
        if pad_closed {
            let standalone = current.cursor_pad.as_ref().is_some_and(|p| p.standalone);
            current.cursor_pad = None;
            if standalone {
                self.set_page_hints_for_tab(tab_id, None);
            } else {
                current.schedule_poll(CHALLENGE_SETTLE_MS);
                self.set_page_hints_for_tab(tab_id, session);
                let _ = self.refresh_page_hints_for_tab(tab_id);
            }
            return;
        }
        self.set_page_hints_for_tab(tab_id, session);
    }

    fn on_targets(&self, tab_id: TabId, update: TargetsUpdate) {
        let TargetsUpdate {
            jump,
            pattern,
            targets,
            captcha_tokens,
            captcha_areas,
            viewport,
        } = update;
        if jump == JumpKind::Inputs {
            let mut fields = PageHintSession::new(tab_id, JumpKind::Inputs, false);
            fields.set_targets(targets);
            self.set_page_hints_for_tab(tab_id, Some(fields));
            return;
        }
        let mut session = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.vim.pages.get(&tab_id).and_then(|p| p.page_hints.clone())
        };
        let Some(current) = session.as_mut() else {
            self.bus.publish(Event::PopupTargetsUpdated { tab_id, targets });
            return;
        };
        if current.tab_id != tab_id || current.kind != jump {
            return;
        }
        if let Some(vp) = viewport
            && vp.0 > 0.0
            && vp.1 > 0.0
        {
            current.viewport = vp;
        }
        if !captcha_areas.is_empty() || matches!(jump, JumpKind::Captcha | JumpKind::Challenge) {
            current.captcha_areas = captcha_areas.clone();
        }
        let first_scan =
            !current.tokens_recorded && current.items.is_empty() && current.pattern.is_empty() && pattern.is_empty();
        if !current.tokens_recorded {
            current.initial_tokens = captcha_tokens;
            current.tokens_recorded = true;
        } else if current.staying && captcha_tokens > current.initial_tokens {
            // Solved: a token field filled or widget gained a solved mark (`CAP-02`, `CAP-03`).
            self.set_page_hints_for_tab(tab_id, None);
            return;
        }

        // Initial `gc` with 0 labelable targets falls back to `jump.hints` (`orPage`) or opens `PointerPad` (`CAP-03`, `CAP-05`).
        if first_scan && jump == JumpKind::Captcha && targets.is_empty() {
            current.awaiting = None;
            let found = !captcha_areas.is_empty();
            if !found && current.or_page {
                let _ = self.start_jump_for_tab(tab_id, JumpKind::Hints, false);
                return;
            }
            let init_steps = current.open_cursor_pad(PointerPadOptions {
                areas: captcha_areas,
                viewport: current.viewport,
                drag: false,
                grab: None,
                grab_rect: None,
                slider: false,
                track: None,
                piece: None,
                lattice: None,
                banner: if found {
                    String::new()
                } else {
                    NO_CAPTCHA_CURSOR_BANNER.to_string()
                },
                can_verify: found,
                watches_captcha: found,
                standalone: true,
                click_order: false,
            });
            self.set_page_hints_for_tab(tab_id, session);
            self.dispatch_pointer_for_tab(tab_id, &init_steps);
            return;
        }

        let was_switching = current.switching;
        current.switching = false;
        if first_scan || current.awaiting.as_deref() == Some(pattern.as_str()) && pattern == current.pattern {
            current.awaiting = None;
            current.set_targets(targets);
        } else {
            current.answer(&pattern, targets);
        }

        if current.staying {
            if current.items.is_empty() {
                // Only count vacant polls once the initial scan or transition has begun (`VACANT_POLLS = 3`).
                if !was_switching {
                    current.vacant_polls = current.vacant_polls.saturating_add(1);
                }
                if current.vacant_polls >= VACANT_POLLS {
                    self.set_page_hints_for_tab(tab_id, None);
                    return;
                }
            } else {
                current.vacant_polls = 0;
            }
        } else if current.kind.labels_at_once() && current.items.is_empty() {
            self.set_page_hints_for_tab(tab_id, None);
            return;
        }

        let pending_purpose = current.pending_purpose.take();
        let refresh_needed = std::mem::take(&mut current.refresh_queued);
        let queued: Vec<HintKey> = current.queued.drain(..).collect();
        self.set_page_hints_for_tab(tab_id, session);

        if let Some(purposes) = pending_purpose {
            let _ = self.complete_purpose_after_reread(tab_id, &purposes);
        }
        for key in queued {
            let _ = self.feed_page_hint_for_tab(tab_id, key);
        }
        if refresh_needed {
            let _ = self.refresh_page_hints_for_tab(tab_id);
        }
    }

    fn feed_page_hint(&self, key: HintKey) -> CommandResult {
        let Some(tab_id) = self.effective_keyed_tab() else {
            return Ok(());
        };
        self.feed_page_hint_for_tab(tab_id, key)
    }

    fn feed_page_hint_for_tab(&self, tab_id: TabId, key: HintKey) -> CommandResult {
        let (outcome, session) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(page) = state.vim.pages.get_mut(&tab_id) else {
                return Ok(());
            };
            let Some(session) = page.page_hints.as_mut() else {
                return Ok(());
            };
            let outcome = session.feed(key);
            (outcome, page.page_hints.clone())
        };
        let Some(session) = session else {
            return Ok(());
        };
        match outcome {
            HintOutcome::Pick { id, new_tab } => {
                if let Some(item) = session.items.iter().find(|i| i.id == id).cloned()
                    && item.challenge
                    && session.supports_challenge()
                {
                    return self.pick_in_challenge(tab_id, item, new_tab, false);
                }
                self.set_page_hints_for_tab(tab_id, None);
                self.ask_page_mode_changing(
                    session.tab_id,
                    AutomationRequest::JumpPick {
                        id,
                        new_tab,
                        extend: session.extend,
                    },
                )
            }
            HintOutcome::PressPurpose { purposes } => self.press_purpose_in_challenge(tab_id, purposes),
            HintOutcome::OpenCursor { drag } => self.open_cursor_from_labels(tab_id, drag),
            HintOutcome::CursorAct { outcome } => self.apply_cursor_outcome(tab_id, outcome),
            HintOutcome::Ask { pattern } => {
                self.sync_published_vim_state();
                self.ask_page_for_tab(
                    session.tab_id,
                    AutomationRequest::JumpScan {
                        jump: session.kind,
                        pattern,
                        top_only: false,
                        include_fields: false,
                        watch_mutations: false,
                    },
                )
            }
            HintOutcome::Changed => {
                self.sync_published_vim_state();
                Ok(())
            }
            HintOutcome::Finished | HintOutcome::FinishedPassingKey => {
                self.set_page_hints_for_tab(tab_id, None);
                Ok(())
            }
            HintOutcome::Ignored => Ok(()),
        }
    }

    fn pick_in_challenge(
        &self,
        tab_id: TabId,
        item: PageHintItem,
        new_tab: bool,
        is_purpose_press: bool,
    ) -> CommandResult {
        let mut open_pad_steps: Option<Vec<PointerStep>> = None;
        let mut need_transition_scan = false;
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(page) = state.vim.pages.get_mut(&tab_id) else {
                return Ok(());
            };
            let Some(session) = page.page_hints.as_mut() else {
                return Ok(());
            };
            let first = !session.staying;
            if first {
                for each in &session.items {
                    if each.challenge
                        && let Some(lbl) = each.label.as_ref()
                    {
                        session.kept.insert(each.id.clone(), lbl.clone());
                    }
                }
                session.staying = true;
                session.kind = JumpKind::Challenge;
                session.switching = true;
            }
            session.typed.clear();

            let board = if is_purpose_press {
                None
            } else {
                session.board_of(&item)
            };
            let hold = !is_purpose_press
                && (item.drag || new_tab || board.as_ref().is_some_and(|(places, pieces)| places.len() > *pieces));
            if hold || (item.picture && !is_purpose_press) {
                session.switching = false;
                if hold {
                    let slider = item.slider && board.is_none();
                    let shown_pictures: Vec<ViewportRect> = session
                        .items
                        .iter()
                        .filter(|each| each.picture && each.challenge && each.id != item.id)
                        .map(|each| each.rect)
                        .collect();
                    let areas = if !slider && board.is_none() && !shown_pictures.is_empty() {
                        let mut a = shown_pictures;
                        a.push(item.rect);
                        a
                    } else if slider {
                        let around_handle = session.captcha_areas.iter().copied().find(|a| {
                            let c = middle(item.rect);
                            c.0 >= a.x && c.0 <= a.x + a.width && c.1 >= a.y && c.1 <= a.y + a.height
                        });
                        if around_handle.is_some_and(|a| a.y < item.rect.y - 40.0) {
                            session.captcha_areas.clone()
                        } else {
                            let along = item.track.unwrap_or(item.rect);
                            let puzzle_above = shown_pictures.into_iter().find(|p| {
                                p.y + p.height <= item.rect.y + 24.0
                                    && p.x < along.x + along.width
                                    && p.x + p.width > along.x
                            });
                            let mut left = item.rect.x.min(along.x);
                            let mut right = (item.rect.x + item.rect.width).max(along.x + along.width);
                            let mut top = item.rect.y.min(along.y);
                            let mut bottom = (item.rect.y + item.rect.height).max(along.y + along.height);
                            if let Some(p) = puzzle_above {
                                left = left.min(p.x);
                                right = right.max(p.x + p.width);
                                top = top.min(p.y);
                                bottom = bottom.max(p.y + p.height);
                            }
                            vec![ViewportRect {
                                x: left,
                                y: top,
                                width: right - left,
                                height: bottom - top,
                            }]
                        }
                    } else if !session.captcha_areas.is_empty() {
                        session.captcha_areas.clone()
                    } else {
                        vec![item.rect]
                    };
                    let piece = if item.slider { session.piece_of(&item) } else { None };
                    open_pad_steps = Some(session.open_cursor_pad(PointerPadOptions {
                        areas,
                        viewport: session.viewport,
                        drag: true,
                        grab: Some(middle(item.rect)),
                        grab_rect: Some(item.rect),
                        slider,
                        track: item.track,
                        piece,
                        lattice: board.map(|(places, _)| places),
                        banner: String::new(),
                        can_verify: true,
                        watches_captcha: true,
                        standalone: false,
                        click_order: false,
                    }));
                } else {
                    open_pad_steps = Some(session.open_cursor_pad(PointerPadOptions {
                        areas: vec![item.rect],
                        viewport: session.viewport,
                        drag: false,
                        grab: None,
                        grab_rect: None,
                        slider: false,
                        track: None,
                        piece: None,
                        lattice: None,
                        banner: String::new(),
                        can_verify: true,
                        watches_captcha: true,
                        standalone: false,
                        click_order: true,
                    }));
                }
            } else {
                session.schedule_poll(CHALLENGE_SETTLE_MS);
                if first && item.kind != "field" {
                    need_transition_scan = true;
                    session.awaiting = Some(String::new());
                }
            }
        }
        self.sync_published_vim_state();
        if let Some(steps) = open_pad_steps {
            self.dispatch_pointer_for_tab(tab_id, &steps);
            return Ok(());
        }
        self.ask_page_mode_changing(
            tab_id,
            AutomationRequest::JumpPick {
                id: item.id,
                new_tab: false,
                extend: false,
            },
        )?;
        if need_transition_scan {
            self.ask_page_for_tab(
                tab_id,
                AutomationRequest::JumpScan {
                    jump: JumpKind::Challenge,
                    pattern: String::new(),
                    top_only: false,
                    include_fields: false,
                    watch_mutations: false,
                },
            )?;
        }
        Ok(())
    }

    fn press_purpose_in_challenge(&self, tab_id: TabId, purposes: Vec<String>) -> CommandResult {
        let found = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let session = state.vim.pages.get(&tab_id).and_then(|p| p.page_hints.as_ref());
            session.and_then(|s| {
                purposes.iter().find_map(|p| {
                    s.items
                        .iter()
                        .find(|each| each.purpose.as_deref() == Some(p.as_str()))
                        .cloned()
                })
            })
        };
        if let Some(target) = found {
            return self.pick_in_challenge(tab_id, target, false, true);
        }
        // Re-read once before flashing `"no <purpose> here"` (`ZendiaJumpContent:1106-1117`, `CAP-04`).
        let scan_kind = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(session) = state.vim.pages.get_mut(&tab_id).and_then(|p| p.page_hints.as_mut()) else {
                return Ok(());
            };
            session.pending_purpose = Some(purposes);
            session.awaiting = Some(String::new());
            session.kind
        };
        self.sync_published_vim_state();
        self.ask_page_for_tab(
            tab_id,
            AutomationRequest::JumpScan {
                jump: scan_kind,
                pattern: String::new(),
                top_only: false,
                include_fields: false,
                watch_mutations: false,
            },
        )
    }

    fn complete_purpose_after_reread(&self, tab_id: TabId, purposes: &[String]) -> CommandResult {
        let found = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(session) = state.vim.pages.get_mut(&tab_id).and_then(|p| p.page_hints.as_mut()) else {
                return Ok(());
            };
            let item = purposes.iter().find_map(|p| {
                session
                    .items
                    .iter()
                    .find(|each| each.purpose.as_deref() == Some(p.as_str()))
                    .cloned()
            });
            if item.is_none() {
                session.flash_staying_notice(PageHintSession::missing_purpose_notice(purposes));
            }
            item
        };
        self.sync_published_vim_state();
        if let Some(target) = found {
            return self.pick_in_challenge(tab_id, target, false, true);
        }
        Ok(())
    }

    fn open_cursor_from_labels(&self, tab_id: TabId, drag: bool) -> CommandResult {
        let (steps, need_areas_scan) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(session) = state.vim.pages.get_mut(&tab_id).and_then(|p| p.page_hints.as_mut()) else {
                return Ok(());
            };
            if session.captcha_areas.is_empty() && !session.staying {
                session.pending_cursor_drag = Some(drag);
                (Vec::new(), true)
            } else {
                let areas = session.captcha_areas.clone();
                let staying = session.staying;
                let found = !areas.is_empty();
                let steps = session.open_cursor_pad(PointerPadOptions {
                    areas,
                    viewport: session.viewport,
                    drag,
                    grab: None,
                    grab_rect: None,
                    slider: false,
                    track: None,
                    piece: None,
                    lattice: None,
                    banner: if found {
                        String::new()
                    } else {
                        NO_CAPTCHA_CURSOR_BANNER.to_string()
                    },
                    can_verify: staying || found,
                    watches_captcha: staying || found,
                    standalone: false,
                    click_order: false,
                });
                (steps, false)
            }
        };
        self.sync_published_vim_state();
        if need_areas_scan {
            return self.ask_page_for_tab(tab_id, AutomationRequest::CaptchaAreas);
        }
        let _ = self.ask_page_for_tab(tab_id, AutomationRequest::RefocusTop);
        self.dispatch_pointer_for_tab(tab_id, &steps);
        Ok(())
    }

    fn apply_cursor_outcome(&self, tab_id: TabId, outcome: PadStepOutcome) -> CommandResult {
        self.dispatch_pointer_for_tab(tab_id, &outcome.steps);
        let mut run_purpose: Option<Vec<String>> = None;
        let mut run_standalone_verify = false;
        let mut need_resume_refresh = false;
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(page) = state.vim.pages.get_mut(&tab_id) else {
                return Ok(());
            };
            let Some(session) = page.page_hints.as_mut() else {
                return Ok(());
            };
            if outcome.flash_click {
                session.flash_cursor_click();
            }
            if outcome.check_soon && !outcome.finished {
                session.schedule_poll(WATCH_AFTER_CLICK_MS);
            }
            if outcome.finished {
                let standalone = session.cursor_pad.as_ref().is_some_and(|p| p.standalone);
                session.cursor_pad = None;
                if let Some(purposes) = outcome.trigger_purpose {
                    run_purpose = Some(purposes.into_iter().map(str::to_string).collect());
                } else if outcome.trigger_verify {
                    if standalone {
                        run_standalone_verify = true;
                        page.page_hints = None;
                    } else {
                        run_purpose = Some(vec!["verify".to_string()]);
                    }
                } else if standalone {
                    page.page_hints = None;
                } else if session.staying {
                    session.schedule_poll(CHALLENGE_SETTLE_MS);
                    need_resume_refresh = true;
                } else {
                    need_resume_refresh = true;
                }
            }
        }
        self.sync_published_vim_state();
        if run_standalone_verify {
            return self.ask_page_for_tab(
                tab_id,
                AutomationRequest::CaptchaAction {
                    action: CaptchaOp::Verify,
                },
            );
        }
        if let Some(purposes) = run_purpose {
            return self.press_purpose_in_challenge(tab_id, purposes);
        }
        if need_resume_refresh {
            let _ = self.refresh_page_hints_for_tab(tab_id);
        }
        Ok(())
    }

    fn on_visual_state(&self, tab_id: TabId, update: VisualStateUpdate) {
        let VisualStateUpdate {
            active,
            yanked,
            copied_by_page,
            caret,
            land,
            flash,
        } = update;
        if !self.model().vim_mode_enabled {
            return;
        }
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let page = state.vim.pages.entry(tab_id).or_default();
            page.visual_caret = if active { caret } else { None };
            if active && page.mode != InputMode::Visual {
                page.mode = InputMode::Visual;
                page.explicit_insert = false;
            } else if !active && page.mode == InputMode::Visual {
                page.mode = InputMode::Normal;
            }
        }
        self.sync_published_vim_state();
        if let Some(text) = yanked
            && !text.is_empty()
        {
            self.record_yank_from_page(&text, tab_id, None);
            if !copied_by_page {
                self.bus.publish(Event::ClipboardCopyRequested { text });
            }
        }
        if !flash.is_empty() {
            self.flash_page(tab_id, flash, PageFlashKind::Yank);
        }
        if land {
            let _ = self.start_jump_for_tab(tab_id, JumpKind::Land, false);
        }
    }

    fn flash_page(&self, tab_id: TabId, rects: Vec<ViewportRect>, kind: PageFlashKind) {
        let flash = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let serial = state.model.page_flash.as_ref().map_or(1, |flash| flash.serial + 1);
            let flash = PageFlash {
                serial,
                tab_id,
                rects,
                kind,
            };
            state.model.page_flash = Some(flash.clone());
            flash
        };
        self.bus.publish(Event::PageFlashed { flash });
    }

    pub(super) fn set_page_hints_for_tab(&self, tab_id: TabId, hints: Option<PageHintSession>) {
        let mut release_steps = Vec::new();
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let page = state.vim.pages.entry(tab_id).or_default();
            if hints.is_none()
                && let Some(mut prev) = page.page_hints.take()
                && let Some(mut pad) = prev.cursor_pad.take()
            {
                release_steps = pad.release_if_held();
            }
            page.page_hints = hints;
        }
        self.dispatch_pointer_for_tab(tab_id, &release_steps);
        self.sync_published_vim_state();
    }

    pub(super) fn set_visual_caret_for_tab(&self, tab_id: TabId, caret: Option<ViewportRect>) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.vim.pages.entry(tab_id).or_default().visual_caret = caret;
        }
        self.sync_published_vim_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::captcha::{PadLayoutKind, STAYING_PROMPT};
    use crate::tab_source::{PointerEventKind, TabSource};
    use crate::vim_keys::VimCommand;
    use std::sync::{Arc, Mutex};

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
            challenge: false,
            drag: false,
            slider: false,
            track: None,
            picture: false,
            purpose: None,
        }
    }

    fn challenge_target(id: &str, x: f32, y: f32, purpose: Option<&str>) -> PageTarget {
        PageTarget {
            challenge: true,
            y,
            purpose: purpose.map(str::to_string),
            ..target(id, x)
        }
    }

    fn vim_core() -> (StubBackend, TabId) {
        let core = StubBackend::new();
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        let tab_id = core.model().active_tab.unwrap();
        (core, tab_id)
    }

    fn send_hint_key(core: &StubBackend, key: &str) {
        core.invoke(Command::from_name_and_args("page_hint.input", &[("key".to_string(), key.to_string())]).unwrap())
            .unwrap();
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
                captcha_tokens: 0,
                captcha_areas: Vec::new(),
                viewport: None,
            },
        );
        let hints = core.model().page_hints.expect("labels are up");
        assert_eq!(hints.items.len(), 2);
        let key = hints.items[1].label.clone().unwrap();
        send_hint_key(&core, &key);
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
                captcha_tokens: 0,
                captcha_areas: Vec::new(),
                viewport: None,
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
                copied_by_page: false,
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
                copied_by_page: true,
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
    fn capital_press_forwards_ctrl_modifier_and_inflight_scroll_queues_refresh() {
        struct MockClickSource {
            clicks: Mutex<Vec<(TabId, f64, f64, bool)>>,
            scans: Mutex<usize>,
        }
        impl TabSource for MockClickSource {
            fn new_tab(&self, _: Option<&str>, _: Option<TabId>, _: bool) -> CommandResult {
                Ok(())
            }
            fn close_tab(&self, _: TabId, _: bool) -> CommandResult {
                Ok(())
            }
            fn activate_tab(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn navigate_back(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn navigate_forward(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn reload(&self, _: TabId, _: bool) -> CommandResult {
                Ok(())
            }
            fn stop_loading(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn load_url(&self, _: TabId, _: &str) -> CommandResult {
                Ok(())
            }
            fn click_at_with_modifiers(&self, tab_id: TabId, x: f64, y: f64, ctrl: bool) -> CommandResult {
                self.clicks.lock().unwrap().push((tab_id, x, y, ctrl));
                Ok(())
            }
            fn run_automation(&self, _: TabId, req: AutomationRequest) -> CommandResult {
                if matches!(req, AutomationRequest::JumpScan { .. }) {
                    *self.scans.lock().unwrap() += 1;
                }
                Ok(())
            }
        }

        let (core, tab_id) = vim_core();
        let src = Arc::new(MockClickSource {
            clicks: Mutex::new(Vec::new()),
            scans: Mutex::new(0),
        });
        core.attach_tab_source(Arc::clone(&src) as Arc<dyn TabSource>);

        core.feed_automation_response(
            tab_id,
            AutomationResponse::Press {
                x: 42.0,
                y: 84.0,
                ctrl: true,
            },
        );
        assert_eq!(*src.clicks.lock().unwrap(), vec![(tab_id, 42.0, 84.0, true)]);

        core.invoke(Command::from_name_and_args("jump.hints", &[]).unwrap())
            .unwrap();
        assert_eq!(*src.scans.lock().unwrap(), 1);
        core.feed_automation_response(tab_id, AutomationResponse::PageMoved { scrolled: true });
        assert!(core.model().page_hints.as_ref().unwrap().refresh_queued);
        core.feed_automation_response(
            tab_id,
            AutomationResponse::Targets {
                jump: JumpKind::Hints,
                pattern: String::new(),
                targets: vec![target("one", 10.0)],
                captcha_tokens: 0,
                captcha_areas: Vec::new(),
                viewport: None,
            },
        );
        assert_eq!(*src.scans.lock().unwrap(), 2);
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
                captcha_tokens: 0,
                captcha_areas: Vec::new(),
                viewport: None,
            },
        );
        assert_eq!(core.model().page_hints.map(|hints| hints.kind), Some(JumpKind::Inputs));
    }

    #[test]
    fn captcha_tiles_staying_jump_purpose_notices_and_solved_exit() {
        // Covers `CAP-03`, `CAP-04`, `CAP-08`, `CAP-09`, and the fork's Tiles test scenario (`VimParity.md:577-580`).
        struct MockCaptchaSource {
            picks: Mutex<Vec<String>>,
            pointer_steps: Mutex<Vec<PointerStep>>,
        }
        impl TabSource for MockCaptchaSource {
            fn new_tab(&self, _: Option<&str>, _: Option<TabId>, _: bool) -> CommandResult {
                Ok(())
            }
            fn close_tab(&self, _: TabId, _: bool) -> CommandResult {
                Ok(())
            }
            fn activate_tab(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn navigate_back(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn navigate_forward(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn reload(&self, _: TabId, _: bool) -> CommandResult {
                Ok(())
            }
            fn stop_loading(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn load_url(&self, _: TabId, _: &str) -> CommandResult {
                Ok(())
            }
            fn dispatch_pointer_steps(&self, _: TabId, steps: &[PointerStep]) -> CommandResult {
                self.pointer_steps.lock().unwrap().extend_from_slice(steps);
                Ok(())
            }
            fn run_automation(&self, _: TabId, req: AutomationRequest) -> CommandResult {
                if let AutomationRequest::JumpPick { id, .. } = req {
                    self.picks.lock().unwrap().push(id);
                }
                Ok(())
            }
        }

        let (core, tab_id) = vim_core();
        let src = Arc::new(MockCaptchaSource {
            picks: Mutex::new(Vec::new()),
            pointer_steps: Mutex::new(Vec::new()),
        });
        core.attach_tab_source(Arc::clone(&src) as Arc<dyn TabSource>);

        // `gc` labels 3 tiles and verify (`a s d f`) and shows `STAYING_PROMPT`.
        core.invoke(Command::from_name_and_args("jump.captcha", &[]).unwrap())
            .unwrap();
        let tiles = vec![
            challenge_target("t1", 10.0, 20.0, None),
            challenge_target("t2", 60.0, 20.0, None),
            challenge_target("t3", 110.0, 20.0, None),
            challenge_target("ver", 60.0, 90.0, Some("verify")),
        ];
        let area = ViewportRect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 150.0,
        };
        core.feed_automation_response(
            tab_id,
            AutomationResponse::Targets {
                jump: JumpKind::Captcha,
                pattern: String::new(),
                targets: tiles.clone(),
                captcha_tokens: 0,
                captcha_areas: vec![area],
                viewport: Some((800.0, 600.0)),
            },
        );
        let hints = core.model().page_hints.unwrap();
        let labels: Vec<&str> = hints.items.iter().filter_map(|i| i.label.as_deref()).collect();
        assert_eq!(labels, vec!["a", "s", "d", "f"]);
        assert_eq!(hints.prompt_text(), STAYING_PROMPT);

        // `a`, `s` press two tiles and keep the labels open!
        send_hint_key(&core, "a");
        send_hint_key(&core, "s");
        assert_eq!(*src.picks.lock().unwrap(), vec!["t1", "t2"]);
        assert!(core.model().page_hints.is_some());

        // `<Tab>` re-reads then flashes `"no answer field here"`; `<C-p>` flashes `"no play button here"`.
        send_hint_key(&core, "<Tab>");
        core.feed_automation_response(
            tab_id,
            AutomationResponse::Targets {
                jump: JumpKind::Captcha,
                pattern: String::new(),
                targets: tiles.clone(),
                captcha_tokens: 0,
                captcha_areas: vec![area],
                viewport: Some((800.0, 600.0)),
            },
        );
        assert_eq!(core.model().page_hints.unwrap().prompt_text(), "no answer field here");

        send_hint_key(&core, "<C-p>");
        core.feed_automation_response(
            tab_id,
            AutomationResponse::Targets {
                jump: JumpKind::Captcha,
                pattern: String::new(),
                targets: tiles.clone(),
                captcha_tokens: 0,
                captcha_areas: vec![area],
                viewport: Some((800.0, 600.0)),
            },
        );
        assert_eq!(core.model().page_hints.unwrap().prompt_text(), "no play button here");

        // `<CR>` presses verify (`ver`), and when `captcha_tokens` increments to 1, the staying jump ends!
        send_hint_key(&core, "<CR>");
        assert_eq!(src.picks.lock().unwrap().last().map(String::as_str), Some("ver"));
        core.feed_automation_response(
            tab_id,
            AutomationResponse::Targets {
                jump: JumpKind::Captcha,
                pattern: String::new(),
                targets: tiles,
                captcha_tokens: 1,
                captcha_areas: vec![area],
                viewport: Some((800.0, 600.0)),
            },
        );
        assert!(core.model().page_hints.is_none());
    }

    #[test]
    fn answer_field_yields_to_insert_and_esc_returns_to_staying_labels_and_board_drops_piece() {
        // Covers `CAP-06` (Board `gc, a, s`), `CAP-05` (`.` grid click), and `CAP-08` (`<Esc>` from answer field back to labels).
        struct MockSteps {
            steps: Mutex<Vec<PointerStep>>,
        }
        impl TabSource for MockSteps {
            fn new_tab(&self, _: Option<&str>, _: Option<TabId>, _: bool) -> CommandResult {
                Ok(())
            }
            fn close_tab(&self, _: TabId, _: bool) -> CommandResult {
                Ok(())
            }
            fn activate_tab(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn navigate_back(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn navigate_forward(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn reload(&self, _: TabId, _: bool) -> CommandResult {
                Ok(())
            }
            fn stop_loading(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn load_url(&self, _: TabId, _: &str) -> CommandResult {
                Ok(())
            }
            fn dispatch_pointer_steps(&self, _: TabId, steps: &[PointerStep]) -> CommandResult {
                self.steps.lock().unwrap().extend_from_slice(steps);
                Ok(())
            }
        }

        let (core, tab_id) = vim_core();
        let src = Arc::new(MockSteps {
            steps: Mutex::new(Vec::new()),
        });
        core.attach_tab_source(Arc::clone(&src) as Arc<dyn TabSource>);

        // 1. Answer field `<Esc>` returns to staying labels (`CAP-08`).
        core.invoke(Command::from_name_and_args("jump.captcha", &[]).unwrap())
            .unwrap();
        let mut field = challenge_target("ans", 20.0, 40.0, Some("answer"));
        field.kind = "field".to_string();
        core.feed_automation_response(
            tab_id,
            AutomationResponse::Targets {
                jump: JumpKind::Captcha,
                pattern: String::new(),
                targets: vec![field, challenge_target("ver", 20.0, 90.0, Some("verify"))],
                captcha_tokens: 0,
                captcha_areas: vec![ViewportRect {
                    x: 0.0,
                    y: 0.0,
                    width: 200.0,
                    height: 150.0,
                }],
                viewport: Some((800.0, 600.0)),
            },
        );
        send_hint_key(&core, "<Tab>");
        core.feed_automation_response(
            tab_id,
            AutomationResponse::Activated {
                mode: Some("i".to_string()),
                url: None,
                message: None,
            },
        );
        assert_eq!(core.model().input_mode, InputMode::Insert);
        assert!(core.model().page_editing);
        assert!(
            core.model().page_hints.as_ref().is_some_and(|h| h.staying),
            "staying jump stays alive while typing answer"
        );

        // Pressing `<Esc>` in Insert mode (`VimCommand::ModeNormal`) blurs the field and keeps staying labels up!
        core.invoke(Command::Vim(VimCommand::ModeNormal)).unwrap();
        assert_eq!(core.model().input_mode, InputMode::Normal);
        assert!(!core.model().page_editing);
        assert!(
            core.model().page_hints.as_ref().is_some_and(|h| h.staying),
            "labels take the keys back on <Esc>"
        );
        send_hint_key(&core, "<Esc>");
        assert!(core.model().page_hints.is_none());

        // 2. Board (`gc, a, s`): 3 pieces on a 3x3 board -> `a` holds piece 0 and labels 9 places -> `s` drops on place 1 and returns to labels!
        let piece = |id: &str, col: usize, row: usize| PageTarget {
            id: id.to_string(),
            x: 100.0 + col as f32 * 50.0,
            y: 100.0 + row as f32 * 50.0,
            width: 40.0,
            height: 40.0,
            kind: "control".to_string(),
            nexts: Vec::new(),
            distance: 0.0,
            href: None,
            challenge: true,
            drag: false,
            slider: false,
            track: None,
            picture: false,
            purpose: None,
        };
        core.invoke(Command::from_name_and_args("jump.captcha", &[]).unwrap())
            .unwrap();
        core.feed_automation_response(
            tab_id,
            AutomationResponse::Targets {
                jump: JumpKind::Captcha,
                pattern: String::new(),
                targets: vec![piece("p0", 0, 0), piece("p1", 1, 1), piece("p2", 2, 2)],
                captcha_tokens: 0,
                captcha_areas: vec![ViewportRect {
                    x: 90.0,
                    y: 90.0,
                    width: 170.0,
                    height: 170.0,
                }],
                viewport: Some((800.0, 600.0)),
            },
        );
        send_hint_key(&core, "a");
        let pad = core
            .model()
            .page_hints
            .unwrap()
            .cursor_pad
            .expect("cursor opened on board");
        assert_eq!(pad.shown_layout().unwrap().kind, PadLayoutKind::Board);
        assert_eq!(pad.shown_layout().unwrap().rects.len(), 9);
        send_hint_key(&core, "s");
        assert!(
            core.model().page_hints.unwrap().cursor_pad.is_none(),
            "dropping on board place returns to labels"
        );
        assert_eq!(src.steps.lock().unwrap().last().unwrap().kind, PointerEventKind::Up);
    }

    #[test]
    fn live_captcha_watch_updates_model_and_on_demand_cursor_fetches_areas() {
        use zendia_automation::{CaptchaWatchDetection, CaptchaWatchStateReading};

        struct MockWatchSource {
            requests: Mutex<Vec<AutomationRequest>>,
        }
        impl TabSource for MockWatchSource {
            fn new_tab(&self, _: Option<&str>, _: Option<TabId>, _: bool) -> CommandResult {
                Ok(())
            }
            fn close_tab(&self, _: TabId, _: bool) -> CommandResult {
                Ok(())
            }
            fn activate_tab(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn navigate_back(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn navigate_forward(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn reload(&self, _: TabId, _: bool) -> CommandResult {
                Ok(())
            }
            fn stop_loading(&self, _: TabId) -> CommandResult {
                Ok(())
            }
            fn load_url(&self, _: TabId, _: &str) -> CommandResult {
                Ok(())
            }
            fn run_automation(&self, _: TabId, req: AutomationRequest) -> CommandResult {
                self.requests.lock().unwrap().push(req);
                Ok(())
            }
        }

        let (core, tab_id) = vim_core();
        let src = Arc::new(MockWatchSource {
            requests: Mutex::new(Vec::new()),
        });
        core.attach_tab_source(Arc::clone(&src) as Arc<dyn TabSource>);

        // 1. Unsolicited `CaptchaWatchReport` populates `model.tab_captcha_findings` live.
        core.feed_automation_response(
            tab_id,
            AutomationResponse::CaptchaWatchReport {
                frame_id: 0,
                top: true,
                url: "https://2captcha.com/demo/recaptcha-v2".to_string(),
                detections: vec![CaptchaWatchDetection {
                    signal: "frame".to_string(),
                    name: Some("reCAPTCHA".to_string()),
                    kind: Some("checkbox".to_string()),
                    challenge: false,
                    hidden: false,
                    detail: Some("https://www.google.com/recaptcha/enterprise/anchor".to_string()),
                }],
                state: CaptchaWatchStateReading::default(),
            },
        );
        assert_eq!(
            core.model().tab_captcha_findings.get(&tab_id).map(String::as_str),
            Some("reCAPTCHA (checkbox)")
        );

        // Navigating away clears live findings.
        core.on_tab_navigated(tab_id, Some("https://example.com/"));
        assert!(!core.model().tab_captcha_findings.contains_key(&tab_id));

        // 2. Pressing `.` from `s` (`JumpKind::Hints`) fetches `CaptchaAreas` on demand and scopes `PointerPad`.
        core.invoke(Command::from_name_and_args("jump.hints", &[]).unwrap())
            .unwrap();
        core.feed_automation_response(
            tab_id,
            AutomationResponse::Targets {
                jump: JumpKind::Hints,
                pattern: String::new(),
                targets: vec![target("btn", 40.0)],
                captcha_tokens: 0,
                captcha_areas: Vec::new(),
                viewport: Some((1000.0, 700.0)),
            },
        );
        send_hint_key(&core, ".");
        assert!(
            src.requests
                .lock()
                .unwrap()
                .iter()
                .any(|r| matches!(r, AutomationRequest::CaptchaAreas)),
            "`.` from `s` should request CaptchaAreas on demand"
        );
        let box_rect = ViewportRect {
            x: 200.0,
            y: 120.0,
            width: 300.0,
            height: 200.0,
        };
        core.feed_automation_response(
            tab_id,
            AutomationResponse::CaptchaAreas {
                areas: vec![box_rect],
                captcha_tokens: 0,
                viewport: Some((1000.0, 700.0)),
            },
        );
        let pad = core
            .model()
            .page_hints
            .unwrap()
            .cursor_pad
            .expect("PointerPad opens after CaptchaAreas arrives");
        assert_eq!(pad.areas, vec![box_rect]);

        // 3. Picking a `picture: true` challenge target opens `PointerPad` in click-order mode (`FineGrid`).
        send_hint_key(&core, "<Esc>");
        send_hint_key(&core, "<Esc>");
        core.invoke(Command::from_name_and_args("jump.captcha", &[]).unwrap())
            .unwrap();
        let mut pic = challenge_target("pic1", 100.0, 100.0, None);
        pic.width = 300.0;
        pic.height = 200.0;
        pic.picture = true;
        core.feed_automation_response(
            tab_id,
            AutomationResponse::Targets {
                jump: JumpKind::Captcha,
                pattern: String::new(),
                targets: vec![pic],
                captcha_tokens: 0,
                captcha_areas: vec![box_rect],
                viewport: Some((1000.0, 700.0)),
            },
        );
        send_hint_key(&core, "a");
        let pad = core
            .model()
            .page_hints
            .unwrap()
            .cursor_pad
            .expect("picking CAPTCHA picture opens PointerPad");
        assert!(
            pad.click_on_jump,
            "click-order mode should activate on CAPTCHA pictures"
        );
        assert_eq!(pad.shown_layout().unwrap().kind, PadLayoutKind::FineGrid);
    }
}
