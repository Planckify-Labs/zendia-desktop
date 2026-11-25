/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Full-window seven-page native onboarding UI, directional chapter transitions, keyboard navigation, and overlay host.

use crate::ui::agent::agent_logo_element;
use crate::ui::core_view::{CoreView, use_core};
use crate::ui::onboarding::audio;
use crate::ui::onboarding::browser_scene::paint_ambient_field;
use crate::ui::onboarding::logos::{brand_logo_element, sound_icon};
use crate::ui::onboarding::mascot::OnboardingMascot;
use crate::ui::onboarding::scenes::{
    AgentScene, AppsScene, GoScene, MoveInScene, ShieldScene, StageShell, StartEmblemScene, VimScene,
};
use crate::ui::onboarding::theme::{MODE_ORDER, OnboardingPalette, lerp, mix_rgba};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, Rgba, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::InputMode;
use zendia_browser_core::onboarding::{
    ESSENTIAL_APP_PRESETS, EssentialAppPreset, MoveInFocus, OnboardingPage, OnboardingState, onboarding_search_engines,
};

/// View-local state for the window's onboarding overlay (animations, preview pause, audio mute, Move in fieldset focus).
#[derive(Clone, Copy, PartialEq)]
pub struct OnboardingViewState {
    pub open: State<bool>,
    pub muted: State<bool>,
    pub paused: State<bool>,
    pub start_pose: State<InputMode>,
    pub move_focus: State<MoveInFocus>,
    pub nudge_seq: State<u64>,
    pub transition_seq: State<u64>,
}

impl OnboardingViewState {
    pub fn is_open(&self) -> bool {
        *self.open.peek()
    }

    pub fn open(&self) {
        let mut open = self.open;
        let mut paused = self.paused;
        open.set_if_modified(true);
        paused.set_if_modified(false);
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

    pub fn set_muted(&self, muted: bool) {
        let mut m = self.muted;
        m.set_if_modified(muted);
        audio::set_muted(muted);
    }

    pub fn reset_for_gallery(&self) {
        let mut open = self.open;
        let mut paused = self.paused;
        let mut start_pose = self.start_pose;
        let mut move_focus = self.move_focus;
        open.set_if_modified(false);
        paused.set_if_modified(false);
        start_pose.set_if_modified(InputMode::Normal);
        move_focus.set_if_modified(MoveInFocus::Import);
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
    let start_pose = use_state(|| InputMode::Normal);
    let move_focus = use_state(|| MoveInFocus::Import);
    let nudge_seq = use_state(|| 0u64);
    let transition_seq = use_state(|| 0u64);

    let state = OnboardingViewState {
        open,
        muted,
        paused,
        start_pose,
        move_focus,
        nudge_seq,
        transition_seq,
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

/// Handles keyboard navigation while onboarding is visible (`Enter`, `Backspace`, `Escape`, arrow keys).
///
/// Returns `true` when the key was consumed by onboarding so browser shortcuts behind the overlay do not fire.
pub fn handle_onboarding_key_down(
    e: &Event<KeyboardEventData>,
    core: &CoreView,
    view_state: &OnboardingViewState,
) -> bool {
    if !view_state.is_open() {
        return false;
    }
    if e.modifiers
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
    {
        return false;
    }

    let onboarding = core.model().peek().onboarding.clone();
    match &e.key {
        Key::Named(NamedKey::Enter) => {
            if onboarding.page == OnboardingPage::Start {
                audio::play_start();
            } else if onboarding.page == OnboardingPage::Go {
                audio::play_finish();
            } else if let Some(next_page) = onboarding.page.next() {
                audio::play_step(next_page.pose(), 1);
            }
            view_state.bump_transition();
            core.invoke_or_log(Command::OnboardingNext);
            true
        }
        Key::Named(NamedKey::Backspace) | Key::Named(NamedKey::ArrowLeft) => {
            if let Some(prev) = onboarding.page.prev_chapter() {
                audio::play_step(prev.pose(), -1);
                view_state.bump_transition();
                core.invoke_or_log(Command::OnboardingBack);
            }
            true
        }
        Key::Named(NamedKey::ArrowRight) => {
            if onboarding.page != OnboardingPage::Go {
                if onboarding.page == OnboardingPage::Start {
                    audio::play_start();
                } else if let Some(next_page) = onboarding.page.next() {
                    audio::play_step(next_page.pose(), 1);
                }
                view_state.bump_transition();
                core.invoke_or_log(Command::OnboardingNext);
            }
            true
        }
        Key::Named(NamedKey::Escape) => {
            core.invoke_or_log(Command::OnboardingSkip);
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
#[derive(Clone, PartialEq)]
pub struct OnboardingOverlay;

impl Component for OnboardingOverlay {
    fn render(&self) -> impl IntoElement {
        let Some(view_state) = try_use_onboarding_state() else {
            return rect().into_element();
        };
        if !*view_state.open.read() {
            return rect().into_element();
        }
        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
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

        let onboarding = core.model().read().onboarding.clone();
        let page = onboarding.page;
        let active_mode = if page == OnboardingPage::Start {
            self.start_pose_override
                .unwrap_or_else(|| *view_state.start_pose.read())
        } else {
            page.pose()
        };
        let mode_accent = pal.mode_accent(active_mode);
        let mode_pastel = OnboardingPalette::mode_pastel(active_mode);

        // Directional chapter entrance animation (`380 ms`, ease-out `cubic-bezier(0.22, 1, 0.36, 1)`).
        let mut last_page = use_state(|| page);
        let mut local_seq = use_state(|| 0u64);
        if *last_page.peek() != page {
            last_page.set(page);
            let next = *local_seq.peek() + 1;
            local_seq.set(next);
        }
        let anim_dep = (*local_seq.read(), *view_state.transition_seq.read());
        let still = self.still;
        let step_anim = use_animation_with_dependencies(&anim_dep, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(if still { OnChange::Finish } else { OnChange::Rerun });
            AnimNum::new(0.0, 1.0)
                .duration(Duration::from_millis(380))
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let trans_p = match self.pinned_transition {
            Some(p) => p.clamp(0.0, 1.0),
            None if self.still => 1.0,
            None => step_anim.get().value().clamp(0.0, 1.0),
        };

        let ambient_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_ambient_field(canvas, ctx.size.width, ctx.size.height, mode_accent, mode_pastel);
            }
        });

        let muted = *view_state.muted.read();
        let paused = *view_state.paused.read();
        let move_focus = self
            .move_focus_override
            .unwrap_or_else(|| *view_state.move_focus.read());
        let nudge_seq = *view_state.nudge_seq.read();

        let body: Element = if page == OnboardingPage::Start {
            StartPageLayout {
                pal,
                pose: active_mode,
                muted,
                still: self.still,
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
                move_focus,
                nudge_seq,
                trans_p,
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

        let on_start = {
            let core = core.clone();
            move |_: ()| {
                audio::play_start();
                view_state.bump_transition();
                core.invoke_or_log(Command::OnboardingNext);
            }
        };

        let on_change_pose = move |next_mode: InputMode| {
            let mut sp = view_state.start_pose;
            sp.set_if_modified(next_mode);
        };

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
                // Centered 2-column hero layout (`max-width: 1180px`)
                rect()
                    .width(Size::fill())
                    .max_width(Size::px(1180.0))
                    .height(Size::fill())
                    .padding(Gaps::new(40.0, 56.0, 40.0, 56.0))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .main_align(Alignment::center())
                    .spacing(64.0)
                    .content(Content::Flex)
                    .child(
                        // Left hero copy + primary CTA
                        rect()
                            .width(Size::flex(1.0))
                            .spacing(20.0)
                            .child(
                                rect()
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(10.0)
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
                                label()
                                    .text("Hands on\nthe keys.")
                                    .font_size(56.0)
                                    .font_weight(FontWeight::EXTRA_BOLD)
                                    .line_height(1.05)
                                    .color(pal.text.to_color()),
                            )
                            .child(
                                rect().max_width(Size::px(450.0)).child(
                                    label()
                                        .text("Zendia is a browser you drive from the keyboard, with your own agent along for the ride and the ads left outside.")
                                        .font_size(17.5)
                                        .line_height(1.56)
                                        .color(pal.muted.to_color()),
                                ),
                            )
                            .child(
                                rect().margin(Gaps::new(8.0, 0.0, 0.0, 0.0)).child(OnboardingButton {
                                    pal,
                                    text: "Set up Zendia".to_string(),
                                    key_hint: Some("Enter".to_string()),
                                    primary: true,
                                    large: true,
                                    ghost: false,
                                    disabled: false,
                                    mode: InputMode::Normal,
                                    focused_ring: false,
                                    on_click: EventHandler::new(on_start),
                                    key: DiffKey::from(&("btn-start")),
                                }),
                            ),
                    )
                    .child(
                        // Right Living Emblem + 6 mode pills
                        rect().width(Size::flex(1.05)).center().child(StartEmblemScene {
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
    move_focus: MoveInFocus,
    nudge_seq: u64,
    trans_p: f32,
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

        // Directional slide offset (`±22px`) and fade for chapter transitions.
        let slide_dir = if onboarding.direction < 0 { -1.0 } else { 1.0 };
        let slide_x = lerp(22.0 * slide_dir, 0.0, self.trans_p);
        let slide_opacity = lerp(0.15, 1.0, self.trans_p);

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

        let on_back = {
            let core = core.clone();
            move |_: ()| {
                if let Some(prev) = page.prev_chapter() {
                    audio::play_step(prev.pose(), -1);
                    view_state.bump_transition();
                    core.invoke_or_log(Command::OnboardingBack);
                }
            }
        };
        let on_skip_apps = {
            let core = core.clone();
            move |_: ()| {
                audio::play_step(OnboardingPage::Go.pose(), 1);
                view_state.bump_transition();
                core.invoke_or_log(Command::OnboardingSkipApps);
            }
        };
        let on_next = {
            let core = core.clone();
            move |_: ()| {
                if is_last_chapter {
                    audio::play_finish();
                } else if let Some(next_p) = page.next() {
                    audio::play_step(next_p.pose(), 1);
                }
                view_state.bump_transition();
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

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .cross_align(Alignment::center())
            .child(
                rect()
                    .width(Size::fill())
                    .max_width(Size::px(1440.0))
                    .height(Size::fill())
                    .padding(Gaps::new(18.0, 32.0, 22.0, 32.0))
                    .spacing(18.0)
                    .content(Content::Flex)
                    .child(
                        // Top rail: Brand lockup + 6-cell chapter stepper + Sound toggle
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(6.0, 8.0, 6.0, 8.0))
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .main_align(Alignment::SpaceBetween)
                            .spacing(16.0)
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
                                                let disabled = chapter.index() > onboarding.reached.index();
                                                ChapterStepPill {
                                                    pal,
                                                    chapter,
                                                    is_current,
                                                    is_done,
                                                    disabled,
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
                    .child(
                        // Main 2-column workspace (`0.94fr` left deck + `1.06fr` right stage)
                        rect()
                            .width(Size::fill())
                            .height(Size::flex(1.0))
                            .direction(Direction::Horizontal)
                            .spacing(32.0)
                            .content(Content::Flex)
                            .child(
                                // Left decision deck
                                rect()
                                    .width(Size::flex(0.94))
                                    .height(Size::fill())
                                    .padding(Gaps::new(26.0, 28.0, 22.0, 28.0))
                                    .spacing(20.0)
                                    .corner_radius(CornerRadius::new_all(26.0))
                                    .background(pal.surface.to_color())
                                    .border(
                                        Border::new()
                                            .width(1.5)
                                            .alignment(BorderAlignment::Inner)
                                            .fill(pal.border_subtle.to_color()),
                                    )
                                    .shadow(deck_shadow)
                                    .content(Content::Flex)
                                    .child(
                                        // Guide header: 82px mascot + mode tag + title + description
                                        rect()
                                            .width(Size::fill())
                                            .direction(Direction::Horizontal)
                                            .cross_align(Alignment::start())
                                            .spacing(18.0)
                                            .child(
                                                OnboardingMascot::new(pose, 82.0)
                                                    .still(self.still)
                                                    .blink(!self.still)
                                                    .nudge_seq(self.nudge_seq)
                                                    .pinned_morph(self.pinned_mascot_morph),
                                            )
                                            .child(
                                                rect()
                                                    .width(Size::flex(1.0))
                                                    .offset_x(slide_x)
                                                    .opacity(slide_opacity)
                                                    .spacing(6.0)
                                                    .child(
                                                        rect()
                                                            .direction(Direction::Horizontal)
                                                            .cross_align(Alignment::center())
                                                            .spacing(6.0)
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
                                                        label()
                                                            .text(headline)
                                                            .font_size(30.0)
                                                            .font_weight(FontWeight::EXTRA_BOLD)
                                                            .line_height(1.12)
                                                            .color(pal.text.to_color()),
                                                    )
                                                    .child(
                                                        label()
                                                            .text(description)
                                                            .font_size(14.5)
                                                            .line_height(1.54)
                                                            .color(pal.muted.to_color()),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        // Scrollable choices body
                                        ScrollView::new().width(Size::fill()).height(Size::flex(1.0)).child(
                                            rect()
                                                .width(Size::fill())
                                                .padding(Gaps::new(2.0, 4.0, 6.0, 2.0))
                                                .offset_x(slide_x)
                                                .opacity(slide_opacity)
                                                .child(choices_el),
                                        ),
                                    )
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
                                                        key_hint: Some("Backspace".to_string()),
                                                        primary: false,
                                                        large: false,
                                                        ghost: false,
                                                        disabled: is_first_chapter,
                                                        mode: pose,
                                                        focused_ring: false,
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
                                                                disabled: false,
                                                                mode: pose,
                                                                focused_ring: false,
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
                                                                key_hint: Some("Enter".to_string()),
                                                                primary: true,
                                                                large: false,
                                                                ghost: false,
                                                                disabled: false,
                                                                mode: pose,
                                                                focused_ring: true,
                                                                on_click: EventHandler::new(on_next),
                                                                key: DiffKey::from(&("btn-next")),
                                                            }),
                                                    ),
                                            ),
                                    ),
                            )
                            .child(
                                // Right framed 2D living stage
                                rect().width(Size::flex(1.06)).height(Size::fill()).child(StageShell {
                                    accent: mode_accent,
                                    still: self.still,
                                    looping: page.is_looping(),
                                    paused: self.paused,
                                    on_toggle_pause: page.is_looping().then(|| EventHandler::new(on_toggle_pause)),
                                    caption: None,
                                    content: rect()
                                        .width(Size::fill())
                                        .height(Size::fill())
                                        .opacity(slide_opacity)
                                        .center()
                                        .child(stage_el)
                                        .into_element(),
                                    key: DiffKey::from(&(format!("stage-{}", page.as_str()))),
                                }),
                            ),
                    ),
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
        let mut hovered = use_state(|| false);
        let is_hovered = *hovered.read() && !self.disabled;

        let pal = self.pal;
        let pose = self.chapter.pose();
        let accent = pal.mode_accent(pose);
        let pastel = OnboardingPalette::mode_pastel(pose);

        let bg = if self.is_current {
            mix_rgba(pal.surface, accent, 0.16).to_color()
        } else if self.disabled {
            Color::TRANSPARENT
        } else {
            pal.surface.to_color()
        };
        let border = if self.is_current || is_hovered {
            accent
        } else if self.is_done {
            mix_rgba(pal.border_subtle, accent, 0.42)
        } else {
            pal.border_subtle
        };
        let label_fg = if self.is_current { pal.text } else { pal.muted };
        let cell_size = if self.is_current { 10.5 } else { 9.0 };

        let chapter = self.chapter;
        let current_page = self.current_page;
        let disabled = self.disabled;
        let view_state = self.view_state;

        let mut pill = rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .min_height(Size::px(36.0))
            .padding(Gaps::new(5.0, 12.0, 5.0, 12.0))
            .spacing(7.0)
            .corner_radius(CornerRadius::new_all(12.0))
            .background(bg)
            .border(
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(border.to_color()),
            )
            .opacity(if disabled { 0.45 } else { 1.0 })
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .on_press(move |_| {
                if !disabled && chapter != current_page {
                    let dir = if chapter.index() > current_page.index() { 1 } else { -1 };
                    audio::play_step(chapter.pose(), dir);
                    view_state.bump_transition();
                    core.invoke_or_log(Command::OnboardingGoto { page: chapter });
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
            .child(
                label()
                    .text(chapter.chapter_label())
                    .font_size(13.0)
                    .font_weight(if self.is_current {
                        FontWeight::BOLD
                    } else {
                        FontWeight::SEMI_BOLD
                    })
                    .color(label_fg.to_color()),
            );
        if !disabled {
            pill = pill.cursor(CursorIcon::Pointer);
        }
        pill
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
        let mut hovered = use_state(|| false);
        let is_hovered = *hovered.read();
        let pal = self.pal;
        let fg = if is_hovered { pal.text } else { pal.muted };
        let border = if is_hovered { self.accent } else { pal.border_subtle };
        let muted = self.muted;
        let pose = self.pose;
        let view_state = self.view_state;

        rect()
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
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .on_press(move |_| {
                let next_muted = !muted;
                view_state.set_muted(next_muted);
                if !next_muted {
                    audio::play_mode(pose);
                }
            })
            .child(sound_icon(muted, 16.0, fg.to_color()))
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
    focused_ring: bool,
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
        let mut hovered = use_state(|| false);
        let is_hovered = *hovered.read() && !self.disabled;
        let pal = self.pal;
        let accent = pal.mode_accent(self.mode);

        let (bg, fg, border_col) = if self.primary {
            let base = pal.primary_bg(self.mode);
            let bg = if is_hovered {
                mix_rgba(base, Rgba::rgb(255, 255, 255), if pal.dark { 0.10 } else { 0.08 })
            } else {
                base
            };
            (bg.to_color(), pal.primary_fg().to_color(), Color::TRANSPARENT)
        } else if self.ghost {
            let bg = if is_hovered {
                pal.raised.to_color()
            } else {
                Color::TRANSPARENT
            };
            let fg = if is_hovered { pal.text } else { pal.muted };
            (bg, fg.to_color(), Color::TRANSPARENT)
        } else {
            let border = if is_hovered {
                mix_rgba(pal.border, accent, 0.60)
            } else {
                pal.border
            };
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
        let on_click = self.on_click.clone();

        let mut inner = rect()
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
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .on_press(move |_| {
                if !disabled {
                    on_click.call(());
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
            inner = inner.shadow(Shadow::new().y(4.0).blur(14.0).color(Color::from_argb(56, 0, 0, 0)));
        }
        if !disabled {
            inner = inner.cursor(CursorIcon::Pointer);
        }

        if self.focused_ring && !disabled {
            // Focus ring in parent padding so it sits cleanly around the primary Next / Start browsing button
            rect()
                .padding(Gaps::new_all(3.0))
                .corner_radius(CornerRadius::new_all(radius + 3.0))
                .border(
                    Border::new()
                        .width(2.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(accent.to_color()),
                )
                .child(inner)
                .into_element()
        } else {
            inner.into_element()
        }
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
        let mut hovered = use_state(|| false);
        let is_hovered = *hovered.read();
        let pal = self.pal;
        let accent = self.accent;

        let bg = if self.checked {
            mix_rgba(pal.surface, accent, 0.13)
        } else {
            pal.bg
        };
        let border = if self.checked {
            accent
        } else if is_hovered {
            mix_rgba(pal.border, accent, 0.65)
        } else {
            pal.border
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

        rect()
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
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .on_press(move |_| on_choose.call(()))
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
            }))
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
            .spacing(10.0)
            .child(RadioOptionCard {
                pal: self.pal,
                accent: self.accent,
                checked: self.value,
                compact: false,
                bold_prefix: Some(self.yes_bold.to_string()),
                label_text: self.yes_rest.to_string(),
                icon_el: None,
                trailing_text: None,
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
                on_choose: EventHandler::new(move |_| on_no.call(false)),
                key: DiffKey::from(&("opt-no")),
            })
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
        let installed: Vec<_> = self.onboarding.agent_catalog.iter().filter(|a| a.installed).collect();
        let missing: Vec<_> = self.onboarding.agent_catalog.iter().filter(|a| !a.installed).collect();

        rect()
            .width(Size::fill())
            .spacing(12.0)
            .maybe_child(installed.is_empty().then(|| {
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
            .child(
                rect()
                    .width(Size::fill())
                    .spacing(9.0)
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
                            on_choose: EventHandler::new(move |_| {
                                audio::play_select(true);
                                view_state.bump_nudge();
                                core.invoke_or_log(Command::OnboardingSetAgent {
                                    agent_id: agent_id.clone(),
                                });
                            }),
                            key: DiffKey::from(&(agent.id.clone())),
                        }
                    })),
            )
            .maybe_child((!missing.is_empty()).then(|| {
                rect()
                    .width(Size::fill())
                    .margin(Gaps::new(4.0, 0.0, 0.0, 0.0))
                    .padding(Gaps::new(12.0, 14.0, 12.0, 14.0))
                    .spacing(8.0)
                    .corner_radius(CornerRadius::new_all(14.0))
                    .border(
                        Border::new()
                            .width(1.5)
                            .alignment(BorderAlignment::Inner)
                            .fill(pal.border_subtle.to_color()),
                    )
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
                            .maybe_child(agent.install_command.clone().map(|cmd| {
                                rect()
                                    .width(Size::flex(1.0))
                                    .padding(Gaps::new(3.0, 7.0, 3.0, 7.0))
                                    .corner_radius(CornerRadius::new_all(6.0))
                                    .background(pal.raised.to_color())
                                    .overflow(Overflow::Clip)
                                    .child(
                                        label()
                                            .text(cmd)
                                            .font_size(12.0)
                                            .font_family("monospace")
                                            .max_lines(1)
                                            .text_overflow(TextOverflow::Ellipsis)
                                            .color(pal.muted.to_color()),
                                    )
                            }))
                    }))
            }))
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
                    .spacing(6.0)
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
                    .spacing(7.0)
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
                            .spacing(6.0)
                            .child(RadioOptionCard {
                                pal,
                                accent,
                                checked: choices.import_source_key.is_none(),
                                compact: true,
                                bold_prefix: None,
                                label_text: "Nothing, start fresh".to_string(),
                                icon_el: None,
                                trailing_text: None,
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
                                    .spacing(6.0)
                                    .child(RadioOptionCard {
                                        pal,
                                        accent,
                                        checked: is_selected,
                                        compact: true,
                                        bold_prefix: None,
                                        label_text: source.name.clone(),
                                        icon_el: Some(brand_logo_element(&source.icon, 18.0)),
                                        trailing_text: None,
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
                                            .spacing(6.0)
                                            .children(source.profiles.iter().map(|prof| {
                                                let prof_checked =
                                                    choices.import_profile_id.as_deref() == Some(prof.id.as_str());
                                                let src_k = source.key.clone();
                                                let prof_id = prof.id.clone();
                                                let core_p = core.clone();
                                                let bg = if prof_checked {
                                                    mix_rgba(pal.surface, accent, 0.14)
                                                } else {
                                                    pal.surface
                                                };
                                                rect()
                                                    .key(prof.id.clone())
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
                                                            .fill(if prof_checked {
                                                                accent.to_color()
                                                            } else {
                                                                pal.border.to_color()
                                                            }),
                                                    )
                                                    .cursor(CursorIcon::Pointer)
                                                    .on_press(move |_| {
                                                        audio::play_select(true);
                                                        view_state.bump_nudge();
                                                        core_p.invoke_or_log(Command::OnboardingSetImport {
                                                            source_key: Some(src_k.clone()),
                                                            profile_id: Some(prof_id.clone()),
                                                        });
                                                    })
                                                    .child(
                                                        rect()
                                                            .width(Size::px(9.0))
                                                            .height(Size::px(9.0))
                                                            .corner_radius(CornerRadius::new_all(5.0))
                                                            .background(if prof_checked {
                                                                accent.to_color()
                                                            } else {
                                                                Color::TRANSPARENT
                                                            })
                                                            .border(
                                                                Border::new()
                                                                    .width(1.8)
                                                                    .alignment(BorderAlignment::Inner)
                                                                    .fill(if prof_checked {
                                                                        accent.to_color()
                                                                    } else {
                                                                        pal.border.to_color()
                                                                    }),
                                                            ),
                                                    )
                                                    .child(
                                                        label()
                                                            .text(prof.name.clone())
                                                            .font_size(12.5)
                                                            .font_weight(FontWeight::SEMI_BOLD)
                                                            .color(pal.text.to_color()),
                                                    )
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
                    .spacing(7.0)
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
                    .child(rect().width(Size::fill()).spacing(6.0).children(engine_rows)),
            )
            .child(
                // 3. Links from other apps (default browser) fieldset
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(10.0, 12.0, 10.0, 12.0))
                    .spacing(7.0)
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
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .spacing(8.0)
                            .child(rect().width(Size::flex(1.0)).child(RadioOptionCard {
                                pal,
                                accent,
                                checked: choices.default_browser,
                                compact: true,
                                bold_prefix: None,
                                label_text: "Open in Zendia".to_string(),
                                icon_el: None,
                                trailing_text: None,
                                on_choose: EventHandler::new({
                                    let core = core.clone();
                                    move |_| {
                                        let mut mf = view_state.move_focus;
                                        mf.set_if_modified(MoveInFocus::DefaultBrowser);
                                        audio::play_select(true);
                                        view_state.bump_nudge();
                                        core.invoke_or_log(Command::OnboardingSetDefaultBrowser { enabled: true });
                                    }
                                }),
                                key: DiffKey::from(&("def-yes")),
                            }))
                            .child(rect().width(Size::flex(1.0)).child(RadioOptionCard {
                                pal,
                                accent,
                                checked: !choices.default_browser,
                                compact: true,
                                bold_prefix: None,
                                label_text: "Leave as they are".to_string(),
                                icon_el: None,
                                trailing_text: None,
                                on_choose: EventHandler::new({
                                    let core = core.clone();
                                    move |_| {
                                        let mut mf = view_state.move_focus;
                                        mf.set_if_modified(MoveInFocus::DefaultBrowser);
                                        audio::play_select(false);
                                        view_state.bump_nudge();
                                        core.invoke_or_log(Command::OnboardingSetDefaultBrowser { enabled: false });
                                    }
                                }),
                                key: DiffKey::from(&("def-no")),
                            })),
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
            .spacing(10.0)
            .children(ESSENTIAL_APP_PRESETS.chunks(3).enumerate().map(|(row_idx, chunk)| {
                rect()
                    .key(row_idx)
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .spacing(10.0)
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
        let mut hovered = use_state(|| false);
        let is_hovered = *hovered.read();
        let pal = self.pal;
        let app_color = Rgba::rgb(self.app.color_rgb.0, self.app.color_rgb.1, self.app.color_rgb.2);

        let bg = if self.selected {
            mix_rgba(pal.surface, app_color, 0.16)
        } else {
            pal.bg
        };
        let border = if self.selected {
            app_color
        } else if is_hovered {
            mix_rgba(pal.border, app_color, 0.70)
        } else {
            pal.border
        };

        let url = self.app.url.to_string();
        let was_selected = self.selected;
        let count = self.selected_count;
        let view_state = self.view_state;

        rect()
            .width(Size::flex(1.0))
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
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .on_press(move |_| {
                let next_selected = !was_selected;
                let next_count = if next_selected { count + 1 } else { count.max(1) };
                audio::play_app_pin(next_selected, next_count);
                view_state.bump_nudge();
                core.invoke_or_log(Command::OnboardingToggleEssential { url: url.clone() });
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
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
