/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The Living Emblem Start scene, the framed chapter `StageShell`, and all six chapter 2D scenes (`VimScene`,
//! `AgentScene`, `ShieldScene`, `MoveInScene`, `AppsScene`, `GoScene`) matching the fork's `scenes/*.jsx`.

use crate::ui::kit::{use_interactive_feedback, use_last_some};
use crate::ui::onboarding::audio;
use crate::ui::onboarding::browser_scene::{
    BrowserSceneParams, EssentialTileSpec, WhichKeyPage, draw_circle_fill, draw_dashed_quad, draw_dashed_rrect_stroke,
    draw_rrect_fill, draw_rrect_stroke, draw_text_centered, draw_text_left, make_font, paint_browser_2d,
    paint_emblem_rings, paint_stage_backdrop, quad_point,
};
use crate::ui::onboarding::logos::{brand_logo_element, paint_brand_svg_on_canvas, playback_icon};
use crate::ui::onboarding::mascot::OnboardingMascot;
use crate::ui::onboarding::theme::{
    MODE_ORDER, OnboardingPalette, TrackEase, clamp01, cos_f32, ease_in_out, ease_out, eval_track, lerp, mix_rgba,
    mode_name, onboarding_spring, sin_f32, sk_alpha, sk_color, span,
};
use crate::ui::theme::{Rgba, motion, use_theme};
use freya::animation::*;
use freya::engine::prelude as sk;
use freya::prelude::*;
use std::f32::consts::PI;
use std::time::{Duration, Instant};
use zendia_browser_core::model::InputMode;
use zendia_browser_core::onboarding::{EssentialAppPreset, ImportSource, MoveInFocus, OnboardingAgentInfo};

/// Shared animation clock hook for looping 2D scenes. Respects `still`, `paused`, and `frozen_at`.
fn use_scene_clock(duration: f32, pose: f32, still: bool, paused: bool, frozen_at: Option<f32>) -> f32 {
    let initial = frozen_at.unwrap_or(if still { pose } else { 0.0 });
    let mut clock = use_state(|| initial);
    let mut paused_state = use_state(|| paused);
    paused_state.set_if_modified(paused);
    let mut still_state = use_state(|| still);
    still_state.set_if_modified(still);
    let mut frozen_state = use_state(|| frozen_at);
    frozen_state.set_if_modified(frozen_at);

    use_hook(move || {
        if still || frozen_at.is_some() {
            return;
        }
        spawn(async move {
            let mut last = Instant::now();
            loop {
                timer(Duration::from_millis(33)).await;
                let now = Instant::now();
                let dt = (now - last).as_secs_f32().min(0.05);
                last = now;
                if *still_state.peek() || *paused_state.peek() || frozen_state.peek().is_some() {
                    continue;
                }
                let next = (*clock.peek() + dt) % duration;
                clock.set(next);
            }
        });
    });

    if let Some(f) = frozen_at {
        f
    } else if still {
        pose
    } else {
        *clock.read()
    }
}

/// Helper to fit a logical `(vb_w, vb_h)` scene viewBox cleanly inside a canvas of `(width, height)`.
fn fit_viewbox(canvas: &sk::Canvas, width: f32, height: f32, vb_w: f32, vb_h: f32) -> Option<usize> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let scale = (width / vb_w).min(height / vb_h);
    let ox = (width - vb_w * scale) * 0.5;
    let oy = (height - vb_h * scale) * 0.5;
    let save = canvas.save();
    canvas.translate((ox, oy));
    canvas.scale((scale, scale));
    Some(save)
}

/* =========================================================================
Start Page — Living Emblem & 6 Mode Pills
========================================================================= */

#[derive(Clone, PartialEq)]
pub struct StartEmblemScene {
    pub pose: InputMode,
    pub still: bool,
    pub pinned_morph: Option<(InputMode, f32)>,
    pub on_change_pose: EventHandler<InputMode>,
    pub key: DiffKey,
}

impl KeyExt for StartEmblemScene {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for StartEmblemScene {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let pal = OnboardingPalette::for_scheme(theme.scheme);
        let target_accent = pal.mode_accent(self.pose);
        let mut prev_accent = use_state(|| target_accent);
        let mut cur_accent = use_state(|| target_accent);
        let mut color_seq = use_state(|| 0u64);
        if *cur_accent.peek() != target_accent {
            prev_accent.set(*cur_accent.peek());
            cur_accent.set(target_accent);
            let next = color_seq.peek().wrapping_add(1);
            color_seq.set(next);
        }
        let still = self.still;
        let halo_anim = use_animation_with_dependencies(&*color_seq.read(), move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(if still { OnChange::Finish } else { OnChange::Rerun });
            AnimNum::new(0.0, 1.0)
                .duration(motion::ONB_HALO_TRANSITION)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let active_accent = prev_accent
            .read()
            .mix(*cur_accent.read(), halo_anim.get().value().clamp(0.0, 1.0));

        let rings_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_emblem_rings(canvas, ctx.size.width, ctx.size.height, active_accent);
            }
        });

        let on_hover = {
            let cb = self.on_change_pose.clone();
            let current = self.pose;
            move |mode: InputMode| {
                if mode != current {
                    audio::play_mode(mode);
                    cb.call(mode);
                }
            }
        };
        let on_select = {
            let cb = self.on_change_pose.clone();
            move |mode: InputMode| {
                audio::play_mode(mode);
                cb.call(mode);
            }
        };

        // Two rows of 3 mode pills matching the exact 3-column grid in the reference shot.
        let top_row = &MODE_ORDER[0..3];
        let bottom_row = &MODE_ORDER[3..6];

        rect()
            .width(Size::fill())
            .cross_align(Alignment::center())
            .spacing(26.0)
            .child(
                // Creature + concentric dashed halo rings (`330px` emblem matching `zendia-welcome.css:272`)
                rect()
                    .width(Size::px(392.0))
                    .height(Size::px(392.0))
                    .center()
                    .child(
                        rect()
                            .position(Position::new_absolute().top(0.0).left(0.0))
                            .width(Size::percent(100.0))
                            .height(Size::percent(100.0))
                            .background(Fill::Callback(rings_cb)),
                    )
                    .child(
                        OnboardingMascot::new(self.pose, 330.0)
                            .still(self.still)
                            .bloom(!self.still)
                            .blink(!self.still)
                            .track_pointer(!self.still)
                            .interactive(true)
                            .drop_shadow(true)
                            .pinned_morph(self.pinned_morph)
                            .on_hover_pose(on_hover)
                            .on_select_pose(on_select),
                    ),
            )
            .child(
                rect()
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .spacing(8.0)
                            .children(top_row.iter().map(|&mode| ModePillButton {
                                mode,
                                active: self.pose == mode,
                                on_pick: self.on_change_pose.clone(),
                                key: DiffKey::from(&(mode_name(mode))),
                            })),
                    )
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .spacing(8.0)
                            .children(bottom_row.iter().map(|&mode| ModePillButton {
                                mode,
                                active: self.pose == mode,
                                on_pick: self.on_change_pose.clone(),
                                key: DiffKey::from(&(mode_name(mode))),
                            })),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct ModePillButton {
    mode: InputMode,
    active: bool,
    on_pick: EventHandler<InputMode>,
    key: DiffKey,
}

impl KeyExt for ModePillButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ModePillButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let pal = OnboardingPalette::for_scheme(theme.scheme);
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let is_hovered = fb.is_hovered();
        let swatch = pal.mode_accent(self.mode);
        let hover_t = fb.hover_progress();
        let press_t = fb.press_progress();
        let bg = if self.active {
            mix_rgba(pal.surface, swatch, 0.16)
        } else {
            pal.surface.mix(mix_rgba(pal.surface, swatch, 0.08), hover_t)
        };
        let border = if self.active {
            swatch
        } else {
            pal.border_subtle.mix(swatch, hover_t)
        };
        let fg = if self.active {
            pal.text
        } else {
            pal.muted.mix(pal.text, hover_t)
        };

        let mode = self.mode;
        let active = self.active;
        let on_enter_pick = self.on_pick.clone();
        let on_click_pick = self.on_pick.clone();
        let mut fb_enter = fb.on_pointer_enter();

        let lift_y = -1.5 * hover_t + 1.0 * press_t;
        let scale = 1.0 - 0.03 * press_t;

        let inner = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("{} mode", mode_name(mode)))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .main_align(Alignment::center())
            .min_height(Size::px(32.0))
            .padding(Gaps::new(6.0, 13.0, 6.0, 12.0))
            .spacing(6.0)
            .corner_radius(CornerRadius::new_all(999.0))
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
            .on_pointer_enter(move |e| {
                fb_enter(e);
                if !active {
                    audio::play_mode(mode);
                    on_enter_pick.call(mode);
                }
            })
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |_| {
                audio::play_mode(mode);
                on_click_pick.call(mode);
            })
            .child(
                rect()
                    .width(Size::px(8.0))
                    .height(Size::px(8.0))
                    .corner_radius(CornerRadius::new_all(4.0))
                    .background(swatch.to_color()),
            )
            .child(
                label()
                    .text(mode_name(mode))
                    .font_size(11.0)
                    .font_weight(FontWeight::BOLD)
                    .font_family("monospace")
                    .color(if is_hovered { pal.text.to_color() } else { fg.to_color() }),
            );

        let focused = focus() == Focus::Keyboard;
        rect()
            .padding(Gaps::new_all(3.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .border(if focused {
                Border::new()
                    .width(2.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(swatch.to_color())
            } else {
                Border::new()
            })
            .child(inner)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/* =========================================================================
Framed Chapter Stage Shell
========================================================================= */

#[derive(Clone, PartialEq)]
pub struct StageShell {
    pub accent: Rgba,
    pub still: bool,
    pub looping: bool,
    pub paused: bool,
    pub compact: bool,
    pub on_toggle_pause: Option<EventHandler<()>>,
    pub caption: Option<Element>,
    pub content: Element,
    pub key: DiffKey,
}

impl KeyExt for StageShell {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for StageShell {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let pal = OnboardingPalette::for_scheme(theme.scheme);
        let pause_fb = use_interactive_feedback(true);
        let pause_a11y_id = use_a11y();
        let pause_focus = use_focus(pause_a11y_id);

        let target_accent = self.accent;
        let mut prev_accent = use_state(|| target_accent);
        let mut cur_accent = use_state(|| target_accent);
        let mut accent_seq = use_state(|| 0u64);
        if *cur_accent.peek() != target_accent {
            prev_accent.set(*cur_accent.peek());
            cur_accent.set(target_accent);
            let next = accent_seq.peek().wrapping_add(1);
            accent_seq.set(next);
        }
        let still = self.still;
        let backdrop_anim = use_animation_with_dependencies(&*accent_seq.read(), move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(if still { OnChange::Finish } else { OnChange::Rerun });
            AnimNum::new(0.0, 1.0)
                .duration(motion::ONB_STAGE_TRANSITION)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let accent = prev_accent
            .read()
            .mix(*cur_accent.read(), backdrop_anim.get().value().clamp(0.0, 1.0));

        let backdrop_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_stage_backdrop(canvas, ctx.size.width, ctx.size.height, accent);
            }
        });

        // Show Pause/Play toggle on all looping chapters regardless of `still` (matches fork's Stage2D.jsx:32).
        let show_pause = self.looping && self.on_toggle_pause.is_some();
        let paused = self.paused;
        let hover_t = pause_fb.hover_progress();
        let press_t = pause_fb.press_progress();
        let on_toggle = self.on_toggle_pause.clone();

        let shadow = if pal.dark {
            Shadow::new().y(18.0).blur(44.0).color(Color::from_argb(92, 0, 0, 0))
        } else {
            Shadow::new()
                .y(12.0)
                .blur(32.0)
                .color(Color::from_argb(20, 0x1a, 0x1b, 0x26))
        };

        let pad = if self.compact { 14.0 } else { 20.0 };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .corner_radius(CornerRadius::new_all(26.0))
            .background(pal.stage_bg.to_color())
            .border(
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(pal.border_subtle.to_color()),
            )
            .shadow(shadow)
            .overflow(Overflow::Clip)
            .child(
                // Backdrop radial glow + hairline Voronoi arcs
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::percent(100.0))
                    .background(Fill::Callback(backdrop_cb)),
            )
            .child(
                // Foreground content + optional caption
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .padding(Gaps::new_all(pad))
                    .content(Content::Flex)
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::flex(1.0))
                            .center()
                            .overflow(Overflow::Clip)
                            .child(self.content.clone()),
                    )
                    .maybe_child(self.caption.clone().map(|cap| {
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(12.0, 0.0, 0.0, 0.0))
                            .cross_align(Alignment::center())
                            .child(cap)
                    })),
            )
            .maybe_child(show_pause.then(|| {
                let btn_bg = pal.surface.with_alpha(0.86).to_color();
                let btn_border = pal.border_subtle.mix(accent, hover_t);
                let btn_fg = pal.muted.mix(pal.text, hover_t);
                let btn_scale = 1.0 + 0.05 * hover_t - 0.08 * press_t;
                let focused = pause_focus() == Focus::Keyboard;
                rect()
                    .position(Position::new_absolute().top(11.0).right(11.0))
                    .layer(Layer::Relative(6))
                    .padding(Gaps::new_all(3.0))
                    .corner_radius(CornerRadius::new_all(13.0))
                    .border(if focused {
                        Border::new()
                            .width(2.5)
                            .alignment(BorderAlignment::Inner)
                            .fill(accent.to_color())
                    } else {
                        Border::new()
                    })
                    .child(
                        rect()
                            .a11y_id(pause_a11y_id)
                            .a11y_focusable(true)
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt(if paused { "Play preview" } else { "Pause preview" })
                            .width(Size::px(34.0))
                            .height(Size::px(34.0))
                            .corner_radius(CornerRadius::new_all(10.0))
                            .background(btn_bg)
                            .border(
                                Border::new()
                                    .width(1.5)
                                    .alignment(BorderAlignment::Inner)
                                    .fill(btn_border.to_color()),
                            )
                            .scale(btn_scale)
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_pointer_enter(pause_fb.on_pointer_enter())
                            .on_pointer_leave(pause_fb.on_pointer_leave())
                            .on_pointer_down(pause_fb.on_pointer_down())
                            .on_global_pointer_up(pause_fb.on_pointer_up())
                            .on_press(move |_| {
                                if let Some(cb) = &on_toggle {
                                    cb.call(());
                                }
                            })
                            .child(playback_icon(paused, 13.0, btn_fg.to_color())),
                    )
            }))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/* =========================================================================
Chapter 01 — Keys (VimScene)
========================================================================= */

const VIM_LOOP: f32 = 18.0;
const VIM_POSE: f32 = 8.4;
const VIM_BEATS: [f32; 6] = [0.4, 3.6, 6.8, 9.2, 12.2, 15.2];

pub const VIM_KEY_BEATS: &[(&str, &str)] = &[
    ("j  k", "Scroll the page"),
    ("⇧J  ⇧K", "Next and previous tab"),
    ("Space", "Every other command"),
    ("Space e", "Move around the sidebar"),
    ("Space a a", "Ask your agent about the tab"),
    ("Esc", "Leave a text field"),
];

fn vim_scroll(t: f32) -> f32 {
    eval_track(
        0.0,
        &[
            (0.8, 72.0, 0.32, TrackEase::Out),
            (1.3, 144.0, 0.32, TrackEase::Out),
            (1.8, 210.0, 0.32, TrackEase::Out),
            (2.5, 132.0, 0.32, TrackEase::Out),
            (3.0, 56.0, 0.32, TrackEase::Out),
        ],
        t,
    )
}

fn vim_tab(t: f32) -> f32 {
    eval_track(
        0.0,
        &[
            (4.0, 1.0, 0.28, TrackEase::Out),
            (4.9, 2.0, 0.28, TrackEase::Out),
            (5.8, 1.0, 0.28, TrackEase::Out),
            (17.1, 0.0, 0.45, TrackEase::InOut),
        ],
        t,
    )
}

fn vim_which_key(t: f32) -> f32 {
    eval_track(
        0.0,
        &[
            (7.1, 1.0, 0.35, TrackEase::Spring),
            (9.4, 0.0, 0.16, TrackEase::Out),
            (12.4, 1.0, 0.35, TrackEase::Spring),
            (13.7, 0.0, 0.16, TrackEase::Out),
        ],
        t,
    )
}

fn vim_explorer(t: f32) -> f32 {
    eval_track(
        0.0,
        &[(9.4, 1.0, 0.22, TrackEase::Out), (11.6, 0.0, 0.20, TrackEase::Out)],
        t,
    )
}

fn vim_explorer_row(t: f32) -> f32 {
    eval_track(
        1.0,
        &[
            (9.8, 2.0, 0.22, TrackEase::Out),
            (10.4, 3.0, 0.22, TrackEase::Out),
            (11.0, 2.0, 0.22, TrackEase::Out),
        ],
        t,
    )
}

fn vim_dock(t: f32) -> f32 {
    eval_track(
        0.0,
        &[(13.7, 1.0, 0.42, TrackEase::Out), (15.6, 0.0, 0.25, TrackEase::InOut)],
        t,
    )
}

fn vim_page_at(t: f32) -> (usize, usize, f32) {
    if t < 4.0 {
        (0, 0, 0.0)
    } else if t < 4.9 {
        (0, 1, span(t, 4.0, 0.24))
    } else if t < 5.8 {
        (1, 2, span(t, 4.9, 0.24))
    } else if t < 17.1 {
        (2, 1, span(t, 5.8, 0.24))
    } else {
        (1, 0, span(t, 17.1, 0.35))
    }
}

fn vim_mode_at(t: f32) -> InputMode {
    if (9.4..11.6).contains(&t) {
        InputMode::Explorer
    } else if (13.7..15.6).contains(&t) {
        InputMode::Insert
    } else {
        InputMode::Normal
    }
}

fn vim_keys_at(t: f32) -> &'static str {
    if (7.1..9.4).contains(&t) || (12.4..13.0).contains(&t) {
        "<Space>"
    } else if (13.0..13.7).contains(&t) {
        "<Space>a"
    } else {
        ""
    }
}

fn vim_beat_at(enabled: bool, t: f32) -> Option<usize> {
    if !enabled {
        return None;
    }
    VIM_BEATS.iter().rposition(|&start| t >= start)
}

struct KeycapSpec {
    label: &'static str,
    width: f32,
    hits: &'static [(f32, f32)],
}

const KEYCAPS: &[KeycapSpec] = &[
    KeycapSpec {
        label: "Esc",
        width: 48.0,
        hits: &[(11.6, 0.16), (15.5, 0.16)],
    },
    KeycapSpec {
        label: "⇧",
        width: 46.0,
        hits: &[(3.9, 1.3), (5.7, 0.45)],
    },
    KeycapSpec {
        label: "j",
        width: 42.0,
        hits: &[
            (0.8, 0.16),
            (1.3, 0.16),
            (1.8, 0.16),
            (4.0, 0.16),
            (4.9, 0.16),
            (9.8, 0.16),
            (10.4, 0.16),
        ],
    },
    KeycapSpec {
        label: "k",
        width: 42.0,
        hits: &[(2.5, 0.16), (3.0, 0.16), (5.8, 0.16), (11.0, 0.16)],
    },
    KeycapSpec {
        label: "Space",
        width: 94.0,
        hits: &[(7.1, 0.16), (9.1, 0.14), (12.4, 0.16)],
    },
    KeycapSpec {
        label: "e",
        width: 42.0,
        hits: &[(9.4, 0.16)],
    },
    KeycapSpec {
        label: "a",
        width: 42.0,
        hits: &[(13.0, 0.16), (13.6, 0.16)],
    },
];

fn key_pressed(hits: &[(f32, f32)], t: f32) -> f32 {
    let mut down = 0.0_f32;
    for &(at, hold) in hits {
        let dt = t - at;
        if dt >= 0.0 && dt <= hold + 0.24 {
            let press = ease_out(dt / 0.05);
            let release = if dt > hold { ease_out((dt - hold) / 0.22) } else { 0.0 };
            down = down.max(press * (1.0 - release));
        }
    }
    down
}

#[derive(Clone, PartialEq)]
pub struct VimScene {
    pub enabled: bool,
    pub still: bool,
    pub paused: bool,
    pub frozen_at: Option<f32>,
    pub key: DiffKey,
}

impl KeyExt for VimScene {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for VimScene {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let pal = OnboardingPalette::for_scheme(theme.scheme);
        let t = use_scene_clock(VIM_LOOP, VIM_POSE, self.still, self.paused, self.frozen_at);
        let mut last_beat = use_state(|| None::<usize>);

        let beat_now = vim_beat_at(self.enabled, t);
        if *last_beat.peek() != beat_now {
            last_beat.set(beat_now);
            if beat_now.is_some() && !self.still && !self.paused && self.frozen_at.is_none() {
                audio::play_preview_key(vim_mode_at(t));
            }
        }

        let enabled = self.enabled;
        let still = self.still;
        let vim_blend_anim = use_animation_with_dependencies(&enabled, move |conf, is_on| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(if still { OnChange::Finish } else { OnChange::Rerun });
            let (from, to) = if *is_on { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::ONB_SCENE_TOGGLE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let vim_blend = vim_blend_anim.get().value().clamp(0.0, 1.0);
        let mode = if enabled { vim_mode_at(t) } else { InputMode::Normal };
        let mode_pastel = OnboardingPalette::mode_pastel(mode);
        let (page_variant, next_page_variant, page_mix) = if enabled { vim_page_at(t) } else { (0, 0, 0.0) };
        let scroll_y = if enabled && t < 4.9 {
            vim_scroll(t.min(3.8))
        } else {
            0.0
        };

        let browser_params = BrowserSceneParams {
            x: 30.0,
            y: 14.0,
            scale: 1.0,
            vim: vim_blend,
            mode,
            tab_index: if enabled { vim_tab(t) } else { 0.0 },
            scroll_y,
            page_variant,
            next_page_variant,
            page_mix,
            explorer: if enabled { vim_explorer(t) } else { 0.0 },
            explorer_row: vim_explorer_row(t),
            which_key: if enabled { vim_which_key(t) } else { 0.0 },
            which_key_page: if (13.0..14.2).contains(&t) {
                WhichKeyPage::Ai
            } else {
                WhichKeyPage::Leader
            },
            typed_keys: if enabled {
                vim_keys_at(t).to_string()
            } else {
                String::new()
            },
            dock: if enabled { vim_dock(t) } else { 0.0 },
            agent_name: Some("Agent".to_string()),
            agent_color: Some(OnboardingPalette::mode_pastel(InputMode::Insert)),
            agent_time: t,
            tab_glow_color: None,
            essentials: Vec::new(),
        };

        let cb = RenderCallback::new(move |ctx| {
            let RenderTarget::Background { canvas, .. } = &mut ctx.target else {
                return;
            };
            let Some(save) = fit_viewbox(canvas, ctx.size.width, ctx.size.height, 660.0, 472.0) else {
                return;
            };
            paint_browser_2d(canvas, &pal, &browser_params, |_| {});

            // Tactile keycap bar beneath the browser (smoothly glides and fades over 320 ms on toggle)
            let gap = 8.0;
            let row_w: f32 = KEYCAPS.iter().map(|k| k.width).sum::<f32>() + gap * (KEYCAPS.len() as f32 - 1.0);
            let mut cx = (660.0 - row_w) * 0.5;
            let bar_y = lerp(422.0, 410.0, vim_blend);
            let bar_alpha = lerp(0.34, 1.0, vim_blend);
            let cap_font = make_font(12.5, true, true);

            for cap in KEYCAPS {
                let x = cx;
                cx += cap.width + gap;
                let down = if enabled { key_pressed(cap.hits, t) } else { 0.0 };
                let press_y = down * 3.5;
                let active = down > 0.15;

                // Keycap shadow base
                draw_rrect_fill(
                    canvas,
                    x,
                    bar_y + 5.0,
                    cap.width,
                    34.0,
                    10.0,
                    sk_alpha(pal.ink, (if pal.dark { 0.85 } else { 0.22 }) * bar_alpha),
                );
                // Keycap top
                draw_rrect_fill(
                    canvas,
                    x,
                    bar_y + press_y,
                    cap.width,
                    34.0,
                    10.0,
                    sk_alpha(if active { mode_pastel } else { pal.surface }, bar_alpha),
                );
                draw_rrect_stroke(
                    canvas,
                    x,
                    bar_y + press_y,
                    cap.width,
                    34.0,
                    10.0,
                    1.5,
                    sk_alpha(if active { mode_pastel } else { pal.border }, bar_alpha),
                );
                if active {
                    draw_rrect_stroke(
                        canvas,
                        x - down * 4.0,
                        bar_y + press_y - down * 4.0,
                        cap.width + down * 8.0,
                        34.0 + down * 8.0,
                        13.0,
                        1.5,
                        sk_alpha(mode_pastel, clamp01(1.0 - down * 0.4) * 0.65),
                    );
                }
                let fg_col = sk_alpha(if active { pal.ink } else { pal.text }, bar_alpha);
                if cap.label == "⇧" {
                    // Draw a crisp vector Shift arrow (avoids missing-glyph tofu in monospace Skia fonts).
                    let mx = x + cap.width * 0.5;
                    let my = bar_y + 17.5 + press_y;
                    let mut pb = sk::PathBuilder::new();
                    pb.move_to(sk::Point::new(mx, my - 6.5));
                    pb.line_to(sk::Point::new(mx + 6.5, my + 0.5));
                    pb.line_to(sk::Point::new(mx + 2.8, my + 0.5));
                    pb.line_to(sk::Point::new(mx + 2.8, my + 6.0));
                    pb.line_to(sk::Point::new(mx - 2.8, my + 6.0));
                    pb.line_to(sk::Point::new(mx - 2.8, my + 0.5));
                    pb.line_to(sk::Point::new(mx - 6.5, my + 0.5));
                    pb.close();
                    let mut arrow_paint = sk::Paint::default();
                    arrow_paint.set_anti_alias(true);
                    arrow_paint.set_style(sk::PaintStyle::Stroke);
                    arrow_paint.set_stroke_width(1.6);
                    arrow_paint.set_stroke_join(sk::PaintJoin::Round);
                    arrow_paint.set_color(fg_col);
                    canvas.draw_path(&pb.detach(), &arrow_paint);
                } else {
                    draw_text_centered(
                        canvas,
                        cap.label,
                        x + cap.width * 0.5,
                        bar_y + 21.0 + press_y,
                        &cap_font,
                        fg_col,
                    );
                }
            }

            canvas.restore_to_count(save);
        });

        let accent = pal.mode_accent(InputMode::Normal);
        let caption = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::wrap())
            .main_align(Alignment::center())
            .spacing(7.0)
            .children(
                VIM_KEY_BEATS
                    .iter()
                    .enumerate()
                    .map(|(index, &(keys, desc))| VimCaptionChip {
                        pal,
                        accent,
                        keys,
                        desc,
                        active: enabled && beat_now == Some(index),
                        still,
                        key: DiffKey::from(&index),
                    }),
            );

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .background(Fill::Callback(cb)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(10.0, 0.0, 0.0, 0.0))
                    .cross_align(Alignment::center())
                    .child(caption),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct VimCaptionChip {
    pal: OnboardingPalette,
    accent: Rgba,
    keys: &'static str,
    desc: &'static str,
    active: bool,
    still: bool,
    key: DiffKey,
}

impl KeyExt for VimCaptionChip {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for VimCaptionChip {
    fn render(&self) -> impl IntoElement {
        let active = self.active;
        let still = self.still;
        let active_anim = use_animation_with_dependencies(&active, move |conf, is_on| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(if still { OnChange::Finish } else { OnChange::Rerun });
            let (from, to) = if *is_on { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::ONB_VIM_CHIP_LIFT)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let p = active_anim.get().value().clamp(0.0, 1.0);
        let pal = self.pal;
        let accent = self.accent;
        let bg = pal.surface.mix(mix_rgba(pal.surface, accent, 0.16), p);
        let border = pal.border_subtle.mix(accent, p);
        let text = pal.muted.mix(pal.text, p);
        let lift_y = -2.0 * onboarding_spring(p);

        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .padding(Gaps::new(5.0, 9.0, 5.0, 9.0))
            .spacing(6.0)
            .corner_radius(CornerRadius::new_all(10.0))
            .background(bg.to_color())
            .border(
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(border.to_color()),
            )
            .offset_y(lift_y)
            .child(
                label()
                    .text(self.keys)
                    .font_size(11.0)
                    .font_weight(FontWeight::BOLD)
                    .font_family("monospace")
                    .max_lines(1)
                    .color(accent.to_color()),
            )
            .child(
                label()
                    .text(self.desc)
                    .font_size(12.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .max_lines(1)
                    .color(text.to_color()),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

const AGENT_LOOP: f32 = 8.4;
const AGENT_POSE: f32 = 2.1;

const AGENT_STOPS: &[(f32, f32, f32, Option<f32>)] = &[
    (0.2, 240.0, 240.0, None),
    (1.2, 146.0, 142.0, Some(1.7)),
    (3.4, 192.0, 274.0, Some(3.9)),
    (5.8, 118.0, 158.0, Some(6.3)),
];

#[derive(Clone, PartialEq)]
pub struct AgentScene {
    pub agent: Option<OnboardingAgentInfo>,
    pub vim: bool,
    pub still: bool,
    pub paused: bool,
    pub frozen_at: Option<f32>,
    pub key: DiffKey,
}

impl KeyExt for AgentScene {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for AgentScene {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let pal = OnboardingPalette::for_scheme(theme.scheme);
        let t = use_scene_clock(AGENT_LOOP, AGENT_POSE, self.still, self.paused, self.frozen_at);

        let who_name = self
            .agent
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Your agent".to_string());
        let who_color = self
            .agent
            .as_ref()
            .map(|a| Rgba::rgb(a.accent_rgb.0, a.accent_rgb.1, a.accent_rgb.2))
            .unwrap_or_else(|| OnboardingPalette::mode_pastel(InputMode::Explorer));

        let turned = t >= 4.05;
        let reset_fade = 1.0 - span(t, AGENT_LOOP - 0.5, 0.4);
        let page_mix = if turned {
            span(t, 4.05, 0.24).min(reset_fade)
        } else {
            0.0
        };
        let scroll_y = if turned {
            0.0
        } else {
            eval_track(
                0.0,
                &[(1.85, 108.0, 0.6, TrackEase::InOut), (4.05, 0.0, 0.01, TrackEase::Out)],
                t,
            )
        };

        let idx = AGENT_STOPS.iter().rposition(|&(at, ..)| t >= at).unwrap_or(0);
        let to = AGENT_STOPS[idx];
        let from = AGENT_STOPS[idx.saturating_sub(1)];
        let mv = ease_in_out(span(t, to.0, 0.65));
        let px = lerp(from.1, to.1, mv);
        let py = lerp(from.2, to.2, mv);

        let clicked = AGENT_STOPS
            .iter()
            .rev()
            .find(|&&(_, _, _, click)| click.is_some_and(|c| t >= c));
        let click_age = clicked.and_then(|&(_, _, _, c)| c).map(|c| t - c).unwrap_or(9.0);
        let ripple_p = if click_age < 0.55 {
            ease_out(click_age / 0.55)
        } else {
            0.0
        };
        let pressing = (0.0..0.14).contains(&click_age);
        let clicked_pos = clicked.map(|&(_, cx, cy, _)| (cx, cy));

        let params = BrowserSceneParams {
            x: 30.0,
            y: 28.0,
            scale: 1.0,
            vim: if self.vim { 1.0 } else { 0.0 },
            mode: InputMode::Explorer,
            tab_index: if turned && reset_fade > 0.5 { 1.0 } else { 0.0 },
            scroll_y,
            page_variant: 0,
            next_page_variant: 1,
            page_mix,
            dock: 1.0,
            agent_name: Some(who_name.clone()),
            agent_color: Some(who_color),
            agent_time: t,
            tab_glow_color: Some(who_color),
            ..BrowserSceneParams::default()
        };

        let cb = RenderCallback::new(move |ctx| {
            let RenderTarget::Background { canvas, .. } = &mut ctx.target else {
                return;
            };
            let Some(save) = fit_viewbox(canvas, ctx.size.width, ctx.size.height, 660.0, 436.0) else {
                return;
            };
            paint_browser_2d(canvas, &pal, &params, |vp| {
                // Expanding click ripple
                if click_age < 0.55
                    && let Some((cx, cy)) = clicked_pos
                {
                    let mut ring = sk::Paint::default();
                    ring.set_anti_alias(true);
                    ring.set_style(sk::PaintStyle::Stroke);
                    ring.set_stroke_width(2.4);
                    ring.set_color(sk_alpha(who_color, clamp01(1.0 - ripple_p) * 0.9));
                    vp.draw_circle(sk::Point::new(cx, cy), 8.0 + ripple_p * 28.0, &ring);
                    draw_circle_fill(
                        vp,
                        cx,
                        cy,
                        4.0 + ripple_p * 14.0,
                        sk_alpha(who_color, clamp01(1.0 - ripple_p) * 0.28),
                    );
                }

                // Agent cursor arrow + trailing chip
                let cs = vp.save();
                vp.translate((px, py));
                if pressing {
                    vp.scale((0.88, 0.88));
                }
                let mut pb = sk::PathBuilder::new();
                pb.move_to(sk::Point::new(0.0, 0.0));
                pb.line_to(sk::Point::new(0.0, 19.0));
                pb.line_to(sk::Point::new(5.2, 14.4));
                pb.line_to(sk::Point::new(8.8, 22.0));
                pb.line_to(sk::Point::new(11.8, 20.6));
                pb.line_to(sk::Point::new(8.2, 13.0));
                pb.line_to(sk::Point::new(14.5, 12.6));
                pb.close();
                let arrow = pb.detach();

                let mut fill = sk::Paint::default();
                fill.set_anti_alias(true);
                fill.set_style(sk::PaintStyle::Fill);
                fill.set_color(sk_color(who_color));
                vp.draw_path(&arrow, &fill);

                let mut stroke = sk::Paint::default();
                stroke.set_anti_alias(true);
                stroke.set_style(sk::PaintStyle::Stroke);
                stroke.set_stroke_width(1.6);
                stroke.set_stroke_join(sk::PaintJoin::Round);
                stroke.set_color(sk_color(pal.ink));
                vp.draw_path(&arrow, &stroke);

                let chip_w = (who_name.len() as f32 * 6.8 + 18.0).max(58.0);
                draw_rrect_fill(vp, 15.0, 16.0, chip_w, 21.0, 10.5, sk_color(who_color));
                draw_rrect_stroke(vp, 15.0, 16.0, chip_w, 21.0, 10.5, 1.4, sk_color(pal.ink));
                let chip_font = make_font(10.0, true, false);
                draw_text_centered(vp, &who_name, 15.0 + chip_w * 0.5, 30.0, &chip_font, sk_color(pal.ink));
                vp.restore_to_count(cs);
            });
            canvas.restore_to_count(save);
        });

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(Fill::Callback(cb))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/* =========================================================================
Chapter 03 — Shield (ShieldScene)
========================================================================= */

const SHIELD_LOOP: f32 = 7.2;
const SHIELD_POSE: f32 = 3.1;
const SHIELD_FLIGHT: f32 = 1.05;

struct AdCardSpec {
    at: f32,
    to_x: f32,
    to_y: f32,
    w: f32,
    h: f32,
    label: &'static str,
    color_idx: u8,
}

const AD_CARDS: &[AdCardSpec] = &[
    AdCardSpec {
        at: 0.4,
        to_x: 350.0,
        to_y: 82.0,
        w: 146.0,
        h: 48.0,
        label: "BANNER AD",
        color_idx: 0,
    },
    AdCardSpec {
        at: 1.5,
        to_x: 258.0,
        to_y: 168.0,
        w: 132.0,
        h: 74.0,
        label: "POPUP",
        color_idx: 1,
    },
    AdCardSpec {
        at: 2.6,
        to_x: 382.0,
        to_y: 222.0,
        w: 140.0,
        h: 52.0,
        label: "TRACKER",
        color_idx: 2,
    },
    AdCardSpec {
        at: 3.7,
        to_x: 274.0,
        to_y: 274.0,
        w: 158.0,
        h: 46.0,
        label: "PROMO",
        color_idx: 3,
    },
];

#[derive(Clone, PartialEq)]
pub struct ShieldScene {
    pub enabled: bool,
    pub vim: bool,
    pub still: bool,
    pub paused: bool,
    pub frozen_at: Option<f32>,
    pub key: DiffKey,
}

impl KeyExt for ShieldScene {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ShieldScene {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let pal = OnboardingPalette::for_scheme(theme.scheme);
        let t = use_scene_clock(SHIELD_LOOP, SHIELD_POSE, self.still, self.paused, self.frozen_at);
        let mut last_hit = use_state(|| None::<usize>);

        let hit_now = if self.enabled {
            AD_CARDS.iter().rposition(|ad| t >= ad.at + SHIELD_FLIGHT * 0.44)
        } else {
            None
        };
        if *last_hit.peek() != hit_now {
            last_hit.set(hit_now);
            if hit_now.is_some() && !self.still && !self.paused && self.frozen_at.is_none() {
                audio::play_deflect();
            }
        }

        let enabled = self.enabled;
        let still = self.still;
        let shield_blend_anim = use_animation_with_dependencies(&enabled, move |conf, is_on| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(if still { OnChange::Finish } else { OnChange::Rerun });
            let (from, to) = if *is_on { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::ONB_SCENE_TOGGLE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let shield_blend = shield_blend_anim.get().value().clamp(0.0, 1.0);
        let reset_clear = span(t, SHIELD_LOOP - 0.65, 0.35);
        let params = BrowserSceneParams {
            x: 22.0,
            y: 28.0,
            scale: 0.96,
            vim: if self.vim { 1.0 } else { 0.0 },
            mode: InputMode::Toolbar,
            ..BrowserSceneParams::default()
        };

        let cb = RenderCallback::new(move |ctx| {
            let RenderTarget::Background { canvas, .. } = &mut ctx.target else {
                return;
            };
            let Some(save) = fit_viewbox(canvas, ctx.size.width, ctx.size.height, 660.0, 436.0) else {
                return;
            };

            paint_browser_2d(canvas, &pal, &params, |_| {});

            let mut nearest_impact = 9.0_f32;
            let ad_font = make_font(12.0, true, true);

            // Compute impact timing first so barrier knows whether it's flashing
            for ad in AD_CARDS {
                let age = t - ad.at;
                if age <= 0.0 || reset_clear >= 1.0 {
                    continue;
                }
                let fly = ease_in_out(span(t, ad.at, SHIELD_FLIGHT));
                if enabled && fly >= 0.44 {
                    nearest_impact = nearest_impact.min((age - SHIELD_FLIGHT * 0.44).max(0.0));
                }
            }

            // Curved organic barrier arc when ad blocking is on (fades smoothly with shield_blend)
            if shield_blend > 0.01 {
                let yellow = OnboardingPalette::mode_pastel(InputMode::Toolbar);
                let teal = OnboardingPalette::mode_pastel(InputMode::Library);
                draw_dashed_quad(
                    canvas,
                    (472.0, 52.0),
                    (522.0, 210.0),
                    (472.0, 372.0),
                    3.0,
                    Some(&[8.0, 8.0]),
                    sk_alpha(yellow, (if nearest_impact < 0.25 { 0.95 } else { 0.45 }) * shield_blend),
                );
                draw_dashed_quad(
                    canvas,
                    (460.0, 66.0),
                    (506.0, 210.0),
                    (460.0, 358.0),
                    1.5,
                    None,
                    sk_alpha(teal, 0.32 * shield_blend),
                );
            }

            // Incoming ad cards + deflection burst dots
            for (index, ad) in AD_CARDS.iter().enumerate() {
                let age = t - ad.at;
                if age <= 0.0 || reset_clear >= 1.0 {
                    continue;
                }
                let fly = ease_in_out(span(t, ad.at, SHIELD_FLIGHT));
                let impact_at = 0.44;
                let clamped_fly = if enabled { fly.min(impact_at) } else { fly };
                let start_x = 656.0;
                let start_y = ad.to_y - 26.0 + if index % 2 == 1 { 28.0 } else { -20.0 };
                let x = lerp(start_x, ad.to_x, clamped_fly);
                let y = lerp(start_y, ad.to_y, clamped_fly);
                let rot = (1.0 - clamped_fly) * if index % 2 == 1 { 9.0 } else { -8.0 };

                let impacted = enabled && fly >= impact_at;
                let since_impact = if impacted {
                    (age - SHIELD_FLIGHT * impact_at).max(0.0)
                } else {
                    0.0
                };
                let dissolve = if impacted { clamp01(since_impact / 0.24) } else { 0.0 };
                let alpha = (1.0 - reset_clear) * (1.0 - dissolve);
                let scale = (1.0 - dissolve * 0.42) * (1.0 - reset_clear * 0.3);
                let fill = match ad.color_idx {
                    0 => OnboardingPalette::mode_pastel(InputMode::Toolbar),
                    1 => OnboardingPalette::mode_pastel(InputMode::Normal),
                    2 => Rgba::rgb(0xff, 0x96, 0x6c),
                    _ => OnboardingPalette::mode_pastel(InputMode::Visual),
                };

                if alpha > 0.02 {
                    let s = canvas.save();
                    canvas.translate((x, y));
                    canvas.rotate(rot, None);
                    canvas.scale((scale, scale));
                    draw_rrect_fill(
                        canvas,
                        -ad.w * 0.5,
                        -ad.h * 0.5,
                        ad.w,
                        ad.h,
                        12.0,
                        sk_alpha(fill, alpha),
                    );
                    draw_rrect_stroke(
                        canvas,
                        -ad.w * 0.5,
                        -ad.h * 0.5,
                        ad.w,
                        ad.h,
                        12.0,
                        2.0,
                        sk_alpha(pal.ink, alpha),
                    );
                    draw_text_centered(canvas, ad.label, 0.0, 4.5, &ad_font, sk_alpha(pal.ink, alpha));
                    canvas.restore_to_count(s);
                }

                if impacted && since_impact < 0.38 {
                    let p = ease_out(since_impact / 0.38);
                    let dist = 10.0 + p * 34.0;
                    for dot in 0..6 {
                        let angle = (dot as f32 / 6.0) * 2.0 * PI + index as f32 * 0.4;
                        draw_circle_fill(
                            canvas,
                            x + cos_f32(angle) * dist,
                            y + sin_f32(angle) * dist,
                            4.5 * (1.0 - p * 0.7),
                            sk_alpha(fill, 1.0 - p),
                        );
                    }
                }
            }

            // Rounded-visor Zendia shield badge (fades & scales smoothly on toggle)
            if shield_blend > 0.01 {
                let pulse = if enabled && nearest_impact < 0.28 {
                    1.0 + sin_f32((1.0 - nearest_impact / 0.28) * PI) * 0.08
                } else {
                    1.0
                };
                let badge_scale = pulse * lerp(0.84, 1.0, shield_blend);
                let s = canvas.save();
                canvas.translate((494.0, 212.0));
                canvas.scale((badge_scale, badge_scale));

                // Outer ink border
                let mut outer_pb = sk::PathBuilder::new();
                outer_pb.move_to(sk::Point::new(0.0, -48.0));
                outer_pb.line_to(sk::Point::new(39.0, -32.0));
                outer_pb.line_to(sk::Point::new(39.0, 4.0));
                outer_pb.quad_to(sk::Point::new(36.0, 38.0), sk::Point::new(0.0, 54.0));
                outer_pb.quad_to(sk::Point::new(-36.0, 38.0), sk::Point::new(-39.0, 4.0));
                outer_pb.line_to(sk::Point::new(-39.0, -32.0));
                outer_pb.close();
                let outer_path = outer_pb.detach();

                let mut ink_fill = sk::Paint::default();
                ink_fill.set_anti_alias(true);
                ink_fill.set_style(sk::PaintStyle::Fill);
                ink_fill.set_color(sk_alpha(pal.ink, shield_blend));
                canvas.draw_path(&outer_path, &ink_fill);

                let mut ink_stroke = sk::Paint::default();
                ink_stroke.set_anti_alias(true);
                ink_stroke.set_style(sk::PaintStyle::Stroke);
                ink_stroke.set_stroke_width(9.0);
                ink_stroke.set_stroke_join(sk::PaintJoin::Round);
                ink_stroke.set_color(sk_alpha(pal.ink, shield_blend));
                canvas.draw_path(&outer_path, &ink_stroke);

                // Yellow body
                let mut body_pb = sk::PathBuilder::new();
                body_pb.move_to(sk::Point::new(0.0, -44.0));
                body_pb.line_to(sk::Point::new(35.0, -29.0));
                body_pb.line_to(sk::Point::new(35.0, 3.0));
                body_pb.quad_to(sk::Point::new(32.0, 34.0), sk::Point::new(0.0, 49.0));
                body_pb.quad_to(sk::Point::new(-32.0, 34.0), sk::Point::new(-35.0, 3.0));
                body_pb.line_to(sk::Point::new(-35.0, -29.0));
                body_pb.close();
                let body_path = body_pb.detach();

                let mut body_fill = sk::Paint::default();
                body_fill.set_anti_alias(true);
                body_fill.set_style(sk::PaintStyle::Fill);
                body_fill.set_color(sk_alpha(
                    OnboardingPalette::mode_pastel(InputMode::Toolbar),
                    shield_blend,
                ));
                canvas.draw_path(&body_path, &body_fill);

                // Checkmark
                let mut check_pb = sk::PathBuilder::new();
                check_pb.move_to(sk::Point::new(-14.0, 2.0));
                check_pb.line_to(sk::Point::new(-4.0, 13.0));
                check_pb.line_to(sk::Point::new(17.0, -12.0));
                let check_path = check_pb.detach();

                let mut check_paint = sk::Paint::default();
                check_paint.set_anti_alias(true);
                check_paint.set_style(sk::PaintStyle::Stroke);
                check_paint.set_stroke_width(7.0);
                check_paint.set_stroke_cap(sk::PaintCap::Round);
                check_paint.set_stroke_join(sk::PaintJoin::Round);
                check_paint.set_color(sk_alpha(pal.ink, shield_blend));
                canvas.draw_path(&check_path, &check_paint);

                canvas.restore_to_count(s);
            }

            canvas.restore_to_count(save);
        });

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(Fill::Callback(cb))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/* =========================================================================
Chapter 04 — Move in (MoveInScene: Import / Search / Default)
========================================================================= */

#[derive(Clone, PartialEq)]
pub struct MoveInScene {
    pub focus: MoveInFocus,
    pub source: Option<ImportSource>,
    pub engine_name: String,
    pub default_browser: bool,
    pub vim: bool,
    pub still: bool,
    pub paused: bool,
    pub frozen_at: Option<f32>,
    pub key: DiffKey,
}

impl KeyExt for MoveInScene {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for MoveInScene {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let pal = OnboardingPalette::for_scheme(theme.scheme);
        let (loop_dur, pose) = match self.focus {
            MoveInFocus::Import => (5.4, 2.0),
            MoveInFocus::Search => (5.8, 2.3),
            MoveInFocus::DefaultBrowser => (5.2, 1.7),
        };
        let t = use_scene_clock(loop_dur, pose, self.still, self.paused, self.frozen_at);

        let focus = self.focus;
        let source = self.source.clone();
        let last_source = use_last_some(&source);
        let importing_now = source.is_some();
        let still = self.still;
        let import_blend_anim = use_animation_with_dependencies(&importing_now, move |conf, active| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(if still { OnChange::Finish } else { OnChange::Rerun });
            let (from, to) = if *active { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::ONB_MOVEIN_TRANSITION)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let import_blend = import_blend_anim.get().value().clamp(0.0, 1.0);
        let display_source = source.or(last_source);

        let default_browser = self.default_browser;
        let default_blend_anim = use_animation_with_dependencies(&default_browser, move |conf, active| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(if still { OnChange::Finish } else { OnChange::Rerun });
            let (from, to) = if *active { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::ONB_MOVEIN_TRANSITION)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let default_blend = default_blend_anim.get().value().clamp(0.0, 1.0);

        let engine_name = self.engine_name.clone();
        let vim = self.vim;

        let cb = RenderCallback::new(move |ctx| {
            let RenderTarget::Background { canvas, .. } = &mut ctx.target else {
                return;
            };
            let Some(save) = fit_viewbox(canvas, ctx.size.width, ctx.size.height, 660.0, 436.0) else {
                return;
            };
            match focus {
                MoveInFocus::Import => {
                    paint_import_subscene(canvas, &pal, display_source.as_ref(), import_blend, vim, t, loop_dur)
                }
                MoveInFocus::Search => paint_search_subscene(canvas, &pal, &engine_name, vim, t, loop_dur),
                MoveInFocus::DefaultBrowser => {
                    paint_default_subscene(canvas, &pal, default_browser, default_blend, vim, t, loop_dur)
                }
            }
            canvas.restore_to_count(save);
        });

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(Fill::Callback(cb))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn paint_import_subscene(
    canvas: &sk::Canvas,
    pal: &OnboardingPalette,
    source: Option<&ImportSource>,
    import_blend: f32,
    vim: bool,
    t: f32,
    loop_dur: f32,
) {
    let blend = import_blend.clamp(0.0, 1.0);

    if blend > 0.01
        && let Some(src) = source
    {
        // Previous browser card on the left smoothly slides/scales/fades in and out
        let s = canvas.save();
        let card_x = lerp(8.0, 26.0, blend);
        let card_scale = lerp(0.92, 1.0, blend);
        canvas.translate((card_x + 83.0 * (1.0 - card_scale), 124.0 + 88.0 * (1.0 - card_scale)));
        canvas.scale((card_scale, card_scale));
        draw_rrect_fill(canvas, 0.0, 0.0, 166.0, 176.0, 20.0, sk_alpha(pal.surface, blend));
        draw_rrect_fill(canvas, 0.0, 0.0, 166.0, 32.0, 20.0, sk_alpha(pal.raised, blend));
        draw_rrect_stroke(canvas, 0.0, 0.0, 166.0, 176.0, 20.0, 1.6, sk_alpha(pal.border, blend));
        for cx in [18.0_f32, 30.0, 42.0] {
            draw_circle_fill(canvas, cx, 16.0, 3.5, sk_alpha(pal.muted, 0.55 * blend));
        }
        draw_rrect_fill(
            canvas,
            52.0,
            54.0,
            62.0,
            62.0,
            18.0,
            sk_alpha(Rgba::rgb(255, 255, 255), blend),
        );
        draw_rrect_stroke(canvas, 52.0, 54.0, 62.0, 62.0, 18.0, 1.2, sk_alpha(pal.border, blend));
        if blend > 0.2 {
            paint_brand_svg_on_canvas(canvas, &src.icon, 65.0, 67.0, 36.0);
        }
        let name_font = make_font(13.0, true, false);
        draw_text_centered(canvas, &src.name, 83.0, 144.0, &name_font, sk_alpha(pal.text, blend));
        canvas.restore_to_count(s);
    }

    // Zendia browser smoothly shifts right and scales down when importing
    let params = BrowserSceneParams {
        x: lerp(76.0, 242.0, blend),
        y: lerp(52.0, 72.0, blend),
        scale: lerp(0.85, 0.65, blend),
        vim: if vim { 1.0 } else { 0.0 },
        mode: InputMode::Library,
        ..BrowserSceneParams::default()
    };
    paint_browser_2d(canvas, pal, &params, |_| {});

    if blend > 0.02 {
        let teal = OnboardingPalette::mode_pastel(InputMode::Library);
        draw_dashed_quad(
            canvas,
            (174.0, 196.0),
            (246.0, 86.0),
            (314.0, 196.0),
            2.2,
            Some(&[6.0, 7.0]),
            sk_alpha(teal, 0.55 * blend),
        );

        let tokens = [
            (OnboardingPalette::mode_pastel(InputMode::Toolbar), 0u8), // star (bookmarks)
            (teal, 1u8),                                               // clock (history)
            (OnboardingPalette::mode_pastel(InputMode::Normal), 2u8),  // key (passwords)
        ];
        for (i, (col, kind)) in tokens.into_iter().enumerate() {
            let p = (t / loop_dur + i as f32 / 3.0) % 1.0;
            if p > 0.78 {
                continue;
            }
            let u = ease_in_out(p / 0.78);
            let (tx, ty) = quad_point((174.0, 196.0), (246.0, 86.0), (314.0, 196.0), u);
            let sc = (p / 0.1).min(1.0) * ((0.78 - p) / 0.1).min(1.0) * blend;
            let ts = canvas.save();
            canvas.translate((tx, ty));
            canvas.scale((sc, sc));
            draw_rrect_fill(canvas, -20.0, -20.0, 40.0, 40.0, 13.0, sk_alpha(col, blend));
            draw_rrect_stroke(canvas, -20.0, -20.0, 40.0, 40.0, 13.0, 2.2, sk_alpha(pal.ink, blend));

            let mut stroke = sk::Paint::default();
            stroke.set_anti_alias(true);
            stroke.set_style(sk::PaintStyle::Stroke);
            stroke.set_stroke_width(2.4);
            stroke.set_stroke_cap(sk::PaintCap::Round);
            stroke.set_color(sk_alpha(pal.ink, blend));

            match kind {
                0 => {
                    let mut pb = sk::PathBuilder::new();
                    let star = [
                        (0.0, -9.0),
                        (2.6, -3.2),
                        (9.0, -2.5),
                        (4.2, 1.8),
                        (5.5, 8.2),
                        (0.0, 4.9),
                        (-5.5, 8.2),
                        (-4.2, 1.8),
                        (-9.0, -2.5),
                        (-2.6, -3.2),
                    ];
                    pb.move_to(sk::Point::new(star[0].0, star[0].1));
                    for &(sx, sy) in &star[1..] {
                        pb.line_to(sk::Point::new(sx, sy));
                    }
                    pb.close();
                    let mut fill = sk::Paint::default();
                    fill.set_anti_alias(true);
                    fill.set_color(sk_alpha(pal.ink, blend));
                    canvas.draw_path(&pb.detach(), &fill);
                }
                1 => {
                    canvas.draw_circle(sk::Point::new(0.0, 0.0), 8.0, &stroke);
                    canvas.draw_line(sk::Point::new(0.0, -4.5), sk::Point::new(0.0, 0.0), &stroke);
                    canvas.draw_line(sk::Point::new(0.0, 0.0), sk::Point::new(3.5, 2.5), &stroke);
                }
                _ => {
                    canvas.draw_circle(sk::Point::new(-4.0, 0.0), 4.0, &stroke);
                    canvas.draw_line(sk::Point::new(0.0, 0.0), sk::Point::new(9.0, 0.0), &stroke);
                    canvas.draw_line(sk::Point::new(6.0, 0.0), sk::Point::new(6.0, 4.0), &stroke);
                }
            }
            canvas.restore_to_count(ts);
        }
    }
}

fn paint_search_subscene(
    canvas: &sk::Canvas,
    pal: &OnboardingPalette,
    engine_name: &str,
    vim: bool,
    t: f32,
    loop_dur: f32,
) {
    let query = "vim keys";
    let type_start = 0.9_f32;
    let type_step = 0.12_f32;
    let submit = type_start + query.len() as f32 * type_step + 0.65;

    let open = onboarding_spring(span(t, 0.25, 0.45)) * (1.0 - span(t, submit, 0.16));
    let typed_count = if t >= type_start {
        (((t - type_start) / type_step).floor() as isize + 1).clamp(0, query.len() as isize) as usize
    } else {
        0
    };
    let query_text = &query[..typed_count];
    let page_mix = span(t, submit + 0.08, 0.24) * (1.0 - span(t, loop_dur - 0.5, 0.4));

    let params = BrowserSceneParams {
        x: 30.0,
        y: 28.0,
        scale: 1.0,
        vim: if vim { 1.0 } else { 0.0 },
        mode: InputMode::Library,
        page_variant: 0,
        next_page_variant: 2,
        page_mix,
        ..BrowserSceneParams::default()
    };

    paint_browser_2d(canvas, pal, &params, |vp| {
        if open > 0.02 {
            let alpha = clamp01(open * 1.4);
            let teal = OnboardingPalette::mode_pastel(InputMode::Library);
            let s = vp.save();
            vp.translate((220.0, lerp(136.0, 120.0, open)));
            let sc = lerp(0.92, 1.0, open);
            vp.scale((sc, sc));

            draw_rrect_fill(vp, -166.0, -24.0, 332.0, 48.0, 24.0, sk_alpha(pal.surface, alpha));
            draw_rrect_stroke(vp, -166.0, -24.0, 332.0, 48.0, 24.0, 2.0, sk_alpha(teal, alpha));

            draw_circle_fill(vp, -138.0, 0.0, 14.0, sk_alpha(Rgba::rgb(255, 255, 255), alpha));
            let mut ring = sk::Paint::default();
            ring.set_anti_alias(true);
            ring.set_style(sk::PaintStyle::Stroke);
            ring.set_stroke_width(1.0);
            ring.set_color(sk_alpha(pal.border, alpha));
            vp.draw_circle(sk::Point::new(-138.0, 0.0), 14.0, &ring);
            paint_brand_svg_on_canvas(vp, engine_name, -147.0, -9.0, 18.0);

            let q_font = make_font(14.5, false, false);
            draw_text_left(vp, query_text, -114.0, 5.0, &q_font, sk_alpha(pal.text, alpha));
            let caret_a = if sin_f32(t * 7.0) > -0.1 { 1.0 } else { 0.15 };
            draw_rrect_fill(
                vp,
                -112.0 + query_text.len() as f32 * 8.1,
                -9.0,
                2.0,
                18.0,
                1.0,
                sk_alpha(teal, caret_a * alpha),
            );

            if !engine_name.is_empty() {
                draw_rrect_fill(vp, 72.0, -11.0, 82.0, 22.0, 11.0, sk_alpha(teal, 0.20 * alpha));
                let badge_font = make_font(9.5, true, false);
                let short: String = engine_name.chars().take(12).collect();
                draw_text_centered(vp, &short, 113.0, 3.5, &badge_font, sk_alpha(pal.teal, alpha));
            }
            vp.restore_to_count(s);
        }
    });
}

fn paint_default_subscene(
    canvas: &sk::Canvas,
    pal: &OnboardingPalette,
    enabled: bool,
    default_blend: f32,
    vim: bool,
    t: f32,
    loop_dur: f32,
) {
    let blend = default_blend.clamp(0.0, 1.0);
    let click = 1.0_f32;
    let flight = 0.95_f32;
    let fly = ease_in_out(span(t, click, flight));
    let landed = t > click + flight;
    let reset = 1.0 - span(t, loop_dur - 0.5, 0.4);
    let opened = if landed && enabled { reset } else { 0.0 };

    // Left chat/app card
    {
        let s = canvas.save();
        canvas.translate((24.0, 106.0));
        draw_rrect_fill(canvas, 0.0, 0.0, 164.0, 224.0, 20.0, sk_color(pal.surface));
        draw_rrect_stroke(canvas, 0.0, 0.0, 164.0, 224.0, 20.0, 1.5, sk_color(pal.border));
        draw_rrect_fill(canvas, 16.0, 18.0, 64.0, 8.0, 4.0, sk_alpha(pal.muted, 0.45));
        draw_rrect_fill(canvas, 16.0, 42.0, 116.0, 44.0, 13.0, sk_color(pal.page_soft));
        draw_rrect_fill(canvas, 28.0, 56.0, 82.0, 6.0, 3.0, sk_alpha(pal.muted, 0.55));
        draw_rrect_fill(canvas, 28.0, 68.0, 54.0, 6.0, 3.0, sk_alpha(pal.muted, 0.40));
        draw_rrect_fill(canvas, 16.0, 98.0, 132.0, 60.0, 13.0, sk_color(pal.page_soft));
        draw_rrect_fill(canvas, 28.0, 112.0, 90.0, 6.0, 3.0, sk_alpha(pal.muted, 0.55));
        draw_rrect_fill(
            canvas,
            28.0,
            128.0,
            104.0,
            20.0,
            10.0,
            sk_color(OnboardingPalette::mode_pastel(InputMode::Explorer)),
        );
        draw_rrect_fill(canvas, 40.0, 135.5, 76.0, 5.0, 2.5, sk_alpha(pal.ink, 0.82));
        canvas.restore_to_count(s);
    }

    // Other browser card smoothly swaps prominence with Zendia browser over 380 ms
    {
        let s = canvas.save();
        let ox = lerp(246.0, 304.0, blend);
        let oy = lerp(120.0, 38.0, blend);
        let osc = lerp(1.0, 0.72, blend);
        let oa = lerp(1.0, 0.45, blend);
        canvas.translate((ox, oy));
        canvas.scale((osc, osc));
        draw_rrect_fill(canvas, 0.0, 0.0, 256.0, 170.0, 18.0, sk_alpha(pal.raised, oa));
        draw_rrect_stroke(canvas, 0.0, 0.0, 256.0, 170.0, 18.0, 1.5, sk_alpha(pal.border, oa));
        draw_rrect_fill(canvas, 14.0, 16.0, 66.0, 16.0, 6.0, sk_alpha(pal.surface, oa));
        draw_rrect_fill(canvas, 14.0, 44.0, 228.0, 112.0, 10.0, sk_alpha(pal.surface, 0.7 * oa));
        canvas.restore_to_count(s);
    }

    // Zendia browser (prominent when Default = YES)
    let params = BrowserSceneParams {
        x: lerp(318.0, 226.0, blend),
        y: lerp(246.0, 96.0, blend),
        scale: lerp(0.44, 0.67, blend),
        vim: if vim { 1.0 } else { 0.0 },
        mode: InputMode::Library,
        tab_index: if opened > 0.5 { 2.0 } else { 0.0 },
        page_variant: 0,
        next_page_variant: 2,
        page_mix: opened * span(t, click + flight, 0.2),
        ..BrowserSceneParams::default()
    };
    paint_browser_2d(canvas, pal, &params, |_| {});

    // Flying link capsule
    if t > click && !landed {
        let goal_x = lerp(374.0, 378.0, blend);
        let goal_y = lerp(114.0, 228.0, blend);
        let lx = lerp(114.0, goal_x, fly);
        let ly = lerp(236.0, goal_y, fly) - sin_f32(fly * PI) * 74.0;
        let sc = ease_out((t - click) / 0.14);
        let ls = canvas.save();
        canvas.translate((lx, ly));
        canvas.scale((sc, sc));
        draw_rrect_fill(
            canvas,
            -46.0,
            -13.0,
            92.0,
            26.0,
            13.0,
            sk_color(OnboardingPalette::mode_pastel(InputMode::Explorer)),
        );
        draw_rrect_stroke(canvas, -46.0, -13.0, 92.0, 26.0, 13.0, 2.0, sk_color(pal.ink));
        draw_circle_fill(canvas, -30.0, 0.0, 4.0, sk_color(pal.ink));
        draw_rrect_fill(canvas, -20.0, -2.5, 52.0, 5.0, 2.5, sk_alpha(pal.ink, 0.85));
        canvas.restore_to_count(ls);
    }
}

/* =========================================================================
Chapter 05 — Apps (Close-up Zen Sidebar Preview)
========================================================================= */

#[derive(Clone, PartialEq)]
pub struct AppsScene {
    pub apps: Vec<&'static EssentialAppPreset>,
    pub vim: bool,
    pub still: bool,
    pub key: DiffKey,
}

impl KeyExt for AppsScene {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for AppsScene {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let pal = OnboardingPalette::for_scheme(theme.scheme);
        let green_pastel = OnboardingPalette::mode_pastel(InputMode::Insert);
        let win_border = mix_rgba(pal.border, green_pastel, 0.55);

        let sidebar_tabs = [
            ("Project roadmap", pal.pink, true),
            ("Design notes", pal.blue, false),
            ("Pull requests", pal.green, false),
            ("Documentation", pal.teal, false),
        ];

        // Build rows of 3 tiles (or 3 dashed empty slots when empty)
        let still = self.still;
        let tile_rows: Vec<Element> = if self.apps.is_empty() {
            vec![
                rect()
                    .key("empty-row")
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .spacing(8.0)
                    .children((0..3).map(|slot| {
                        let dash_col = pal.border;
                        let dash_cb = RenderCallback::new(move |ctx| {
                            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                                draw_dashed_rrect_stroke(
                                    canvas,
                                    sk::Rect::from_xywh(0.0, 0.0, ctx.size.width, ctx.size.height),
                                    13.0,
                                    1.5,
                                    &[5.0, 4.0],
                                    sk_alpha(dash_col, 0.65),
                                );
                            }
                        });
                        rect()
                            .key(slot)
                            .width(Size::flex(1.0))
                            .height(Size::px(48.0))
                            .corner_radius(CornerRadius::new_all(13.0))
                            .background(Fill::Callback(dash_cb))
                    }))
                    .into_element(),
            ]
        } else {
            self.apps
                .chunks(3)
                .enumerate()
                .map(|(row_idx, chunk)| {
                    rect()
                        .key(row_idx)
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .spacing(8.0)
                        .children((0..3).map(|col| {
                            if let Some(app) = chunk.get(col) {
                                PinnedAppPreviewTile {
                                    app,
                                    still,
                                    key: DiffKey::from(&(app.id)),
                                }
                                .into_element()
                            } else {
                                rect()
                                    .key(format!("pad-{row_idx}-{col}"))
                                    .width(Size::flex(1.0))
                                    .height(Size::px(48.0))
                                    .into_element()
                            }
                        }))
                        .into_element()
                })
                .collect()
        };

        let shadow = if pal.dark {
            Shadow::new().y(18.0).blur(44.0).color(Color::from_argb(96, 0, 0, 0))
        } else {
            Shadow::new()
                .y(12.0)
                .blur(32.0)
                .color(Color::from_argb(24, 0x1a, 0x1b, 0x26))
        };

        rect()
            .width(Size::fill())
            .max_width(Size::px(560.0))
            .padding(Gaps::new_all(8.0))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(384.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .corner_radius(CornerRadius::new_all(20.0))
                    .background(pal.surface.to_color())
                    .border(
                        Border::new()
                            .width(1.5)
                            .alignment(BorderAlignment::Inner)
                            .fill(win_border.to_color()),
                    )
                    .shadow(shadow)
                    .overflow(Overflow::Clip)
                    .child(
                        // Left Zen sidebar closeup
                        rect()
                            .width(Size::px(224.0))
                            .height(Size::fill())
                            .padding(Gaps::new_all(14.0))
                            .spacing(12.0)
                            .background(pal.raised.to_color())
                            .child(
                                // Window traffic dots
                                rect()
                                    .direction(Direction::Horizontal)
                                    .spacing(6.0)
                                    .child(win_dot(Rgba::rgb(0xff, 0x75, 0x7f)))
                                    .child(win_dot(Rgba::rgb(0xff, 0xc7, 0x77)))
                                    .child(win_dot(Rgba::rgb(0xc3, 0xe8, 0x8d))),
                            )
                            .child(
                                // Zen pill urlbar
                                rect()
                                    .width(Size::fill())
                                    .height(Size::px(28.0))
                                    .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(8.0)
                                    .corner_radius(CornerRadius::new_all(999.0))
                                    .background(pal.surface.to_color())
                                    .border(
                                        Border::new()
                                            .width(1.0)
                                            .alignment(BorderAlignment::Inner)
                                            .fill(pal.border_subtle.to_color()),
                                    )
                                    .child(
                                        rect()
                                            .width(Size::px(7.0))
                                            .height(Size::px(7.0))
                                            .corner_radius(CornerRadius::new_all(4.0))
                                            .background(green_pastel.to_color()),
                                    )
                                    .child(
                                        rect()
                                            .width(Size::px(72.0))
                                            .height(Size::px(5.0))
                                            .corner_radius(CornerRadius::new_all(3.0))
                                            .background(pal.muted.with_alpha(0.45).to_color()),
                                    ),
                            )
                            .child(rect().width(Size::fill()).spacing(8.0).children(tile_rows))
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .height(Size::px(1.0))
                                    .background(pal.border_subtle.to_color()),
                            )
                            .child(rect().width(Size::fill()).spacing(6.0).children(
                                sidebar_tabs.into_iter().enumerate().map(|(i, (title, dot, active))| {
                                    let mut row = rect()
                                        .key(i)
                                        .width(Size::fill())
                                        .direction(Direction::Horizontal)
                                        .cross_align(Alignment::center())
                                        .padding(Gaps::new(7.0, 10.0, 7.0, 10.0))
                                        .spacing(9.0)
                                        .corner_radius(CornerRadius::new_all(10.0));
                                    if active {
                                        row = row.background(pal.surface.to_color()).shadow(
                                            Shadow::new().y(1.0).blur(2.0).color(Color::from_argb(16, 0, 0, 0)),
                                        );
                                    }
                                    row.child(
                                        rect()
                                            .width(Size::px(7.0))
                                            .height(Size::px(7.0))
                                            .corner_radius(CornerRadius::new_all(4.0))
                                            .background(dot.to_color()),
                                    )
                                    .child(
                                        label()
                                            .text(title)
                                            .font_size(12.5)
                                            .font_weight(FontWeight::SEMI_BOLD)
                                            .max_lines(1)
                                            .text_overflow(TextOverflow::Ellipsis)
                                            .color(if active {
                                                pal.text.to_color()
                                            } else {
                                                pal.muted.to_color()
                                            }),
                                    )
                                }),
                            )),
                    )
                    .child(
                        // Crisp 1px right divider between sidebar and page peek (matches fork `border-inline-end`)
                        rect()
                            .width(Size::px(1.0))
                            .height(Size::fill())
                            .background(pal.border_subtle.to_color()),
                    )
                    .child(
                        // Right page peek
                        rect()
                            .width(Size::flex(1.0))
                            .height(Size::fill())
                            .padding(Gaps::new(24.0, 22.0, 24.0, 22.0))
                            .spacing(12.0)
                            .background(pal.surface.to_color())
                            .child(
                                rect()
                                    .width(Size::px(68.0))
                                    .height(Size::px(12.0))
                                    .corner_radius(CornerRadius::new_all(6.0))
                                    .background(green_pastel.with_alpha(0.26).to_color()),
                            )
                            .child(
                                rect()
                                    .width(Size::percent(82.0))
                                    .height(Size::px(16.0))
                                    .corner_radius(CornerRadius::new_all(6.0))
                                    .background(pal.text.with_alpha(0.80).to_color()),
                            )
                            .child(
                                rect()
                                    .width(Size::percent(58.0))
                                    .height(Size::px(12.0))
                                    .corner_radius(CornerRadius::new_all(5.0))
                                    .background(pal.muted.with_alpha(0.45).to_color()),
                            )
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .margin(Gaps::new(8.0, 0.0, 0.0, 0.0))
                                    .padding(Gaps::new_all(18.0))
                                    .spacing(10.0)
                                    .corner_radius(CornerRadius::new_all(16.0))
                                    .background(pal.raised.to_color())
                                    .child(
                                        rect()
                                            .width(Size::percent(86.0))
                                            .height(Size::px(8.0))
                                            .corner_radius(CornerRadius::new_all(4.0))
                                            .background(pal.muted.with_alpha(0.40).to_color()),
                                    )
                                    .child(
                                        rect()
                                            .width(Size::percent(52.0))
                                            .height(Size::px(8.0))
                                            .corner_radius(CornerRadius::new_all(4.0))
                                            .background(pal.muted.with_alpha(0.40).to_color()),
                                    ),
                            )
                            .maybe_child(self.vim.then(|| {
                                rect()
                                    .position(Position::new_absolute().right(14.0).bottom(14.0))
                                    .width(Size::px(48.0))
                                    .height(Size::px(48.0))
                                    .child(
                                        OnboardingMascot::new(InputMode::Insert, 48.0)
                                            .still(self.still)
                                            .blink(false),
                                    )
                            })),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn win_dot(color: Rgba) -> Element {
    rect()
        .width(Size::px(8.0))
        .height(Size::px(8.0))
        .corner_radius(CornerRadius::new_all(4.0))
        .background(color.with_alpha(0.78).to_color())
        .into_element()
}

#[derive(Clone, PartialEq)]
struct PinnedAppPreviewTile {
    app: &'static EssentialAppPreset,
    still: bool,
    key: DiffKey,
}

impl KeyExt for PinnedAppPreviewTile {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for PinnedAppPreviewTile {
    fn render(&self) -> impl IntoElement {
        let still = self.still;
        let pop_anim = use_animation(move |conf| {
            conf.on_creation(if still { OnCreation::Finish } else { OnCreation::Run });
            AnimNum::new(0.0, 1.0)
                .duration(motion::ONB_TILE_POP)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let p = if still {
            1.0
        } else {
            pop_anim.get().value().clamp(0.0, 1.0)
        };
        let scale = lerp(0.6, 1.0, onboarding_spring(p));
        let opacity = clamp01(p * 1.6);

        let tile_col = Rgba::rgb(self.app.color_rgb.0, self.app.color_rgb.1, self.app.color_rgb.2);
        let tile_bg = mix_rgba(Rgba::rgb(255, 255, 255), tile_col, 0.18);

        rect()
            .width(Size::flex(1.0))
            .height(Size::px(48.0))
            .corner_radius(CornerRadius::new_all(13.0))
            .background(tile_bg.to_color())
            .border(
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(tile_col.with_alpha(0.65).to_color()),
            )
            .shadow(Shadow::new().y(2.0).blur(6.0).color(Color::from_argb(20, 0, 0, 0)))
            .scale(scale)
            .opacity(opacity)
            .center()
            .child(brand_logo_element(self.app.icon, 22.0))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/* =========================================================================
Chapter 06 — Go (Personalized Finale Preview + Summary Chips)
========================================================================= */

#[derive(Clone, PartialEq)]
pub struct GoScene {
    pub vim: bool,
    pub ads: bool,
    pub agent: Option<OnboardingAgentInfo>,
    pub essentials: Vec<&'static EssentialAppPreset>,
    pub still: bool,
    pub key: DiffKey,
}

impl KeyExt for GoScene {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for GoScene {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let pal = OnboardingPalette::for_scheme(theme.scheme);

        let agent_color = self
            .agent
            .as_ref()
            .map(|a| Rgba::rgb(a.accent_rgb.0, a.accent_rgb.1, a.accent_rgb.2));
        let tiles: Vec<EssentialTileSpec> = self
            .essentials
            .iter()
            .take(6)
            .map(|app| EssentialTileSpec {
                icon: app.icon.to_string(),
                color: Rgba::rgb(app.color_rgb.0, app.color_rgb.1, app.color_rgb.2),
            })
            .collect();

        let params = BrowserSceneParams {
            x: 20.0,
            y: 12.0,
            scale: 1.0,
            vim: if self.vim { 1.0 } else { 0.0 },
            mode: InputMode::Visual,
            tab_glow_color: agent_color,
            agent_name: self.agent.as_ref().map(|a| a.name.clone()),
            agent_color,
            essentials: tiles,
            ..BrowserSceneParams::default()
        };

        let cb = RenderCallback::new(move |ctx| {
            let RenderTarget::Background { canvas, .. } = &mut ctx.target else {
                return;
            };
            let Some(save) = fit_viewbox(canvas, ctx.size.width, ctx.size.height, 640.0, 400.0) else {
                return;
            };
            paint_browser_2d(canvas, &pal, &params, |_| {});
            canvas.restore_to_count(save);
        });

        let mut summary_chips: Vec<(Rgba, String)> = Vec::new();
        summary_chips.push((
            pal.mode_accent(InputMode::Normal),
            if self.vim {
                "Vim keys on".to_string()
            } else {
                "Standard shortcuts".to_string()
            },
        ));
        if let Some(ref agent) = self.agent {
            summary_chips.push((
                Rgba::rgb(agent.accent_rgb.0, agent.accent_rgb.1, agent.accent_rgb.2),
                agent.name.clone(),
            ));
        }
        summary_chips.push((
            pal.mode_accent(InputMode::Toolbar),
            if self.ads {
                "uBlock shield on".to_string()
            } else {
                "Ads allowed".to_string()
            },
        ));
        if !self.essentials.is_empty() {
            let count = self.essentials.len();
            summary_chips.push((
                pal.mode_accent(InputMode::Insert),
                format!("{count} pinned {}", if count == 1 { "app" } else { "apps" }),
            ));
        }

        rect()
            .width(Size::fill())
            .max_width(Size::px(620.0))
            .cross_align(Alignment::center())
            .spacing(18.0)
            .child(
                // Browser hero + overlapping bottom-left celebratory Visual mascot badge
                rect()
                    .width(Size::fill())
                    .height(Size::px(386.0))
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::fill())
                            .background(Fill::Callback(cb)),
                    )
                    .child(
                        rect()
                            .position(Position::new_absolute().left(-8.0).bottom(-14.0))
                            .layer(Layer::Relative(4))
                            .width(Size::px(126.0))
                            .height(Size::px(126.0))
                            .child(
                                OnboardingMascot::new(InputMode::Visual, 126.0)
                                    .still(self.still)
                                    .bloom(!self.still)
                                    .blink(!self.still)
                                    .track_pointer(!self.still)
                                    .drop_shadow(true),
                            ),
                    ),
            )
            .child(
                // Honest summary chips
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .main_align(Alignment::center())
                    .spacing(8.0)
                    .children(summary_chips.into_iter().enumerate().map(|(i, (chip_col, text))| {
                        let border = mix_rgba(pal.border_subtle, chip_col, 0.42);
                        rect()
                            .key(i)
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .padding(Gaps::new(6.0, 12.0, 6.0, 12.0))
                            .spacing(7.0)
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(pal.surface.to_color())
                            .border(
                                Border::new()
                                    .width(1.5)
                                    .alignment(BorderAlignment::Inner)
                                    .fill(border.to_color()),
                            )
                            .child(
                                rect()
                                    .width(Size::px(8.0))
                                    .height(Size::px(8.0))
                                    .corner_radius(CornerRadius::new_all(4.0))
                                    .background(chip_col.to_color()),
                            )
                            .child(
                                label()
                                    .text(text)
                                    .font_size(12.5)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(pal.text.to_color()),
                            )
                    })),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
