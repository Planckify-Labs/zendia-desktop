/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Full-window seven-page native onboarding UI, directional chapter transitions, keyboard navigation, and overlay host.

use crate::ui::agent::agent_logo_element;
use crate::ui::core_view::{CoreView, use_core};
use crate::ui::kit::{try_use_text_focus, use_interactive_feedback};
use crate::ui::onboarding::audio;
use crate::ui::onboarding::browser_scene::{draw_dashed_rrect_stroke, paint_ambient_field};
use crate::ui::onboarding::logos::{brand_logo_element, sound_icon};
use crate::ui::onboarding::mascot::OnboardingMascot;
use crate::ui::onboarding::scenes::{
    AgentScene, AppsScene, GoScene, MoveInScene, ShieldScene, StageShell, StartEmblemScene, VimScene,
};
use crate::ui::onboarding::theme::{MODE_ORDER, OnboardingPalette, clamp01, ease_out, lerp, mix_rgba, sk_alpha};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, Rgba, motion, use_theme};
use freya::animation::*;
use freya::engine::prelude as sk;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::InputMode;
use zendia_browser_core::onboarding::{
    ESSENTIAL_APP_PRESETS, EssentialAppPreset, MoveInFocus, OnboardingPage, OnboardingState, onboarding_search_engines,
};

fn is_activate_key(key: &Key) -> bool {
    matches!(key, Key::Named(NamedKey::Enter)) || matches!(key, Key::Character(c) if c == " ")
}

fn stagger_ease(elapsed_ms: f32, delay_ms: f32, dur_ms: f32) -> f32 {
    if dur_ms <= 0.0 {
        return 1.0;
    }
    ease_out(clamp01((elapsed_ms - delay_ms) / dur_ms))
}

fn onboarding_focus_ring(focused: bool, accent: Rgba) -> Border {
    if focused {
        Border::new()
            .width(2.5)
            .alignment(BorderAlignment::Inner)
            .fill(accent.to_color())
    } else {
        Border::new()
    }
}

/// View-local state for the window's onboarding overlay (animations, preview pause, audio mute, Move in fieldset focus).
#[derive(Clone, Copy, PartialEq)]
pub struct OnboardingViewState {
    pub open: State<bool>,
    pub muted: State<bool>,
    pub paused: State<bool>,
    pub finishing: State<bool>,
    pub start_pose: State<InputMode>,
    pub move_focus: State<MoveInFocus>,
    pub nudge_seq: State<u64>,
    pub transition_seq: State<u64>,
    pub primary_focus_seq: State<u64>,
    pub non_primary_focused: State<Option<AccessibilityId>>,
}

impl OnboardingViewState {
    pub fn is_open(&self) -> bool {
        *self.open.peek()
    }

    pub fn is_finishing(&self) -> bool {
        *self.finishing.peek()
    }

    pub fn begin_finish(&self) -> bool {
        let mut finishing = self.finishing;
        if *finishing.peek() {
            return false;
        }
        finishing.set(true);
        true
    }

    pub fn open(&self) {
        let mut open = self.open;
        let mut paused = self.paused;
        let mut finishing = self.finishing;
        let mut non_primary = self.non_primary_focused;
        open.set_if_modified(true);
        paused.set_if_modified(false);
        finishing.set_if_modified(false);
        non_primary.set_if_modified(None);
        self.bump_primary_focus();
        audio::warm_up();
    }

    pub fn close(&self) {
        let mut open = self.open;
        open.set_if_modified(false);
    }

    pub fn bump_nudge(&self) {
        let mut nudge = self.nudge_seq;
        let next = nudge.peek().wrapping_add(1);
        nudge.set(next);
    }

    pub fn bump_transition(&self) {
        let mut seq = self.transition_seq;
        let next = seq.peek().wrapping_add(1);
        seq.set(next);
    }

    pub fn bump_primary_focus(&self) {
        let mut seq = self.primary_focus_seq;
        let next = seq.peek().wrapping_add(1);
        seq.set(next);
    }

    pub fn note_non_primary_focus(&self, id: AccessibilityId) {
        let mut slot = self.non_primary_focused;
        slot.set_if_modified(Some(id));
    }

    pub fn has_non_primary_focus(&self) -> bool {
        self.non_primary_focused.peek().is_some_and(|id| id.is_focused())
    }

    pub fn set_muted(&self, muted: bool) {
        let mut m = self.muted;
        m.set_if_modified(muted);
        audio::set_muted(muted);
    }

    pub fn reset_for_gallery(&self) {
        let mut open = self.open;
        let mut paused = self.paused;
        let mut finishing = self.finishing;
        let mut start_pose = self.start_pose;
        let mut move_focus = self.move_focus;
        let mut non_primary = self.non_primary_focused;
        open.set_if_modified(false);
        paused.set_if_modified(false);
        finishing.set_if_modified(false);
        start_pose.set_if_modified(InputMode::Normal);
        move_focus.set_if_modified(MoveInFocus::Import);
        non_primary.set_if_modified(None);
    }
}

/// Installs [`OnboardingViewState`] into Freya context for the current window.
///
/// Pass `start_muted = true` in the gallery so gallery browsing stays silent until the user clicks the sound toggle.
pub fn use_provide_onboarding_state(start_muted: bool) -> OnboardingViewState {
    let open = use_state(|| false);
    let muted = use_state(|| {
        let initial = start_muted || audio::is_muted();
        audio::set_muted(initial);
        initial
    });
    let paused = use_state(|| false);
    let finishing = use_state(|| false);
    let start_pose = use_state(|| InputMode::Normal);
    let move_focus = use_state(|| MoveInFocus::Import);
    let nudge_seq = use_state(|| 0u64);
    let transition_seq = use_state(|| 0u64);
    let primary_focus_seq = use_state(|| 0u64);
    let non_primary_focused = use_state(|| None::<AccessibilityId>);

    let state = OnboardingViewState {
        open,
        muted,
        paused,
        finishing,
        start_pose,
        move_focus,
        nudge_seq,
        transition_seq,
        primary_focus_seq,
        non_primary_focused,
    };
    use_provide_context(|| state);
    state
}

/// Consumes [`OnboardingViewState`] from Freya context.
pub fn use_onboarding_state() -> OnboardingViewState {
    use_consume::<OnboardingViewState>()
}

/// Consumes [`OnboardingViewState`] if installed in this window.
pub fn try_use_onboarding_state() -> Option<OnboardingViewState> {
    try_consume_context::<OnboardingViewState>()
}

/// Handles keyboard navigation while onboarding is visible (`Enter`, `Backspace`, arrow keys).
///
/// Returns `true` when the key was consumed by onboarding so browser shortcuts behind the overlay do not fire.
pub fn handle_onboarding_key_down(
    e: &Event<KeyboardEventData>,
    core: &CoreView,
    view_state: &OnboardingViewState,
) -> bool {
    handle_onboarding_key_inner(e, core, view_state, true)
}

fn handle_onboarding_key_inner(
    e: &Event<KeyboardEventData>,
    core: &CoreView,
    view_state: &OnboardingViewState,
    require_overlay_open: bool,
) -> bool {
    if require_overlay_open && !view_state.is_open() {
        return false;
    }
    if view_state.is_finishing() {
        return true;
    }
    if e.modifiers
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
    {
        return false;
    }

    let onboarding = core.model().peek().onboarding.clone();
    match &e.key {
        Key::Named(NamedKey::Enter) => {
            if view_state.has_non_primary_focus() {
                // Let the focused button/radio card handle its own Enter activation without also advancing the step.
                return true;
            }
            if onboarding.page == OnboardingPage::Go {
                if !view_state.begin_finish() {
                    return true;
                }
                audio::play_finish();
                core.invoke_or_log(Command::OnboardingNext);
                return true;
            }
            if onboarding.page == OnboardingPage::Start {
                audio::play_start();
            } else if let Some(next_page) = onboarding.page.next() {
                audio::play_step(next_page.pose(), 1);
            }
            view_state.bump_transition();
            view_state.bump_primary_focus();
            core.invoke_or_log(Command::OnboardingNext);
            true
        }
        Key::Named(NamedKey::Backspace) => {
            if let Some(prev) = onboarding.page.prev_chapter() {
                audio::play_step(prev.pose(), -1);
                view_state.bump_transition();
                view_state.bump_primary_focus();
                core.invoke_or_log(Command::OnboardingBack);
            }
            true
        }
        Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowRight) => {
            let forward = matches!(e.key, Key::Named(NamedKey::ArrowRight));
            handle_horizontal_arrow(forward, &onboarding, core, view_state);
            true
        }
        Key::Named(NamedKey::Escape) => {
            // Fork (`App.jsx:429-453`) does NOT skip onboarding on Escape; consume so background shortcuts never fire.
            true
        }
        Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown) => {
            let down = matches!(e.key, Key::Named(NamedKey::ArrowDown));
            handle_vertical_arrow(down, &onboarding, core, view_state);
            true
        }
        _ => false,
    }
}

fn handle_horizontal_arrow(
    forward: bool,
    onboarding: &OnboardingState,
    core: &CoreView,
    view_state: &OnboardingViewState,
) {
    match onboarding.page {
        OnboardingPage::Start => {
            let mut pose = view_state.start_pose;
            let current = *pose.peek();
            let idx = MODE_ORDER.iter().position(|&m| m == current).unwrap_or(0);
            let next_idx = if forward {
                (idx + 1) % MODE_ORDER.len()
            } else {
                (idx + MODE_ORDER.len() - 1) % MODE_ORDER.len()
            };
            let next_mode = MODE_ORDER[next_idx];
            pose.set(next_mode);
            audio::play_mode(next_mode);
        }
        OnboardingPage::MoveIn => match *view_state.move_focus.peek() {
            MoveInFocus::Import => {
                // Cycle None + each discovered browser import source
                let count = 1 + onboarding.import_sources.len();
                let current_idx = match onboarding.choices.import_source_key.as_deref() {
                    None => 0,
                    Some(k) => onboarding
                        .import_sources
                        .iter()
                        .position(|s| s.key == k)
                        .map(|i| i + 1)
                        .unwrap_or(0),
                };
                let next_idx = if forward {
                    (current_idx + 1) % count
                } else {
                    (current_idx + count - 1) % count
                };
                if next_idx == 0 {
                    audio::play_select(false);
                    view_state.bump_nudge();
                    core.invoke_or_log(Command::OnboardingSetImport {
                        source_key: None,
                        profile_id: None,
                    });
                } else if let Some(src) = onboarding.import_sources.get(next_idx - 1) {
                    audio::play_select(true);
                    view_state.bump_nudge();
                    core.invoke_or_log(Command::OnboardingSetImport {
                        source_key: Some(src.key.clone()),
                        profile_id: src.profiles.first().map(|p| p.id.clone()),
                    });
                }
            }
            MoveInFocus::Search => {
                let engines = onboarding_search_engines();
                if !engines.is_empty() {
                    let current_idx = engines
                        .iter()
                        .position(|e| e.name.eq_ignore_ascii_case(&onboarding.choices.search_engine))
                        .unwrap_or(0);
                    let next_idx = if forward {
                        (current_idx + 1) % engines.len()
                    } else {
                        (current_idx + engines.len() - 1) % engines.len()
                    };
                    audio::play_select(true);
                    view_state.bump_nudge();
                    core.invoke_or_log(Command::OnboardingSetSearchEngine {
                        name: engines[next_idx].name.clone(),
                    });
                }
            }
            MoveInFocus::DefaultBrowser => {
                let enabled = !onboarding.choices.default_browser;
                audio::play_select(enabled);
                view_state.bump_nudge();
                core.invoke_or_log(Command::OnboardingSetDefaultBrowser { enabled });
            }
        },
        _ => {
            if forward {
                if onboarding.page != OnboardingPage::Go {
                    if let Some(next_page) = onboarding.page.next() {
                        audio::play_step(next_page.pose(), 1);
                    }
                    view_state.bump_transition();
                    view_state.bump_primary_focus();
                    core.invoke_or_log(Command::OnboardingNext);
                }
            } else if let Some(prev) = onboarding.page.prev_chapter() {
                audio::play_step(prev.pose(), -1);
                view_state.bump_transition();
                view_state.bump_primary_focus();
                core.invoke_or_log(Command::OnboardingBack);
            }
        }
    }
}

fn handle_vertical_arrow(down: bool, onboarding: &OnboardingState, core: &CoreView, view_state: &OnboardingViewState) {
    match onboarding.page {
        OnboardingPage::Start => {
            let mut pose = view_state.start_pose;
            let current = *pose.peek();
            let idx = MODE_ORDER.iter().position(|&m| m == current).unwrap_or(0);
            let next_idx = if down {
                (idx + 1) % MODE_ORDER.len()
            } else {
                (idx + MODE_ORDER.len() - 1) % MODE_ORDER.len()
            };
            let next_mode = MODE_ORDER[next_idx];
            pose.set(next_mode);
            audio::play_mode(next_mode);
        }
        OnboardingPage::Keys => {
            let enabled = !down;
            audio::play_select(enabled);
            view_state.bump_nudge();
            core.invoke_or_log(Command::OnboardingSetVim { enabled });
        }
        OnboardingPage::Agents => {
            let installed: Vec<&str> = onboarding
                .agent_catalog
                .iter()
                .filter(|a| a.installed)
                .map(|a| a.id.as_str())
                .collect();
            if !installed.is_empty() {
                let current_idx = onboarding
                    .choices
                    .agent_id
                    .as_deref()
                    .and_then(|id| installed.iter().position(|&c| c == id))
                    .unwrap_or(0);
                let next_idx = if down {
                    (current_idx + 1) % installed.len()
                } else {
                    (current_idx + installed.len() - 1) % installed.len()
                };
                audio::play_select(true);
                view_state.bump_nudge();
                core.invoke_or_log(Command::OnboardingSetAgent {
                    agent_id: installed[next_idx].to_string(),
                });
            }
        }
        OnboardingPage::Shield => {
            let enabled = !down;
            audio::play_select(enabled);
            view_state.bump_nudge();
            core.invoke_or_log(Command::OnboardingSetShield { enabled });
        }
        OnboardingPage::MoveIn => {
            let mut focus = view_state.move_focus;
            let next = match (*focus.peek(), down) {
                (MoveInFocus::Import, true) => MoveInFocus::Search,
                (MoveInFocus::Search, true) => MoveInFocus::DefaultBrowser,
                (MoveInFocus::DefaultBrowser, true) => MoveInFocus::Import,
                (MoveInFocus::Import, false) => MoveInFocus::DefaultBrowser,
                (MoveInFocus::Search, false) => MoveInFocus::Import,
                (MoveInFocus::DefaultBrowser, false) => MoveInFocus::Search,
            };
            focus.set(next);
            audio::play_select(true);
        }
        OnboardingPage::Apps | OnboardingPage::Go => {}
    }
}

/// Full-window overlay mounted above [`crate::ui::chrome::WindowShell`] in the real browser window.
///
/// Holds the shell mounted for [`motion::ONB_EXIT_FADE`] (`280 ms`) while fading out on completion (`ONB-07`).
#[derive(Clone, PartialEq)]
pub struct OnboardingOverlay;

impl Component for OnboardingOverlay {
    fn render(&self) -> impl IntoElement {
        let view_state = try_use_onboarding_state();
        let open = view_state.map(|vs| *vs.open.read()).unwrap_or(false);
        let mut has_been_open = use_state(|| open);
        if open && !*has_been_open.peek() {
            has_been_open.set(true);
        }
        let exit_anim = use_animation_with_dependencies(&open, move |conf, is_open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(if *is_open { OnChange::Finish } else { OnChange::Rerun });
            let (from, to) = if *is_open { (1.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::ONB_EXIT_FADE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let opacity = if open {
            1.0
        } else {
            exit_anim.get().value().clamp(0.0, 1.0)
        };

        if !open && (!*has_been_open.read() || opacity <= 0.005) {
            if let Some(vs) = view_state
                && *vs.finishing.peek()
            {
                let mut finishing = vs.finishing;
                finishing.set(false);
            }
            return rect().into_element();
        }
        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .opacity(opacity)
            .interactive(if open { Interactive::Yes } else { Interactive::No })
            .child(OnboardingView::new())
            .into_element()
    }
}

/// Full-window onboarding surface rendered inside either [`OnboardingOverlay`] (real window) or the gallery stage.
#[derive(Clone, PartialEq)]
pub struct OnboardingView {
    /// Disable continuous animation loops and blinking (used for deterministic gallery screenshots).
    pub still: bool,
    /// Optional fixed scene clock value in seconds (for deterministic screenshots).
    pub frozen_at: Option<f32>,
    /// Optional pinned chapter transition progress (`0.0..=1.0`) for showcasing mid-transition motion in the gallery.
    pub pinned_transition: Option<f32>,
    /// Optional pinned mascot morph `(from_mode, progress)` for mid-transition preview.
    pub pinned_mascot_morph: Option<(InputMode, f32)>,
    /// Optional Move in focus override (gallery fixtures).
    pub move_focus_override: Option<MoveInFocus>,
    /// Optional Start page pose override (gallery fixtures).
    pub start_pose_override: Option<InputMode>,
    pub key: DiffKey,
}

impl KeyExt for OnboardingView {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl OnboardingView {
    pub fn new() -> Self {
        Self {
            still: false,
            frozen_at: None,
            pinned_transition: None,
            pinned_mascot_morph: None,
            move_focus_override: None,
            start_pose_override: None,
            key: DiffKey::None,
        }
    }

    pub fn still(mut self, still: bool) -> Self {
        self.still = still;
        self
    }

    pub fn frozen_at(mut self, seconds: Option<f32>) -> Self {
        self.frozen_at = seconds;
        self
    }

    pub fn pinned_transition(mut self, progress: Option<f32>) -> Self {
        self.pinned_transition = progress;
        self
    }

    pub fn pinned_mascot_morph(mut self, morph: Option<(InputMode, f32)>) -> Self {
        self.pinned_mascot_morph = morph;
        self
    }

    pub fn move_focus_override(mut self, focus: Option<MoveInFocus>) -> Self {
        self.move_focus_override = focus;
        self
    }

    pub fn start_pose_override(mut self, pose: Option<InputMode>) -> Self {
        self.start_pose_override = pose;
        self
    }
}

impl Default for OnboardingView {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for OnboardingView {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let pal = OnboardingPalette::for_scheme(theme.scheme);
        let view_state = use_onboarding_state();
        let mut width_px = use_state(|| 1280.0_f32);

        let onboarding = core.model().read().onboarding.clone();
        let page = onboarding.page;
        let active_mode = if page == OnboardingPage::Start {
            self.start_pose_override
                .unwrap_or_else(|| *view_state.start_pose.read())
        } else {
            page.pose()
        };
        let target_accent = pal.mode_accent(active_mode);
        let target_pastel = OnboardingPalette::mode_pastel(active_mode);

        // Smooth ambient field color crossfade (`550 ms`, matches `zendia-welcome.css:154`).
        let mut prev_ambient = use_state(|| (target_accent, target_pastel));
        let mut cur_ambient = use_state(|| (target_accent, target_pastel));
        let mut ambient_seq = use_state(|| 0u64);
        if *cur_ambient.peek() != (target_accent, target_pastel) {
            prev_ambient.set(*cur_ambient.peek());
            cur_ambient.set((target_accent, target_pastel));
            let next = ambient_seq.peek().wrapping_add(1);
            ambient_seq.set(next);
        }
        let still = self.still;
        let ambient_anim = use_animation_with_dependencies(&*ambient_seq.read(), move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(if still { OnChange::Finish } else { OnChange::Rerun });
            AnimNum::new(0.0, 1.0)
                .duration(motion::ONB_AMBIENT_TRANSITION)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let amb_t = ambient_anim.get().value().clamp(0.0, 1.0);
        let (pa, pp) = *prev_ambient.read();
        let (ca, cp) = *cur_ambient.read();
        let mode_accent = pa.mix(ca, amb_t);
        let mode_pastel = pp.mix(cp, amb_t);

        // Directional chapter entrance animation (`460 ms` span driving staggered header 0/40/80ms, choices 380ms, stage 440ms).
        let mut last_page = use_state(|| page);
        let mut local_seq = use_state(|| 0u64);
        if *last_page.peek() != page {
            last_page.set(page);
            let next = *local_seq.peek() + 1;
            local_seq.set(next);
            if view_state.is_open() {
                view_state.bump_primary_focus();
            }
        }
        let anim_dep = (*local_seq.read(), *view_state.transition_seq.read());
        let step_anim = use_animation_with_dependencies(&anim_dep, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(if still { OnChange::Finish } else { OnChange::Rerun });
            AnimNum::new(0.0, 460.0)
                .duration(Duration::from_millis(460))
                .ease(Ease::Out)
                .function(Function::Linear)
        });

        let trans_elapsed_ms = match self.pinned_transition {
            Some(p) => p.clamp(0.0, 1.0) * 460.0,
            None if self.still => 460.0,
            None => step_anim.get().value().clamp(0.0, 460.0),
        };

        let ambient_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_ambient_field(canvas, ctx.size.width, ctx.size.height, mode_accent, mode_pastel);
            }
        });

        let muted = *view_state.muted.read();
        let paused = *view_state.paused.read();
        let finishing = *view_state.finishing.read();
        let move_focus = self
            .move_focus_override
            .unwrap_or_else(|| *view_state.move_focus.read());
        let nudge_seq = *view_state.nudge_seq.read();
        let primary_focus_seq = *view_state.primary_focus_seq.read();
        let cur_w = *width_px.read();
        let compact_layout = cur_w < 1020.0;
        let narrow_layout = cur_w < 640.0;

        // Handle keyboard navigation directly when OnboardingView is embedded outside OnboardingOverlay (e.g. Gallery, ONB-12).
        let text_focus = try_use_text_focus();
        let on_embedded_key_down = {
            let core = core.clone();
            move |e: Event<KeyboardEventData>| {
                if view_state.is_open() {
                    // Real-window overlay keys are dispatched centrally by browser.rs to avoid double-handling.
                    return;
                }
                if text_focus.is_some_and(|tf| tf.is_any_focused()) {
                    return;
                }
                let _ = handle_onboarding_key_inner(&e, &core, &view_state, false);
            }
        };

        let body: Element = if page == OnboardingPage::Start {
            StartPageLayout {
                pal,
                pose: active_mode,
                muted,
                still: self.still,
                compact: compact_layout,
                narrow: narrow_layout,
                primary_focus_seq,
                pinned_morph: self.pinned_mascot_morph,
                view_state,
                key: DiffKey::from(&("onboarding-start")),
            }
            .into_element()
        } else {
            ChapterFlowLayout {
                pal,
                onboarding,
                muted,
                paused,
                finishing,
                compact: compact_layout,
                narrow: narrow_layout,
                move_focus,
                nudge_seq,
                primary_focus_seq,
                trans_elapsed_ms,
                still: self.still,
                frozen_at: self.frozen_at,
                pinned_mascot_morph: self.pinned_mascot_morph,
                view_state,
                key: DiffKey::from(&("onboarding-flow")),
            }
            .into_element()
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(pal.bg.to_color())
            .overflow(Overflow::Clip)
            .on_sized(move |e: Event<SizedEventData>| {
                let w = e.area.width();
                if w > 0.0 && (*width_px.peek() - w).abs() > 1.0 {
                    width_px.set(w);
                }
            })
            .on_global_key_down(on_embedded_key_down)
            .child(
                // Subtle dual-radial ambient glow tinted by the active mode cell
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::percent(100.0))
                    .background(Fill::Callback(ambient_cb)),
            )
            .child(body)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/* =========================================================================
Start Page — Living Emblem
========================================================================= */

#[derive(Clone, PartialEq)]
struct StartPageLayout {
    pal: OnboardingPalette,
    pose: InputMode,
    muted: bool,
    still: bool,
    compact: bool,
    narrow: bool,
    primary_focus_seq: u64,
    pinned_morph: Option<(InputMode, f32)>,
    view_state: OnboardingViewState,
    key: DiffKey,
}

impl KeyExt for StartPageLayout {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for StartPageLayout {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let pal = self.pal;
        let view_state = self.view_state;
        let pose = self.pose;
        let accent = pal.mode_accent(pose);
        let still = self.still;

        // Staggered Start hero entrance (`0/60/120/180 ms`, `550 ms` ease-out, `18px -> 0`).
        let hero_anim = use_animation(move |conf| {
            conf.on_creation(if still { OnCreation::Finish } else { OnCreation::Run });
            AnimNum::new(0.0, 730.0)
                .duration(Duration::from_millis(730))
                .ease(Ease::Out)
                .function(Function::Linear)
        });
        let hero_ms = if still {
            730.0
        } else {
            hero_anim.get().value().clamp(0.0, 730.0)
        };
        let dur = motion::ONB_START_FADE.as_millis() as f32;
        let p_brand = stagger_ease(hero_ms, 0.0, dur);
        let p_title = stagger_ease(hero_ms, 60.0, dur);
        let p_lead = stagger_ease(hero_ms, 120.0, dur);
        let p_cta = stagger_ease(hero_ms, 180.0, dur);
        let p_emblem = stagger_ease(hero_ms, 60.0, dur);

        let on_start = {
            let core = core.clone();
            move |_: ()| {
                audio::play_start();
                view_state.bump_transition();
                view_state.bump_primary_focus();
                core.invoke_or_log(Command::OnboardingNext);
            }
        };

        let on_change_pose = move |next_mode: InputMode| {
            let mut sp = view_state.start_pose;
            sp.set_if_modified(next_mode);
        };

        let container_dir = if self.compact {
            Direction::Vertical
        } else {
            Direction::Horizontal
        };
        let container_pad = if self.compact {
            Gaps::new(24.0, 28.0, 24.0, 28.0)
        } else {
            Gaps::new(40.0, 56.0, 40.0, 56.0)
        };
        let container_gap = if self.compact { 28.0 } else { 64.0 };
        let title_size = if self.compact { 44.0 } else { 60.0 };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .center()
            .child(
                // Floating Sound Toggle in top-right
                rect()
                    .position(Position::new_absolute().top(20.0).right(24.0))
                    .layer(Layer::Relative(6))
                    .child(SoundToggleButton {
                        pal,
                        muted: self.muted,
                        accent,
                        pose,
                        view_state,
                        key: DiffKey::from(&("sound-start")),
                    }),
            )
            .child(
                // Centered 2-column hero layout (`max-width: 1180px`, stacks below 1020px)
                rect()
                    .width(Size::fill())
                    .max_width(Size::px(1180.0))
                    .height(Size::fill())
                    .padding(container_pad)
                    .direction(container_dir)
                    .cross_align(Alignment::center())
                    .main_align(Alignment::center())
                    .spacing(container_gap)
                    .content(Content::Flex)
                    .child(
                        // Left hero copy + primary CTA
                        rect()
                            .width(if self.compact { Size::fill() } else { Size::flex(1.0) })
                            .spacing(20.0)
                            .child(
                                rect()
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(10.0)
                                    .offset_y(lerp(18.0, 0.0, p_brand))
                                    .opacity(p_brand)
                                    .child(
                                        OnboardingMascot::new(InputMode::Normal, 30.0)
                                            .still(true)
                                            .blink(false),
                                    )
                                    .child(
                                        label()
                                            .text("Zendia")
                                            .font_size(16.0)
                                            .font_weight(FontWeight::EXTRA_BOLD)
                                            .color(pal.text.to_color()),
                                    ),
                            )
                            .child(
                                rect()
                                    .offset_y(lerp(18.0, 0.0, p_title))
                                    .opacity(p_title)
                                    .child(
                                        label()
                                            .text("Hands on\nthe keys.")
                                            .font_size(title_size)
                                            .font_weight(FontWeight::EXTRA_BOLD)
                                            .line_height(1.04)
                                            .color(pal.text.to_color()),
                                    ),
                            )
                            .child(
                                rect()
                                    .max_width(Size::px(450.0))
                                    .offset_y(lerp(18.0, 0.0, p_lead))
                                    .opacity(p_lead)
                                    .child(
                                        label()
                                            .text("Zendia is a browser you drive from the keyboard, with your own agent along for the ride and the ads left outside.")
                                            .font_size(17.5)
                                            .line_height(1.56)
                                            .color(pal.muted.to_color()),
                                    ),
                            )
                            .child(
                                rect()
                                    .margin(Gaps::new(8.0, 0.0, 0.0, 0.0))
                                    .offset_y(lerp(18.0, 0.0, p_cta))
                                    .opacity(p_cta)
                                    .child(OnboardingButton {
                                        pal,
                                        text: "Set up Zendia".to_string(),
                                        key_hint: (!self.narrow).then(|| "Enter".to_string()),
                                        primary: true,
                                        large: true,
                                        ghost: false,
                                        disabled: false,
                                        mode: InputMode::Normal,
                                        auto_focus_seq: Some(self.primary_focus_seq),
                                        view_state,
                                        on_click: EventHandler::new(on_start),
                                        key: DiffKey::from(&("btn-start")),
                                    }),
                            ),
                    )
                    .child(
                        // Right Living Emblem + 6 mode pills
                        rect()
                            .width(if self.compact { Size::fill() } else { Size::flex(1.05) })
                            .offset_y(lerp(16.0, 0.0, p_emblem))
                            .opacity(p_emblem)
                            .center()
                            .child(StartEmblemScene {
                                pose,
                                still: self.still,
                                pinned_morph: self.pinned_morph,
                                on_change_pose: EventHandler::new(on_change_pose),
                                key: DiffKey::from(&("start-emblem")),
                            }),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/* =========================================================================
Chapter Flow Shell — Top Rail + 2-Column Workspace
========================================================================= */

#[derive(Clone, PartialEq)]
struct ChapterFlowLayout {
    pal: OnboardingPalette,
    onboarding: OnboardingState,
    muted: bool,
    paused: bool,
    finishing: bool,
    compact: bool,
    narrow: bool,
    move_focus: MoveInFocus,
    nudge_seq: u64,
    primary_focus_seq: u64,
    trans_elapsed_ms: f32,
    still: bool,
    frozen_at: Option<f32>,
    pinned_mascot_morph: Option<(InputMode, f32)>,
    view_state: OnboardingViewState,
    key: DiffKey,
}

impl KeyExt for ChapterFlowLayout {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ChapterFlowLayout {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let pal = self.pal;
        let onboarding = &self.onboarding;
        let page = onboarding.page;
        let pose = page.pose();
        let mode_accent = pal.mode_accent(pose);
        let view_state = self.view_state;
        let still = self.still;

        // One-shot shell entrance when transitioning from Start into the chapter flow (`ONB-04`).
        let shell_anim = use_animation(move |conf| {
            conf.on_creation(if still { OnCreation::Finish } else { OnCreation::Run });
            AnimNum::new(0.0, 450.0)
                .duration(motion::ONB_DECK_IN)
                .ease(Ease::Out)
                .function(Function::Linear)
        });
        let shell_ms = if still {
            450.0
        } else {
            shell_anim.get().value().clamp(0.0, 450.0)
        };
        let rail_p = stagger_ease(shell_ms, 0.0, motion::ONB_RAIL_IN.as_millis() as f32);
        let deck_p = stagger_ease(shell_ms, 0.0, motion::ONB_DECK_IN.as_millis() as f32);

        // Staggered directional slide (`±22px`) and fade for chapter header children (`0/40/80 ms`, `360 ms`),
        // choices (`380 ms`), and stage entrance (`scale 0.965 -> 1`, `440 ms`).
        let slide_dir = if onboarding.direction < 0 { -1.0 } else { 1.0 };
        let ms = self.trans_elapsed_ms;
        let hdr_dur = motion::ONB_TEXT_ENTER.as_millis() as f32;
        let p_tag = stagger_ease(ms, 0.0, hdr_dur);
        let p_title = stagger_ease(ms, 40.0, hdr_dur);
        let p_desc = stagger_ease(ms, 80.0, hdr_dur);
        let p_choices = stagger_ease(ms, 40.0, motion::ONB_PAGE_ENTER.as_millis() as f32);
        let p_stage = stagger_ease(ms, 0.0, motion::ONB_SCENE_ENTER.as_millis() as f32);

        let (headline, description) = chapter_copy(page);
        let vim_active_after_keys = page.index() > OnboardingPage::Keys.index() && onboarding.choices.vim_enabled;
        let selected_agent = onboarding.selected_agent().cloned();
        let selected_apps = onboarding.selected_essential_presets();

        // Build chapter choices widget
        let choices_el: Element = match page {
            OnboardingPage::Keys => YesNoChoices {
                pal,
                accent: mode_accent,
                yes_bold: "YES",
                yes_rest: ", turn on vim mode.",
                no_bold: "NO",
                no_rest: ", keep the usual shortcuts.",
                value: onboarding.choices.vim_enabled,
                view_state,
                on_choose: EventHandler::new({
                    let core = core.clone();
                    move |enabled: bool| {
                        audio::play_select(enabled);
                        view_state.bump_nudge();
                        core.invoke_or_log(Command::OnboardingSetVim { enabled });
                    }
                }),
                key: DiffKey::from(&("choices-keys")),
            }
            .into_element(),
            OnboardingPage::Agents => AgentChoicesPanel {
                pal,
                accent: mode_accent,
                onboarding: onboarding.clone(),
                view_state,
                key: DiffKey::from(&("choices-agents")),
            }
            .into_element(),
            OnboardingPage::Shield => YesNoChoices {
                pal,
                accent: mode_accent,
                yes_bold: "YES",
                yes_rest: ", keep blocking ads.",
                no_bold: "NO",
                no_rest: ", show me everything.",
                value: onboarding.choices.shield_enabled,
                view_state,
                on_choose: EventHandler::new({
                    let core = core.clone();
                    move |enabled: bool| {
                        audio::play_select(enabled);
                        view_state.bump_nudge();
                        core.invoke_or_log(Command::OnboardingSetShield { enabled });
                    }
                }),
                key: DiffKey::from(&("choices-shield")),
            }
            .into_element(),
            OnboardingPage::MoveIn => MoveInChoicesPanel {
                pal,
                accent: mode_accent,
                onboarding: onboarding.clone(),
                focus: self.move_focus,
                narrow: self.narrow,
                view_state,
                key: DiffKey::from(&("choices-move-in")),
            }
            .into_element(),
            OnboardingPage::Apps => AppsChoicesPanel {
                pal,
                selected_urls: onboarding.choices.essential_urls.clone(),
                view_state,
                key: DiffKey::from(&("choices-apps")),
            }
            .into_element(),
            OnboardingPage::Go => {
                if onboarding.choices.vim_enabled {
                    let hint_border = mix_rgba(pal.border_subtle, mode_accent, 0.40);
                    let hint_bg = mix_rgba(pal.bg, mode_accent, 0.10);
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(12.0, 14.0, 12.0, 14.0))
                        .corner_radius(CornerRadius::new_all(14.0))
                        .background(hint_bg.to_color())
                        .border(
                            Border::new()
                                .width(1.5)
                                .alignment(BorderAlignment::Inner)
                                .fill(hint_border.to_color()),
                        )
                        .child(
                            label()
                                .text("Press Space, then ?, to see every key.")
                                .font_size(14.0)
                                .font_weight(FontWeight::MEDIUM)
                                .color(pal.text.to_color()),
                        )
                        .into_element()
                } else {
                    rect().into_element()
                }
            }
            OnboardingPage::Start => rect().into_element(),
        };

        // Build chapter 2D stage content
        let stage_el: Element = match page {
            OnboardingPage::Keys => VimScene {
                enabled: onboarding.choices.vim_enabled,
                still: self.still,
                paused: self.paused,
                frozen_at: self.frozen_at,
                key: DiffKey::from(&("scene-keys")),
            }
            .into_element(),
            OnboardingPage::Agents => AgentScene {
                agent: selected_agent.clone(),
                vim: vim_active_after_keys,
                still: self.still,
                paused: self.paused,
                frozen_at: self.frozen_at,
                key: DiffKey::from(&("scene-agents")),
            }
            .into_element(),
            OnboardingPage::Shield => ShieldScene {
                enabled: onboarding.choices.shield_enabled,
                vim: vim_active_after_keys,
                still: self.still,
                paused: self.paused,
                frozen_at: self.frozen_at,
                key: DiffKey::from(&("scene-shield")),
            }
            .into_element(),
            OnboardingPage::MoveIn => MoveInScene {
                focus: self.move_focus,
                source: onboarding.selected_import_source().cloned(),
                engine_name: onboarding.choices.search_engine.clone(),
                default_browser: onboarding.choices.default_browser,
                vim: vim_active_after_keys,
                still: self.still,
                paused: self.paused,
                frozen_at: self.frozen_at,
                key: DiffKey::from(&("scene-move-in")),
            }
            .into_element(),
            OnboardingPage::Apps => AppsScene {
                apps: selected_apps.clone(),
                vim: vim_active_after_keys,
                still: self.still,
                key: DiffKey::from(&("scene-apps")),
            }
            .into_element(),
            OnboardingPage::Go => GoScene {
                vim: vim_active_after_keys,
                ads: onboarding.choices.shield_enabled,
                agent: selected_agent,
                essentials: selected_apps,
                still: self.still,
                key: DiffKey::from(&("scene-go")),
            }
            .into_element(),
            OnboardingPage::Start => rect().into_element(),
        };

        let is_first_chapter = page == OnboardingPage::Keys;
        let is_last_chapter = page == OnboardingPage::Go;
        let finishing = self.finishing;

        let on_back = {
            let core = core.clone();
            move |_: ()| {
                if view_state.is_finishing() {
                    return;
                }
                if let Some(prev) = page.prev_chapter() {
                    audio::play_step(prev.pose(), -1);
                    view_state.bump_transition();
                    view_state.bump_primary_focus();
                    core.invoke_or_log(Command::OnboardingBack);
                }
            }
        };
        let on_skip_apps = {
            let core = core.clone();
            move |_: ()| {
                if view_state.is_finishing() {
                    return;
                }
                audio::play_step(OnboardingPage::Go.pose(), 1);
                view_state.bump_transition();
                view_state.bump_primary_focus();
                core.invoke_or_log(Command::OnboardingSkipApps);
            }
        };
        let on_next = {
            let core = core.clone();
            move |_: ()| {
                if is_last_chapter {
                    if !view_state.begin_finish() {
                        return;
                    }
                    audio::play_finish();
                    core.invoke_or_log(Command::OnboardingNext);
                    return;
                }
                if view_state.is_finishing() {
                    return;
                }
                if let Some(next_p) = page.next() {
                    audio::play_step(next_p.pose(), 1);
                }
                view_state.bump_transition();
                view_state.bump_primary_focus();
                core.invoke_or_log(Command::OnboardingNext);
            }
        };
        let on_toggle_pause = move |_: ()| {
            let mut p = view_state.paused;
            let next = !*p.peek();
            p.set(next);
        };

        let deck_shadow = if pal.dark {
            Shadow::new().y(18.0).blur(44.0).color(Color::from_argb(92, 0, 0, 0))
        } else {
            Shadow::new()
                .y(12.0)
                .blur(32.0)
                .color(Color::from_argb(20, 0x1a, 0x1b, 0x26))
        };

        let mascot_size = if self.compact { 62.0 } else { 82.0 };
        let outer_pad = if self.compact {
            Gaps::new(14.0, 18.0, 16.0, 18.0)
        } else {
            Gaps::new(18.0, 32.0, 22.0, 32.0)
        };

        // Left decision deck (in compact `<1020px`, stacks naturally below the stage inside a vertical scroll view)
        let choices_body: Element = if self.compact {
            rect()
                .width(Size::fill())
                .padding(Gaps::new(2.0, 2.0, 4.0, 2.0))
                .offset_x(lerp(22.0 * slide_dir, 0.0, p_choices))
                .opacity(p_choices)
                .child(choices_el)
                .into_element()
        } else {
            ScrollView::new()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(2.0, 4.0, 6.0, 2.0))
                        .offset_x(lerp(22.0 * slide_dir, 0.0, p_choices))
                        .opacity(p_choices)
                        .child(choices_el),
                )
                .into_element()
        };

        let deck_el = rect()
            .width(if self.compact { Size::fill() } else { Size::flex(0.94) })
            .height(if self.compact { Size::auto() } else { Size::fill() })
            .padding(if self.compact {
                Gaps::new(18.0, 20.0, 16.0, 20.0)
            } else {
                Gaps::new(26.0, 28.0, 22.0, 28.0)
            })
            .spacing(if self.compact { 14.0 } else { 20.0 })
            .corner_radius(CornerRadius::new_all(26.0))
            .background(pal.surface.to_color())
            .border(
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(pal.border_subtle.to_color()),
            )
            .shadow(deck_shadow)
            .offset_y(lerp(18.0, 0.0, deck_p))
            .opacity(deck_p)
            .content(Content::Flex)
            .child(
                // Guide header: mascot + mode tag + title + description (staggered 0/40/80 ms)
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::start())
                    .spacing(if self.compact { 14.0 } else { 18.0 })
                    .child(
                        OnboardingMascot::new(pose, mascot_size)
                            .still(self.still)
                            .blink(!self.still)
                            .nudge_seq(self.nudge_seq)
                            .pinned_morph(self.pinned_mascot_morph),
                    )
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .spacing(6.0)
                            .child(
                                rect()
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(6.0)
                                    .offset_x(lerp(22.0 * slide_dir, 0.0, p_tag))
                                    .opacity(p_tag)
                                    .child(
                                        rect()
                                            .width(Size::px(7.0))
                                            .height(Size::px(7.0))
                                            .corner_radius(CornerRadius::new_all(4.0))
                                            .background(mode_accent.to_color()),
                                    )
                                    .child(
                                        label()
                                            .text(page.mode_tag())
                                            .font_size(11.0)
                                            .font_weight(FontWeight::BOLD)
                                            .font_family("monospace")
                                            .color(mode_accent.to_color()),
                                    ),
                            )
                            .child(
                                rect()
                                    .offset_x(lerp(22.0 * slide_dir, 0.0, p_title))
                                    .opacity(p_title)
                                    .child(
                                        label()
                                            .text(headline)
                                            .font_size(if self.compact { 24.0 } else { 30.0 })
                                            .font_weight(FontWeight::EXTRA_BOLD)
                                            .line_height(1.12)
                                            .color(pal.text.to_color()),
                                    ),
                            )
                            .child(
                                rect()
                                    .offset_x(lerp(22.0 * slide_dir, 0.0, p_desc))
                                    .opacity(p_desc)
                                    .child(
                                        label()
                                            .text(description)
                                            .font_size(14.5)
                                            .line_height(1.54)
                                            .color(pal.muted.to_color()),
                                    ),
                            ),
                    ),
            )
            .child(choices_body)
            .child(
                // Footer nav bar with a clean top-only divider
                rect()
                    .width(Size::fill())
                    .spacing(14.0)
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(1.0))
                            .background(pal.border_subtle.to_color()),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .main_align(Alignment::SpaceBetween)
                            .child(OnboardingButton {
                                pal,
                                text: "Back".to_string(),
                                key_hint: (!self.narrow).then(|| "Backspace".to_string()),
                                primary: false,
                                large: false,
                                ghost: false,
                                disabled: is_first_chapter || finishing,
                                mode: pose,
                                auto_focus_seq: None,
                                view_state,
                                on_click: EventHandler::new(on_back),
                                key: DiffKey::from(&("btn-back")),
                            })
                            .child(
                                rect()
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(10.0)
                                    .maybe_child(page.has_skip().then(|| OnboardingButton {
                                        pal,
                                        text: "Skip".to_string(),
                                        key_hint: None,
                                        primary: false,
                                        large: false,
                                        ghost: true,
                                        disabled: finishing,
                                        mode: pose,
                                        auto_focus_seq: None,
                                        view_state,
                                        on_click: EventHandler::new(on_skip_apps),
                                        key: DiffKey::from(&("btn-skip-apps")),
                                    }))
                                    .child(OnboardingButton {
                                        pal,
                                        text: if is_last_chapter {
                                            "Start browsing".to_string()
                                        } else {
                                            "Next".to_string()
                                        },
                                        key_hint: (!self.narrow).then(|| "Enter".to_string()),
                                        primary: true,
                                        large: false,
                                        ghost: false,
                                        disabled: finishing,
                                        mode: pose,
                                        auto_focus_seq: Some(self.primary_focus_seq),
                                        view_state,
                                        on_click: EventHandler::new(on_next),
                                        key: DiffKey::from(&("btn-next")),
                                    }),
                            ),
                    ),
            );

        // Right framed 2D living stage (in compact `<1020px`, renders above the deck)
        let stage_box = rect()
            .width(if self.compact { Size::fill() } else { Size::flex(1.06) })
            .height(if self.compact { Size::px(330.0) } else { Size::fill() })
            .child(StageShell {
                accent: mode_accent,
                still: self.still,
                looping: page.is_looping(),
                paused: self.paused,
                compact: self.compact,
                on_toggle_pause: page.is_looping().then(|| EventHandler::new(on_toggle_pause)),
                caption: None,
                content: rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .opacity(p_stage)
                    .scale(lerp(0.965, 1.0, p_stage))
                    .center()
                    .child(stage_el)
                    .into_element(),
                key: DiffKey::from(&(format!("stage-{}", page.as_str()))),
            });

        let workspace: Element = if self.compact {
            ScrollView::new()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(0.0, 2.0, 8.0, 2.0))
                        .spacing(16.0)
                        .child(stage_box)
                        .child(deck_el),
                )
                .into_element()
        } else {
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .direction(Direction::Horizontal)
                .spacing(32.0)
                .content(Content::Flex)
                .child(deck_el)
                .child(stage_box)
                .into_element()
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .cross_align(Alignment::center())
            .child(
                rect()
                    .width(Size::fill())
                    .max_width(Size::px(1440.0))
                    .height(Size::fill())
                    .padding(outer_pad)
                    .spacing(if self.compact { 12.0 } else { 18.0 })
                    .content(Content::Flex)
                    .child(
                        // Top rail: Brand lockup + 6-cell chapter stepper + Sound toggle
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(6.0, 8.0, 6.0, 8.0))
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .main_align(Alignment::SpaceBetween)
                            .spacing(12.0)
                            .offset_y(lerp(-14.0, 0.0, rail_p))
                            .opacity(rail_p)
                            .child(
                                rect()
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(10.0)
                                    .child(OnboardingMascot::new(pose, 26.0).still(self.still).blink(false))
                                    .child(
                                        label()
                                            .text("Zendia")
                                            .font_size(16.0)
                                            .font_weight(FontWeight::EXTRA_BOLD)
                                            .color(pal.text.to_color()),
                                    ),
                            )
                            .child(
                                rect()
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(10.0)
                                    .child(
                                        rect()
                                            .direction(Direction::Horizontal)
                                            .cross_align(Alignment::center())
                                            .spacing(6.0)
                                            .children(OnboardingPage::CHAPTERS.iter().map(|&chapter| {
                                                let is_current = chapter == page;
                                                let is_done = chapter.index() < page.index();
                                                let disabled =
                                                    finishing || chapter.index() > onboarding.reached.index();
                                                ChapterStepPill {
                                                    pal,
                                                    chapter,
                                                    is_current,
                                                    is_done,
                                                    disabled,
                                                    show_label: !self.compact,
                                                    current_page: page,
                                                    view_state,
                                                    key: DiffKey::from(&(chapter.as_str())),
                                                }
                                            })),
                                    )
                                    .child(SoundToggleButton {
                                        pal,
                                        muted: self.muted,
                                        accent: mode_accent,
                                        pose,
                                        view_state,
                                        key: DiffKey::from(&("sound-rail")),
                                    }),
                            ),
                    )
                    .child(workspace),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn chapter_copy(page: OnboardingPage) -> (&'static str, &'static str) {
    match page {
        OnboardingPage::Start => (
            "Hands on\nthe keys.",
            "Zendia is a browser you drive from the keyboard, with your own agent along for the ride and the ads left outside.",
        ),
        OnboardingPage::Keys => (
            "Drive it like Neovim.",
            "Vim mode gives every key a job, for people who already think in modes. Not your thing? Turn it off here, or any time with the vim mode switch in the menu.",
        ),
        OnboardingPage::Agents => (
            "Bring your own agent.",
            "Press Space A A on any tab and ask the coding agent you already use, signed in as you. It reads the page, clicks, types and navigates for you, and asks before it closes anything.",
        ),
        OnboardingPage::Shield => (
            "Leave the ads outside.",
            "Zendia comes with uBlock Origin, so pages load cleaner and faster. You can remove it any time.",
        ),
        OnboardingPage::MoveIn => (
            "Move in.",
            "Bring your bookmarks, history and passwords, pick where your searches go, and say whether links from other apps open here.",
        ),
        OnboardingPage::Apps => (
            "Pin what you\nopen every day.",
            "The ones you pick stay at the top of the sidebar, a key away. You can add or drop them later.",
        ),
        OnboardingPage::Go => (
            "You are set.",
            "Everything you chose here can be changed later. Go and browse.",
        ),
    }
}

/* =========================================================================
Chapter Stepper Pill & Sound Toggle & Buttons
========================================================================= */

#[derive(Clone, PartialEq)]
struct ChapterStepPill {
    pal: OnboardingPalette,
    chapter: OnboardingPage,
    is_current: bool,
    is_done: bool,
    disabled: bool,
    show_label: bool,
    current_page: OnboardingPage,
    view_state: OnboardingViewState,
    key: DiffKey,
}

impl KeyExt for ChapterStepPill {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ChapterStepPill {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let fb = use_interactive_feedback(!self.disabled);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        if focus() != Focus::Not {
            self.view_state.note_non_primary_focus(a11y_id);
        }

        let pal = self.pal;
        let pose = self.chapter.pose();
        let accent = pal.mode_accent(pose);
        let pastel = OnboardingPalette::mode_pastel(pose);
        let hover_t = fb.hover_progress();
        let press_t = fb.press_progress();

        let bg = if self.is_current {
            mix_rgba(pal.surface, accent, 0.16).to_color()
        } else if self.disabled {
            Color::TRANSPARENT
        } else {
            pal.surface.to_color()
        };
        let border_base = if self.is_current {
            accent
        } else if self.is_done {
            mix_rgba(pal.border_subtle, accent, 0.42)
        } else {
            pal.border_subtle
        };
        let border = border_base.mix(accent, hover_t);
        let label_fg = if self.is_current {
            pal.text
        } else {
            pal.muted.mix(pal.text, hover_t)
        };
        let cell_size = if self.is_current { 10.5 } else { 9.0 };

        let chapter = self.chapter;
        let current_page = self.current_page;
        let disabled = self.disabled;
        let view_state = self.view_state;

        let activate = {
            let core = core.clone();
            move || {
                if !disabled && chapter != current_page {
                    let dir = if chapter.index() > current_page.index() { 1 } else { -1 };
                    audio::play_step(chapter.pose(), dir);
                    view_state.bump_transition();
                    view_state.bump_primary_focus();
                    core.invoke_or_log(Command::OnboardingGoto { page: chapter });
                }
            }
        };
        let activate_key = activate.clone();

        let dash_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                draw_dashed_rrect_stroke(
                    canvas,
                    sk::Rect::from_xywh(0.0, 0.0, ctx.size.width, ctx.size.height),
                    12.0,
                    1.5,
                    &[5.0, 4.0],
                    sk_alpha(pal.border, 0.95),
                );
            }
        });

        let mut pill = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(!disabled)
            .a11y_enabled(!disabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(chapter.chapter_label())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .min_height(Size::px(36.0))
            .padding(Gaps::new(5.0, 12.0, 5.0, 12.0))
            .spacing(7.0)
            .corner_radius(CornerRadius::new_all(12.0))
            .opacity(if disabled { 0.45 } else { 1.0 })
            .offset_y(-hover_t + 0.5 * press_t)
            .scale(1.0 - 0.025 * press_t)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |_| activate())
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if is_activate_key(&e.key) {
                    e.stop_propagation();
                    activate_key();
                }
            })
            .child(
                rect()
                    .width(Size::px(cell_size))
                    .height(Size::px(cell_size))
                    .corner_radius(CornerRadius::new_all(3.5))
                    .background(if disabled {
                        pal.border.to_color()
                    } else {
                        pastel.to_color()
                    })
                    .border(
                        Border::new()
                            .width(if disabled { 0.0 } else { 1.5 })
                            .alignment(BorderAlignment::Inner)
                            .fill(pal.ink.to_color()),
                    ),
            )
            .child(
                label()
                    .text(chapter.chapter_number().unwrap_or(""))
                    .font_size(11.0)
                    .font_weight(FontWeight::BOLD)
                    .font_family("monospace")
                    .color(pal.muted.to_color()),
            )
            .maybe_child(self.show_label.then(|| {
                label()
                    .text(chapter.chapter_label())
                    .font_size(13.0)
                    .font_weight(if self.is_current {
                        FontWeight::BOLD
                    } else {
                        FontWeight::SEMI_BOLD
                    })
                    .color(label_fg.to_color())
            }));

        if disabled {
            pill = pill.background(Fill::Callback(dash_cb));
        } else {
            pill = pill
                .background(bg)
                .border(
                    Border::new()
                        .width(1.5)
                        .alignment(BorderAlignment::Inner)
                        .fill(border.to_color()),
                )
                .cursor(CursorIcon::Pointer);
        }

        rect()
            .padding(Gaps::new_all(3.0))
            .corner_radius(CornerRadius::new_all(15.0))
            .border(onboarding_focus_ring(focus() == Focus::Keyboard && !disabled, accent))
            .child(pill)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SoundToggleButton {
    pal: OnboardingPalette,
    muted: bool,
    accent: Rgba,
    pose: InputMode,
    view_state: OnboardingViewState,
    key: DiffKey,
}

impl KeyExt for SoundToggleButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SoundToggleButton {
    fn render(&self) -> impl IntoElement {
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        if focus() != Focus::Not {
            self.view_state.note_non_primary_focus(a11y_id);
        }
        let hover_t = fb.hover_progress();
        let press_t = fb.press_progress();
        let pal = self.pal;
        let fg = pal.muted.mix(pal.text, hover_t);
        let border = pal.border_subtle.mix(self.accent, hover_t);
        let muted = self.muted;
        let pose = self.pose;
        let view_state = self.view_state;

        let toggle = move || {
            let next_muted = !muted;
            view_state.set_muted(next_muted);
            if !next_muted {
                audio::play_mode(pose);
            }
        };

        rect()
            .padding(Gaps::new_all(3.0))
            .corner_radius(CornerRadius::new_all(15.0))
            .border(onboarding_focus_ring(focus() == Focus::Keyboard, self.accent))
            .child(
                rect()
                    .a11y_id(a11y_id)
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(if muted { "Unmute sound" } else { "Mute sound" })
                    .width(Size::px(36.0))
                    .height(Size::px(36.0))
                    .corner_radius(CornerRadius::new_all(12.0))
                    .background(pal.surface.to_color())
                    .border(
                        Border::new()
                            .width(1.5)
                            .alignment(BorderAlignment::Inner)
                            .fill(border.to_color()),
                    )
                    .scale(1.0 + 0.04 * hover_t - 0.07 * press_t)
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(fb.on_pointer_enter())
                    .on_pointer_leave(fb.on_pointer_leave())
                    .on_pointer_down(fb.on_pointer_down())
                    .on_global_pointer_up(fb.on_pointer_up())
                    .on_press(move |_| toggle())
                    .on_key_down(move |e: Event<KeyboardEventData>| {
                        if is_activate_key(&e.key) {
                            e.stop_propagation();
                            toggle();
                        }
                    })
                    .child(sound_icon(muted, 16.0, fg.to_color())),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct OnboardingButton {
    pal: OnboardingPalette,
    text: String,
    key_hint: Option<String>,
    primary: bool,
    large: bool,
    ghost: bool,
    disabled: bool,
    mode: InputMode,
    auto_focus_seq: Option<u64>,
    view_state: OnboardingViewState,
    on_click: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for OnboardingButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for OnboardingButton {
    fn render(&self) -> impl IntoElement {
        let fb = use_interactive_feedback(!self.disabled);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let mut last_focus_seq = use_state(|| None::<u64>);

        if !self.primary && focus() != Focus::Not {
            self.view_state.note_non_primary_focus(a11y_id);
        }
        if self.primary
            && !self.disabled
            && self.view_state.is_open()
            && let Some(seq) = self.auto_focus_seq
            && *last_focus_seq.peek() != Some(seq)
        {
            last_focus_seq.set(Some(seq));
            a11y_id.request_focus();
        }

        let hover_t = fb.hover_progress();
        let press_t = fb.press_progress();
        let pal = self.pal;
        let accent = pal.mode_accent(self.mode);

        let (bg, fg, border_col) = if self.primary {
            let base = pal.primary_bg(self.mode);
            let hovered = mix_rgba(base, Rgba::rgb(255, 255, 255), if pal.dark { 0.10 } else { 0.08 });
            (
                base.mix(hovered, hover_t).to_color(),
                pal.primary_fg().to_color(),
                Color::TRANSPARENT,
            )
        } else if self.ghost {
            let bg = if hover_t > 0.01 {
                pal.raised.with_alpha(hover_t).to_color()
            } else {
                Color::TRANSPARENT
            };
            let fg = pal.muted.mix(pal.text, hover_t);
            (bg, fg.to_color(), Color::TRANSPARENT)
        } else {
            let border = pal.border.mix(mix_rgba(pal.border, accent, 0.65), hover_t);
            (pal.bg.to_color(), pal.text.to_color(), border.to_color())
        };

        let min_h = if self.large { 52.0 } else { 44.0 };
        let radius = if self.large { 16.0 } else { 14.0 };
        let pad = if self.large {
            Gaps::new(13.0, 24.0, 13.0, 24.0)
        } else {
            Gaps::new(10.0, 18.0, 10.0, 18.0)
        };
        let font_sz = if self.large { 16.5 } else { 14.5 };

        let disabled = self.disabled;
        let primary = self.primary;
        let on_click = self.on_click.clone();
        let on_key_click = self.on_click.clone();

        let lift_y = -1.5 * hover_t + 1.5 * press_t;
        let scale = 1.0 - 0.02 * press_t;

        let mut inner = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(!disabled)
            .a11y_enabled(!disabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.text.clone())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .main_align(Alignment::center())
            .min_height(Size::px(min_h))
            .padding(pad)
            .spacing(10.0)
            .corner_radius(CornerRadius::new_all(radius))
            .background(bg)
            .border(
                Border::new()
                    .width(if self.primary || self.ghost { 0.0 } else { 1.5 })
                    .alignment(BorderAlignment::Inner)
                    .fill(border_col),
            )
            .opacity(if disabled { 0.38 } else { 1.0 })
            .offset_y(lift_y)
            .scale(scale)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |_| {
                if !disabled {
                    on_click.call(());
                }
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if !disabled {
                    // For primary CTA, Enter is dispatched globally by handle_onboarding_key_inner so only Space activates
                    // locally (prevents double-advancing when the primary button is auto-focused).
                    let should_activate = if primary {
                        matches!(&e.key, Key::Character(c) if c == " ")
                    } else {
                        is_activate_key(&e.key)
                    };
                    if should_activate {
                        e.stop_propagation();
                        on_key_click.call(());
                    }
                }
            })
            .child(
                label()
                    .text(self.text.clone())
                    .font_size(font_sz)
                    .font_weight(FontWeight::BOLD)
                    .color(fg),
            )
            .maybe_child(self.key_hint.clone().map(|hint| {
                rect()
                    .padding(Gaps::new(3.0, 7.0, 3.0, 7.0))
                    .corner_radius(CornerRadius::new_all(7.0))
                    .background(if self.primary {
                        pal.primary_key_bg()
                    } else {
                        pal.raised.to_color()
                    })
                    .border(
                        Border::new()
                            .width(if self.primary { 0.0 } else { 1.0 })
                            .alignment(BorderAlignment::Inner)
                            .fill(pal.border.to_color()),
                    )
                    .child(
                        label()
                            .text(hint)
                            .font_size(11.0)
                            .font_weight(FontWeight::BOLD)
                            .font_family("monospace")
                            .color(fg),
                    )
            }));

        if self.primary {
            let shadow_y = lerp(4.0, 7.0, hover_t);
            let shadow_blur = lerp(14.0, 20.0, hover_t);
            inner = inner.shadow(
                Shadow::new()
                    .y(shadow_y)
                    .blur(shadow_blur)
                    .color(Color::from_argb(60, 0, 0, 0)),
            );
        }
        if !disabled {
            inner = inner.cursor(CursorIcon::Pointer);
        }

        let show_ring = !disabled && focus() == Focus::Keyboard;
        rect()
            .padding(Gaps::new_all(3.0))
            .corner_radius(CornerRadius::new_all(radius + 3.0))
            .border(onboarding_focus_ring(show_ring, accent))
            .child(inner)
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/* =========================================================================
Choice Controls — Radio Option Cards, YesNo, Agents, Move In, Apps
========================================================================= */

#[derive(Clone, PartialEq)]
struct RadioOptionCard {
    pal: OnboardingPalette,
    accent: Rgba,
    checked: bool,
    compact: bool,
    bold_prefix: Option<String>,
    label_text: String,
    icon_el: Option<Element>,
    trailing_text: Option<String>,
    view_state: OnboardingViewState,
    on_choose: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for RadioOptionCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for RadioOptionCard {
    fn render(&self) -> impl IntoElement {
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        if focus() != Focus::Not {
            self.view_state.note_non_primary_focus(a11y_id);
        }
        let hover_t = fb.hover_progress();
        let press_t = fb.press_progress();
        let pal = self.pal;
        let accent = self.accent;

        let bg = if self.checked {
            mix_rgba(pal.surface, accent, 0.13)
        } else {
            pal.bg.mix(mix_rgba(pal.bg, accent, 0.05), hover_t)
        };
        let border = if self.checked {
            accent
        } else {
            pal.border.mix(mix_rgba(pal.border, accent, 0.65), hover_t)
        };

        let min_h = if self.compact { 40.0 } else { 54.0 };
        let pad = if self.compact {
            Gaps::new(6.0, 11.0, 6.0, 11.0)
        } else {
            Gaps::new(12.0, 16.0, 12.0, 16.0)
        };
        let gap = if self.compact { 9.0 } else { 12.0 };
        let radius = if self.compact { 11.0 } else { 15.0 };
        let font_sz = if self.compact { 13.5 } else { 15.0 };

        let on_choose = self.on_choose.clone();
        let on_key_choose = self.on_choose.clone();

        // Radio indicator circle (with inner ring dot when checked)
        let radio_el = rect()
            .width(Size::px(19.0))
            .height(Size::px(19.0))
            .corner_radius(CornerRadius::new_all(10.0))
            .background(pal.surface.to_color())
            .border(
                Border::new()
                    .width(2.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(if self.checked {
                        accent.to_color()
                    } else {
                        pal.border.to_color()
                    }),
            )
            .center()
            .maybe_child(self.checked.then(|| {
                rect()
                    .width(Size::px(9.0))
                    .height(Size::px(9.0))
                    .corner_radius(CornerRadius::new_all(5.0))
                    .background(accent.to_color())
            }));

        let text_el: Element = if let Some(prefix) = &self.bold_prefix {
            paragraph()
                .max_lines(2)
                .span(
                    Span::new(prefix.clone())
                        .font_size(font_sz)
                        .font_weight(FontWeight::BOLD)
                        .color(pal.text.to_color()),
                )
                .span(
                    Span::new(self.label_text.clone())
                        .font_size(font_sz)
                        .color(pal.text.to_color()),
                )
                .into_element()
        } else {
            label()
                .text(self.label_text.clone())
                .font_size(font_sz)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .color(pal.text.to_color())
                .into_element()
        };

        let mut card = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::RadioButton)
            .a11y_alt(self.label_text.clone())
            .width(Size::fill())
            .min_height(Size::px(min_h))
            .padding(pad)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(gap)
            .corner_radius(CornerRadius::new_all(radius))
            .background(bg.to_color())
            .border(
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(border.to_color()),
            )
            .scale(1.0 - 0.015 * press_t)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |_| on_choose.call(()))
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if is_activate_key(&e.key) {
                    e.stop_propagation();
                    on_key_choose.call(());
                }
            })
            .child(radio_el)
            .maybe_child(self.icon_el.clone().map(|icon| {
                rect()
                    .width(Size::px(28.0))
                    .height(Size::px(28.0))
                    .corner_radius(CornerRadius::new_all(9.0))
                    .background(Color::WHITE)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(Color::from_argb(36, 0x1a, 0x1b, 0x26)),
                    )
                    .center()
                    .child(icon)
            }))
            .child(rect().width(Size::flex(1.0)).overflow(Overflow::Clip).child(text_el))
            .maybe_child(self.trailing_text.clone().filter(|s| !s.is_empty()).map(|ver| {
                label()
                    .text(ver)
                    .font_size(12.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .font_family("monospace")
                    .color(pal.muted.to_color())
            }));

        if self.checked {
            card = card.shadow(
                Shadow::new()
                    .spread(1.0)
                    .blur(0.0)
                    .color(accent.with_alpha(0.35).to_color()),
            );
        }

        rect()
            .width(Size::fill())
            .padding(Gaps::new_all(2.0))
            .corner_radius(CornerRadius::new_all(radius + 2.0))
            .border(onboarding_focus_ring(focus() == Focus::Keyboard, accent))
            .child(card)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct YesNoChoices {
    pal: OnboardingPalette,
    accent: Rgba,
    yes_bold: &'static str,
    yes_rest: &'static str,
    no_bold: &'static str,
    no_rest: &'static str,
    value: bool,
    view_state: OnboardingViewState,
    on_choose: EventHandler<bool>,
    key: DiffKey,
}

impl KeyExt for YesNoChoices {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for YesNoChoices {
    fn render(&self) -> impl IntoElement {
        let on_yes = self.on_choose.clone();
        let on_no = self.on_choose.clone();
        rect()
            .width(Size::fill())
            .spacing(8.0)
            .child(RadioOptionCard {
                pal: self.pal,
                accent: self.accent,
                checked: self.value,
                compact: false,
                bold_prefix: Some(self.yes_bold.to_string()),
                label_text: self.yes_rest.to_string(),
                icon_el: None,
                trailing_text: None,
                view_state: self.view_state,
                on_choose: EventHandler::new(move |_| on_yes.call(true)),
                key: DiffKey::from(&("opt-yes")),
            })
            .child(RadioOptionCard {
                pal: self.pal,
                accent: self.accent,
                checked: !self.value,
                compact: false,
                bold_prefix: Some(self.no_bold.to_string()),
                label_text: self.no_rest.to_string(),
                icon_el: None,
                trailing_text: None,
                view_state: self.view_state,
                on_choose: EventHandler::new(move |_| on_no.call(false)),
                key: DiffKey::from(&("opt-no")),
            })
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct InstallCommandPill {
    pal: OnboardingPalette,
    accent: Rgba,
    command: String,
    view_state: OnboardingViewState,
    key: DiffKey,
}

impl KeyExt for InstallCommandPill {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for InstallCommandPill {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let mut copied = use_state(|| false);
        if focus() != Focus::Not {
            self.view_state.note_non_primary_focus(a11y_id);
        }

        let pal = self.pal;
        let accent = self.accent;
        let is_copied = *copied.read();
        let hover_t = fb.hover_progress();
        let press_t = fb.press_progress();

        let cmd_text = self.command.clone();
        let mut copy_action = move || {
            let _ = freya::clipboard::Clipboard::set(cmd_text.clone());
            audio::play_select(true);
            copied.set(true);
            core.invoke_or_log(Command::ToastShow {
                message: "Install command copied".to_string(),
                timeout_ms: Some(1800),
                action_label: None,
                action_command: None,
            });
            spawn(async move {
                timer(Duration::from_millis(1600)).await;
                copied.set_if_modified(false);
            });
        };
        let mut copy_key = copy_action.clone();

        let bg = if is_copied {
            mix_rgba(pal.raised, accent, 0.22)
        } else {
            pal.raised.mix(mix_rgba(pal.raised, accent, 0.12), hover_t)
        };
        let border = if is_copied {
            accent
        } else {
            pal.border_subtle.mix(accent, hover_t)
        };

        rect()
            .width(Size::flex(1.0))
            .padding(Gaps::new_all(2.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .border(onboarding_focus_ring(focus() == Focus::Keyboard, accent))
            .child(
                rect()
                    .a11y_id(a11y_id)
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(format!("Copy install command {}", self.command))
                    .width(Size::fill())
                    .height(Size::px(28.0))
                    .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .main_align(Alignment::SpaceBetween)
                    .spacing(8.0)
                    .corner_radius(CornerRadius::new_all(6.0))
                    .background(bg.to_color())
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(border.to_color()),
                    )
                    .scale(1.0 - 0.02 * press_t)
                    .overflow(Overflow::Clip)
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(fb.on_pointer_enter())
                    .on_pointer_leave(fb.on_pointer_leave())
                    .on_pointer_down(fb.on_pointer_down())
                    .on_global_pointer_up(fb.on_pointer_up())
                    .on_press(move |_| copy_action())
                    .on_key_down(move |e: Event<KeyboardEventData>| {
                        if is_activate_key(&e.key) {
                            e.stop_propagation();
                            copy_key();
                        }
                    })
                    .child(
                        rect().width(Size::flex(1.0)).overflow(Overflow::Clip).child(
                            label()
                                .text(self.command.clone())
                                .font_size(12.0)
                                .font_family("monospace")
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis)
                                .color(if is_copied {
                                    pal.text.to_color()
                                } else {
                                    pal.muted.mix(pal.text, hover_t).to_color()
                                }),
                        ),
                    )
                    .child(
                        label()
                            .text(if is_copied { "Copied" } else { "Copy" })
                            .font_size(11.0)
                            .font_weight(FontWeight::BOLD)
                            .color(if is_copied {
                                accent.to_color()
                            } else {
                                pal.muted.to_color()
                            }),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct AgentChoicesPanel {
    pal: OnboardingPalette,
    accent: Rgba,
    onboarding: OnboardingState,
    view_state: OnboardingViewState,
    key: DiffKey,
}

impl KeyExt for AgentChoicesPanel {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for AgentChoicesPanel {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let pal = self.pal;
        let selected_id = self.onboarding.choices.agent_id.as_deref();
        let probing = self.onboarding.probing_agents;
        let installed: Vec<_> = self.onboarding.agent_catalog.iter().filter(|a| a.installed).collect();
        let missing: Vec<_> = self.onboarding.agent_catalog.iter().filter(|a| !a.installed).collect();

        let missing_dash_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                draw_dashed_rrect_stroke(
                    canvas,
                    sk::Rect::from_xywh(0.0, 0.0, ctx.size.width, ctx.size.height),
                    14.0,
                    1.5,
                    &[5.0, 4.0],
                    sk_alpha(pal.border_subtle, 0.95),
                );
            }
        });

        rect()
            .width(Size::fill())
            .spacing(12.0)
            .maybe_child(probing.then(|| {
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(12.0, 14.0, 12.0, 14.0))
                    .corner_radius(CornerRadius::new_all(14.0))
                    .background(pal.bg.to_color())
                    .border(
                        Border::new()
                            .width(1.5)
                            .alignment(BorderAlignment::Inner)
                            .fill(pal.border_subtle.to_color()),
                    )
                    .child(
                        label()
                            .text("Looking for agents on this computer…")
                            .font_size(14.0)
                            .color(pal.muted.to_color()),
                    )
            }))
            .maybe_child((!probing && installed.is_empty()).then(|| {
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(12.0, 14.0, 12.0, 14.0))
                    .corner_radius(CornerRadius::new_all(14.0))
                    .background(pal.bg.to_color())
                    .border(
                        Border::new()
                            .width(1.5)
                            .alignment(BorderAlignment::Inner)
                            .fill(pal.border_subtle.to_color()),
                    )
                    .child(
                        label()
                            .text("No agent found yet. Install one, then press Space A S to pick it.")
                            .font_size(14.0)
                            .color(pal.muted.to_color()),
                    )
            }))
            .maybe_child((!probing && !installed.is_empty()).then(|| {
                rect()
                    .width(Size::fill())
                    .spacing(7.0)
                    .children(installed.into_iter().map(|agent| {
                        let agent_id = agent.id.clone();
                        let core = core.clone();
                        let view_state = self.view_state;
                        RadioOptionCard {
                            pal,
                            accent: self.accent,
                            checked: selected_id == Some(agent.id.as_str()),
                            compact: false,
                            bold_prefix: None,
                            label_text: agent.name.clone(),
                            icon_el: Some(agent_logo_element(&agent.id, 18.0, false)),
                            trailing_text: (!agent.version.is_empty()).then(|| agent.version.clone()),
                            view_state,
                            on_choose: EventHandler::new(move |_| {
                                audio::play_select(true);
                                view_state.bump_nudge();
                                core.invoke_or_log(Command::OnboardingSetAgent {
                                    agent_id: agent_id.clone(),
                                });
                            }),
                            key: DiffKey::from(&(agent.id.clone())),
                        }
                    }))
            }))
            .maybe_child((!probing && !missing.is_empty()).then(|| {
                rect()
                    .width(Size::fill())
                    .margin(Gaps::new(4.0, 0.0, 0.0, 0.0))
                    .padding(Gaps::new(12.0, 14.0, 12.0, 14.0))
                    .spacing(8.0)
                    .corner_radius(CornerRadius::new_all(14.0))
                    .background(Fill::Callback(missing_dash_cb))
                    .child(
                        label()
                            .text("Not installed")
                            .font_size(12.0)
                            .font_weight(FontWeight::BOLD)
                            .color(pal.muted.to_color()),
                    )
                    .children(missing.into_iter().map(|agent| {
                        rect()
                            .key(agent.id.clone())
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(9.0)
                            .child(
                                rect()
                                    .width(Size::px(22.0))
                                    .height(Size::px(22.0))
                                    .corner_radius(CornerRadius::new_all(7.0))
                                    .background(Color::WHITE)
                                    .border(
                                        Border::new()
                                            .width(1.0)
                                            .alignment(BorderAlignment::Inner)
                                            .fill(Color::from_argb(36, 0x1a, 0x1b, 0x26)),
                                    )
                                    .center()
                                    .child(agent_logo_element(&agent.id, 14.0, true)),
                            )
                            .child(
                                label()
                                    .text(agent.name.clone())
                                    .font_size(13.0)
                                    .font_weight(FontWeight::BOLD)
                                    .color(pal.muted.to_color()),
                            )
                            .maybe_child(agent.install_command.clone().map(|cmd| InstallCommandPill {
                                pal,
                                accent: self.accent,
                                command: cmd,
                                view_state: self.view_state,
                                key: DiffKey::from(&(format!("install-{}", agent.id))),
                            }))
                    }))
            }))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct MoveInProfilePill {
    pal: OnboardingPalette,
    accent: Rgba,
    name: String,
    checked: bool,
    source_key: String,
    profile_id: String,
    view_state: OnboardingViewState,
    key: DiffKey,
}

impl KeyExt for MoveInProfilePill {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for MoveInProfilePill {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        if focus() != Focus::Not {
            self.view_state.note_non_primary_focus(a11y_id);
        }
        let hover_t = fb.hover_progress();
        let press_t = fb.press_progress();
        let pal = self.pal;
        let accent = self.accent;
        let checked = self.checked;
        let bg = if checked {
            mix_rgba(pal.surface, accent, 0.14)
        } else {
            pal.surface.mix(mix_rgba(pal.surface, accent, 0.08), hover_t)
        };
        let border = if checked {
            accent
        } else {
            pal.border.mix(accent, hover_t)
        };

        let src_k = self.source_key.clone();
        let prof_id = self.profile_id.clone();
        let view_state = self.view_state;
        let select = move || {
            audio::play_select(true);
            view_state.bump_nudge();
            core.invoke_or_log(Command::OnboardingSetImport {
                source_key: Some(src_k.clone()),
                profile_id: Some(prof_id.clone()),
            });
        };
        let select_key = select.clone();

        rect()
            .padding(Gaps::new_all(2.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .border(onboarding_focus_ring(focus() == Focus::Keyboard, accent))
            .child(
                rect()
                    .a11y_id(a11y_id)
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::RadioButton)
                    .a11y_alt(self.name.clone())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .min_height(Size::px(30.0))
                    .padding(Gaps::new(4.0, 11.0, 4.0, 11.0))
                    .spacing(7.0)
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(bg.to_color())
                    .border(
                        Border::new()
                            .width(1.5)
                            .alignment(BorderAlignment::Inner)
                            .fill(border.to_color()),
                    )
                    .scale(1.0 - 0.025 * press_t)
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(fb.on_pointer_enter())
                    .on_pointer_leave(fb.on_pointer_leave())
                    .on_pointer_down(fb.on_pointer_down())
                    .on_global_pointer_up(fb.on_pointer_up())
                    .on_press(move |_| select())
                    .on_key_down(move |e: Event<KeyboardEventData>| {
                        if is_activate_key(&e.key) {
                            e.stop_propagation();
                            select_key();
                        }
                    })
                    .child(
                        rect()
                            .width(Size::px(9.0))
                            .height(Size::px(9.0))
                            .corner_radius(CornerRadius::new_all(5.0))
                            .background(if checked { accent.to_color() } else { Color::TRANSPARENT })
                            .border(
                                Border::new()
                                    .width(1.8)
                                    .alignment(BorderAlignment::Inner)
                                    .fill(if checked {
                                        accent.to_color()
                                    } else {
                                        pal.border.to_color()
                                    }),
                            ),
                    )
                    .child(
                        label()
                            .text(self.name.clone())
                            .font_size(12.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(pal.text.to_color()),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct MoveInChoicesPanel {
    pal: OnboardingPalette,
    accent: Rgba,
    onboarding: OnboardingState,
    focus: MoveInFocus,
    narrow: bool,
    view_state: OnboardingViewState,
    key: DiffKey,
}

impl KeyExt for MoveInChoicesPanel {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for MoveInChoicesPanel {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let pal = self.pal;
        let accent = self.accent;
        let choices = &self.onboarding.choices;
        let view_state = self.view_state;

        let fieldset_bg = mix_rgba(pal.surface, pal.raised, 0.42);
        let fieldset_border = |active: bool| {
            if active {
                mix_rgba(pal.border_subtle, accent, 0.55)
            } else {
                pal.border_subtle
            }
        };

        let engines = onboarding_search_engines();
        let engine_rows: Vec<Element> = engines
            .chunks(2)
            .enumerate()
            .map(|(row_idx, chunk)| {
                rect()
                    .key(row_idx)
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .spacing(4.0)
                    .children((0..2).map(|col| {
                        if let Some(engine) = chunk.get(col) {
                            let name = engine.name.clone();
                            let core = core.clone();
                            rect()
                                .key(engine.name.clone())
                                .width(Size::flex(1.0))
                                .child(RadioOptionCard {
                                    pal,
                                    accent,
                                    checked: choices.search_engine.eq_ignore_ascii_case(&engine.name),
                                    compact: true,
                                    bold_prefix: None,
                                    label_text: engine.name.clone(),
                                    icon_el: Some(brand_logo_element(&engine.name, 18.0)),
                                    trailing_text: None,
                                    view_state,
                                    on_choose: EventHandler::new(move |_| {
                                        let mut mf = view_state.move_focus;
                                        mf.set_if_modified(MoveInFocus::Search);
                                        audio::play_select(true);
                                        view_state.bump_nudge();
                                        core.invoke_or_log(Command::OnboardingSetSearchEngine { name: name.clone() });
                                    }),
                                    key: DiffKey::from(&(format!("eng-{}", engine.name))),
                                })
                                .into_element()
                        } else {
                            rect()
                                .key(format!("eng-pad-{row_idx}-{col}"))
                                .width(Size::flex(1.0))
                                .into_element()
                        }
                    }))
                    .into_element()
            })
            .collect();

        rect()
            .width(Size::fill())
            .spacing(10.0)
            .child(
                // 1. Bring from fieldset
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(10.0, 12.0, 10.0, 12.0))
                    .spacing(6.0)
                    .corner_radius(CornerRadius::new_all(16.0))
                    .background(fieldset_bg.to_color())
                    .border(
                        Border::new()
                            .width(1.5)
                            .alignment(BorderAlignment::Inner)
                            .fill(fieldset_border(self.focus == MoveInFocus::Import).to_color()),
                    )
                    .on_pointer_down(move |_| {
                        let mut mf = view_state.move_focus;
                        mf.set_if_modified(MoveInFocus::Import);
                    })
                    .child(
                        label()
                            .text("Bring from")
                            .font_size(12.0)
                            .font_weight(FontWeight::BOLD)
                            .color(pal.muted.to_color()),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .spacing(4.0)
                            .child(RadioOptionCard {
                                pal,
                                accent,
                                checked: choices.import_source_key.is_none(),
                                compact: true,
                                bold_prefix: None,
                                label_text: "Nothing, start fresh".to_string(),
                                icon_el: None,
                                trailing_text: None,
                                view_state,
                                on_choose: EventHandler::new({
                                    let core = core.clone();
                                    move |_| {
                                        let mut mf = view_state.move_focus;
                                        mf.set_if_modified(MoveInFocus::Import);
                                        audio::play_select(false);
                                        view_state.bump_nudge();
                                        core.invoke_or_log(Command::OnboardingSetImport {
                                            source_key: None,
                                            profile_id: None,
                                        });
                                    }
                                }),
                                key: DiffKey::from(&("import-none")),
                            })
                            .children(self.onboarding.import_sources.iter().map(|source| {
                                let is_selected = choices.import_source_key.as_deref() == Some(source.key.as_str());
                                let key_str = source.key.clone();
                                let default_prof = source.profiles.first().map(|p| p.id.clone());
                                let core_src = core.clone();
                                let show_profiles = is_selected && source.profiles.len() > 1;
                                rect()
                                    .key(source.key.clone())
                                    .width(Size::fill())
                                    .spacing(4.0)
                                    .child(RadioOptionCard {
                                        pal,
                                        accent,
                                        checked: is_selected,
                                        compact: true,
                                        bold_prefix: None,
                                        label_text: source.name.clone(),
                                        icon_el: Some(brand_logo_element(&source.icon, 18.0)),
                                        trailing_text: None,
                                        view_state,
                                        on_choose: EventHandler::new(move |_| {
                                            let mut mf = view_state.move_focus;
                                            mf.set_if_modified(MoveInFocus::Import);
                                            audio::play_select(true);
                                            view_state.bump_nudge();
                                            core_src.invoke_or_log(Command::OnboardingSetImport {
                                                source_key: Some(key_str.clone()),
                                                profile_id: default_prof.clone(),
                                            });
                                        }),
                                        key: DiffKey::from(&(format!("imp-{}", source.key))),
                                    })
                                    .maybe_child(show_profiles.then(|| {
                                        rect()
                                            .width(Size::fill())
                                            .padding(Gaps::new(2.0, 0.0, 2.0, 28.0))
                                            .direction(Direction::Horizontal)
                                            .content(Content::wrap())
                                            .spacing(4.0)
                                            .children(source.profiles.iter().map(|prof| MoveInProfilePill {
                                                pal,
                                                accent,
                                                name: prof.name.clone(),
                                                checked: choices.import_profile_id.as_deref() == Some(prof.id.as_str()),
                                                source_key: source.key.clone(),
                                                profile_id: prof.id.clone(),
                                                view_state,
                                                key: DiffKey::from(&(prof.id.clone())),
                                            }))
                                    }))
                            })),
                    ),
            )
            .child(
                // 2. Search with fieldset
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(10.0, 12.0, 10.0, 12.0))
                    .spacing(6.0)
                    .corner_radius(CornerRadius::new_all(16.0))
                    .background(fieldset_bg.to_color())
                    .border(
                        Border::new()
                            .width(1.5)
                            .alignment(BorderAlignment::Inner)
                            .fill(fieldset_border(self.focus == MoveInFocus::Search).to_color()),
                    )
                    .on_pointer_down(move |_| {
                        let mut mf = view_state.move_focus;
                        mf.set_if_modified(MoveInFocus::Search);
                    })
                    .child(
                        label()
                            .text("Search with")
                            .font_size(12.0)
                            .font_weight(FontWeight::BOLD)
                            .color(pal.muted.to_color()),
                    )
                    .child(rect().width(Size::fill()).spacing(4.0).children(engine_rows)),
            )
            .child(
                // 3. Links from other apps (default browser) fieldset
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(10.0, 12.0, 10.0, 12.0))
                    .spacing(6.0)
                    .corner_radius(CornerRadius::new_all(16.0))
                    .background(fieldset_bg.to_color())
                    .border(
                        Border::new()
                            .width(1.5)
                            .alignment(BorderAlignment::Inner)
                            .fill(fieldset_border(self.focus == MoveInFocus::DefaultBrowser).to_color()),
                    )
                    .on_pointer_down(move |_| {
                        let mut mf = view_state.move_focus;
                        mf.set_if_modified(MoveInFocus::DefaultBrowser);
                    })
                    .child(
                        label()
                            .text("Links from other apps")
                            .font_size(12.0)
                            .font_weight(FontWeight::BOLD)
                            .color(pal.muted.to_color()),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(if self.narrow {
                                Direction::Vertical
                            } else {
                                Direction::Horizontal
                            })
                            .content(Content::Flex)
                            .spacing(6.0)
                            .child(
                                rect()
                                    .width(if self.narrow { Size::fill() } else { Size::flex(1.0) })
                                    .child(RadioOptionCard {
                                        pal,
                                        accent,
                                        checked: choices.default_browser,
                                        compact: true,
                                        bold_prefix: None,
                                        label_text: "Open in Zendia".to_string(),
                                        icon_el: None,
                                        trailing_text: None,
                                        view_state,
                                        on_choose: EventHandler::new({
                                            let core = core.clone();
                                            move |_| {
                                                let mut mf = view_state.move_focus;
                                                mf.set_if_modified(MoveInFocus::DefaultBrowser);
                                                audio::play_select(true);
                                                view_state.bump_nudge();
                                                core.invoke_or_log(Command::OnboardingSetDefaultBrowser {
                                                    enabled: true,
                                                });
                                            }
                                        }),
                                        key: DiffKey::from(&("def-yes")),
                                    }),
                            )
                            .child(
                                rect()
                                    .width(if self.narrow { Size::fill() } else { Size::flex(1.0) })
                                    .child(RadioOptionCard {
                                        pal,
                                        accent,
                                        checked: !choices.default_browser,
                                        compact: true,
                                        bold_prefix: None,
                                        label_text: "Leave as they are".to_string(),
                                        icon_el: None,
                                        trailing_text: None,
                                        view_state,
                                        on_choose: EventHandler::new({
                                            let core = core.clone();
                                            move |_| {
                                                let mut mf = view_state.move_focus;
                                                mf.set_if_modified(MoveInFocus::DefaultBrowser);
                                                audio::play_select(false);
                                                view_state.bump_nudge();
                                                core.invoke_or_log(Command::OnboardingSetDefaultBrowser {
                                                    enabled: false,
                                                });
                                            }
                                        }),
                                        key: DiffKey::from(&("def-no")),
                                    }),
                            ),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct AppsChoicesPanel {
    pal: OnboardingPalette,
    selected_urls: Vec<String>,
    view_state: OnboardingViewState,
    key: DiffKey,
}

impl KeyExt for AppsChoicesPanel {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for AppsChoicesPanel {
    fn render(&self) -> impl IntoElement {
        let count = self.selected_urls.len();
        rect()
            .width(Size::fill())
            .spacing(8.0)
            .children(ESSENTIAL_APP_PRESETS.chunks(3).enumerate().map(|(row_idx, chunk)| {
                rect()
                    .key(row_idx)
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .spacing(8.0)
                    .children(chunk.iter().map(|app| {
                        let selected = self.selected_urls.iter().any(|u| u.eq_ignore_ascii_case(app.url));
                        EssentialAppCard {
                            pal: self.pal,
                            app,
                            selected,
                            selected_count: count,
                            view_state: self.view_state,
                            key: DiffKey::from(&(app.id)),
                        }
                    }))
            }))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct EssentialAppCard {
    pal: OnboardingPalette,
    app: &'static EssentialAppPreset,
    selected: bool,
    selected_count: usize,
    view_state: OnboardingViewState,
    key: DiffKey,
}

impl KeyExt for EssentialAppCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for EssentialAppCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        if focus() != Focus::Not {
            self.view_state.note_non_primary_focus(a11y_id);
        }
        let hover_t = fb.hover_progress();
        let press_t = fb.press_progress();
        let pal = self.pal;
        let app_color = Rgba::rgb(self.app.color_rgb.0, self.app.color_rgb.1, self.app.color_rgb.2);

        let bg = if self.selected {
            mix_rgba(pal.surface, app_color, 0.16)
        } else {
            pal.bg.mix(mix_rgba(pal.bg, app_color, 0.06), hover_t)
        };
        let border = if self.selected {
            app_color
        } else {
            pal.border.mix(mix_rgba(pal.border, app_color, 0.70), hover_t)
        };

        let url = self.app.url.to_string();
        let was_selected = self.selected;
        let count = self.selected_count;
        let view_state = self.view_state;

        let toggle = move || {
            let next_selected = !was_selected;
            let next_count = if next_selected { count + 1 } else { count.max(1) };
            audio::play_app_pin(next_selected, next_count);
            view_state.bump_nudge();
            core.invoke_or_log(Command::OnboardingToggleEssential { url: url.clone() });
        };
        let toggle_key = toggle.clone();

        let lift_y = -2.0 * hover_t + 1.5 * press_t;
        let scale = 1.0 - 0.03 * press_t;

        let mut card = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::CheckBox)
            .a11y_alt(self.app.name)
            .width(Size::fill())
            .height(Size::px(86.0))
            .padding(Gaps::new(10.0, 8.0, 10.0, 8.0))
            .spacing(6.0)
            .cross_align(Alignment::center())
            .main_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(17.0))
            .background(bg.to_color())
            .border(
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(border.to_color()),
            )
            .offset_y(lift_y)
            .scale(scale)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |_| toggle())
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if is_activate_key(&e.key) {
                    e.stop_propagation();
                    toggle_key();
                }
            })
            .child(
                rect()
                    .width(Size::px(36.0))
                    .height(Size::px(36.0))
                    .corner_radius(CornerRadius::new_all(11.0))
                    .background(Color::WHITE)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(Color::from_argb(30, 0x1a, 0x1b, 0x26)),
                    )
                    .center()
                    .child(brand_logo_element(self.app.icon, 20.0)),
            )
            .child(
                label()
                    .text(self.app.name)
                    .font_size(12.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .color(pal.text.to_color()),
            );

        if self.selected {
            card = card.shadow(
                Shadow::new()
                    .spread(1.0)
                    .blur(0.0)
                    .color(app_color.with_alpha(0.38).to_color()),
            );
        }

        rect()
            .width(Size::flex(1.0))
            .padding(Gaps::new_all(2.0))
            .corner_radius(CornerRadius::new_all(19.0))
            .border(onboarding_focus_ring(focus() == Focus::Keyboard, app_color))
            .child(card)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
